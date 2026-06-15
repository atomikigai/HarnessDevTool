//! Normalised transcript persistence per session. Writes one JSONL file per
//! session under `<harness_home>/profiles/<p>/sessions/<sid>/transcript.jsonl`.
//! Lets the frontend reconnect with `?since=<seq>` without depending on the
//! upstream CLI's transcript format.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde_json::Value;
use tokio::sync::Mutex;

use super::event::{TranscriptEvent, TranscriptKind};

/// Per-session writer that assigns monotonic `seq` numbers and persists each
/// event as a JSONL line. Cheap to share via `Arc` across the watcher task
/// and the SSE replay route.
pub struct TranscriptStore {
    dir: PathBuf,
    seq: AtomicU64,
    file: Mutex<std::fs::File>,
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptQueryOptions {
    pub since: u64,
    pub limit: Option<usize>,
    pub kind: Option<String>,
    pub role: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptToolResultsOptions {
    pub since: u64,
    pub limit: Option<usize>,
    pub tool_name: Option<String>,
    pub errors_only: bool,
}

impl TranscriptStore {
    /// Open (creating + appending) the transcript log for a session. Reads
    /// the existing file (if any) to recover the next `seq` so restarts of
    /// the watcher don't reset the counter.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Arc<Self>> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("transcript.jsonl");
        let last_seq = recover_last_seq(&path).unwrap_or(0);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Arc::new(Self {
            dir,
            seq: AtomicU64::new(last_seq),
            file: Mutex::new(file),
        }))
    }

    /// Assign the next `seq` and persist the event. Returns the event with
    /// `seq` populated so callers can broadcast the same value they wrote.
    pub async fn ingest(&self, mut ev: TranscriptEvent) -> std::io::Result<TranscriptEvent> {
        ev.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let line = serde_json::to_string(&ev).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("serialize: {e}"))
        })?;
        {
            let mut f = self.file.lock().await;
            writeln!(f, "{line}")?;
            f.flush()?;
        }
        if let Err(e) = index_event(&self.dir, &ev) {
            tracing::warn!(
                session_id = %ev.session_id,
                seq = ev.seq,
                error = %e,
                "failed to update derived transcript index"
            );
        }
        Ok(ev)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Highest `seq` persisted so far (0 for a fresh store). Used by the
    /// watcher to decide whether re-ingesting source history could create
    /// duplicates after a server restart.
    pub fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

/// Replay every persisted event with `seq > since` from disk.
///
/// Reads the whole file in one syscall, then deserialises lines in parallel
/// across rayon's thread pool (offloaded from the tokio executor via
/// `spawn_blocking`). Sorting after collection preserves seq order regardless
/// of which thread finished first.
pub async fn read_events_since(
    transcript_path: &Path,
    since: u64,
) -> std::io::Result<Vec<TranscriptEvent>> {
    if let Some(dir) = transcript_path.parent() {
        match query_transcript_events(
            dir,
            TranscriptQueryOptions {
                since,
                ..Default::default()
            },
        )
        .await
        {
            Ok(events) => return Ok(events),
            Err(e) => tracing::warn!(
                path = %transcript_path.display(),
                error = %e,
                "transcript index query failed; falling back to JSONL replay"
            ),
        }
    }
    read_events_since_raw(transcript_path, since).await
}

pub async fn query_transcript_events(
    transcript_dir: &Path,
    options: TranscriptQueryOptions,
) -> std::io::Result<Vec<TranscriptEvent>> {
    let dir = transcript_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        ensure_transcript_index(&dir)?;
        let conn = open_index(&dir)?;
        query_index(&conn, options)
    })
    .await
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
}

pub async fn transcript_tool_results(
    transcript_dir: &Path,
    options: TranscriptToolResultsOptions,
) -> std::io::Result<Vec<TranscriptEvent>> {
    let dir = transcript_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        ensure_transcript_index(&dir)?;
        let conn = open_index(&dir)?;
        query_tool_results(&conn, options)
    })
    .await
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
}

async fn read_events_since_raw(
    transcript_path: &Path,
    since: u64,
) -> std::io::Result<Vec<TranscriptEvent>> {
    if !transcript_path.exists() {
        return Ok(Vec::new());
    }
    // Single async read — one syscall, no per-line await overhead.
    let bytes = tokio::fs::read(transcript_path).await?;

    // CPU-bound work runs on the blocking pool so the tokio executor is free.
    tokio::task::spawn_blocking(move || {
        use rayon::prelude::*;

        let text = std::str::from_utf8(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut events: Vec<TranscriptEvent> = text
            .par_lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                match serde_json::from_str::<TranscriptEvent>(line) {
                    Ok(ev) if ev.seq > since => Some(ev),
                    Ok(_) => None,
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping malformed transcript line");
                        None
                    }
                }
            })
            .collect();

        // Parallel collection doesn't guarantee order — restore it.
        events.sort_unstable_by_key(|ev| ev.seq);
        Ok(events)
    })
    .await
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
}

