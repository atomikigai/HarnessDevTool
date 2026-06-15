//! Profile memory search rails backed by a derived FTS index.
//!
//! The source of truth remains Markdown/docs on disk. The SQLite database is a
//! rebuildable accelerator so agents can retrieve compact memory slices on
//! demand instead of loading whole docs or transcripts into context.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use harness_core::validate_profile_id;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAX_FILES: usize = 700;
const MAX_FILE_BYTES: u64 = 192 * 1024;
const MAX_BODY_BYTES: usize = 192 * 1024;

#[derive(Debug, Clone)]
struct SourceDoc {
    kind: String,
    source: String,
    path: PathBuf,
    title: String,
    status: Option<String>,
    tags: Vec<String>,
    mtime: i64,
    len: i64,
    body: String,
}

#[derive(Debug, Serialize)]
struct MemoryHit {
    id: String,
    kind: String,
    source: String,
    path: String,
    title: String,
    status: Option<String>,
    tags: Vec<String>,
    snippet: String,
    score: f64,
    updated_at: i64,
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn profile_dir(harness_home: &Path, profile: &str) -> PathBuf {
    harness_home.join("profiles").join(profile)
}

fn index_path(harness_home: &Path, profile: &str) -> PathBuf {
    profile_dir(harness_home, profile)
        .join("memory")
        .join("memory_fts.sqlite")
}

fn open_index(harness_home: &Path, profile: &str) -> Result<Connection, String> {
    validate_profile_id(profile).map_err(|e| format!("memory profile: {e}"))?;
    let path = index_path(harness_home, profile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("memory index dir: {e}"))?;
    }
    let conn = Connection::open(path).map_err(|e| format!("memory index open: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_millis(1000))
        .map_err(|e| format!("memory index busy_timeout: {e}"))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("memory index wal: {e}"))?;
    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_entries (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            source TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            status TEXT,
            tags_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            len INTEGER NOT NULL,
            body TEXT NOT NULL
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            id UNINDEXED,
            title,
            body,
            kind UNINDEXED,
            source UNINDEXED,
            path UNINDEXED,
            status UNINDEXED,
            tags UNINDEXED,
            updated_at UNINDEXED,
            tokenize = 'unicode61'
         );
         PRAGMA user_version = 1;",
    )
    .map_err(|e| format!("memory index schema: {e}"))
}

fn sync_index(
    conn: &Connection,
    harness_home: &Path,
    profile: &str,
    cwd: &Path,
) -> Result<(), String> {
    let docs = collect_docs(harness_home, profile, cwd)?;
    let mut live_paths = HashSet::new();
    for doc in docs {
        let path = doc.path.display().to_string();
        live_paths.insert(path.clone());
        let unchanged = conn
            .query_row(
                "SELECT mtime, len FROM memory_entries WHERE path = ?1",
                params![path],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|e| format!("memory index stat read: {e}"))?
            .is_some_and(|(mtime, len)| mtime == doc.mtime && len == doc.len);
        if unchanged {
            continue;
        }
        upsert_doc(conn, &doc)?;
    }

    let existing = {
        let mut stmt = conn
            .prepare("SELECT id, path FROM memory_entries")
            .map_err(|e| format!("memory index path scan: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("memory index path rows: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("memory index path collect: {e}"))?
    };
    for (id, path) in existing {
        if !live_paths.contains(&path) {
            delete_doc(conn, &id)?;
        }
    }
    Ok(())
}

fn upsert_doc(conn: &Connection, doc: &SourceDoc) -> Result<(), String> {
    let id = stable_id(&doc.path);
    let path = doc.path.display().to_string();
    let tags_json =
        serde_json::to_string(&doc.tags).map_err(|e| format!("memory tags json: {e}"))?;
    let tags_text = doc.tags.join(" ");
    let updated_at = doc.mtime.max(0);
    delete_doc(conn, &id)?;
    conn.execute(
        "INSERT OR REPLACE INTO memory_entries
            (id, kind, source, path, title, status, tags_json, updated_at, mtime, len, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id, doc.kind, doc.source, path, doc.title, doc.status, tags_json, updated_at,
            doc.mtime, doc.len, doc.body
        ],
    )
    .map_err(|e| format!("memory entry insert: {e}"))?;
    conn.execute(
        "INSERT INTO memory_fts (id, title, body, kind, source, path, status, tags, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            stable_id(&doc.path),
            doc.title,
            doc.body,
            doc.kind,
            doc.source,
            doc.path.display().to_string(),
            doc.status,
            tags_text,
            updated_at
        ],
    )
    .map_err(|e| format!("memory fts insert: {e}"))?;
    Ok(())
}

