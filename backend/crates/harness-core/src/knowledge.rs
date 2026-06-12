//! Knowledge ingestion helpers for turning large technical documents into
//! compact, indexable Markdown shards under a profile.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use calamine::{open_workbook_auto, Data, Reader};
use chrono::{DateTime, Utc};
use pdf_oxide::converters::ConversionOptions;
use pdf_oxide::PdfDocument;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use undoc::render::{CleanupOptions, RenderOptions, SectionMarkerStyle};

use crate::Error;

const MAX_SHARD_CHARS: usize = 7_500;
const MIN_SHARD_CHARS: usize = 2_000;
const KNOWLEDGE_INDEX_SCHEMA_VERSION: i32 = 1;
const DATA_SAMPLE_ROWS: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeIngestRequest {
    pub source_path: PathBuf,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIngestResult {
    pub title: String,
    pub slug: String,
    pub output_dir: PathBuf,
    pub index_path: PathBuf,
    pub shard_count: usize,
    pub shards: Vec<KnowledgeShard>,
    pub source_path: PathBuf,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeShard {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub pages: Vec<usize>,
    pub headings: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeSearchHit {
    pub source: String,
    pub heading: String,
    pub snippet: String,
    pub score: f64,
    pub path: PathBuf,
    pub shard_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
struct TextShard {
    title: String,
    pages: Vec<usize>,
    headings: Vec<String>,
    content: String,
}

#[derive(Debug, Deserialize)]
struct PersistedKnowledgeMetadata {
    title: String,
    source_path: PathBuf,
    shards: Vec<KnowledgeShard>,
}

pub fn ingest_pdf(
    harness_home: &Path,
    profile: &str,
    req: KnowledgeIngestRequest,
) -> Result<KnowledgeIngestResult, Error> {
    validate_pdf_path(&req.source_path)?;
    let text = extract_pdf_text(&req.source_path)?;
    ingest_text_with_kind(
        harness_home,
        profile,
        req.source_path,
        req.title,
        "pdf",
        &text,
    )
}

pub fn ingest_office(
    harness_home: &Path,
    profile: &str,
    req: KnowledgeIngestRequest,
) -> Result<KnowledgeIngestResult, Error> {
    validate_office_path(&req.source_path)?;
    let text = extract_office_markdown(&req.source_path)?;
    ingest_text_with_kind(
        harness_home,
        profile,
        req.source_path,
        req.title,
        "office",
        &text,
    )
}

pub fn ingest_data(
    harness_home: &Path,
    profile: &str,
    req: KnowledgeIngestRequest,
) -> Result<KnowledgeIngestResult, Error> {
    validate_data_path(&req.source_path)?;
    let text = extract_data_markdown(&req.source_path)?;
    ingest_text_with_kind(
        harness_home,
        profile,
        req.source_path,
        req.title,
        "data",
        &text,
    )
}

pub fn ingest_text(
    harness_home: &Path,
    profile: &str,
    source_path: PathBuf,
    title: Option<String>,
    text: &str,
) -> Result<KnowledgeIngestResult, Error> {
    ingest_text_with_kind(harness_home, profile, source_path, title, "pdf", text)
}

fn ingest_text_with_kind(
    harness_home: &Path,
    profile: &str,
    source_path: PathBuf,
    title: Option<String>,
    kind: &str,
    text: &str,
) -> Result<KnowledgeIngestResult, Error> {
    let title = title
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| title_from_path(&source_path));
    let slug = sanitize_segment(&title);
    let root = harness_home
        .join("profiles")
        .join(sanitize_segment(profile))
        .join("knowledge")
        .join(sanitize_segment(kind));
    std::fs::create_dir_all(&root)?;
    let output_dir = unique_dir(&root, &slug)?;
    let shards_dir = output_dir.join("shards");
    std::fs::create_dir_all(&shards_dir)?;

    let generated_at = Utc::now();
    let normalized = normalize_text(text);
    if normalized.trim().is_empty() {
        return Err(Error::Validation(format!(
            "{kind} extraction produced no readable text"
        )));
    }
    let text_shards = shard_text(&normalized);
    let mut shards = Vec::with_capacity(text_shards.len());
    for (idx, shard) in text_shards.iter().enumerate() {
        let id = format!("{:03}", idx + 1);
        let filename = format!("{id}-{}.md", sanitize_segment(&shard.title));
        let path = shards_dir.join(filename);
        let content = render_shard(&title, &id, shard);
        write_file(&path, &content)?;
        shards.push(KnowledgeShard {
            id,
            title: shard.title.clone(),
            path: path.clone(),
            pages: shard.pages.clone(),
            headings: shard.headings.clone(),
            bytes: content.len() as u64,
        });
    }

    let index_path = output_dir.join("index.md");
    write_file(
        &index_path,
        &render_index(&title, &source_path, generated_at, &shards),
    )?;
    write_file(
        &output_dir.join("metadata.json"),
        &serde_json::to_string_pretty(&serde_json::json!({
            "title": title,
            "slug": slug,
            "source_path": source_path,
            "generated_at": generated_at,
            "shard_count": shards.len(),
            "shards": shards,
        }))
        .map_err(|e| Error::Other(e.into()))?,
    )?;

    index_ingested_shards(harness_home, profile, kind, &title, &source_path, &shards)?;

    Ok(KnowledgeIngestResult {
        title,
        slug,
        output_dir,
        index_path,
        shard_count: shards.len(),
        shards,
        source_path,
        generated_at,
    })
}

pub fn knowledge_search(
    harness_home: &Path,
    profile: &str,
    query: &str,
    limit: Option<usize>,
) -> Result<Vec<KnowledgeSearchHit>, Error> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::Validation(
            "knowledge_search query must not be empty".to_string(),
        ));
    }
    let root = knowledge_root(harness_home, profile);
    ensure_knowledge_index(harness_home, profile)?;
    let conn = open_knowledge_connection(root.join("index.sqlite"))?;
    let fts_query = fts_phrase_query(query);
    let limit = limit.unwrap_or(5).clamp(1, 50) as i64;
    let mut stmt = conn.prepare(
        "SELECT source, heading, snippet(knowledge_fts, 2, '[', ']', '...', 24), \
         bm25(knowledge_fts), path, shard_id, title \
         FROM knowledge_fts WHERE knowledge_fts MATCH ?1 \
         ORDER BY bm25(knowledge_fts) LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit], |row| {
        Ok(KnowledgeSearchHit {
            source: row.get(0)?,
            heading: row.get(1)?,
            snippet: row.get(2)?,
            score: row.get(3)?,
            path: PathBuf::from(row.get::<_, String>(4)?),
            shard_id: row.get(5)?,
            title: row.get(6)?,
        })
    })?;
    let mut hits = Vec::new();
    for row in rows {
        hits.push(row?);
    }
    Ok(hits)
}