fn index_path(transcript_dir: &Path) -> PathBuf {
    transcript_dir.join("transcript_index.sqlite")
}

fn open_index(transcript_dir: &Path) -> std::io::Result<Connection> {
    let conn = Connection::open(index_path(transcript_dir)).map_err(sql_err)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS transcript_events (
            seq INTEGER PRIMARY KEY,
            session_id TEXT NOT NULL,
            ts TEXT NOT NULL,
            source TEXT NOT NULL,
            kind TEXT NOT NULL,
            role TEXT,
            tool_name TEXT,
            tool_use_id TEXT,
            is_error INTEGER,
            content_preview TEXT NOT NULL,
            content_text TEXT NOT NULL,
            payload_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_transcript_kind ON transcript_events(kind);
        CREATE INDEX IF NOT EXISTS idx_transcript_role ON transcript_events(role);
        CREATE INDEX IF NOT EXISTS idx_transcript_tool_name ON transcript_events(tool_name);
        CREATE INDEX IF NOT EXISTS idx_transcript_tool_use_id ON transcript_events(tool_use_id);
        CREATE VIRTUAL TABLE IF NOT EXISTS transcript_events_fts
            USING fts5(content_text, payload_json, kind UNINDEXED, role UNINDEXED, tool_name UNINDEXED);
        CREATE TABLE IF NOT EXISTS index_meta (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );
        "#,
    )
    .map_err(sql_err)?;
    Ok(conn)
}

fn ensure_transcript_index(transcript_dir: &Path) -> std::io::Result<()> {
    let conn = open_index(transcript_dir)?;
    let has_offset = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'last_seq'",
            [],
            |_row| Ok(()),
        )
        .is_ok();
    if has_offset {
        return Ok(());
    }
    let events = read_events_from_jsonl_sync(&transcript_dir.join("transcript.jsonl"))?;
    rebuild_index(&conn, &events)
}

fn rebuild_index(conn: &Connection, events: &[TranscriptEvent]) -> std::io::Result<()> {
    conn.execute("DELETE FROM transcript_events", [])
        .map_err(sql_err)?;
    conn.execute("DELETE FROM transcript_events_fts", [])
        .map_err(sql_err)?;
    conn.execute("DELETE FROM index_meta", [])
        .map_err(sql_err)?;
    let mut max_seq = None;
    for event in events {
        upsert_index_event(conn, event)?;
        max_seq = Some(max_seq.map_or(event.seq, |seq: u64| seq.max(event.seq)));
    }
    let indexed_seq = max_seq.map(|seq| seq as i64).unwrap_or(-1);
    conn.execute(
        "INSERT OR REPLACE INTO index_meta(key, value) VALUES ('last_seq', ?1)",
        params![indexed_seq],
    )
    .map_err(sql_err)?;
    Ok(())
}

fn index_event(transcript_dir: &Path, event: &TranscriptEvent) -> std::io::Result<()> {
    let conn = open_index(transcript_dir)?;
    upsert_index_event(&conn, event)
}

fn upsert_index_event(conn: &Connection, event: &TranscriptEvent) -> std::io::Result<()> {
    let payload_json = serde_json::to_string(event).map_err(json_err)?;
    let source = enum_json_name(&event.source);
    let kind = enum_json_name(&event.kind);
    let content_text = event_search_text(event);
    let content_preview = truncate_preview(&content_text);
    conn.execute(
        r#"
        INSERT INTO transcript_events(
            seq, session_id, ts, source, kind, role, tool_name, tool_use_id,
            is_error, content_preview, content_text, payload_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(seq) DO UPDATE SET
            session_id=excluded.session_id,
            ts=excluded.ts,
            source=excluded.source,
            kind=excluded.kind,
            role=excluded.role,
            tool_name=excluded.tool_name,
            tool_use_id=excluded.tool_use_id,
            is_error=excluded.is_error,
            content_preview=excluded.content_preview,
            content_text=excluded.content_text,
            payload_json=excluded.payload_json
        "#,
        params![
            event.seq as i64,
            event.session_id.as_str(),
            event.ts.as_str(),
            source.as_str(),
            kind.as_str(),
            event.role.as_deref(),
            event.tool_name.as_deref(),
            event.tool_use_id.as_deref(),
            event.is_error.map(i64::from),
            content_preview.as_str(),
            content_text.as_str(),
            payload_json.as_str(),
        ],
    )
    .map_err(sql_err)?;
    conn.execute(
        "DELETE FROM transcript_events_fts WHERE rowid = ?1",
        params![event.seq as i64],
    )
    .map_err(sql_err)?;
    conn.execute(
        r#"
        INSERT INTO transcript_events_fts(rowid, content_text, payload_json, kind, role, tool_name)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            event.seq as i64,
            content_text.as_str(),
            payload_json.as_str(),
            kind.as_str(),
            event.role.as_deref(),
            event.tool_name.as_deref(),
        ],
    )
    .map_err(sql_err)?;
    conn.execute(
        r#"
        INSERT INTO index_meta(key, value) VALUES ('last_seq', ?1)
        ON CONFLICT(key) DO UPDATE SET
            value = CASE WHEN excluded.value > index_meta.value THEN excluded.value ELSE index_meta.value END
        "#,
        params![event.seq as i64],
    )
    .map_err(sql_err)?;
    Ok(())
}