fn delete_doc(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM memory_fts WHERE id = ?1", params![id])
        .map_err(|e| format!("memory fts delete: {e}"))?;
    conn.execute("DELETE FROM memory_entries WHERE id = ?1", params![id])
        .map_err(|e| format!("memory entry delete: {e}"))?;
    Ok(())
}

fn collect_docs(harness_home: &Path, profile: &str, cwd: &Path) -> Result<Vec<SourceDoc>, String> {
    let mut files = Vec::new();
    let profile_root = profile_dir(harness_home, profile);
    collect_markdown_files(&profile_root.join("memory"), "memory", &mut files)?;
    collect_markdown_files(&profile_root.join("skills"), "skill", &mut files)?;
    collect_markdown_files(&profile_root.join("learner"), "learner", &mut files)?;
    collect_markdown_files(&cwd.join("docs"), "doc", &mut files)?;
    collect_markdown_files(&cwd.join("skills"), "skill", &mut files)?;
    for name in ["AGENTS.md", "DESIGN.md"] {
        let path = cwd.join(name);
        if path.is_file() {
            files.push(("doc".to_string(), path));
        }
    }

    let mut docs = Vec::new();
    let mut seen = HashSet::new();
    for (default_kind, path) in files {
        if docs.len() >= MAX_FILES {
            break;
        }
        let path_key = path.display().to_string();
        if !seen.insert(path_key) {
            continue;
        }
        let meta = match fs::metadata(&path) {
            Ok(meta) if meta.is_file() && meta.len() <= MAX_FILE_BYTES => meta,
            _ => continue,
        };
        let body = match fs::read_to_string(&path) {
            Ok(body) => truncate_to_char_boundary(body, MAX_BODY_BYTES),
            Err(_) => continue,
        };
        let kind = classify_kind(&path, &default_kind);
        let source = classify_source(&path, cwd, &profile_root);
        let title = markdown_title(&body).unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("memory")
                .replace('-', " ")
        });
        let status = frontmatter_value(&body, "status").or_else(|| status_from_path(&path));
        let tags = frontmatter_tags(&body);
        docs.push(SourceDoc {
            kind,
            source,
            path,
            title,
            status,
            tags,
            mtime: system_time_secs(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
            len: meta.len() as i64,
            body,
        });
    }
    Ok(docs)
}

fn collect_markdown_files(
    root: &Path,
    kind: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if out.len() >= MAX_FILES {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if file_type.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                out.push((kind.to_string(), path));
            }
        }
    }
    Ok(())
}

pub fn search(
    harness_home: &Path,
    profile: &str,
    cwd: &Path,
    args: &Value,
) -> Result<Value, String> {
    let query = str_arg(args, "query")?.trim();
    if query.is_empty() {
        return Err("memory_search query must not be empty".to_string());
    }
    let top_k = args
        .get("top_k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 50) as usize;
    let kind = opt_str(args, "kind");
    let status = opt_str(args, "status");
    let tags = string_array(args, "tags");
    let conn = open_index(harness_home, profile)?;
    sync_index(&conn, harness_home, profile, cwd)?;
    let limit = (top_k * 10).clamp(top_k, 200) as i64;
    let fts_query = fts_phrase_query(query);
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, source, path, title, status, tags,
                    snippet(memory_fts, 2, '[', ']', '...', 28),
                    bm25(memory_fts), updated_at
             FROM memory_fts
             WHERE memory_fts MATCH ?1
             ORDER BY bm25(memory_fts)
             LIMIT ?2",
        )
        .map_err(|e| format!("memory search prepare: {e}"))?;
    let rows = stmt
        .query_map(params![fts_query, limit], |row| {
            Ok(MemoryHit {
                id: row.get(0)?,
                kind: row.get(1)?,
                source: row.get(2)?,
                path: row.get(3)?,
                title: row.get(4)?,
                status: row.get(5)?,
                tags: split_tags(row.get::<_, String>(6)?.as_str()),
                snippet: row.get(7)?,
                score: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| format!("memory search query: {e}"))?;
    let mut hits = Vec::new();
    for row in rows {
        let hit = row.map_err(|e| format!("memory search row: {e}"))?;
        if kind.as_deref().is_some_and(|expected| hit.kind != expected) {
            continue;
        }
        if status
            .as_deref()
            .is_some_and(|expected| hit.status.as_deref() != Some(expected))
        {
            continue;
        }
        if !tags.is_empty() && !tags.iter().all(|tag| hit.tags.iter().any(|h| h == tag)) {
            continue;
        }
        hits.push(hit);
        if hits.len() >= top_k {
            break;
        }
    }
    Ok(json!({
        "query": query,
        "top_k": top_k,
        "index_path": index_path(harness_home, profile),
        "hits": hits
    }))
}

pub fn read(harness_home: &Path, profile: &str, cwd: &Path, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id")?;
    let conn = open_index(harness_home, profile)?;
    sync_index(&conn, harness_home, profile, cwd)?;
    conn.query_row(
        "SELECT id, kind, source, path, title, status, tags_json, updated_at, body
         FROM memory_entries WHERE id = ?1",
        params![id],
        |row| {
            let tags_json: String = row.get(6)?;
            let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "kind": row.get::<_, String>(1)?,
                "source": row.get::<_, String>(2)?,
                "path": row.get::<_, String>(3)?,
                "title": row.get::<_, String>(4)?,
                "status": row.get::<_, Option<String>>(5)?,
                "tags": tags,
                "updated_at": row.get::<_, i64>(7)?,
                "body": row.get::<_, String>(8)?,
            }))
        },
    )
    .optional()
    .map_err(|e| format!("memory_read: {e}"))?
    .ok_or_else(|| format!("memory_read: entry not found: {id}"))
}

