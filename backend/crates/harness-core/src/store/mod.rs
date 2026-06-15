use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use thiserror::Error;
use uuid::Uuid;

use crate::events::{Event, TimelineEntity, TimelineItem, TimelineQueryOptions, TimelineReport};
use crate::repos::RepoContext;
use crate::threads::{AutonomyProfile, ExecutionMode, Handoff, ReadinessReport, Thread};
use crate::{validate_profile_id, validate_task_id, validate_thread_id};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
}

/// Filesystem-backed store rooted at `<home>/profiles/<profile>`.
///
/// Layout:
/// ```text
/// <home>/profiles/default/threads/<uuid>/meta.json
/// <home>/profiles/default/threads/<uuid>/events.jsonl
/// ```
#[derive(Debug)]
pub struct Store {
    threads_dir: PathBuf,
    write_lock: Mutex<()>,
    next_event_seq: Mutex<HashMap<String, u64>>,
}

impl Store {
    pub fn new(home: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::with_profile(home, "default")
    }

    pub fn with_profile(home: impl AsRef<Path>, profile: &str) -> Result<Self, StoreError> {
        validate_profile_id(profile).map_err(StoreError::Validation)?;
        let threads_dir = home.as_ref().join("profiles").join(profile).join("threads");
        std::fs::create_dir_all(&threads_dir)?;
        Ok(Self {
            threads_dir,
            write_lock: Mutex::new(()),
            next_event_seq: Mutex::new(HashMap::new()),
        })
    }

    pub fn threads_dir(&self) -> &Path {
        &self.threads_dir
    }

    fn thread_dir(&self, thread_id: &str) -> Result<PathBuf, StoreError> {
        validate_thread_id(thread_id).map_err(StoreError::Validation)?;
        Ok(self.threads_dir.join(thread_id))
    }

    pub fn create_thread(&self, title: Option<String>) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();
        let thread = Thread::new(id.clone(), title, created_at);

        let dir = self.threads_dir.join(&id);
        std::fs::create_dir_all(&dir)?;

        let meta_path = dir.join("meta.json");
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;