fn query_index(
    conn: &Connection,
    options: TranscriptQueryOptions,
) -> std::io::Result<Vec<TranscriptEvent>> {
    let mut sql = String::from("SELECT e.payload_json FROM transcript_events e");
    let mut args = Vec::<rusqlite::types::Value>::new();
    if options.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        sql.push_str(" JOIN transcript_events_fts ON transcript_events_fts.rowid = e.seq");
    }
    sql.push_str(" WHERE e.seq > ?");
    args.push(rusqlite::types::Value::Integer(options.since as i64));
    if let Some(kind) = options.kind.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND e.kind = ?");
        args.push(rusqlite::types::Value::Text(kind));
    }
    if let Some(role) = options.role.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND e.role = ?");
        args.push(rusqlite::types::Value::Text(role));
    }
    if let Some(q) = options.q.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND transcript_events_fts MATCH ?");
        args.push(rusqlite::types::Value::Text(fts_query(&q)));
    }
    sql.push_str(" ORDER BY e.seq ASC");
    if let Some(limit) = options.limit {
        sql.push_str(" LIMIT ?");
        args.push(rusqlite::types::Value::Integer(
            limit.min(i64::MAX as usize) as i64,
        ));
    }
    query_payloads(conn, &sql, args)
}

fn query_tool_results(
    conn: &Connection,
    options: TranscriptToolResultsOptions,
) -> std::io::Result<Vec<TranscriptEvent>> {
    let mut sql = String::from(
        "SELECT e.payload_json FROM transcript_events e WHERE e.seq > ? AND e.kind = ?",
    );
    let mut args = vec![
        rusqlite::types::Value::Integer(options.since as i64),
        rusqlite::types::Value::Text(enum_json_name(&TranscriptKind::ToolResult)),
    ];
    if let Some(tool_name) = options.tool_name.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND e.tool_name = ?");
        args.push(rusqlite::types::Value::Text(tool_name));
    }
    if options.errors_only {
        sql.push_str(" AND e.is_error = 1");
    }
    sql.push_str(" ORDER BY e.seq ASC LIMIT ?");
    args.push(rusqlite::types::Value::Integer(
        options.limit.unwrap_or(50).clamp(1, 200) as i64,
    ));
    query_payloads(conn, &sql, args)
}

fn query_payloads(
    conn: &Connection,
    sql: &str,
    args: Vec<rusqlite::types::Value>,
) -> std::io::Result<Vec<TranscriptEvent>> {
    let mut stmt = conn.prepare(sql).map_err(sql_err)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |row| {
            let payload: String = row.get(0)?;
            serde_json::from_str::<TranscriptEvent>(&payload).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .map_err(sql_err)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql_err)?);
    }
    Ok(out)
}

fn read_events_from_jsonl_sync(transcript_path: &Path) -> std::io::Result<Vec<TranscriptEvent>> {
    if !transcript_path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(transcript_path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TranscriptEvent>(&line) {
            Ok(event) => out.push(event),
            Err(e) => tracing::warn!(error = %e, "skipping malformed transcript line"),
        }
    }
    out.sort_unstable_by_key(|event| event.seq);
    Ok(out)
}

fn event_search_text(event: &TranscriptEvent) -> String {
    let mut parts = Vec::new();
    push_opt(&mut parts, event.role.as_deref());
    push_opt(&mut parts, event.content.as_deref());
    push_opt(&mut parts, event.tool_name.as_deref());
    push_opt(&mut parts, event.tool_use_id.as_deref());
    push_json(&mut parts, event.tool_args.as_ref());
    push_json(&mut parts, event.tool_result.as_ref());
    push_opt(&mut parts, event.subtype.as_deref());
    push_json(&mut parts, event.raw.as_ref());
    parts.join(" ")
}