pub fn continuity(
    harness_home: &Path,
    profile: &str,
    cwd: &Path,
    args: &Value,
) -> Result<Value, String> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .clamp(1, 25) as i64;
    let conn = open_index(harness_home, profile)?;
    sync_index(&conn, harness_home, profile, cwd)?;
    let counts = {
        let mut stmt = conn
            .prepare("SELECT kind, COUNT(*) FROM memory_entries GROUP BY kind ORDER BY kind")
            .map_err(|e| format!("memory continuity counts: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(json!({
                    "kind": row.get::<_, String>(0)?,
                    "count": row.get::<_, i64>(1)?
                }))
            })
            .map_err(|e| format!("memory continuity count rows: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("memory continuity count collect: {e}"))?
    };
    let recent = {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, source, path, title, status, tags_json, updated_at
                 FROM memory_entries ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(|e| format!("memory continuity recent: {e}"))?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let tags_json: String = row.get(6)?;
                let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "kind": row.get::<_, String>(1)?,
                    "source": row.get::<_, String>(2)?,
                    "path": row.get::<_, String>(3)?,
                    "title": row.get::<_, String>(4)?,
                    "status": row.get::<_, Option<String>>(5)?,
                    "tags": tags,
                    "updated_at": row.get::<_, i64>(7)?
                }))
            })
            .map_err(|e| format!("memory continuity recent rows: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("memory continuity recent collect: {e}"))?
    };
    Ok(json!({
        "profile": profile,
        "index_path": index_path(harness_home, profile),
        "counts": counts,
        "recent": recent,
        "guidance": "Use memory_search for narrow recall and memory_read only for a selected hit. memory_note_propose creates reviewable proposals, not active memory."
    }))
}

pub fn note_propose(
    harness_home: &Path,
    profile: &str,
    cwd: &Path,
    args: &Value,
) -> Result<Value, String> {
    validate_profile_id(profile).map_err(|e| format!("memory_note_propose profile: {e}"))?;
    let title = str_arg(args, "title")?.trim();
    let body = str_arg(args, "body")?.trim();
    if title.is_empty() || body.is_empty() {
        return Err("memory_note_propose title and body must not be empty".to_string());
    }
    let tags = string_array(args, "tags");
    let reason = opt_str(args, "reason").unwrap_or_else(|| "agent proposed memory".to_string());
    let dir = profile_dir(harness_home, profile)
        .join("memory")
        .join("proposals");
    fs::create_dir_all(&dir).map_err(|e| format!("memory proposals dir: {e}"))?;
    let timestamp = Utc::now();
    let slug = slugify(title);
    let file_name = format!("{}-{}.md", timestamp.timestamp_millis(), slug);
    let path = dir.join(file_name);
    let content = format!(
        "---\nstatus: proposed\ntitle: {}\ntags: [{}]\nreason: {}\ncreated_at: {}\n---\n\n# {}\n\n{}\n",
        yaml_scalar(title),
        tags.iter().map(|tag| yaml_scalar(tag)).collect::<Vec<_>>().join(", "),
        yaml_scalar(&reason),
        timestamp.to_rfc3339(),
        title,
        body
    );
    fs::write(&path, content).map_err(|e| format!("memory proposal write: {e}"))?;
    let conn = open_index(harness_home, profile)?;
    sync_index(&conn, harness_home, profile, cwd)?;
    let id = stable_id(&path);
    Ok(json!({
        "id": id,
        "status": "proposed",
        "approval_required": true,
        "path": path,
        "message": "Proposal written for review; active memory was not modified."
    }))
}

fn stable_id(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.display().to_string().as_bytes());
    let digest = hasher.finalize();
    format!("mem-{}", hex_prefix(&digest, 16))
}

