import type { DbEngine, QueryResult, TableMeta } from '$lib/api/db';
import { formatDbValue } from './valueFormat';

export type TableExportFormat = 'json' | 'csv' | 'xlsx' | 'markdown';
export type GeneratedQueryKind = 'select' | 'insert' | 'update' | 'delete';

function quoteIdent(engine: DbEngine | undefined, ident: string): string {
  const q = engine === 'mysql' ? '`' : '"';
  return `${q}${ident.replaceAll(q, `${q}${q}`)}${q}`;
}

export function qualifiedTableName(engine: DbEngine | undefined, schema: string, table: string): string {
  return schema
    ? `${quoteIdent(engine, schema)}.${quoteIdent(engine, table)}`
    : quoteIdent(engine, table);
}

export function selectTableQuery(
  engine: DbEngine | undefined,
  schema: string | undefined,
  table: string,
  columns?: string[],
  limit?: number
): string {
  const selected = columns && columns.length > 0 ? columns.map((c) => quoteIdent(engine, c)) : ['*'];
  const sql = `SELECT ${selected.join(', ')}\nFROM ${qualifiedTableName(engine, schema ?? '', table)}`;
  return limit == null ? sql : `${sql}\nLIMIT ${limit}`;
}

export function generatedQuery(
  engine: DbEngine | undefined,
  schema: string,
  table: TableMeta,
  kind: GeneratedQueryKind
): string {
  const name = qualifiedTableName(engine, schema, table.name);
  const cols = table.columns.map((c) => quoteIdent(engine, c.name));
  const writable = table.columns.filter((c) => !c.pk || !isLikelyAutoColumn(c));
  const writableCols = writable.length > 0 ? writable : table.columns;
  const updateCols = writableCols.filter((c) => !c.pk);
  const pkCols = table.columns.filter((c) => c.pk);
  const whereCols = pkCols.length > 0 ? pkCols : table.columns.slice(0, 1);
  const placeholders = writableCols.map((_, i) => placeholder(engine, i + 1));

  switch (kind) {
    case 'select':
      return `${selectTableQuery(engine, schema, table.name, table.columns.map((c) => c.name), 100)};`;
    case 'insert':
      return `INSERT INTO ${name} (${writableCols.map((c) => quoteIdent(engine, c.name)).join(', ')})\nVALUES (${placeholders.join(', ')});`;
    case 'update':
      return `UPDATE ${name}\nSET ${updateCols
        .map((c, i) => `${quoteIdent(engine, c.name)} = ${placeholder(engine, i + 1)}`)
        .join(', ')}\nWHERE ${whereClause(engine, whereCols, updateCols.length + 1)};`;
    case 'delete':
      return `DELETE FROM ${name}\nWHERE ${whereClause(engine, whereCols, 1)};`;
  }
}

function placeholder(engine: DbEngine | undefined, n: number): string {
  return engine === 'postgres' ? `$${n}` : '?';
}

function whereClause(engine: DbEngine | undefined, cols: TableMeta['columns'], start: number): string {
  return cols.map((c, i) => `${quoteIdent(engine, c.name)} = ${placeholder(engine, start + i)}`).join(' AND ');
}

function isLikelyAutoColumn(col: TableMeta['columns'][number]): boolean {
  const t = col.data_type.toLowerCase();
  return !!col.pk && (t.includes('serial') || t.includes('identity'));
}

function cellText(value: unknown): string {
  return formatDbValue(value).replace(/\r?\n/g, ' ');
}

export function resultToMarkdown(result: QueryResult): string {
  const headers = result.columns.map((c) => c.name);
  const lines = [
    `| ${headers.map(escapeMarkdownCell).join(' | ')} |`,
    `| ${headers.map(() => '---').join(' | ')} |`
  ];
  for (const row of result.rows) {
    lines.push(`| ${row.map((v) => escapeMarkdownCell(cellText(v))).join(' | ')} |`);
  }
  return `${lines.join('\n')}\n`;
}

function escapeMarkdownCell(value: string): string {
  return value.replaceAll('\\', '\\\\').replaceAll('|', '\\|');
}

export function resultToXlsxBlob(result: QueryResult): Blob {
  const files = new Map<string, string>();
  files.set('[Content_Types].xml', contentTypesXml());
  files.set('_rels/.rels', rootRelsXml());
  files.set('xl/workbook.xml', workbookXml());
  files.set('xl/_rels/workbook.xml.rels', workbookRelsXml());
  files.set('xl/worksheets/sheet1.xml', sheetXml(result));
  const bytes = zipStore(files);
  return new Blob([bytes.buffer as ArrayBuffer], {
    type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  });
}

function sheetXml(result: QueryResult): string {
  const rows = [result.columns.map((c) => c.name), ...result.rows.map((r) => r.map(cellText))];
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>${rows
    .map(
      (row, ri) =>
        `<row r="${ri + 1}">${row
          .map((cell, ci) => `<c r="${columnName(ci)}${ri + 1}" t="inlineStr"><is><t>${xml(cell)}</t></is></c>`)
          .join('')}</row>`
    )
    .join('')}</sheetData></worksheet>`;
}

function columnName(index: number): string {
  let n = index + 1;
  let out = '';
  while (n > 0) {
    const rem = (n - 1) % 26;
    out = String.fromCharCode(65 + rem) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}

function contentTypesXml(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>`;
}

function rootRelsXml(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>`;
}

function workbookXml(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>`;
}

function workbookRelsXml(): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>`;
}

function xml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function zipStore(files: Map<string, string>): Uint8Array {
  const encoder = new TextEncoder();
  const locals: Uint8Array[] = [];
  const centrals: Uint8Array[] = [];
  let offset = 0;

  for (const [name, text] of files) {
    const nameBytes = encoder.encode(name);
    const data = encoder.encode(text);
    const crc = crc32(data);
    locals.push(localHeader(nameBytes, data, crc));
    centrals.push(centralHeader(nameBytes, data, crc, offset));
    offset += 30 + nameBytes.length + data.length;
  }

  const centralSize = centrals.reduce((n, b) => n + b.length, 0);
  const end = endRecord(files.size, centralSize, offset);
  return concat([...locals, ...centrals, end]);
}

function localHeader(name: Uint8Array, data: Uint8Array, crc: number): Uint8Array {
  const out = new Uint8Array(30 + name.length + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x04034b50, true);
  view.setUint16(4, 20, true);
  view.setUint32(14, crc, true);
  view.setUint32(18, data.length, true);
  view.setUint32(22, data.length, true);
  view.setUint16(26, name.length, true);
  out.set(name, 30);
  out.set(data, 30 + name.length);
  return out;
}

function centralHeader(name: Uint8Array, data: Uint8Array, crc: number, offset: number): Uint8Array {
  const out = new Uint8Array(46 + name.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x02014b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 20, true);
  view.setUint32(16, crc, true);
  view.setUint32(20, data.length, true);
  view.setUint32(24, data.length, true);
  view.setUint16(28, name.length, true);
  view.setUint32(42, offset, true);
  out.set(name, 46);
  return out;
}

function endRecord(count: number, centralSize: number, centralOffset: number): Uint8Array {
  const out = new Uint8Array(22);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(8, count, true);
  view.setUint16(10, count, true);
  view.setUint32(12, centralSize, true);
  view.setUint32(16, centralOffset, true);
  return out;
}

function concat(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((n, b) => n + b.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let i = 0; i < 8; i += 1) {
      crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}