fn validate_pdf_path(path: &Path) -> Result<(), Error> {
    if path.extension().and_then(OsStr::to_str) != Some("pdf") {
        return Err(Error::Validation(
            "source_path must end with .pdf".to_string(),
        ));
    }
    if !path.is_file() {
        return Err(Error::NotFound(format!("pdf {}", path.display())));
    }
    Ok(())
}

fn validate_office_path(path: &Path) -> Result<(), Error> {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("docx" | "pptx")) {
        return Err(Error::Validation(
            "source_path must end with .docx or .pptx".to_string(),
        ));
    }
    if !path.is_file() {
        return Err(Error::NotFound(format!(
            "office document {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_data_path(path: &Path) -> Result<(), Error> {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    if !matches!(
        ext.as_deref(),
        Some("csv" | "tsv" | "xlsx" | "xlsm" | "xlsb" | "xls")
    ) {
        return Err(Error::Validation(
            "source_path must end with .csv, .tsv, .xlsx, .xlsm, .xlsb, or .xls".to_string(),
        ));
    }
    if !path.is_file() {
        return Err(Error::NotFound(format!("data file {}", path.display())));
    }
    Ok(())
}

fn extract_data_markdown(path: &Path) -> Result<String, Error> {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("csv" | "tsv") => render_csv_markdown(path),
        Some("xlsx" | "xlsm" | "xlsb" | "xls") => render_xlsx_markdown(path),
        _ => Err(Error::Validation("unsupported data file extension".into())),
    }
}

fn extract_pdf_text(path: &Path) -> Result<String, Error> {
    let doc = PdfDocument::open(path)
        .map_err(|e| Error::Validation(format!("failed to open PDF: {e}")))?;
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Validation(format!("failed to read PDF page count: {e}")))?;
    let options = ConversionOptions {
        detect_headings: true,
        ..Default::default()
    };
    let mut pages = Vec::with_capacity(page_count);
    for page in 0..page_count {
        let markdown = doc.to_markdown(page, &options).map_err(|e| {
            Error::Validation(format!("failed to extract PDF page {}: {e}", page + 1))
        })?;
        pages.push(markdown);
    }
    Ok(pages.join("\u{c}"))
}