        // touch events.jsonl
        let events_path = dir.join("events.jsonl");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_path)?;

        Ok(thread)
    }

    pub fn list_threads(&self) -> Result<Vec<Thread>, StoreError> {
        let mut out = Vec::new();
        let read = match std::fs::read_dir(&self.threads_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in read {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            let bytes = std::fs::read(&meta_path)?;
            match serde_json::from_slice::<Thread>(&bytes) {
                Ok(t) => out.push(t),
                Err(e) => {
                    tracing::warn!(error = %e, path = %meta_path.display(), "skipping unreadable thread meta");
                }
            }
        }
        out.sort_by_key(|t| t.created_at);
        Ok(out)
    }

    pub fn get_thread(&self, id: &str) -> Result<Thread, StoreError> {
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn set_execution_mode(&self, id: &str, mode: ExecutionMode) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        let mut thread: Thread = serde_json::from_slice(&bytes)?;
        thread.execution_mode = Some(mode);
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;
        Ok(thread)
    }

    pub fn set_autonomy_profile(
        &self,
        id: &str,
        profile: AutonomyProfile,
    ) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        let mut thread: Thread = serde_json::from_slice(&bytes)?;
        thread.autonomy_profile = Some(profile);
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;
        Ok(thread)
    }

    pub fn set_thread_repo(&self, id: &str, repo: RepoContext) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        let mut thread: Thread = serde_json::from_slice(&bytes)?;
        thread.repo = Some(repo);
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;
        Ok(thread)
    }

    pub fn write_readiness_report(
        &self,
        thread_id: &str,
        report: &ReadinessReport,
    ) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let path = dir.join("readiness.json");
        let mut f = File::create(&path)?;
        f.write_all(serde_json::to_vec_pretty(report)?.as_slice())?;
        f.sync_all()?;
        Ok(())
    }

    pub fn read_readiness_report(
        &self,
        thread_id: &str,
    ) -> Result<Option<ReadinessReport>, StoreError> {
        let path = self.thread_dir(thread_id)?.join("readiness.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn append_handoff(&self, thread_id: &str, handoff: &Handoff) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        validate_task_id(&handoff.task_id).map_err(StoreError::Validation)?;
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let handoffs_dir = dir.join("handoffs");
        std::fs::create_dir_all(&handoffs_dir)?;
        let path = handoffs_dir.join(format!("{}.jsonl", handoff.task_id));
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(handoff)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        Ok(())
    }

    pub fn read_handoffs(
        &self,
        thread_id: &str,
        task_id: &str,
    ) -> Result<Vec<Handoff>, StoreError> {
        validate_task_id(task_id).map_err(StoreError::Validation)?;
        let path = self
            .thread_dir(thread_id)?
            .join("handoffs")
            .join(format!("{task_id}.jsonl"));
        if !path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&path)?;
        read_jsonl_lossy(BufReader::new(f), &path)
    }

    /// Append a single event to a thread's `events.jsonl`. Returns the seq written.
    pub fn append_event(&self, thread_id: &str, event: &Event) -> Result<u64, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let path = dir.join("events.jsonl");
        let seq = {
            let mut seqs = self
                .next_event_seq
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let next = match seqs.get_mut(thread_id) {
                Some(next) => next,
                None => {
                    let initialized = max_jsonl_seq(&path)?.map_or(0, |seq| seq + 1);
                    seqs.entry(thread_id.to_string()).or_insert(initialized)
                }
            };
            let seq = *next;
            *next += 1;
            seq
        };
        let mut event = event.clone();
        event.seq = seq;
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(&event)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        if let Err(e) = self.index_event(thread_id, &event) {
            tracing::warn!(
                thread_id,
                seq,
                error = %e,
                "failed to update derived events index"
            );
        }
        Ok(seq)
    }

    pub fn read_events(&self, thread_id: &str) -> Result<Vec<Event>, StoreError> {
        let path = self.thread_dir(thread_id)?.join("events.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&path)?;
        read_jsonl_lossy(BufReader::new(f), &path)
    }

    pub fn read_timeline(&self, thread_id: &str) -> Result<TimelineReport, StoreError> {
        self.get_thread(thread_id)?;
        let items = self.query_timeline(thread_id, TimelineQueryOptions::default())?;
        Ok(TimelineReport {
            thread_id: thread_id.to_string(),
            generated_at: Utc::now().timestamp_millis(),
            event_count: items.len(),
            items,
        })
    }

    pub fn query_timeline(
        &self,
        thread_id: &str,
        options: TimelineQueryOptions,
    ) -> Result<Vec<TimelineItem>, StoreError> {
        self.get_thread(thread_id)?;
        let dir = self.thread_dir(thread_id)?;
        self.ensure_events_index(thread_id, &dir)?;
        let conn = open_events_index(&dir)?;
        query_events_index(&conn, options)
    }

    fn ensure_events_index(&self, thread_id: &str, dir: &Path) -> Result<(), StoreError> {
        let conn = open_events_index(dir)?;
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
        drop(conn);
        let events = self.read_events(thread_id)?;
        let conn = open_events_index(dir)?;
        rebuild_events_index(&conn, &events)?;
        Ok(())
    }

    fn index_event(&self, thread_id: &str, event: &Event) -> Result<(), StoreError> {
        let dir = self.thread_dir(thread_id)?;
        let conn = open_events_index(&dir)?;
        upsert_event_index(&conn, event)?;
        Ok(())
    }
}

fn events_index_path(thread_dir: &Path) -> PathBuf {
    thread_dir.join("events_index.sqlite")
}

