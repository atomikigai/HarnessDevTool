use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use harness_core::Event;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ContextSearchHit {
    pub thread_id: String,
    pub session_id: String,
    pub event_type: String,
    pub at: i64,
    pub pressure: Option<f64>,
    pub model: Option<String>,
    pub snippet: String,
}

fn db_path(harness_home: &Path, profile: &str) -> PathBuf {
    harness_home
        .join("profiles")
        .join(profile)
        .join("context.sqlite")
}

fn open(harness_home: &Path, profile: &str) -> Result<Connection> {
    let path = db_path(harness_home, profile);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS context_events (
            thread_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            session_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            at INTEGER NOT NULL,
            pressure REAL,
            model TEXT,
            body TEXT NOT NULL,
            payload_json TEXT,
            PRIMARY KEY(thread_id, seq)
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS context_events_fts
            USING fts5(thread_id UNINDEXED, session_id UNINDEXED, event_type UNINDEXED, body);
        CREATE TABLE IF NOT EXISTS index_offsets (
            thread_id TEXT PRIMARY KEY,
            last_seq INTEGER NOT NULL
        );
        "#,
    )?;
    Ok(conn)
}

pub fn index_context_events(harness_home: &Path, profile: &str, events: &[Event]) -> Result<usize> {
    let mut conn = open(harness_home, profile)?;
    let tx = conn.transaction()?;
    let mut indexed = 0usize;
    let mut max_seq_by_thread = BTreeMap::<String, u64>::new();
    for event in events {
        if let Some(thread_id) = event.thread_id.as_deref() {
            max_seq_by_thread
                .entry(thread_id.to_string())
                .and_modify(|seq| *seq = (*seq).max(event.seq))
                .or_insert(event.seq);
        }
        if !event.event_type.starts_with("session.context.") {
            continue;
        }
        let Some(thread_id) = event.thread_id.as_deref() else {
            continue;
        };
        let payload = event.payload.as_ref();
        let session_id = payload
            .and_then(|p| p.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if session_id.is_empty() {
            continue;
        }
        let pressure = payload
            .and_then(|p| p.get("pressure"))
            .and_then(|v| v.as_f64());
        let model = payload
            .and_then(|p| p.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let payload_json = payload.map(serde_json::to_string).transpose()?;
        let item_text = event
            .items
            .iter()
            .map(|harness_core::Item::Text { text }| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let checkpoint = payload
            .and_then(|p| p.get("checkpoint"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let body = [item_text.as_str(), checkpoint]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");

        tx.execute(
            r#"
            INSERT OR REPLACE INTO context_events
              (thread_id, seq, session_id, event_type, at, pressure, model, body, payload_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                thread_id,
                event.seq as i64,
                session_id,
                event.event_type,
                event.at,
                pressure,
                model,
                body,
                payload_json
            ],
        )?;
        let rowid: i64 = tx.query_row(
            "SELECT rowid FROM context_events WHERE thread_id = ?1 AND seq = ?2",
            params![thread_id, event.seq as i64],
            |row| row.get(0),
        )?;
        tx.execute(
            "DELETE FROM context_events_fts WHERE rowid = ?1",
            params![rowid],
        )?;
        tx.execute(
            r#"
            INSERT INTO context_events_fts(rowid, thread_id, session_id, event_type, body)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![rowid, thread_id, session_id, event.event_type, body],
        )?;
        indexed += 1;
    }
    for (thread_id, last_seq) in max_seq_by_thread {
        tx.execute(
            r#"
            INSERT INTO index_offsets(thread_id, last_seq)
            VALUES (?1, ?2)
            ON CONFLICT(thread_id) DO UPDATE SET
                last_seq = CASE
                    WHEN excluded.last_seq > index_offsets.last_seq THEN excluded.last_seq
                    ELSE index_offsets.last_seq
                END
            "#,
            params![thread_id, last_seq as i64],
        )?;
    }
    tx.commit()?;
    Ok(indexed)
}

pub fn last_indexed_seq(
    harness_home: &Path,
    profile: &str,
    thread_id: &str,
) -> Result<Option<u64>> {
    let conn = open(harness_home, profile)?;
    let mut stmt = conn.prepare("SELECT last_seq FROM index_offsets WHERE thread_id = ?1")?;
    let mut rows = stmt.query(params![thread_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let seq: i64 = row.get(0)?;
    Ok(u64::try_from(seq).ok())
}

pub fn context_events_for_session(
    harness_home: &Path,
    profile: &str,
    session_id: &str,
) -> Result<Vec<Event>> {
    let conn = open(harness_home, profile)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT thread_id, seq, event_type, at, payload_json
        FROM context_events
        WHERE session_id = ?1
        ORDER BY seq ASC
        "#,
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        let payload_json: Option<String> = row.get(4)?;
        let payload = payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok());
        Ok(Event {
            thread_id: Some(row.get(0)?),
            seq: row.get::<_, i64>(1)? as u64,
            event_type: row.get(2)?,
            at: row.get(3)?,
            items: Vec::new(),
            actor: None,
            payload,
        })
    })?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn search_context_events(
    harness_home: &Path,
    profile: &str,
    session_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<ContextSearchHit>> {
    let conn = open(harness_home, profile)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let fts_query = fts_query(query);
    let limit = i64::try_from(limit.clamp(1, 50)).unwrap_or(20);
    let mut stmt = conn.prepare(
        r#"
        SELECT e.thread_id, e.session_id, e.event_type, e.at, e.pressure, e.model,
               snippet(context_events_fts, 3, '', '', ' ... ', 12) AS snippet
        FROM context_events_fts
        JOIN context_events e ON e.rowid = context_events_fts.rowid
        WHERE context_events_fts MATCH ?1
          AND e.session_id = ?2
        ORDER BY bm25(context_events_fts), e.at DESC
        LIMIT ?3
        "#,
    )?;
    let rows = stmt.query_map(params![fts_query, session_id, limit], |row| {
        Ok(ContextSearchHit {
            thread_id: row.get(0)?,
            session_id: row.get(1)?,
            event_type: row.get(2)?,
            at: row.get(3)?,
            pressure: row.get(4)?,
            model: row.get::<_, Option<String>>(5)?.filter(|s| !s.is_empty()),
            snippet: row.get(6)?,
        })
    })?;
    Ok(rows.filter_map(|row| row.ok()).collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::{Event, Item};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn indexes_context_events_only() {
        let dir = tempdir().unwrap();
        let events = vec![
            Event {
                seq: 1,
                at: 10,
                event_type: "session.context.checkpoint_saved".into(),
                items: vec![Item::Text {
                    text: "Saved compact context checkpoint.".into(),
                }],
                thread_id: Some("t1".into()),
                actor: Some("context-governor".into()),
                payload: Some(json!({
                    "session_id": "s1",
                    "pressure": 0.4,
                    "checkpoint": "CONTEXT CHECKPOINT\nnext_action: test"
                })),
            },
            Event {
                seq: 2,
                at: 11,
                event_type: "task.created".into(),
                items: vec![],
                thread_id: Some("t1".into()),
                actor: None,
                payload: None,
            },
        ];

        assert_eq!(
            index_context_events(dir.path(), "default", &events).unwrap(),
            1
        );
        let conn = open(dir.path(), "default").unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM context_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn searches_context_events_by_session() {
        let dir = tempdir().unwrap();
        let events = vec![
            Event {
                seq: 1,
                at: 10,
                event_type: "session.context.checkpoint_saved".into(),
                items: vec![Item::Text {
                    text: "Saved compact context checkpoint.".into(),
                }],
                thread_id: Some("t1".into()),
                actor: Some("context-governor".into()),
                payload: Some(json!({
                    "session_id": "s1",
                    "checkpoint": "CONTEXT CHECKPOINT\nnext_action: fix terminal transcript"
                })),
            },
            Event {
                seq: 2,
                at: 11,
                event_type: "session.context.checkpoint_saved".into(),
                items: vec![Item::Text {
                    text: "Other".into(),
                }],
                thread_id: Some("t1".into()),
                actor: Some("context-governor".into()),
                payload: Some(json!({
                    "session_id": "s2",
                    "checkpoint": "CONTEXT CHECKPOINT\nnext_action: fix terminal transcript"
                })),
            },
        ];

        index_context_events(dir.path(), "default", &events).unwrap();
        let hits = search_context_events(dir.path(), "default", "s1", "terminal", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "s1");
    }

    #[test]
    fn records_thread_offset_even_for_non_context_events() {
        let dir = tempdir().unwrap();
        let events = vec![
            Event {
                seq: 10,
                at: 10,
                event_type: "task.created".into(),
                items: vec![],
                thread_id: Some("t1".into()),
                actor: None,
                payload: None,
            },
            Event {
                seq: 11,
                at: 11,
                event_type: "session.context.checkpoint_saved".into(),
                items: vec![Item::Text {
                    text: "Saved compact context checkpoint.".into(),
                }],
                thread_id: Some("t1".into()),
                actor: Some("context-governor".into()),
                payload: Some(json!({
                    "session_id": "s1",
                    "checkpoint": "CONTEXT CHECKPOINT\nnext_action: continue"
                })),
            },
        ];

        assert_eq!(
            index_context_events(dir.path(), "default", &events).unwrap(),
            1
        );
        assert_eq!(
            last_indexed_seq(dir.path(), "default", "t1").unwrap(),
            Some(11)
        );

        let indexed = context_events_for_session(dir.path(), "default", "s1").unwrap();
        assert_eq!(indexed.len(), 1);
        assert_eq!(indexed[0].seq, 11);
        assert_eq!(indexed[0].payload.as_ref().unwrap()["session_id"], "s1");
    }

    #[test]
    fn offset_does_not_move_backwards_when_old_event_is_reindexed() {
        let dir = tempdir().unwrap();
        let old = Event {
            seq: 1,
            at: 10,
            event_type: "session.context.checkpoint_saved".into(),
            items: vec![],
            thread_id: Some("t1".into()),
            actor: Some("context-governor".into()),
            payload: Some(json!({ "session_id": "s1", "checkpoint": "old" })),
        };
        let latest = Event {
            seq: 9,
            at: 11,
            event_type: "task.created".into(),
            items: vec![],
            thread_id: Some("t1".into()),
            actor: None,
            payload: None,
        };

        index_context_events(dir.path(), "default", &[latest]).unwrap();
        index_context_events(dir.path(), "default", &[old]).unwrap();

        assert_eq!(
            last_indexed_seq(dir.path(), "default", "t1").unwrap(),
            Some(9)
        );
    }

    #[test]
    fn long_thread_rebuild_records_offset_for_repeated_searches() {
        let dir = tempdir().unwrap();
        let mut events = Vec::new();
        for seq in 0..500 {
            events.push(Event {
                seq,
                at: seq as i64,
                event_type: if seq == 499 {
                    "session.context.checkpoint_saved".into()
                } else {
                    "task.updated".into()
                },
                items: vec![],
                thread_id: Some("long-thread".into()),
                actor: None,
                payload: if seq == 499 {
                    Some(json!({
                        "session_id": "s-long",
                        "checkpoint": "CONTEXT CHECKPOINT\nnext_action: continue long thread work"
                    }))
                } else {
                    None
                },
            });
        }

        assert!(last_indexed_seq(dir.path(), "default", "long-thread")
            .unwrap()
            .is_none());
        assert_eq!(
            index_context_events(dir.path(), "default", &events).unwrap(),
            1
        );
        assert_eq!(
            last_indexed_seq(dir.path(), "default", "long-thread").unwrap(),
            Some(499)
        );
        assert_eq!(
            search_context_events(dir.path(), "default", "s-long", "continue", 10)
                .unwrap()
                .len(),
            1
        );
    }
}