fn extract_office_markdown(path: &Path) -> Result<String, Error> {
    let options = RenderOptions::default()
        .with_cleanup_options(CleanupOptions::standard())
        .with_max_heading(4);
    let mut options = options;
    options.section_markers = SectionMarkerStyle::Comment;
    undoc::to_markdown_with_options(path, &options)
        .map_err(|e| Error::Validation(format!("failed to extract Office document: {e}")))
}

fn render_csv_markdown(path: &Path) -> Result<String, Error> {
    let delimiter = if path.extension().and_then(OsStr::to_str) == Some("tsv") {
        b'\t'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_path(path)
        .map_err(|e| Error::Validation(format!("failed to open CSV: {e}")))?;
    let headers = reader
        .headers()
        .map_err(|e| Error::Validation(format!("failed to read CSV headers: {e}")))?
        .iter()
        .enumerate()
        .map(|(idx, header)| normalize_header_with_fallback(header, idx))
        .collect::<Vec<_>>();
    let mut samples = Vec::new();
    let mut type_samples = vec![Vec::new(); headers.len()];
    let mut row_count = 0usize;
    for record in reader.records() {
        let record =
            record.map_err(|e| Error::Validation(format!("failed to read CSV row: {e}")))?;
        row_count += 1;
        let row = headers
            .iter()
            .enumerate()
            .map(|(idx, _)| record.get(idx).unwrap_or_default().trim().to_string())
            .collect::<Vec<_>>();
        for (idx, value) in row.iter().enumerate() {
            if !value.is_empty() && type_samples[idx].len() < DATA_SAMPLE_ROWS {
                type_samples[idx].push(value.clone());
            }
        }
        if samples.len() < DATA_SAMPLE_ROWS {
            samples.push(row);
        }
    }
    Ok(render_data_sheet_markdown(
        &title_from_path(path),
        "Sheet1",
        &headers,
        &type_samples,
        &samples,
        Some(row_count),
    ))
}

fn render_xlsx_markdown(path: &Path) -> Result<String, Error> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| Error::Validation(format!("failed to open workbook: {e}")))?;
    let mut out = String::new();
    let title = title_from_path(path);
    out.push_str(&format!("# {title}\n\n"));
    for sheet_name in workbook.sheet_names().to_vec() {
        let Ok(range) = workbook.worksheet_range(&sheet_name) else {
            continue;
        };
        let mut rows = range.rows();
        let Some(header_row) = rows.next() else {
            continue;
        };
        let headers = header_row
            .iter()
            .enumerate()
            .map(|(idx, cell)| normalize_header_with_fallback(&xlsx_cell_to_string(cell), idx))
            .collect::<Vec<_>>();
        let mut samples = Vec::new();
        let mut type_samples = vec![Vec::new(); headers.len()];
        let mut row_count = 0usize;
        for row_cells in rows {
            row_count += 1;
            let row = headers
                .iter()
                .enumerate()
                .map(|(idx, _)| {
                    row_cells
                        .get(idx)
                        .map(xlsx_cell_to_string)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            for (idx, value) in row.iter().enumerate() {
                if !value.is_empty() && type_samples[idx].len() < DATA_SAMPLE_ROWS {
                    type_samples[idx].push(value.clone());
                }
            }
            if samples.len() < DATA_SAMPLE_ROWS {
                samples.push(row);
            }
        }
        out.push_str(&render_data_sheet_markdown(
            &title,
            &sheet_name,
            &headers,
            &type_samples,
            &samples,
            Some(row_count),
        ));
        out.push('\n');
    }
    if out.trim() == format!("# {title}") {
        return Err(Error::Validation(
            "workbook contained no readable sheets".to_string(),
        ));
    }
    Ok(out)
}

fn render_data_sheet_markdown(
    title: &str,
    sheet_name: &str,
    headers: &[String],
    type_samples: &[Vec<String>],
    samples: &[Vec<String>],
    row_count: Option<usize>,
) -> String {
    let mut out = String::new();
    if sheet_name == "Sheet1" {
        out.push_str(&format!("# {title}\n\n"));
    }
    out.push_str(&format!("## Sheet: {sheet_name}\n\n"));
    if let Some(count) = row_count {
        out.push_str(&format!("- Rows: `{count}`\n"));
    }
    out.push_str(&format!("- Columns: `{}`\n\n", headers.len()));
    out.push_str("### Column schema\n\n");
    out.push_str("| Column | Inferred type | Sample values |\n");
    out.push_str("|---|---|---|\n");
    for (idx, header) in headers.iter().enumerate() {
        let values = type_samples.get(idx).cloned().unwrap_or_default();
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            markdown_cell(header),
            infer_data_type(&values),
            markdown_cell(&values.into_iter().take(3).collect::<Vec<_>>().join(", "))
        ));
    }
    out.push_str("\n### Sample rows\n\n");
    if headers.is_empty() {
        out.push_str("No columns detected.\n");
        return out;
    }
    out.push('|');
    for header in headers {
        out.push_str(&format!(" {} |", markdown_cell(header)));
    }
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str("---|");
    }
    out.push('\n');
    for row in samples {
        out.push('|');
        for idx in 0..headers.len() {
            out.push_str(&format!(
                " {} |",
                markdown_cell(row.get(idx).map(String::as_str).unwrap_or_default())
            ));
        }
        out.push('\n');
    }
    out
}