fn open_events_index(thread_dir: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(events_index_path(thread_dir))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS events (
            seq INTEGER PRIMARY KEY,
            at INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            actor TEXT,
            entity_kind TEXT,
            entity_id TEXT,
            session_id TEXT,
            task_id TEXT,
            summary TEXT NOT NULL,
            payload_json TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
        CREATE INDEX IF NOT EXISTS idx_events_actor ON events(actor);
        CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
        CREATE INDEX IF NOT EXISTS idx_events_task ON events(task_id);
        CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
            USING fts5(summary, payload_json, event_type UNINDEXED, actor UNINDEXED, session_id UNINDEXED, task_id UNINDEXED);
        CREATE TABLE IF NOT EXISTS index_meta (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );
        "#,
    )?;
    Ok(conn)
}

fn rebuild_events_index(conn: &Connection, events: &[Event]) -> Result<(), StoreError> {
    conn.execute("DELETE FROM events", [])?;
    conn.execute("DELETE FROM events_fts", [])?;
    conn.execute("DELETE FROM index_meta", [])?;
    let mut max_seq = None;
    for event in events {
        upsert_event_index(conn, event)?;
        max_seq = Some(max_seq.map_or(event.seq, |seq: u64| seq.max(event.seq)));
    }
    let indexed_seq = max_seq.map(|seq| seq as i64).unwrap_or(-1);
    conn.execute(
        "INSERT OR REPLACE INTO index_meta(key, value) VALUES ('last_seq', ?1)",
        params![indexed_seq],
    )?;
    Ok(())
}