fn push_opt(parts: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        parts.push(value.trim().to_string());
    }
}

fn push_json(parts: &mut Vec<String>, value: Option<&Value>) {
    if let Some(value) = value {
        if let Ok(text) = serde_json::to_string(value) {
            parts.push(text);
        }
    }
}

fn truncate_preview(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_CHARS {
        return compact;
    }
    compact.chars().take(MAX_CHARS).collect::<String>() + "..."
}

fn enum_json_name(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"unknown\"".into())
        .trim_matches('"')
        .to_string()
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|term| {
            let clean = term
                .chars()
                .filter(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '-')
                .collect::<String>();
            if clean.is_empty() {
                None
            } else {
                Some(format!("\"{clean}\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn sql_err(error: rusqlite::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, error)
}

fn json_err(error: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}

/// Scan the existing transcript file for the highest `seq` so we resume the
/// counter on watcher restart. Cheap — single pass, allocates one line at
/// a time. Tolerates malformed lines.
fn recover_last_seq(path: &Path) -> std::io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut max_seq: u64 = 0;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        // Cheap field extraction without full deserialisation; safe because
        // we control the writer.
        if let Some(idx) = line.find("\"seq\":") {
            let after = &line[idx + 6..];
            let end = after
                .find(',')
                .or_else(|| after.find('}'))
                .unwrap_or(after.len());
            if let Ok(n) = after[..end].trim().parse::<u64>() {
                if n > max_seq {
                    max_seq = n;
                }
            }
        }
    }
    Ok(max_seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::event::{TranscriptKind, TranscriptSource};

    fn event(sid: &str, kind: TranscriptKind, content: &str) -> TranscriptEvent {
        TranscriptEvent {
            seq: 0,
            session_id: sid.to_string(),
            ts: "2026-06-15T00:00:00Z".to_string(),
            source: TranscriptSource::Codex,
            kind,
            role: if kind == TranscriptKind::Message {
                Some("assistant".into())
            } else {
                None
            },
            content: (kind == TranscriptKind::Message).then(|| content.to_string()),
            tool_name: (kind == TranscriptKind::ToolCall).then(|| "shell.exec".into()),
            tool_args: (kind == TranscriptKind::ToolCall)
                .then(|| serde_json::json!({ "cmd": content })),
            tool_use_id: matches!(kind, TranscriptKind::ToolCall | TranscriptKind::ToolResult)
                .then(|| "call-1".into()),
            tool_result: (kind == TranscriptKind::ToolResult)
                .then(|| serde_json::json!({ "output": content })),
            is_error: (kind == TranscriptKind::ToolResult).then_some(false),
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    }

    #[tokio::test]
    async fn transcript_query_reads_from_index_without_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let store = TranscriptStore::open(dir.path()).unwrap();
        store
            .ingest(event("sid-1", TranscriptKind::Message, "hello alpha"))
            .await
            .unwrap();
        store
            .ingest(event("sid-1", TranscriptKind::ToolCall, "cargo test"))
            .await
            .unwrap();
        store
            .ingest(event("sid-1", TranscriptKind::ToolResult, "alpha passed"))
            .await
            .unwrap();
        std::fs::rename(
            dir.path().join("transcript.jsonl"),
            dir.path().join("transcript.jsonl.off"),
        )
        .unwrap();

        let events = query_transcript_events(
            dir.path(),
            TranscriptQueryOptions {
                since: 1,
                limit: Some(10),
                q: Some("alpha".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            events.iter().map(|event| event.seq).collect::<Vec<_>>(),
            vec![3]
        );

        let results = transcript_tool_results(
            dir.path(),
            TranscriptToolResultsOptions {
                limit: Some(10),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].tool_result.as_ref().unwrap()["output"],
            "alpha passed"
        );
    }

    #[tokio::test]
    async fn transcript_query_rebuilds_missing_index_from_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let store = TranscriptStore::open(dir.path()).unwrap();
        store
            .ingest(event("sid-1", TranscriptKind::Message, "rebuild me"))
            .await
            .unwrap();
        drop(store);
        for suffix in ["", "-wal", "-shm"] {
            let path = dir.path().join(format!("transcript_index.sqlite{suffix}"));
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }

        let events = query_transcript_events(
            dir.path(),
            TranscriptQueryOptions {
                kind: Some("message".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].content.as_deref(), Some("rebuild me"));
    }
}