fn normalize_header_with_fallback(raw: &str, idx: usize) -> String {
    let header = raw.trim_start_matches('\u{feff}').trim();
    if header.is_empty() {
        format!("column_{}", idx + 1)
    } else {
        header.to_string()
    }
}

fn xlsx_cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) => value.trim().to_string(),
        Data::Float(value) => {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                value.to_string()
            }
        }
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) | Data::DurationIso(value) => value.clone(),
        Data::Error(value) => format!("{value:?}"),
    }
}

fn infer_data_type(values: &[String]) -> &'static str {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return "empty";
    }
    if values
        .iter()
        .all(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "false"))
    {
        return "boolean";
    }
    if values.iter().all(|value| value.parse::<i64>().is_ok()) {
        return "integer";
    }
    if values.iter().all(|value| value.parse::<f64>().is_ok()) {
        return "number";
    }
    "text"
}

fn knowledge_root(harness_home: &Path, profile: &str) -> PathBuf {
    harness_home
        .join("profiles")
        .join(sanitize_segment(profile))
        .join("knowledge")
}

fn ensure_knowledge_index(harness_home: &Path, profile: &str) -> Result<(), Error> {
    let root = knowledge_root(harness_home, profile);
    std::fs::create_dir_all(&root)?;
    let mut conn = open_knowledge_connection(root.join("index.sqlite"))?;
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let table_count: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_fts'",
        [],
        |row| row.get(0),
    )?;
    if version != KNOWLEDGE_INDEX_SCHEMA_VERSION || table_count == 0 {
        rebuild_knowledge_index(&mut conn, &root)?;
    }
    Ok(())
}

fn rebuild_knowledge_index(conn: &mut Connection, root: &Path) -> Result<(), Error> {
    create_knowledge_index_schema(conn)?;
    for kind_entry in std::fs::read_dir(root)? {
        let kind_entry = kind_entry?;
        let kind_path = kind_entry.path();
        if !kind_path.is_dir() {
            continue;
        }
        let kind = kind_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("knowledge");
        for doc_entry in std::fs::read_dir(&kind_path)? {
            let doc_path = doc_entry?.path();
            if !doc_path.is_dir() {
                continue;
            }
            let metadata_path = doc_path.join("metadata.json");
            if !metadata_path.is_file() {
                continue;
            }
            let raw = match std::fs::read_to_string(&metadata_path) {
                Ok(raw) => raw,
                Err(e) => {
                    tracing::warn!(
                        path = %metadata_path.display(),
                        error = %e,
                        "skipping unreadable knowledge metadata"
                    );
                    continue;
                }
            };
            let metadata: PersistedKnowledgeMetadata = match serde_json::from_str(&raw) {
                Ok(metadata) => metadata,
                Err(e) => {
                    tracing::warn!(
                        path = %metadata_path.display(),
                        error = %e,
                        "skipping invalid knowledge metadata"
                    );
                    continue;
                }
            };
            if let Err(e) = insert_shards_into_index(
                conn,
                kind,
                &metadata.title,
                &metadata.source_path,
                &metadata.shards,
            ) {
                tracing::warn!(
                    path = %metadata_path.display(),
                    error = %e,
                    "skipping knowledge document during index rebuild"
                );
            }
        }
    }
    Ok(())
}