fn upsert_event_index(conn: &Connection, event: &Event) -> Result<(), StoreError> {
    let item = TimelineItem::from_event(event.clone());
    let payload_json = item
        .payload
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let (session_id, task_id) = payload_ids(item.payload.as_ref(), item.entity.as_ref());
    let entity_kind = item.entity.as_ref().map(|entity| entity.kind.as_str());
    let entity_id = item.entity.as_ref().map(|entity| entity.id.as_str());
    conn.execute(
        r#"
        INSERT INTO events(seq, at, event_type, actor, entity_kind, entity_id, session_id, task_id, summary, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(seq) DO UPDATE SET
            at=excluded.at,
            event_type=excluded.event_type,
            actor=excluded.actor,
            entity_kind=excluded.entity_kind,
            entity_id=excluded.entity_id,
            session_id=excluded.session_id,
            task_id=excluded.task_id,
            summary=excluded.summary,
            payload_json=excluded.payload_json
        "#,
        params![
            item.seq as i64,
            item.at,
            item.event_type.as_str(),
            item.actor.as_deref(),
            entity_kind,
            entity_id,
            session_id.as_deref(),
            task_id.as_deref(),
            item.summary.as_str(),
            payload_json.as_deref(),
        ],
    )?;
    conn.execute(
        "DELETE FROM events_fts WHERE rowid = ?1",
        params![event.seq as i64],
    )?;
    conn.execute(
        r#"
        INSERT INTO events_fts(rowid, summary, payload_json, event_type, actor, session_id, task_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            event.seq as i64,
            item.summary.as_str(),
            payload_json.as_deref(),
            item.event_type.as_str(),
            item.actor.as_deref(),
            session_id.as_deref(),
            task_id.as_deref(),
        ],
    )?;
    conn.execute(
        r#"
        INSERT INTO index_meta(key, value) VALUES ('last_seq', ?1)
        ON CONFLICT(key) DO UPDATE SET
            value = CASE WHEN excluded.value > index_meta.value THEN excluded.value ELSE index_meta.value END
        "#,
        params![event.seq as i64],
    )?;
    Ok(())
}

fn payload_ids(
    payload: Option<&serde_json::Value>,
    entity: Option<&TimelineEntity>,
) -> (Option<String>, Option<String>) {
    let payload_session = payload
        .and_then(|payload| payload.get("session_id"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let payload_task = payload
        .and_then(|payload| payload.get("task_id"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let entity_session = entity
        .filter(|entity| entity.kind == "session")
        .map(|entity| entity.id.clone());
    let entity_task = entity
        .filter(|entity| entity.kind == "task")
        .map(|entity| entity.id.clone());
    (
        payload_session.or(entity_session),
        payload_task.or(entity_task),
    )
}

fn query_events_index(
    conn: &Connection,
    options: TimelineQueryOptions,
) -> Result<Vec<TimelineItem>, StoreError> {
    let mut sql = String::from(
        "SELECT e.seq, e.at, e.event_type, e.actor, e.entity_kind, e.entity_id, e.summary, e.payload_json \
         FROM events e",
    );
    let mut args = Vec::<rusqlite::types::Value>::new();
    if options.q.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        sql.push_str(" JOIN events_fts ON events_fts.rowid = e.seq");
    }
    sql.push_str(" WHERE 1=1");
    if let Some(after) = options.after {
        sql.push_str(" AND e.seq > ?");
        args.push(rusqlite::types::Value::Integer(after as i64));
    }
    if let Some(event_type) = options.event_type.filter(|v| !v.trim().is_empty()) {
        sql.push_str(" AND e.event_type = ?");
        args.push(rusqlite::types::Value::Text(event_type));
    }
    if let Some(actor) = options.actor.filter(|v| !v.trim().is_empty()) {
        sql.push_str(" AND e.actor = ?");
        args.push(rusqlite::types::Value::Text(actor));
    }
    if let Some(task_id) = options.task_id.filter(|v| !v.trim().is_empty()) {
        sql.push_str(" AND e.task_id = ?");
        args.push(rusqlite::types::Value::Text(task_id));
    }
    if let Some(session_id) = options.session_id.filter(|v| !v.trim().is_empty()) {
        sql.push_str(" AND e.session_id = ?");
        args.push(rusqlite::types::Value::Text(session_id));
    }
    if let Some(q) = options.q.filter(|q| !q.trim().is_empty()) {
        sql.push_str(" AND events_fts MATCH ?");
        args.push(rusqlite::types::Value::Text(fts_query(&q)));
    }
    sql.push_str(" ORDER BY e.seq ASC LIMIT ?");
    let limit = options.limit.unwrap_or(usize::MAX).min(i64::MAX as usize) as i64;
    args.push(rusqlite::types::Value::Integer(limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |row| {
        let entity_kind: Option<String> = row.get(4)?;
        let entity_id: Option<String> = row.get(5)?;
        let payload_json: Option<String> = row.get(7)?;
        let payload = payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok());
        Ok(TimelineItem {
            seq: row.get::<_, i64>(0)? as u64,
            at: row.get(1)?,
            event_type: row.get(2)?,
            actor: row.get(3)?,
            entity: entity_kind
                .zip(entity_id)
                .map(|(kind, id)| TimelineEntity { kind, id }),
            summary: row.get(6)?,
            payload,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
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

fn max_jsonl_seq(path: &Path) -> Result<Option<u64>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let f = File::open(path)?;
    let mut max_seq = None;
    for line in BufReader::new(f).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(&line) {
            Ok(event) => {
                max_seq = Some(max_seq.map_or(event.seq, |seq: u64| seq.max(event.seq)));
            }
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "skipping corrupt jsonl record while initializing append seq"
                );
            }
        }
    }
    Ok(max_seq)
}

fn read_jsonl_lossy<T: DeserializeOwned>(
    reader: impl BufRead,
    path: &Path,
) -> Result<Vec<T>, StoreError> {
    let mut out = Vec::new();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(value) => out.push(value),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    line = line_no + 1,
                    error = %error,
                    "skipping corrupt jsonl record"
                );
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home() -> tempdir_like::TempDir {
        tempdir_like::TempDir::new("harness-core-test")
    }

    #[test]
    fn create_and_list_threads() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        assert!(store.list_threads().unwrap().is_empty());
        let t = store.create_thread(Some("hello".into())).unwrap();
        let listed = store.list_threads().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, t.id);
        assert_eq!(listed[0].title.as_deref(), Some("hello"));
    }

    #[test]
    fn append_and_read_events() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let ev = Event {
            seq: 0,
            at: 123,
            event_type: "tick".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        };
        let seq = store.append_event(&t.id, &ev).unwrap();
        let read = store.read_events(&t.id).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].event_type, "tick");
    }

    #[test]
    fn concurrent_appends_assign_unique_monotonic_seq() {
        let home = tmp_home();
        let store = std::sync::Arc::new(Store::new(home.path()).unwrap());
        let t = store.create_thread(None).unwrap();
        let mut handles = Vec::new();

        for i in 0..16 {
            let store = store.clone();
            let tid = t.id.clone();
            handles.push(std::thread::spawn(move || {
                let ev = Event {
                    seq: 999,
                    at: i,
                    event_type: "tick".into(),
                    items: vec![],
                    thread_id: Some(tid.clone()),
                    actor: None,
                    payload: Some(serde_json::json!({ "i": i })),
                };
                store.append_event(&tid, &ev).unwrap()
            }));
        }

        let mut returned: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        returned.sort_unstable();
        assert_eq!(returned, (0..16).collect::<Vec<_>>());

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(read.len(), 16);
        assert_eq!(
            read.iter().map(|ev| ev.seq).collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
    }

    #[test]
    fn append_event_initializes_seq_from_existing_max_seq() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let existing = Event {
            seq: 41,
            at: 123,
            event_type: "existing".into(),
            items: vec![],
            thread_id: Some(t.id.clone()),
            actor: None,
            payload: None,
        };
        let path = store.threads_dir().join(&t.id).join("events.jsonl");
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, "{}", serde_json::to_string(&existing).unwrap()).unwrap();
        }

        let store = Store::new(home.path()).unwrap();
        let ev = Event {
            seq: 0,
            at: 124,
            event_type: "next".into(),
            items: vec![],
            thread_id: Some(t.id.clone()),
            actor: None,
            payload: None,
        };
        let seq = store.append_event(&t.id, &ev).unwrap();
        assert_eq!(seq, 42);
        let read = store.read_events(&t.id).unwrap();
        assert_eq!(
            read.iter().map(|event| event.seq).collect::<Vec<_>>(),
            vec![41, 42]
        );
    }

    #[test]
    fn old_event_without_envelope_fields_deserializes() {
        let json = r#"{"seq":7,"at":123,"type":"capability.decided","items":[]}"#;
        let ev: Event = serde_json::from_str(json).unwrap();
        assert_eq!(ev.seq, 7);
        assert_eq!(ev.event_type, "capability.decided");
        assert!(ev.thread_id.is_none());
        assert!(ev.actor.is_none());
        assert!(ev.payload.is_none());
    }

    #[test]
    fn new_envelope_round_trips() {
        let ev = Event {
            seq: 3,
            at: 123,
            event_type: "task.created".into(),
            items: vec![],
            thread_id: Some("thr-1".into()),
            actor: Some("human".into()),
            payload: Some(serde_json::json!({
                "type": "task.created",
                "task_id": "T-0001",
                "by": "human",
            })),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"thread_id\":\"thr-1\""));
        let decoded: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type, "task.created");
        assert_eq!(decoded.thread_id.as_deref(), Some("thr-1"));
        assert_eq!(decoded.actor.as_deref(), Some("human"));
        let payload = decoded.payload.unwrap();
        assert_eq!(payload["type"], "task.created");
        assert_eq!(payload["task_id"], "T-0001");
    }

    #[test]
    fn read_events_skips_corrupt_jsonl_records() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let ev = Event {
            seq: 0,
            at: 123,
            event_type: "tick".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        };
        let ev2 = Event {
            seq: 1,
            at: 124,
            event_type: "tock".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        };
        let path = store.threads_dir().join(&t.id).join("events.jsonl");
        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).unwrap();
        writeln!(f, "{{not-json").unwrap();
        writeln!(f, "{}", serde_json::to_string(&ev2).unwrap()).unwrap();

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].event_type, "tick");
        assert_eq!(read[1].event_type, "tock");
    }

    #[test]
    fn replay_orders_mixed_envelopes_by_append_seq() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();

        for event_type in [
            "task.created",
            "thread.readiness.checked",
            "capability.decided",
            "task.ready",
        ] {
            let ev = Event {
                seq: 999,
                at: 123,
                event_type: event_type.into(),
                items: vec![],
                thread_id: Some(t.id.clone()),
                actor: None,
                payload: Some(serde_json::json!({ "type": event_type })),
            };
            store.append_event(&t.id, &ev).unwrap();
        }

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(
            read.iter()
                .map(|ev| (ev.seq, ev.event_type.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "task.created"),
                (1, "thread.readiness.checked"),
                (2, "capability.decided"),
                (3, "task.ready"),
            ]
        );
    }

    #[test]
    fn read_timeline_returns_ordered_summaries() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        for (event_type, payload) in [
            (
                "task.created",
                serde_json::json!({ "type": "task.created", "task_id": "T-0001" }),
            ),
            (
                "task.updated",
                serde_json::json!({
                    "type": "task.updated",
                    "task_id": "T-0001",
                    "fields": ["status"]
                }),
            ),
        ] {
            store
                .append_event(
                    &t.id,
                    &Event {
                        seq: 999,
                        at: 123,
                        event_type: event_type.into(),
                        items: vec![],
                        thread_id: Some(t.id.clone()),
                        actor: Some("test".into()),
                        payload: Some(payload),
                    },
                )
                .unwrap();
        }

        let report = store.read_timeline(&t.id).unwrap();
        assert_eq!(report.event_count, 2);
        assert_eq!(report.items[0].seq, 0);
        assert_eq!(report.items[0].summary, "Created task T-0001");
        assert_eq!(report.items[1].seq, 1);
        assert_eq!(report.items[1].summary, "Updated task T-0001: status");
    }

    #[test]
    fn query_timeline_filters_from_index_without_events_jsonl() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();

        for (event_type, actor, payload) in [
            (
                "session.spawned",
                "agent:codex",
                serde_json::json!({ "session_id": "S-0001", "task_id": "T-0001", "note": "boot alpha" }),
            ),
            (
                "task.updated",
                "agent:codex",
                serde_json::json!({ "type": "task.updated", "task_id": "T-0001", "fields": ["status"], "note": "alpha done" }),
            ),
            (
                "task.updated",
                "agent:qa",
                serde_json::json!({ "type": "task.updated", "task_id": "T-0002", "fields": ["status"], "note": "beta done" }),
            ),
        ] {
            store
                .append_event(
                    &t.id,
                    &Event {
                        seq: 999,
                        at: 123,
                        event_type: event_type.into(),
                        items: vec![],
                        thread_id: Some(t.id.clone()),
                        actor: Some(actor.into()),
                        payload: Some(payload),
                    },
                )
                .unwrap();
        }

        let events_path = store.threads_dir().join(&t.id).join("events.jsonl");
        std::fs::rename(&events_path, events_path.with_extension("jsonl.off")).unwrap();

        let task_hits = store
            .query_timeline(
                &t.id,
                TimelineQueryOptions {
                    task_id: Some("T-0001".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            task_hits.iter().map(|item| item.seq).collect::<Vec<_>>(),
            vec![0, 1]
        );

        let search_hits = store
            .query_timeline(
                &t.id,
                TimelineQueryOptions {
                    q: Some("beta".into()),
                    limit: Some(5),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(search_hits.len(), 1);
        assert_eq!(search_hits[0].actor.as_deref(), Some("agent:qa"));
    }

    #[test]
    fn query_timeline_rebuilds_index_when_missing() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        store
            .append_event(
                &t.id,
                &Event {
                    seq: 999,
                    at: 123,
                    event_type: "task.created".into(),
                    items: vec![],
                    thread_id: Some(t.id.clone()),
                    actor: Some("human".into()),
                    payload: Some(serde_json::json!({
                        "type": "task.created",
                        "task_id": "T-0001"
                    })),
                },
            )
            .unwrap();

        let thread_dir = store.threads_dir().join(&t.id);
        for suffix in ["", "-wal", "-shm"] {
            let path = thread_dir.join(format!("events_index.sqlite{suffix}"));
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }

        let items = store
            .query_timeline(
                &t.id,
                TimelineQueryOptions {
                    event_type: Some("task.created".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].summary, "Created task T-0001");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();

        let err = store.read_events("../escape").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));

        let err = store.read_handoffs(&t.id, "../T-0001").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn readiness_and_execution_mode_roundtrip() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(Some("hello".into())).unwrap();
        let report = ReadinessReport::new(
            123,
            "/tmp/project",
            vec![],
            vec![],
            serde_json::json!({ "package_manager": "pnpm" }),
            ExecutionMode::Quick,
        );
        store.write_readiness_report(&t.id, &report).unwrap();
        let read = store.read_readiness_report(&t.id).unwrap().unwrap();
        assert_eq!(read.suggested_execution_mode, ExecutionMode::Quick);

        let updated = store
            .set_execution_mode(&t.id, ExecutionMode::Quick)
            .unwrap();
        assert_eq!(updated.execution_mode, Some(ExecutionMode::Quick));
        let updated = store
            .set_autonomy_profile(&t.id, AutonomyProfile::Autonomous)
            .unwrap();
        assert_eq!(updated.autonomy_profile, Some(AutonomyProfile::Autonomous));
        assert_eq!(
            store.get_thread(&t.id).unwrap().execution_mode,
            Some(ExecutionMode::Quick)
        );
    }

    #[test]
    fn handoffs_are_append_only_per_task() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let handoff = Handoff {
            at: 123,
            from: "agent:frontend-1".to_string(),
            to_role: "qa".to_string(),
            task_id: "T-0001".to_string(),
            status: "ready_for_verification".to_string(),
            goal: "Verify pagination".to_string(),
            assumptions: vec![],
            files_changed: vec!["src/orders.rs".to_string()],
            commands_run: vec!["cargo test orders".to_string()],
            verification_passed: vec!["cargo test orders".to_string()],
            verification_not_run: vec![],
            blocked_on: vec![],
            next_agent_action: "QA runs edge cases".to_string(),
        };
        store.append_handoff(&t.id, &handoff).unwrap();
        store.append_handoff(&t.id, &handoff).unwrap();
        let read = store.read_handoffs(&t.id, "T-0001").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].files_changed, vec!["src/orders.rs"]);
    }

    #[test]
    fn read_handoffs_skips_corrupt_jsonl_records() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let handoff = Handoff {
            at: 123,
            from: "agent:frontend-1".to_string(),
            to_role: "qa".to_string(),
            task_id: "T-0001".to_string(),
            status: "ready_for_verification".to_string(),
            goal: "Verify pagination".to_string(),
            assumptions: vec![],
            files_changed: vec!["src/orders.rs".to_string()],
            commands_run: vec!["cargo test orders".to_string()],
            verification_passed: vec!["cargo test orders".to_string()],
            verification_not_run: vec![],
            blocked_on: vec![],
            next_agent_action: "QA runs edge cases".to_string(),
        };
        let mut handoff2 = handoff.clone();
        handoff2.status = "accepted".to_string();
        let handoffs_dir = store.threads_dir().join(&t.id).join("handoffs");
        std::fs::create_dir_all(&handoffs_dir).unwrap();
        let path = handoffs_dir.join("T-0001.jsonl");
        let mut f = File::create(path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&handoff).unwrap()).unwrap();
        writeln!(f, "{{not-json").unwrap();
        writeln!(f, "{}", serde_json::to_string(&handoff2).unwrap()).unwrap();

        let read = store.read_handoffs(&t.id, "T-0001").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].status, "ready_for_verification");
        assert_eq!(read[1].status, "accepted");
    }
}

// Tiny ad-hoc tempdir helper to avoid an extra dev-dep.
#[cfg(test)]
mod tempdir_like {
    use std::path::{Path, PathBuf};

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let pid = std::process::id();
            let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}"));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