fn hex_prefix(bytes: &[u8], n: usize) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .take(n)
        .map(|nibble| match nibble {
            0..=9 => (b'0' + nibble) as char,
            _ => (b'a' + (nibble - 10)) as char,
        })
        .collect()
}

fn classify_kind(path: &Path, default_kind: &str) -> String {
    let text = path.display().to_string();
    if text.contains("/memory/proposals/") {
        "proposal".to_string()
    } else if text.contains("/docs/12-build-plan/") {
        "plan".to_string()
    } else if text.contains("/docs/13-agents/") {
        "agent_doc".to_string()
    } else if text.contains("/docs/14-memory/") {
        "memory".to_string()
    } else if text.contains("/skills/") {
        "skill".to_string()
    } else {
        default_kind.to_string()
    }
}

fn classify_source(path: &Path, cwd: &Path, profile_root: &Path) -> String {
    if path.starts_with(profile_root) {
        "profile".to_string()
    } else if path.starts_with(cwd) {
        "repo".to_string()
    } else {
        "external".to_string()
    }
}

fn status_from_path(path: &Path) -> Option<String> {
    let text = path.display().to_string();
    if text.contains("/memory/proposals/") || text.contains("/skills/proposed/") {
        Some("proposed".to_string())
    } else if text.contains("/skills/.archive/") {
        Some("archived".to_string())
    } else if text.contains("/skills/agent_created/") {
        Some("active".to_string())
    } else {
        None
    }
}

fn markdown_title(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| {
            line.strip_prefix("# ")
                .map(|title| title.trim().to_string())
        })
        .filter(|title| !title.is_empty())
}

fn frontmatter_value(body: &str, key: &str) -> Option<String> {
    let mut lines = body.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            return None;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() == key {
            return Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

fn frontmatter_tags(body: &str) -> Vec<String> {
    let Some(value) = frontmatter_value(body, "tags") else {
        return Vec::new();
    };
    if value.starts_with('[') && value.ends_with(']') {
        value
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|tag| tag.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
    } else {
        value
            .split_whitespace()
            .map(str::to_string)
            .filter(|tag| !tag.is_empty())
            .collect()
    }
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split_whitespace().map(str::to_string).collect()
}

fn fts_phrase_query(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn truncate_to_char_boundary(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

fn system_time_secs(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn slugify(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "memory-note".to_string()
    } else {
        out
    }
}

fn yaml_scalar(raw: &str) -> String {
    format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_search_indexes_repo_docs_and_profile_notes() {
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let docs = cwd.path().join("docs").join("12-build-plan");
        fs::create_dir_all(&docs).unwrap();
        fs::write(
            docs.join("runtime.md"),
            "# Runtime Plan\n\nEl indice magnetometro debe alimentar continuidad.",
        )
        .unwrap();
        let mem = profile_dir(home.path(), "default").join("memory");
        fs::create_dir_all(&mem).unwrap();
        fs::write(
            mem.join("facts.md"),
            "---\nstatus: active\ntags: [runtime, memory]\n---\n# Facts\n\nLa memoria heliometro resume decisiones.",
        )
        .unwrap();

        let hits = search(
            home.path(),
            "default",
            cwd.path(),
            &json!({"query": "heliometro", "tags": ["runtime"]}),
        )
        .unwrap();
        let arr = hits["hits"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let id = arr[0]["id"].as_str().unwrap();
        let read = read(home.path(), "default", cwd.path(), &json!({"id": id})).unwrap();
        assert!(read["body"].as_str().unwrap().contains("heliometro"));

        let plan_hits = search(
            home.path(),
            "default",
            cwd.path(),
            &json!({"query": "magnetometro", "kind": "plan"}),
        )
        .unwrap();
        assert_eq!(plan_hits["hits"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn memory_note_propose_writes_reviewable_proposal() {
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();

        let proposal = note_propose(
            home.path(),
            "default",
            cwd.path(),
            &json!({
                "title": "Scheduler continuity",
                "body": "Recordar que los ticks deben saltar threads invalidos.",
                "tags": ["scheduler", "memory"],
                "reason": "test"
            }),
        )
        .unwrap();
        assert_eq!(proposal["status"], "proposed");
        assert_eq!(proposal["approval_required"], true);
        let path = PathBuf::from(proposal["path"].as_str().unwrap());
        assert!(path.exists());
        assert!(path
            .display()
            .to_string()
            .contains("profiles/default/memory/proposals"));

        let hits = search(
            home.path(),
            "default",
            cwd.path(),
            &json!({"query": "threads invalidos", "kind": "proposal", "status": "proposed"}),
        )
        .unwrap();
        assert_eq!(hits["hits"].as_array().unwrap().len(), 1);
    }
}