fn create_knowledge_index_schema(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS knowledge_fts;
         CREATE VIRTUAL TABLE knowledge_fts USING fts5(
            title,
            heading,
            content,
            source,
            path UNINDEXED,
            kind UNINDEXED,
            shard_id UNINDEXED,
            tokenize = 'unicode61'
         );
         PRAGMA user_version = 1;",
    )?;
    Ok(())
}

fn index_ingested_shards(
    harness_home: &Path,
    profile: &str,
    kind: &str,
    title: &str,
    source_path: &Path,
    shards: &[KnowledgeShard],
) -> Result<(), Error> {
    ensure_knowledge_index(harness_home, profile)?;
    let mut conn =
        open_knowledge_connection(knowledge_root(harness_home, profile).join("index.sqlite"))?;
    insert_shards_into_index(&mut conn, kind, title, source_path, shards)
}

fn insert_shards_into_index(
    conn: &mut Connection,
    kind: &str,
    title: &str,
    source_path: &Path,
    shards: &[KnowledgeShard],
) -> Result<(), Error> {
    let tx = conn.transaction()?;
    let source = source_path.display().to_string();
    tx.execute(
        "DELETE FROM knowledge_fts WHERE source = ?1",
        params![source.as_str()],
    )?;
    let mut stmt = tx.prepare(
        "INSERT INTO knowledge_fts(title, heading, content, source, path, kind, shard_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for shard in shards {
        let content = std::fs::read_to_string(&shard.path)?;
        let heading = shard
            .headings
            .first()
            .cloned()
            .unwrap_or_else(|| shard.title.clone());
        stmt.execute(params![
            title,
            heading,
            content,
            source.as_str(),
            shard.path.display().to_string(),
            kind,
            shard.id
        ])?;
    }
    drop(stmt);
    tx.commit()?;
    Ok(())
}

