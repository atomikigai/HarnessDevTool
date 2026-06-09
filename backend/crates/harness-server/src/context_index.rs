use std::path::{Path, PathBuf};

use anyhow::Result;
use harness_core::Event;
use rusqlite::{params, Connection};

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
        "#,
    )?;
    Ok(conn)
}

pub fn index_context_events(harness_home: &Path, profile: &str, events: &[Event]) -> Result<usize> {
    let mut conn = open(harness_home, profile)?;
    let tx = conn.transaction()?;
    let mut indexed = 0usize;
    for event in events {
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
    tx.commit()?;
    Ok(indexed)
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
}
