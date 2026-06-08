<!--
  Connection workspace.
  Layout (left → right):
    [ Sidebar: db selector + schema tree ]
    [ Main:    tab bar + active tab body  ]
  Tab kinds:
    sql   — CodeMirror editor + run button + results
    table — paginated SELECT * grid + Insert button
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import * as Resizable from '$lib/components/ui/resizable';
  import type { SessionKind } from '$lib/api/client';
  import { dbStore, type DbTab } from '$lib/stores/db.svelte';
  import { engineLabel, type Column, type TableMeta } from '$lib/api/db';
  import SchemaTree, {
    type SchemaTreeExportTarget,
    type SchemaTreeQueryGenerate,
    type SchemaTreeTableExport
  } from '$lib/components/db/SchemaTree.svelte';
  import ExportDialog, { type ExportDialogTarget } from '$lib/components/db/ExportDialog.svelte';
  import SqlEditor from '$lib/components/db/SqlEditor.svelte';
  import ResultGrid from '$lib/components/db/ResultGrid.svelte';
  import RowEditorPanel from '$lib/components/db/RowEditorPanel.svelte';
  import TableSchemaView from '$lib/components/db/TableSchemaView.svelte';
  import ConnectionFormDialog from '$lib/components/db/ConnectionFormDialog.svelte';
  import { ContextMenu, type ContextMenuItem } from '$lib/components/ui/context-menu';
  import TerminalView from '$lib/components/app/TerminalView.svelte';
  import { api } from '$lib/api/client';
  import {
    generatedQuery,
    qualifiedTableName,
    resultToMarkdown,
    resultToXlsxBlob
  } from '$lib/components/db/tableActions';
  import {
    Play,
    Plus,
    X,
    RefreshCw,
    Loader2,
    ChevronLeft,
    ChevronRight,
    MoreHorizontal,
    AlertTriangle,
    Bot,
    Copy,
    Database,
    Download,
    Edit3,
    FileCode2,
    FileJson,
    FileSpreadsheet,
    FileText,
    Maximize2,
    Minimize2,
    PanelLeftClose,
    PanelLeftOpen,
    PanelRightClose,
    PanelRightOpen,
    Trash2
  } from '$lib/icons';
  import { dbApi } from '$lib/api/db';
  import { confirmDialog } from '$lib/components/ui/confirm-dialog';
  import { toast } from 'svelte-sonner';

  const connId = $derived(($page.params.id ?? '') as string);
  const conn = $derived(dbStore.connections.find((c) => c.id === connId) ?? null);
  const ws = $derived(dbStore.workspace(connId));
  const activeTab = $derived<DbTab | null>(ws.tabs.find((t) => t.id === ws.activeTabId) ?? null);
  const activeMeta = $derived<TableMeta | null>(
    activeTab?.kind === 'table' ? (activeTab.tableMeta ?? null) : null
  );
  const activePkCols = $derived<string[]>(
    activeMeta ? activeMeta.columns.filter((c) => c.pk).map((c) => c.name) : []
  );
  const MAX_VISIBLE_TABS = 5;
  const shouldGroupTabs = $derived(ws.tabs.length > MAX_VISIBLE_TABS);
  const visibleTabs = $derived.by(() => {
    if (!shouldGroupTabs) return ws.tabs;
    const first = ws.tabs.slice(0, MAX_VISIBLE_TABS);
    const active = ws.tabs.find((t) => t.id === ws.activeTabId);
    if (!active || first.some((t) => t.id === active.id)) return first;
    return [...first.slice(0, MAX_VISIBLE_TABS - 1), active];
  });

  // Export dialog state (driven by SchemaTree right-click).
  let exportOpen = $state(false);
  let exportTarget = $state<ExportDialogTarget | null>(null);
  let connectionDialogOpen = $state(false);
  let startingDbAgent = $state(false);
  let selectedDbAgentKind = $state<SessionKind>('claude');
  const dbAgent = $derived(ws.agent);
  const dbAgentSize = $derived(dbAgent.size ?? 30);

  $effect(() => {
    selectedDbAgentKind = dbAgent.kind;
  });

  function onSchemaTreeExport(t: SchemaTreeExportTarget) {
    if (t.kind === 'table') {
      exportTarget = {
        kind: 'table',
        schema: t.schema,
        name: t.table.name,
        columns: t.table.columns
      };
    } else {
      exportTarget = { kind: 'schema', name: t.name, tables: t.tables };
    }
    exportOpen = true;
  }

  function triggerDownload(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1_000);
  }

  function safeName(value: string): string {
    return value.replace(/[^A-Za-z0-9_.-]+/g, '_').replace(/^_+|_+$/g, '') || 'export';
  }

  function resizeDbAgent(delta: number) {
    dbStore.setAgentSize(connId, dbAgentSize + delta);
  }

  async function onSchemaTreeTableExport(t: SchemaTreeTableExport) {
    const baseName = safeName(`${t.schema}.${t.table.name}`);
    try {
      if (t.format === 'json' || t.format === 'csv') {
        const { blob, filename } = await dbApi.export(connId, {
          database: ws.database ?? undefined,
          target: {
            kind: 'table',
            schema: t.schema,
            name: t.table.name
          },
          format: t.format,
          scope: 'data_only'
        });
        triggerDownload(blob, filename);
        toast.success(`Exported ${filename}`);
        return;
      }

      const sql = `SELECT * FROM ${qualifiedTableName(conn?.engine, t.schema, t.table.name)}`;
      const res = await dbApi.query(connId, {
        database: ws.database ?? undefined,
        sql,
        page: 0,
        page_size: 5000
      });
      if (t.format === 'markdown') {
        const blob = new Blob([resultToMarkdown(res.data)], {
          type: 'text/markdown;charset=utf-8'
        });
        triggerDownload(blob, `${baseName}.md`);
      } else {
        triggerDownload(resultToXlsxBlob(res.data), `${baseName}.xlsx`);
      }
      if (res.data.truncated) {
        toast.warning('Exported first page only; result was truncated by the query endpoint');
      } else {
        toast.success(`Exported ${baseName}.${t.format === 'markdown' ? 'md' : 'xlsx'}`);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Export failed');
    }
  }

  function onSchemaTreeGenerateQuery(t: SchemaTreeQueryGenerate) {
    const sql = generatedQuery(conn?.engine, t.schema, t.table, t.query);
    const id = dbStore.openSqlTab(connId, sql);
    dbStore.patchTab(connId, id, {
      title: `${t.query.toUpperCase()} ${t.schema}.${t.table.name}`
    });
  }

  // Row editor state
  let editorOpen = $state(false);
  let editorMode = $state<'insert' | 'update' | 'duplicate'>('insert');
  let editorInitial = $state<Record<string, unknown> | undefined>(undefined);
  let selectedRowByTab = $state<Record<string, number>>({});
  let rowMenuOpen = $state(false);
  let rowMenuX = $state(0);
  let rowMenuY = $state(0);
  let rowMenuTabId = $state<string | null>(null);
  let rowMenuIndex = $state<number | null>(null);
  let rowMenuItems = $state<ContextMenuItem[]>([]);

  // ── Inline cell-edit + insert pending buffer (derived from store) ───────
  /** This tab's cell-edit map (rowIndex → entry). */
  const pendingForActiveTab = $derived(
    activeTab ? (ws.pendingEdits[activeTab.id]?.edits ?? {}) : {}
  );
  /** This tab's inline-insert list (rendered as a band above the grid). */
  const pendingInsertsForActiveTab = $derived(
    activeTab ? (ws.pendingEdits[activeTab.id]?.inserts ?? []) : []
  );
  /** Cross-tab counters for the pending-changes sub-header bar. */
  const pendingTotals = $derived.by(() => {
    let cells = 0;
    let rowCount = 0;
    let inserts = 0;
    let errors = 0;
    for (const tabId in ws.pendingEdits) {
      const bucket = ws.pendingEdits[tabId];
      for (const ri in bucket.edits) {
        rowCount += 1;
        cells += Object.keys(bucket.edits[ri].changes).length;
      }
      for (const ins of bucket.inserts) {
        inserts += 1;
        if (ins.errors) errors += Object.keys(ins.errors).length;
      }
    }
    return { cells, rows: rowCount, inserts, errors };
  });
  let applyingPending = $state(false);
  const selectedRowIndex = $derived(
    activeTab?.kind === 'table' ? (selectedRowByTab[activeTab.id] ?? null) : null
  );
  const selectedRow = $derived(
    activeTab?.kind === 'table' && selectedRowIndex != null
      ? (activeTab.result?.rows?.[selectedRowIndex] ?? null)
      : null
  );
  const selectedRecord = $derived(
    activeMeta && selectedRow
      ? rowToRecordWithPending(activeMeta.columns, selectedRow, selectedRowIndex ?? -1)
      : null
  );

  /** Heuristic — auto-increment columns are read-only on inserts. */
  function isAutoIncrement(col: Column): boolean {
    if (!col.pk) return false;
    if (col.default != null && col.default !== '') return false;
    const t = col.data_type.toLowerCase();
    return t.includes('int') || t.includes('serial');
  }

  /** Per-insert validation: NOT-NULL required + simple type sanity. */
  function validateInsert(
    meta: TableMeta,
    values: Record<string, unknown>
  ): Record<string, string> {
    const errors: Record<string, string> = {};
    for (const col of meta.columns) {
      const raw = values[col.name];
      const empty = raw === undefined || raw === null || raw === '';
      if (empty) {
        const hasDefault = col.default != null && col.default !== '';
        if (!col.nullable && !hasDefault && !isAutoIncrement(col)) {
          errors[col.name] = 'Required';
        }
        continue;
      }
      const t = col.data_type.toLowerCase();
      const numeric =
        t.includes('int') ||
        t.includes('float') ||
        t.includes('numeric') ||
        t.includes('decimal') ||
        t.includes('double') ||
        t.includes('real') ||
        t.includes('serial');
      if (numeric) {
        const n = typeof raw === 'number' ? raw : Number(raw);
        if (!Number.isFinite(n)) errors[col.name] = 'Invalid number';
        continue;
      }
      if (t === 'date') {
        if (!/^\d{4}-\d{2}-\d{2}$/.test(String(raw))) errors[col.name] = 'Use YYYY-MM-DD';
        continue;
      }
      if (t.includes('timestamp') || t.includes('datetime')) {
        if (Number.isNaN(Date.parse(String(raw)))) errors[col.name] = 'Invalid date/time';
        continue;
      }
      if (t.includes('bool')) {
        if (typeof raw !== 'boolean' && raw !== 'true' && raw !== 'false' && raw !== 0 && raw !== 1)
          errors[col.name] = 'Use true/false';
      }
    }
    return errors;
  }

  function pkFromRow(meta: TableMeta, row: unknown[]): Record<string, unknown> {
    const pk: Record<string, unknown> = {};
    meta.columns.forEach((c, i) => {
      if (c.pk) pk[c.name] = row[i];
    });
    return pk;
  }

  function onCellCommit(rowIndex: number, columnName: string, newValue: unknown) {
    if (!activeTab || activeTab.kind !== 'table' || !activeMeta) return;
    const row = activeTab.result?.rows?.[rowIndex];
    if (!row) return;
    const original = rowToRecord(activeMeta.columns, row);
    const pk = pkFromRow(activeMeta, row);
    dbStore.stageCellEdit(connId, activeTab.id, rowIndex, columnName, newValue, original, pk);
  }

  function onInsertCellCommit(tempId: string, columnName: string, newValue: unknown) {
    if (!activeTab || activeTab.kind !== 'table') return;
    dbStore.updateInsertCell(connId, activeTab.id, tempId, columnName, newValue);
  }

  function onInsertDiscardRow(tempId: string) {
    if (!activeTab) return;
    dbStore.removeInsert(connId, activeTab.id, tempId);
  }

  async function discardPending() {
    const ok = await confirmDialog({
      title: 'Discard all pending changes?',
      description:
        'All pending cell edits and unsaved new rows across every tab in this connection will be dropped.',
      confirmLabel: 'Discard all',
      destructive: true
    });
    if (!ok) return;
    dbStore.clearPendingAll(connId);
    toast.success('Discarded pending changes');
  }

  /**
   * Apply pending changes: inserts FIRST (so freshly-typed rows don't fight
   * ordering with edits), then per-row PUT for cell edits. Validates inserts
   * up front and aborts cleanly if any have errors.
   */
  async function applyPending() {
    if (applyingPending) return;

    // ── Validate inserts up front; abort cleanly if any have errors.
    let firstErrorTabId: string | null = null;
    let anyErrors = false;
    for (const tabId in ws.pendingEdits) {
      const tab = ws.tabs.find((t) => t.id === tabId);
      if (!tab || tab.kind !== 'table' || !tab.tableMeta) continue;
      const bucket = ws.pendingEdits[tabId];
      for (const ins of bucket.inserts) {
        const errs = validateInsert(tab.tableMeta, ins.values);
        dbStore.setInsertErrors(connId, tabId, ins.tempId, errs);
        if (Object.keys(errs).length > 0) {
          anyErrors = true;
          if (!firstErrorTabId) firstErrorTabId = tabId;
        }
      }
    }
    if (anyErrors) {
      if (firstErrorTabId) dbStore.setActiveTab(connId, firstErrorTabId);
      toast.error('Some new rows need attention before apply');
      return;
    }

    applyingPending = true;
    let inserted = 0;
    let updated = 0;
    let failed = 0;
    const tabsTouched = new Set<string>();

    // Snapshot work units BEFORE mutating the buffer.
    const insertWork: Array<{
      tabId: string;
      tempId: string;
      table: string;
      schema?: string;
      database?: string;
      values: Record<string, unknown>;
    }> = [];
    const editWork: Array<{
      tabId: string;
      rowIndex: number;
      table: string;
      schema?: string;
      database?: string;
      pk: Record<string, unknown>;
      changes: Record<string, unknown>;
      original: Record<string, unknown>;
    }> = [];
    for (const tabId in ws.pendingEdits) {
      const tab = ws.tabs.find((t) => t.id === tabId);
      if (!tab || tab.kind !== 'table' || !tab.tableMeta) continue;
      const bucket = ws.pendingEdits[tabId];
      for (const ins of bucket.inserts) {
        // Strip undefined/'' so the backend can apply defaults / auto-inc PKs.
        const cleaned: Record<string, unknown> = {};
        for (const col of tab.tableMeta.columns) {
          const v = ins.values[col.name];
          if (v === undefined || v === '') continue;
          if (isAutoIncrement(col)) continue;
          cleaned[col.name] = v;
        }
        insertWork.push({
          tabId,
          tempId: ins.tempId,
          table: tab.tableMeta.name,
          schema: tab.schema,
          database: tab.database,
          values: cleaned
        });
      }
      for (const ri in bucket.edits) {
        editWork.push({
          tabId,
          rowIndex: Number(ri),
          table: tab.tableMeta.name,
          schema: tab.schema,
          database: tab.database,
          pk: bucket.edits[ri].pk,
          changes: bucket.edits[ri].changes,
          original: bucket.edits[ri].original
        });
      }
    }

    // Inserts first.
    for (const ins of insertWork) {
      try {
        await dbApi.rows.insert(connId, ins.table, {
          schema: ins.schema,
          database: ins.database,
          values: ins.values
        });
        dbStore.removeInsert(connId, ins.tabId, ins.tempId);
        inserted += 1;
        tabsTouched.add(ins.tabId);
      } catch (err) {
        failed += 1;
        // eslint-disable-next-line no-console
        console.error('apply insert failed', err);
      }
    }

    // Then edits.
    for (const ed of editWork) {
      try {
        await dbApi.rows.update(connId, ed.table, {
          schema: ed.schema,
          database: ed.database,
          pk: ed.pk,
          values: ed.changes
        });
        for (const col of Object.keys(ed.changes)) {
          dbStore.stageCellEdit(
            connId,
            ed.tabId,
            ed.rowIndex,
            col,
            ed.original[col],
            ed.original,
            ed.pk
          );
        }
        updated += 1;
        tabsTouched.add(ed.tabId);
      } catch (err) {
        failed += 1;
        // eslint-disable-next-line no-console
        console.error('apply edit failed', err);
      }
    }

    applyingPending = false;
    if (failed === 0) toast.success(`Inserted ${inserted}, updated ${updated}`);
    else toast.error(`Inserted ${inserted}, updated ${updated}, ${failed} failed`);
    for (const tabId of tabsTouched) await dbStore.runTab(connId, tabId);
  }

  async function closeTabSafe(tabId: string) {
    const bucket = ws.pendingEdits[tabId];
    let cellCount = 0;
    let insertCount = 0;
    if (bucket) {
      for (const ri in bucket.edits) cellCount += Object.keys(bucket.edits[ri].changes).length;
      insertCount = bucket.inserts.length;
    }
    if (cellCount + insertCount > 0) {
      const parts: string[] = [];
      if (cellCount > 0) parts.push(`${cellCount} pending edit${cellCount === 1 ? '' : 's'}`);
      if (insertCount > 0) parts.push(`${insertCount} new row${insertCount === 1 ? '' : 's'}`);
      const ok = await confirmDialog({
        title: 'Discard pending changes in this tab?',
        description: `${parts.join(' and ')} will be dropped when the tab closes.`,
        confirmLabel: 'Discard',
        destructive: true
      });
      if (!ok) return;
    }
    dbStore.closeTab(connId, tabId);
  }

  const activeSubTab = $derived<'data' | 'schema'>(
    activeTab?.kind === 'table' ? (ws.tableSubTab[activeTab.id] ?? 'data') : 'data'
  );
  function setSubTab(kind: 'data' | 'schema') {
    if (!activeTab || activeTab.kind !== 'table') return;
    dbStore.setTableSubTab(connId, activeTab.id, kind);
  }

  onMount(async () => {
    dbStore.setActiveConnection(connId);
    if (dbStore.connections.length === 0) await dbStore.refresh();
    const current = dbStore.workspace(connId);
    if (current.databases.length === 0) await dbStore.loadDatabases(connId);
    const next = dbStore.workspace(connId);
    if (!next.schema) await dbStore.loadSchema(connId, next.database ?? undefined);
  });

  function changeDatabase(db: string) {
    dbStore.setDatabase(connId, db);
  }

  function onOpenTable(schema: string, t: TableMeta) {
    const id = dbStore.openTableTab(connId, schema, t);
    void dbStore.runTab(connId, id);
  }

  function onNewSqlTab() {
    dbStore.openSqlTab(connId);
  }

  function switchConnection(id: string) {
    dbStore.setActiveConnection(id);
    goto(`/db/${id}`);
  }

  function closeTab(id: string) {
    void closeTabSafe(id);
  }

  async function runActive() {
    if (!activeTab) return;
    await dbStore.runTab(connId, activeTab.id);
  }

  async function cancelActive() {
    if (!activeTab) return;
    const result = await dbStore.cancelTab(connId, activeTab.id);
    if (result.ok) toast.success(result.message);
    else toast.error(result.message);
  }

  function setPageSize(n: number) {
    if (!activeTab) return;
    dbStore.patchTab(connId, activeTab.id, { pageSize: n, page: 0 });
    void dbStore.runTab(connId, activeTab.id);
  }

  function gotoPage(delta: number) {
    if (!activeTab) return;
    const next = Math.max(0, activeTab.page + delta);
    dbStore.patchTab(connId, activeTab.id, { page: next });
    void dbStore.runTab(connId, activeTab.id);
  }

  function rowToRecord(cols: Column[] | undefined, row: unknown[]): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    if (!cols) return out;
    cols.forEach((c, i) => (out[c.name] = row[i]));
    return out;
  }

  function rowToRecordWithPending(
    cols: Column[] | undefined,
    row: unknown[],
    rowIndex: number
  ): Record<string, unknown> {
    const out = rowToRecord(cols, row);
    if (!activeTab || rowIndex < 0) return out;
    const pending = ws.pendingEdits[activeTab.id]?.edits?.[rowIndex]?.changes ?? {};
    return { ...out, ...pending };
  }

  function queryResultForRow(row: unknown[]) {
    return {
      columns: activeTab?.result?.columns ?? [],
      rows: [row],
      elapsed_ms: activeTab?.result?.elapsed_ms ?? 0,
      truncated: false,
      query_id: activeTab?.result?.query_id ?? 'row-preview'
    };
  }

  function csvEscape(value: unknown): string {
    if (value === null || value === undefined) return '';
    const text = typeof value === 'object' ? JSON.stringify(value) : String(value);
    return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
  }

  function rowCsv(row: unknown[]): string {
    const headers = (activeTab?.result?.columns ?? []).map((c) => csvEscape(c.name)).join(',');
    const values = row.map(csvEscape).join(',');
    return `${headers}\n${values}\n`;
  }

  function sqlIdent(value: string): string {
    const q = conn?.engine === 'mysql' ? '`' : '"';
    return `${q}${value.replaceAll(q, `${q}${q}`)}${q}`;
  }

  function sqlLiteral(value: unknown): string {
    if (value === null || value === undefined) return 'NULL';
    if (typeof value === 'number') return Number.isFinite(value) ? String(value) : 'NULL';
    if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
    if (typeof value === 'object') return `'${JSON.stringify(value).replaceAll("'", "''")}'`;
    return `'${String(value).replaceAll("'", "''")}'`;
  }

  function whereForRow(record: Record<string, unknown>): string {
    const pk = activePkCols.length > 0 ? activePkCols : Object.keys(record).slice(0, 1);
    return pk.map((name) => `${sqlIdent(name)} = ${sqlLiteral(record[name])}`).join(' AND ');
  }

  function rowSql(kind: 'select' | 'insert' | 'update', row: unknown[]): string {
    if (!activeTab || !activeMeta) return '';
    const schema = activeTab.schema ?? '';
    const table = qualifiedTableName(conn?.engine, schema, activeMeta.name);
    const record = rowToRecordWithPending(activeMeta.columns, row, selectedRowIndex ?? -1);
    const entries = activeMeta.columns.map((c) => [c.name, record[c.name]] as const);
    if (kind === 'select') {
      return `SELECT *\nFROM ${table}\nWHERE ${whereForRow(record)};`;
    }
    if (kind === 'insert') {
      const writable = entries.filter(([name]) => !activePkCols.includes(name));
      const used = writable.length > 0 ? writable : entries;
      return `INSERT INTO ${table} (${used.map(([name]) => sqlIdent(name)).join(', ')})\nVALUES (${used
        .map(([, value]) => sqlLiteral(value))
        .join(', ')});`;
    }
    const writable = entries.filter(([name]) => !activePkCols.includes(name));
    return `UPDATE ${table}\nSET ${writable
      .map(([name, value]) => `${sqlIdent(name)} = ${sqlLiteral(value)}`)
      .join(',\n    ')}\nWHERE ${whereForRow(record)};`;
  }

  function openGeneratedRowSql(kind: 'select' | 'insert' | 'update', row: unknown[]) {
    const sql = rowSql(kind, row);
    if (!sql) return;
    const id = dbStore.openSqlTab(connId, sql);
    dbStore.patchTab(connId, id, { title: `${kind.toUpperCase()} row` });
  }

  function exportRow(row: unknown[], format: 'json' | 'csv' | 'markdown' | 'xlsx') {
    if (!activeTab) return;
    const base = safeName(`${activeTab.schema ?? 'schema'}.${activeTab.table ?? 'row'}-row`);
    const result = queryResultForRow(row);
    if (format === 'json') {
      const record = rowToRecordWithPending(activeMeta?.columns, row, selectedRowIndex ?? -1);
      triggerDownload(
        new Blob([`${JSON.stringify(record, null, 2)}\n`], {
          type: 'application/json;charset=utf-8'
        }),
        `${base}.json`
      );
    } else if (format === 'csv') {
      triggerDownload(new Blob([rowCsv(row)], { type: 'text/csv;charset=utf-8' }), `${base}.csv`);
    } else if (format === 'markdown') {
      triggerDownload(
        new Blob([resultToMarkdown(result)], { type: 'text/markdown;charset=utf-8' }),
        `${base}.md`
      );
    } else {
      triggerDownload(resultToXlsxBlob(result), `${base}.xlsx`);
    }
  }

  function selectRow(tabId: string, row: unknown[], rowIndex: number) {
    void row;
    selectedRowByTab = { ...selectedRowByTab, [tabId]: rowIndex };
  }

  function openRowContextMenu(row: unknown[], rowIndex: number, x: number, y: number) {
    if (!activeTab || !activeMeta) return;
    rowMenuTabId = activeTab.id;
    rowMenuIndex = rowIndex;
    rowMenuX = x;
    rowMenuY = y;
    rowMenuItems = [
      { label: 'Edit row', icon: Edit3, onSelect: () => onEditRow(row) },
      { label: 'Duplicate row', icon: Copy, onSelect: () => onDuplicateRow(row) },
      { label: 'Delete row', icon: Trash2, destructive: true, onSelect: () => void onDeleteRow(row) },
      { label: 'Export JSON', icon: FileJson, onSelect: () => exportRow(row, 'json') },
      { label: 'Export CSV', icon: Download, onSelect: () => exportRow(row, 'csv') },
      { label: 'Export Markdown', icon: FileText, onSelect: () => exportRow(row, 'markdown') },
      { label: 'Export Excel', icon: FileSpreadsheet, onSelect: () => exportRow(row, 'xlsx') },
      { label: 'Open SELECT query', icon: FileCode2, onSelect: () => openGeneratedRowSql('select', row) },
      { label: 'Open INSERT query', icon: FileCode2, onSelect: () => openGeneratedRowSql('insert', row) },
      { label: 'Open UPDATE query', icon: FileCode2, onSelect: () => openGeneratedRowSql('update', row) }
    ];
    rowMenuOpen = true;
  }

  function onEditRow(row: unknown[]) {
    if (!activeMeta) return;
    editorMode = 'update';
    editorInitial = rowToRecord(activeMeta.columns, row);
    editorOpen = true;
  }

  function onDuplicateRow(row: unknown[]) {
    if (!activeMeta) return;
    editorMode = 'duplicate';
    editorInitial = rowToRecord(activeMeta.columns, row);
    editorOpen = true;
  }

  async function onDeleteRow(row: unknown[]) {
    if (!activeMeta || !activeTab) return;
    const rec = rowToRecord(activeMeta.columns, row);
    const pk: Record<string, unknown> = {};
    for (const c of activeMeta.columns) if (c.pk) pk[c.name] = rec[c.name];
    const pkStr = Object.entries(pk)
      .map(([k, v]) => `${k}=${v}`)
      .join(', ');
    const ok = await confirmDialog({
      title: 'Delete row?',
      description: `Row matching ${pkStr} will be permanently deleted.`,
      confirmLabel: 'Delete row',
      destructive: true
    });
    if (!ok) return;
    try {
      await dbApi.rows.remove(connId, activeMeta.name, {
        schema: activeTab.schema,
        database: activeTab.database,
        pk
      });
      toast.success('Row deleted');
      await dbStore.runTab(connId, activeTab.id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Delete failed');
    }
  }

  /**
   * Default action: inline insert. Creates a new tempId-backed insert row at
   * the top of the grid, immediately ready for the user to type into.
   */
  function onInsertRow() {
    if (!activeMeta || !activeTab) return;
    dbStore.startInsert(connId, activeTab.id, {});
  }

  /** Secondary affordance — opens the full slide-out form (legacy path). */
  function onInsertFullForm() {
    if (!activeMeta) return;
    editorMode = 'insert';
    editorInitial = undefined;
    editorOpen = true;
  }

  function resetDbAgentPanel() {
    dbStore.resetAgent(connId);
  }

  async function stopDbAgentSession(sessionId: string) {
    try {
      await api.sessions.kill(sessionId);
    } catch (err) {
      console.warn('failed to kill DB agent session', err);
    }
  }

  async function startDbAgent() {
    if (startingDbAgent) return;
    if (dbAgent.sessionId) {
      const ok = await confirmDialog({
        title: 'Start a new DB agent?',
        description:
          'The current DB agent session will be closed before starting the selected agent.',
        confirmLabel: 'Start new agent',
        destructive: true
      });
      if (!ok) {
        dbStore.patchAgent(connId, { collapsed: false, fullscreen: false });
        return;
      }
      await stopDbAgentSession(dbAgent.sessionId);
      resetDbAgentPanel();
    }

    startingDbAgent = true;
    try {
      const res = await dbApi.startAgent(connId, {
        database: ws.database ?? undefined,
        kind: selectedDbAgentKind
      });
      dbStore.patchAgent(connId, {
        threadId: res.data.thread_id,
        sessionId: res.data.session_id,
        kind: selectedDbAgentKind,
        collapsed: false,
        fullscreen: false
      });
      toast.success('DB agent started in read-only mode');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start DB agent');
    } finally {
      startingDbAgent = false;
    }
  }

  async function closeDbAgentPanel() {
    if (!dbAgent.sessionId) {
      resetDbAgentPanel();
      return;
    }
    const sessionId = dbAgent.sessionId;
    const ok = await confirmDialog({
      title: 'Close DB agent session?',
      description: 'This will kill the DB agent PTY session and remove the panel.',
      confirmLabel: 'Close session',
      destructive: true
    });
    if (!ok) return;

    await stopDbAgentSession(sessionId);
    resetDbAgentPanel();
    toast.success('DB agent session closed');
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Subheader -->
  <header
    class="flex h-12 shrink-0 items-center justify-between gap-4 border-b px-5"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div class="flex items-center gap-3">
      <a
        href="/db"
        onclick={() => dbStore.setActiveConnection(null)}
        class="text-xs hover:underline"
        style="color: var(--fg-muted);"
        title="Back to connections"
      >
        ← Connections
      </a>
      <span style="color: var(--fg-muted);">/</span>
      <span class="font-semibold" style="color: var(--fg-default);">
        {conn?.name ?? connId}
      </span>
      {#if conn}
        <span
          class="rounded px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider text-white"
          style="background: var(--accent);"
        >
          {engineLabel(conn.engine)}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      <select
        value={selectedDbAgentKind}
        onchange={(e) => {
          selectedDbAgentKind = (e.currentTarget as HTMLSelectElement).value as SessionKind;
          dbStore.patchAgent(connId, { kind: selectedDbAgentKind });
        }}
        disabled={startingDbAgent || !conn}
        class="h-8 rounded-md border px-2 text-xs"
        style="border-color: var(--border-input); background: var(--surface-titlebar); color: var(--fg-default);"
        title="DB agent kind"
      >
        <option value="claude">Claude</option>
        <option value="codex">Codex</option>
        <option value="cursor">Cursor</option>
        <option value="antigravity">Antigravity</option>
        <option value="zeus">Zeus</option>
      </select>
      <Button
        variant="outline"
        size="sm"
        onclick={startDbAgent}
        disabled={startingDbAgent || !conn}
      >
        {#if startingDbAgent}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <Bot class="h-3.5 w-3.5" />
        {/if}
        DB Agent
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={() => dbStore.loadSchema(connId, ws.database ?? undefined)}
        disabled={ws.schemaLoading}
      >
        {#if ws.schemaLoading}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5" />
        {/if}
        Refresh schema
      </Button>
    </div>
  </header>

  <!-- Pending changes bar (amber stripe). Visible whenever there's anything pending. -->
  {#if pendingTotals.cells > 0 || pendingTotals.inserts > 0}
    <div
      class="flex h-9 shrink-0 items-center justify-between gap-3 border-b px-4 text-[11px]"
      style="background: color-mix(in srgb, var(--dot-warn) 14%, transparent); border-color: color-mix(in srgb, var(--dot-warn) 45%, transparent); color: var(--fg-default);"
    >
      <span class="inline-flex items-center gap-2">
        <span
          class="inline-block h-2 w-2 rounded-full"
          style="background: var(--dot-warn);"
          aria-hidden="true"
        ></span>
        <strong class="font-semibold">{pendingTotals.cells}</strong> pending change{pendingTotals.cells ===
        1
          ? ''
          : 's'} across
        <strong class="font-semibold">{pendingTotals.rows}</strong> row{pendingTotals.rows === 1
          ? ''
          : 's'} ·
        <strong class="font-semibold">{pendingTotals.inserts}</strong> new row{pendingTotals.inserts ===
        1
          ? ''
          : 's'}
        {#if pendingTotals.errors > 0}
          <span
            class="ml-2 inline-flex items-center gap-1 rounded px-1.5 py-0.5"
            style="background: color-mix(in srgb, var(--dot-danger) 18%, transparent); color: var(--dot-danger);"
          >
            <AlertTriangle class="h-3 w-3" />
            {pendingTotals.errors} field{pendingTotals.errors === 1 ? '' : 's'} need attention
          </span>
        {/if}
      </span>
      <span class="inline-flex items-center gap-2">
        <Button size="sm" variant="outline" onclick={discardPending} disabled={applyingPending}>
          Discard
        </Button>
        <Button
          size="sm"
          onclick={applyPending}
          disabled={applyingPending || pendingTotals.errors > 0}
        >
          {#if applyingPending}
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
          {/if}
          Apply
        </Button>
      </span>
    </div>
  {/if}

  <Resizable.PaneGroup direction="horizontal" class="min-h-0 flex-1">
    <Resizable.Pane
      defaultSize={dbAgent.threadId && dbAgent.sessionId && !dbAgent.collapsed
        ? 100 - dbAgentSize
        : 100}
      minSize={35}
    >
      <div class="flex h-full min-h-0">
        {#if dbStore.connectionsSidebarCollapsed}
      <aside
        class="flex w-10 shrink-0 flex-col items-center border-r py-2"
        style="background: var(--surface-window); border-color: var(--border-subtle);"
      >
        <button
          type="button"
          class="rounded p-1.5 hover:bg-[var(--accent-soft)]"
          title="Expand connections"
          aria-label="Expand connections"
          onclick={() => dbStore.setConnectionsSidebarCollapsed(false)}
        >
          <PanelLeftOpen class="h-4 w-4" />
        </button>
        <button
          type="button"
          class="mt-2 rounded p-1.5 hover:bg-[var(--accent-soft)]"
          title="New connection"
          aria-label="New connection"
          onclick={() => (connectionDialogOpen = true)}
        >
          <Plus class="h-4 w-4" />
        </button>
        <div class="mt-3 flex min-h-0 flex-1 flex-col items-center gap-1 overflow-auto">
          {#each dbStore.connections as item (item.id)}
            {@const selected = item.id === connId}
            {@const itemWs = dbStore.workspace(item.id)}
            <button
              type="button"
              class="relative rounded-md border p-1.5 transition-colors"
              style={selected
                ? 'background: var(--accent-soft); border-color: var(--accent); color: var(--accent);'
                : 'background: transparent; border-color: transparent; color: var(--fg-muted);'}
              onclick={() => switchConnection(item.id)}
              title={item.name}
              aria-label={item.name}
            >
              <Database class="h-4 w-4" />
              {#if itemWs.agent.sessionId}
                <span
                  class="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full"
                  style="background: var(--accent);"
                ></span>
              {/if}
            </button>
          {/each}
        </div>
      </aside>
    {:else}
      <aside
        class="flex w-56 shrink-0 flex-col border-r"
        style="background: var(--surface-window); border-color: var(--border-subtle);"
      >
        <div
          class="flex h-9 shrink-0 items-center justify-between border-b px-3"
          style="border-color: var(--border-subtle);"
        >
          <span class="h-eyebrow">Connections</span>
          <div class="flex items-center gap-1">
            <button
              type="button"
              class="rounded p-1 hover:bg-[var(--accent-soft)]"
              title="Collapse connections"
              aria-label="Collapse connections"
              onclick={() => dbStore.setConnectionsSidebarCollapsed(true)}
            >
              <PanelLeftClose class="h-4 w-4" />
            </button>
            <button
              type="button"
              class="rounded p-1 hover:bg-[var(--accent-soft)]"
              title="New connection"
              aria-label="New connection"
              onclick={() => (connectionDialogOpen = true)}
            >
              <Plus class="h-4 w-4" />
            </button>
          </div>
        </div>
        <div class="min-h-0 flex-1 overflow-auto p-2">
          {#if dbStore.connections.length === 0}
            <div class="px-2 py-6 text-center text-xs" style="color: var(--fg-muted);">
              No saved connections.
            </div>
          {:else}
            <div class="flex flex-col gap-1">
              {#each dbStore.connections as item (item.id)}
                {@const selected = item.id === connId}
                {@const itemWs = dbStore.workspace(item.id)}
                <button
                  type="button"
                  class="flex min-h-12 w-full flex-col items-start rounded-md border px-2.5 py-2 text-left transition-colors"
                  style={selected
                    ? 'background: var(--accent-soft); border-color: var(--accent); color: var(--accent);'
                    : 'background: transparent; border-color: transparent; color: var(--fg-default);'}
                  onclick={() => switchConnection(item.id)}
                  title={item.name}
                >
                  <span class="flex w-full items-center justify-between gap-2">
                    <span class="truncate text-xs font-semibold">{item.name}</span>
                    {#if itemWs.agent.sessionId}
                      <Bot class="h-3.5 w-3.5 shrink-0" />
                    {/if}
                  </span>
                  <span
                    class="mt-0.5 truncate font-mono text-[10px]"
                    style="color: var(--fg-muted);"
                  >
                    {item.engine}
                    {#if itemWs.tabs.length > 0}
                      · {itemWs.tabs.length} tab{itemWs.tabs.length === 1 ? '' : 's'}
                    {/if}
                  </span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </aside>
    {/if}

    {#if ws.schemaPanelCollapsed}
      <aside
        class="flex w-10 shrink-0 flex-col items-center border-r py-2"
        style="background: var(--surface-panel); border-color: var(--border-subtle);"
      >
        <button
          type="button"
          class="rounded p-1.5 hover:bg-[var(--accent-soft)]"
          title="Expand schema panel"
          aria-label="Expand schema panel"
          onclick={() => dbStore.setSchemaPanelCollapsed(connId, false)}
        >
          <PanelLeftOpen class="h-4 w-4" />
        </button>
      </aside>
    {:else}
      <!-- Sidebar -->
      <aside
        class="flex w-72 shrink-0 flex-col border-r"
        style="background: var(--surface-panel); border-color: var(--border-subtle);"
      >
        <div
          class="flex h-9 shrink-0 items-center justify-between border-b px-3"
          style="border-color: var(--border-subtle);"
        >
          <span class="h-eyebrow">Schema</span>
          <button
            type="button"
            class="rounded p-1 hover:bg-[var(--accent-soft)]"
            title="Collapse schema panel"
            aria-label="Collapse schema panel"
            onclick={() => dbStore.setSchemaPanelCollapsed(connId, true)}
          >
            <PanelLeftClose class="h-4 w-4" />
          </button>
        </div>

        <!-- Database selector -->
        <div class="px-3 pt-3">
          <div class="h-eyebrow mb-1.5">Database</div>
          <select
            value={ws.database ?? ''}
            onchange={(e) => changeDatabase((e.currentTarget as HTMLSelectElement).value)}
            disabled={ws.databases.length === 0 ||
              (conn?.engine === 'sqlite' && ws.databases.length <= 1)}
            class="h-8 w-full rounded-md border px-2 text-xs"
            style="border-color: var(--border-input); background: var(--surface-titlebar); color: var(--fg-default);"
          >
            {#if ws.databases.length === 0}
              <option value="">{conn?.database ?? '(default)'}</option>
            {:else}
              {#each ws.databases as d (d)}
                <option value={d}>{d}</option>
              {/each}
            {/if}
          </select>
        </div>

        <SchemaTree
          tree={ws.schema}
          loading={ws.schemaLoading}
          error={ws.schemaError}
          {onOpenTable}
          onExport={onSchemaTreeExport}
          onTableExport={onSchemaTreeTableExport}
          onGenerateQuery={onSchemaTreeGenerateQuery}
          activeTable={activeTab?.kind === 'table'
            ? { schema: activeTab.schema ?? '', name: activeTab.table ?? '' }
            : null}
        />
      </aside>
    {/if}

    <!-- Main -->
    <section class="flex min-w-0 flex-1 flex-col" style="background: var(--surface-canvas);">
      <!-- Tab bar -->
      <div
        class="flex h-10 shrink-0 items-center gap-2 border-b px-2"
        style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
      >
        <div class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
          {#each visibleTabs as t (t.id)}
            {@const active = t.id === ws.activeTabId}
            <div
              class="flex h-8 max-w-[150px] shrink-0 items-center gap-1.5 rounded-md border px-2.5 text-[12px]"
              style={active
                ? 'background: var(--surface-canvas); border-color: var(--border-subtle); color: var(--accent); font-weight: 600;'
                : 'background: transparent; border-color: transparent; color: var(--fg-muted);'}
            >
              <button
                type="button"
                onclick={() => dbStore.setActiveTab(connId, t.id)}
                class="min-w-0 truncate"
                title={t.title}
              >
                <span class="font-mono text-[11px]">{t.kind === 'sql' ? '⌥' : '⊞'}</span>
                <span class="ml-1.5 truncate">{t.title}</span>
              </button>
              <button
                type="button"
                onclick={() => closeTab(t.id)}
                class="shrink-0 rounded p-0.5 hover:bg-[var(--accent-soft)]"
                title="Close tab"
              >
                <X class="h-3 w-3" />
              </button>
            </div>
          {/each}
        </div>
        {#if shouldGroupTabs}
          <div class="flex shrink-0 items-center gap-1.5">
            <span class="text-[11px]" style="color: var(--fg-muted);">
              Open tabs ({ws.tabs.length})
            </span>
            <select
              value={ws.activeTabId ?? ''}
              onchange={(e) =>
                dbStore.setActiveTab(connId, (e.currentTarget as HTMLSelectElement).value)}
              class="h-7 max-w-[260px] rounded-md border px-2 text-[11px] outline-none"
              style="border-color: var(--border-input); background: var(--surface-window); color: var(--fg-default);"
              title="All open DB tabs"
            >
              {#each ws.tabs as t (t.id)}
                <option value={t.id}>
                  {t.kind === 'sql' ? 'Query' : 'Table'} · {t.title}
                </option>
              {/each}
            </select>
          </div>
        {/if}
        <button
          type="button"
          onclick={onNewSqlTab}
          class="inline-flex h-7 shrink-0 items-center gap-1 rounded-md border border-dashed px-2 text-[11px]"
          style="border-color: var(--border-input); color: var(--fg-muted);"
          title="New SQL tab"
        >
          <Plus class="h-3 w-3" /> New SQL
        </button>
      </div>

      <!-- Tab body -->
      {#if !activeTab}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 text-sm"
          style="color: var(--fg-muted);"
        >
          <p>No tab open. Pick a table in the sidebar or open a new SQL tab.</p>
          <Button size="sm" onclick={onNewSqlTab}>
            <Plus class="h-3.5 w-3.5" /> New SQL tab
          </Button>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col">
          <!-- Toolbar (hidden on table tabs while viewing Schema sub-tab) -->
          {#if !(activeTab.kind === 'table' && activeSubTab === 'schema')}
            <div
              class="flex h-11 shrink-0 items-center justify-between gap-3 border-b px-4"
              style="border-color: var(--border-subtle); background: var(--surface-window);"
            >
              <div class="flex items-center gap-2">
                <Button size="sm" onclick={runActive} disabled={activeTab.loading}>
                  {#if activeTab.loading}
                    <Loader2 class="h-3.5 w-3.5 animate-spin" />
                  {:else}
                    <Play class="h-3.5 w-3.5" />
                  {/if}
                  Run
                </Button>
                {#if activeTab.loading}
                  <Button size="sm" variant="outline" onclick={cancelActive}>
                    <X class="h-3.5 w-3.5" />
                    Cancel
                  </Button>
                {/if}
                {#if activeTab.kind === 'table'}
                  <div class="inline-flex items-stretch">
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={onInsertRow}
                      class="rounded-r-none border-r-0"
                      title="Insert a new row inline (typeable at the top of the grid)"
                    >
                      <Plus class="h-3.5 w-3.5" /> Insert row
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={onInsertFullForm}
                      class="rounded-l-none px-2"
                      title="Use full form (slide-out panel)"
                      aria-label="Use full form"
                    >
                      <MoreHorizontal class="h-3.5 w-3.5" />
                    </Button>
                  </div>
                {/if}
                <label
                  class="ml-2 inline-flex items-center gap-1.5 text-[11px]"
                  style="color: var(--fg-muted);"
                >
                  Page size
                  <select
                    value={activeTab.pageSize}
                    onchange={(e) =>
                      setPageSize(Number((e.currentTarget as HTMLSelectElement).value))}
                    class="h-7 rounded border px-1.5 text-[11px]"
                    style="border-color: var(--border-input); background: var(--surface-titlebar);"
                  >
                    {#each [50, 100, 200, 500, 1000] as n (n)}
                      <option value={n}>{n}</option>
                    {/each}
                  </select>
                </label>
              </div>

              <div
                class="flex items-center gap-2 font-mono text-[11px]"
                style="color: var(--fg-muted);"
              >
                <button
                  type="button"
                  class="rounded p-1 hover:bg-[var(--accent-soft)]"
                  onclick={() => gotoPage(-1)}
                  disabled={activeTab.page === 0}
                >
                  <ChevronLeft class="h-3.5 w-3.5" />
                </button>
                <span>page {activeTab.page + 1}</span>
                <button
                  type="button"
                  class="rounded p-1 hover:bg-[var(--accent-soft)]"
                  onclick={() => gotoPage(1)}
                >
                  <ChevronRight class="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          {/if}

          <!-- Data / Schema sub-tabs (table tabs only) -->
          {#if activeTab.kind === 'table'}
            <div
              class="flex h-9 shrink-0 items-center gap-1 border-b px-3"
              style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
              role="tablist"
              aria-label="Table view"
            >
              {#each [{ k: 'data', label: 'Data' }, { k: 'schema', label: 'Schema' }] as t (t.k)}
                {@const sel = activeSubTab === t.k}
                <button
                  type="button"
                  role="tab"
                  aria-selected={sel}
                  onclick={() => setSubTab(t.k as 'data' | 'schema')}
                  class="h-7 rounded-md px-3 text-[11px] font-medium transition-colors"
                  style={sel
                    ? 'background: var(--surface-canvas); color: var(--accent); border: 1px solid var(--border-subtle);'
                    : 'background: transparent; color: var(--fg-muted); border: 1px solid transparent;'}
                >
                  {t.label}
                </button>
              {/each}
            </div>
          {/if}

          <!-- Editor / table -->
          {#if activeTab.kind === 'sql'}
            <div class="flex min-h-0 flex-1 flex-col">
              <!-- Pin toggle (Q13) — visual lock when the tab is leased to a
                   dedicated connection. Backend auto-pins on BEGIN; this
                   toggle lets the user pin manually for session state. -->
              <div
                class="flex shrink-0 items-center justify-end gap-2 border-b px-2 py-1"
                style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
              >
                <button
                  type="button"
                  onclick={() =>
                    activeTab.pinned
                      ? dbStore.unpinTab(connId, activeTab.id)
                      : dbStore.pinTab(connId, activeTab.id)}
                  class="inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors"
                  style={activeTab.pinned
                    ? 'border-color: var(--accent); background: var(--accent-soft); color: var(--accent);'
                    : 'border-color: var(--border-input); background: transparent; color: var(--fg-muted);'}
                  title={activeTab.pinned
                    ? 'Tab is pinned to a dedicated DB connection (transactions / session state). Click to release.'
                    : 'Pin this tab to a dedicated DB connection. Auto-enabled on BEGIN.'}
                >
                  {activeTab.pinned ? '🔒 pinned' : '🔓 shared'}
                </button>
              </div>
              <div class="h-2/5 min-h-[140px] border-b" style="border-color: var(--border-subtle);">
                <SqlEditor
                  value={activeTab.sql ?? ''}
                  onChange={(v) => dbStore.patchTab(connId, activeTab.id, { sql: v })}
                  onRun={runActive}
                />
              </div>
              <div class="min-h-0 flex-1">
                {#if activeTab.error}
                  <div
                    class="m-4 rounded-md border px-3 py-2 text-xs"
                    style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
                  >
                    {activeTab.error}
                  </div>
                {:else}
                  <ResultGrid result={activeTab.result} />
                {/if}
              </div>
            </div>
          {:else if activeSubTab === 'schema' && activeMeta}
            <div class="min-h-0 flex-1">
              <TableSchemaView table={activeMeta} />
            </div>
          {:else}
            <div class="min-h-0 flex-1">
              {#if activeTab.error}
                <div
                  class="m-4 rounded-md border px-3 py-2 text-xs"
                  style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
                >
                  {activeTab.error}
                </div>
              {:else}
                <ResultGrid
                  result={activeTab.result}
                  pkColumns={activePkCols}
                  editable={true}
                  pendingByRow={pendingForActiveTab}
                  pendingInserts={pendingInsertsForActiveTab}
                  columnsMeta={activeMeta?.columns}
                  {isAutoIncrement}
                  onEdit={(row) => onEditRow(row)}
                  onDuplicate={(row) => onDuplicateRow(row)}
                  onDelete={(row) => onDeleteRow(row)}
                  {onCellCommit}
                  {onInsertCellCommit}
                  {onInsertDiscardRow}
                />
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </section>
      </div>
    </Resizable.Pane>

    {#if dbAgent.threadId && dbAgent.sessionId}
      <Resizable.Handle withHandle disabled={dbAgent.collapsed} />
      <Resizable.Pane
        defaultSize={dbAgent.collapsed ? 4 : dbAgentSize}
        minSize={dbAgent.collapsed ? 4 : 24}
        maxSize={dbAgent.collapsed ? 4 : 60}
        onResize={(size) => {
          if (!dbAgent.collapsed) dbStore.setAgentSize(connId, size);
        }}
      >
      {#if dbAgent.collapsed}
        <aside
          class="flex h-full w-full flex-col items-center border-l py-2"
          style="background: var(--surface-panel); border-color: var(--border-subtle);"
        >
          <button
            type="button"
            class="rounded p-1.5 hover:bg-[var(--accent-soft)]"
            title="Expand DB agent"
            aria-label="Expand DB agent"
            onclick={() => dbStore.patchAgent(connId, { collapsed: false })}
          >
            <PanelRightOpen class="h-4 w-4" />
          </button>
          <Bot class="mt-3 h-4 w-4" style="color: var(--accent);" />
        </aside>
      {:else}
        <aside
          class="flex h-full w-full min-w-0 flex-col border-l"
          style="background: var(--surface-panel); border-color: var(--border-subtle);"
        >
          <div
            class="flex h-10 shrink-0 items-center justify-between gap-2 border-b px-3"
            style="border-color: var(--border-subtle);"
          >
            <div class="flex min-w-0 items-center gap-2">
              <Bot class="h-4 w-4 shrink-0" style="color: var(--accent);" />
              <span class="truncate text-xs font-semibold" style="color: var(--fg-default);">
                DB Agent
              </span>
              <span class="font-mono text-[10px]" style="color: var(--fg-muted);">
                {dbAgent.sessionId.slice(0, 8)}
              </span>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                title="Make DB agent wider"
                aria-label="Make DB agent wider"
                onclick={() => resizeDbAgent(80)}
              >
                <ChevronLeft class="h-4 w-4" />
              </button>
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                title="Make DB agent narrower"
                aria-label="Make DB agent narrower"
                onclick={() => resizeDbAgent(-80)}
              >
                <ChevronRight class="h-4 w-4" />
              </button>
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                title="Collapse DB agent"
                aria-label="Collapse DB agent"
                onclick={() => dbStore.patchAgent(connId, { collapsed: true })}
              >
                <PanelRightClose class="h-4 w-4" />
              </button>
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                title="Fullscreen"
                aria-label="Fullscreen DB agent"
                onclick={() => dbStore.patchAgent(connId, { fullscreen: true })}
              >
                <Maximize2 class="h-4 w-4" />
              </button>
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                title="Close DB agent panel"
                aria-label="Close DB agent panel"
                onclick={closeDbAgentPanel}
              >
                <X class="h-4 w-4" />
              </button>
            </div>
          </div>
          <div class="min-h-0 flex-1">
            {#if dbAgent.fullscreen}
              <div
                class="flex h-full items-center justify-center px-4 text-xs"
                style="color: var(--fg-muted);"
              >
                DB agent is open in fullscreen.
              </div>
            {:else}
              <TerminalView threadId={dbAgent.threadId} sessionId={dbAgent.sessionId} embedded />
            {/if}
          </div>
        </aside>
      {/if}
      </Resizable.Pane>
    {/if}
  </Resizable.PaneGroup>

    <ConnectionFormDialog
      bind:open={connectionDialogOpen}
      existing={null}
      onSaved={async () => {
        await dbStore.refresh();
      }}
    />

    <ExportDialog
      bind:open={exportOpen}
      connectionId={connId}
      database={ws.database}
      engine={conn?.engine}
      target={exportTarget}
    />

    {#if activeMeta}
      <RowEditorPanel
        bind:open={editorOpen}
        mode={editorMode}
        connectionId={connId}
        schema={activeTab?.schema}
        database={activeTab?.database}
        table={activeMeta}
        initialValues={editorInitial}
        onSaved={() => activeTab && dbStore.runTab(connId, activeTab.id)}
      />
    {/if}

  {#if dbAgent.fullscreen && dbAgent.threadId && dbAgent.sessionId}
    <div class="fixed inset-0 z-50 flex flex-col" style="background: var(--surface-window);">
      <div
        class="flex h-11 shrink-0 items-center justify-between border-b px-4"
        style="border-color: var(--border-subtle); background: var(--surface-panel);"
      >
        <div class="flex min-w-0 items-center gap-2">
          <Bot class="h-4 w-4 shrink-0" style="color: var(--accent);" />
          <span class="truncate text-sm font-semibold" style="color: var(--fg-default);">
            DB Agent
          </span>
          <span class="font-mono text-[11px]" style="color: var(--fg-muted);">
            {dbAgent.sessionId.slice(0, 8)}
          </span>
        </div>
        <div class="flex items-center gap-1">
          <Button
            variant="outline"
            size="sm"
            onclick={() => dbStore.patchAgent(connId, { fullscreen: false })}
          >
            <Minimize2 class="h-3.5 w-3.5" />
            Exit fullscreen
          </Button>
          <Button variant="outline" size="sm" onclick={closeDbAgentPanel}>
            <X class="h-3.5 w-3.5" />
            Close
          </Button>
        </div>
      </div>
      <div class="min-h-0 flex-1">
        <TerminalView threadId={dbAgent.threadId} sessionId={dbAgent.sessionId} embedded />
      </div>
    </div>
  {/if}
</div>