fn open_knowledge_connection(path: PathBuf) -> Result<Connection, Error> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(1000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

fn fts_phrase_query(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .split("\n\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn shard_text(text: &str) -> Vec<TextShard> {
    let pages: Vec<&str> = text.split('\u{c}').collect();
    let mut shards = Vec::new();
    let mut current = String::new();
    let mut current_pages = Vec::new();
    let mut current_title = String::new();

    for (page_idx, page) in pages.iter().enumerate() {
        let page_number = page_idx + 1;
        for block in page.split("\n\n").map(str::trim).filter(|b| !b.is_empty()) {
            if current_title.is_empty() {
                current_title =
                    infer_title(block).unwrap_or_else(|| format!("Pages {page_number}"));
            }
            if current.len() >= MIN_SHARD_CHARS && current.len() + block.len() + 2 > MAX_SHARD_CHARS
            {
                shards.push(TextShard {
                    title: current_title.clone(),
                    pages: current_pages.clone(),
                    headings: extract_headings(&current),
                    content: current.trim().to_string(),
                });
                current.clear();
                current_pages.clear();
                current_title =
                    infer_title(block).unwrap_or_else(|| format!("Pages {page_number}"));
            }
            if !current_pages.contains(&page_number) {
                current_pages.push(page_number);
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(block);
        }
    }

    if !current.trim().is_empty() {
        shards.push(TextShard {
            title: current_title,
            pages: current_pages,
            headings: extract_headings(&current),
            content: current.trim().to_string(),
        });
    }
    shards
}

fn extract_headings(content: &str) -> Vec<String> {
    let mut headings = Vec::new();
    for block in content.split("\n\n") {
        if let Some(title) = infer_title(block) {
            if !headings.iter().any(|existing| existing == &title) {
                headings.push(title);
            }
        }
        if headings.len() >= 8 {
            break;
        }
    }
    headings
}

fn infer_title(block: &str) -> Option<String> {
    block.lines().map(str::trim).find_map(|line| {
        let line = line.trim_matches(|c: char| c == '#' || c == '-' || c.is_whitespace());
        if line.len() < 4 || line.len() > 90 {
            return None;
        }
        let alpha = line.chars().filter(|c| c.is_alphabetic()).count();
        if alpha < 3 {
            return None;
        }
        let looks_numbered = line
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        let uppercase = line.chars().filter(|c| c.is_uppercase()).count();
        if looks_numbered || uppercase * 2 >= alpha {
            Some(line.to_string())
        } else {
            None
        }
    })
}

fn render_index(
    title: &str,
    source_path: &Path,
    generated_at: DateTime<Utc>,
    shards: &[KnowledgeShard],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str("## Uso para agentes\n\n");
    out.push_str(
        "Leer este indice primero. Usa el mapa rapido para elegir shards por tema/pagina y abre solo los shards relevantes. Cada shard es corto, estable e independiente.\n\n",
    );
    out.push_str("## Metadata\n\n");
    out.push_str(&format!("- Source: `{}`\n", source_path.display()));
    out.push_str(&format!("- Generated: `{}`\n", generated_at.to_rfc3339()));
    out.push_str(&format!("- Shards: `{}`\n\n", shards.len()));
    out.push_str("## Mapa rapido\n\n");
    out.push_str("| Shard | Pages | Bytes | Signals |\n");
    out.push_str("|---|---:|---:|---|\n");
    for shard in shards {
        let pages = format_pages(&shard.pages);
        let signals = shard
            .headings
            .iter()
            .take(4)
            .map(|heading| markdown_cell(heading))
            .collect::<Vec<_>>()
            .join("<br>");
        out.push_str(&format!(
            "| [{}](shards/{}) | {} | {} | {} |\n",
            shard.id,
            shard_rel_path(shard),
            markdown_cell(&pages),
            shard.bytes,
            if signals.is_empty() {
                markdown_cell(&shard.title)
            } else {
                signals
            }
        ));
    }
    out.push('\n');
    out.push_str("## Shards\n\n");
    for shard in shards {
        let pages = format_pages(&shard.pages);
        out.push_str(&format!(
            "- [{} - {}](shards/{}) - pages {}; {} bytes",
            shard.id,
            shard.title,
            shard_rel_path(shard),
            pages,
            shard.bytes
        ));
        if !shard.headings.is_empty() {
            out.push_str(&format!(
                "; signals: {}",
                shard
                    .headings
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / ")
            ));
        }
        out.push('\n');
    }
    out
}

fn render_shard(doc_title: &str, id: &str, shard: &TextShard) -> String {
    let headings = if shard.headings.is_empty() {
        "- No strong headings detected; scan the content headings and first paragraphs.\n"
            .to_string()
    } else {
        shard
            .headings
            .iter()
            .map(|heading| format!("- {heading}\n"))
            .collect::<String>()
    };
    format!(
        "# {} - {}\n\n- Source document: `{}`\n- Shard: `{}`\n- Source pages: `{}`\n- Chars: `{}`\n\n## Quick map\n\n{}\n## Content\n\n{}\n",
        id,
        shard.title,
        doc_title,
        id,
        format_pages(&shard.pages),
        shard.content.chars().count(),
        headings,
        shard.content
    )
}

fn format_pages(pages: &[usize]) -> String {
    if pages.is_empty() {
        return "unknown".to_string();
    }
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges = Vec::new();
    let mut start = sorted[0];
    let mut prev = sorted[0];
    for page in sorted.into_iter().skip(1) {
        if page == prev + 1 {
            prev = page;
            continue;
        }
        ranges.push(format_page_range(start, prev));
        start = page;
        prev = page;
    }
    ranges.push(format_page_range(start, prev));
    ranges.join(", ")
}

fn format_page_range(start: usize, end: usize) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

fn shard_rel_path(shard: &KnowledgeShard) -> String {
    shard
        .path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_string()
}

fn markdown_cell(raw: &str) -> String {
    raw.replace('|', "\\|").replace('\n', " ")
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("document")
        .replace(['_', '-'], " ")
        .trim()
        .to_string()
}

fn sanitize_segment(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if (ch.is_whitespace() || ch == '-' || ch == '_' || ch == '.') && !out.ends_with('-')
        {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "document".to_string()
    } else {
        out
    }
}

fn unique_dir(root: &Path, slug: &str) -> Result<PathBuf, Error> {
    for i in 0..1000 {
        let name = if i == 0 {
            slug.to_string()
        } else {
            format!("{slug}-{i}")
        };
        let path = root.join(name);
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Validation(format!(
        "could not create unique knowledge directory for {slug}"
    )))
}

fn write_file(path: &Path, content: &str) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Validation(format!("invalid path {}", path.display())))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_text_writes_index_and_small_shards() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/INST.VT.002-V-03.pdf");
        let repeated =
            "1. OBJETIVO\n\nConfigurar el modulo VT con pasos tecnicos y parametros.\n\n";
        let text = format!(
            "{}\u{c}2. PROCEDIMIENTO\n\n{}{}{}",
            repeated,
            repeated.repeat(180),
            "\u{c}",
            "3. VALIDACION\n\nVerificar estados, alarmas y registros finales."
        );

        let result = ingest_text(
            dir.path(),
            "default",
            source.clone(),
            Some("INST VT 002 V 03".to_string()),
            &text,
        )
        .unwrap();

        assert_eq!(result.title, "INST VT 002 V 03");
        assert!(result
            .output_dir
            .ends_with("profiles/default/knowledge/pdf/inst-vt-002-v-03"));
        assert!(result.index_path.is_file());
        assert!(result.shard_count >= 2);
        for shard in &result.shards {
            assert!(shard.path.is_file());
            let content = std::fs::read_to_string(&shard.path).unwrap();
            assert!(content.contains("## Content"));
            assert!(content.contains("## Quick map"));
            assert!(content.len() <= MAX_SHARD_CHARS + 800);
        }
        let index = std::fs::read_to_string(&result.index_path).unwrap();
        assert!(index.contains("## Uso para agentes"));
        assert!(index.contains("## Mapa rapido"));
        assert!(index.contains("| Shard | Pages | Bytes | Signals |"));
        assert!(index.contains("shards/001-"));
    }

    #[test]
    fn repeated_ingest_uses_unique_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/doc.pdf");
        let text = "MANUAL TECNICO\n\nContenido suficiente para un shard.";

        let first = ingest_text(dir.path(), "default", source.clone(), None, text).unwrap();
        let second = ingest_text(dir.path(), "default", source, None, text).unwrap();

        assert_ne!(first.output_dir, second.output_dir);
        assert!(second
            .output_dir
            .ends_with("profiles/default/knowledge/pdf/doc-1"));
    }

    #[test]
    fn repeated_ingest_replaces_fts_rows_for_same_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/repeated.pdf");
        let text = "MANUAL UNICO\n\nEl token heliometro aparece una sola vez para busqueda.";

        ingest_text(dir.path(), "default", source.clone(), None, text).unwrap();
        let count_after_first = knowledge_fts_count_for_source(dir.path(), &source);
        ingest_text(dir.path(), "default", source.clone(), None, text).unwrap();
        let count_after_second = knowledge_fts_count_for_source(dir.path(), &source);

        assert_eq!(count_after_first, count_after_second);
        let hits = knowledge_search(dir.path(), "default", "heliometro", Some(10)).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn knowledge_search_finds_ingested_shard_and_rebuilds_missing_index() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/manual.pdf");
        let text = "PLAN ALFA\n\nEl procedimiento contiene el termino magnetometro vectorial y calibracion fina.";

        let result = ingest_text(
            dir.path(),
            "default",
            source.clone(),
            Some("Manual Alfa".to_string()),
            text,
        )
        .unwrap();

        let hits =
            knowledge_search(dir.path(), "default", "magnetometro vectorial", Some(3)).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, source.display().to_string());
        assert_eq!(hits[0].shard_id, result.shards[0].id);
        assert!(hits[0].snippet.contains("magnetometro"));

        std::fs::remove_file(dir.path().join("profiles/default/knowledge/index.sqlite")).unwrap();
        let rebuilt = knowledge_search(dir.path(), "default", "calibracion", Some(3)).unwrap();
        assert_eq!(rebuilt.len(), 1);
        assert!(rebuilt[0].path.is_file());
    }

    #[test]
    fn knowledge_search_escapes_fts_operators() {
        let dir = tempfile::tempdir().unwrap();
        let text = "OPERADORES\n\nEl texto contiene alpha NEAR beta con comillas y parentesis.";
        ingest_text(
            dir.path(),
            "default",
            PathBuf::from("/tmp/operators.pdf"),
            None,
            text,
        )
        .unwrap();

        let hits = knowledge_search(
            dir.path(),
            "default",
            r#"alpha" OR beta NEAR(foo)"#,
            Some(3),
        )
        .unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn data_ingest_csv_writes_schema_samples_and_is_searchable() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("sales.csv");
        std::fs::write(
            &csv_path,
            "id,region,total\n1,north,10.5\n2,south,20\n3,andes,30\n",
        )
        .unwrap();

        let result = ingest_data(
            dir.path(),
            "default",
            KnowledgeIngestRequest {
                source_path: csv_path.clone(),
                title: Some("Sales Data".to_string()),
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&result.shards[0].path).unwrap();
        assert!(content.contains("### Column schema"));
        assert!(content.contains("| total | number |"));
        assert!(content.contains("andes"));
        let hits = knowledge_search(dir.path(), "default", "andes", Some(5)).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].source.ends_with("sales.csv"));
    }

    #[test]
    fn data_ingest_csv_strips_utf8_bom_from_first_header() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("bom.csv");
        std::fs::write(&csv_path, "\u{feff}id,region\n1,andes\n").unwrap();

        let result = ingest_data(
            dir.path(),
            "default",
            KnowledgeIngestRequest {
                source_path: csv_path,
                title: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&result.shards[0].path).unwrap();
        assert!(content.contains("| id | integer | 1 |"));
        assert!(!content.contains('\u{feff}'));
    }

    #[test]
    fn rebuild_knowledge_index_skips_broken_shard() {
        let dir = tempfile::tempdir().unwrap();
        let broken = ingest_text(
            dir.path(),
            "default",
            PathBuf::from("/tmp/broken.pdf"),
            None,
            "BROKEN\n\nEste shard se eliminara del disco.",
        )
        .unwrap();
        ingest_text(
            dir.path(),
            "default",
            PathBuf::from("/tmp/good.pdf"),
            None,
            "GOOD\n\nEl termino astrolabio queda disponible.",
        )
        .unwrap();
        std::fs::remove_file(&broken.shards[0].path).unwrap();
        std::fs::remove_file(dir.path().join("profiles/default/knowledge/index.sqlite")).unwrap();

        let hits = knowledge_search(dir.path(), "default", "astrolabio", Some(5)).unwrap();

        assert_eq!(hits.len(), 1);
        assert!(hits[0].source.ends_with("good.pdf"));
    }

    #[test]
    fn data_ingest_xlsx_writes_schema_samples_and_is_searchable() {
        let dir = tempfile::tempdir().unwrap();
        let xlsx_path = dir.path().join("inventory.xlsx");
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.write_string(0, 0, "sku").unwrap();
        worksheet.write_string(0, 1, "name").unwrap();
        worksheet.write_string(1, 0, "A-1").unwrap();
        worksheet.write_string(1, 1, "widget lunar").unwrap();
        workbook.save(&xlsx_path).unwrap();

        let result = ingest_data(
            dir.path(),
            "default",
            KnowledgeIngestRequest {
                source_path: xlsx_path.clone(),
                title: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&result.shards[0].path).unwrap();
        assert!(content.contains("Sheet:"));
        assert!(content.contains("widget lunar"));
        let hits = knowledge_search(dir.path(), "default", "widget lunar", Some(5)).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].source.ends_with("inventory.xlsx"));
    }

    fn knowledge_fts_count_for_source(harness_home: &Path, source: &Path) -> i64 {
        let conn = open_knowledge_connection(
            harness_home
                .join("profiles/default/knowledge")
                .join("index.sqlite"),
        )
        .unwrap();
        conn.query_row(
            "SELECT count(*) FROM knowledge_fts WHERE source = ?1",
            params![source.display().to_string()],
            |row| row.get(0),
        )
        .unwrap()
    }
}
