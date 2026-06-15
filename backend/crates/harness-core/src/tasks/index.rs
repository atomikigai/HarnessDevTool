//! Derived SQLite index over the canonical TOML files. Reconstructable at any
//! time from the on-disk task files.

use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::model::{ListFilters, Task, TaskStatus, TaskSummary};
use crate::Error;

pub struct Index {
    conn: Connection,
}

impl Index {
    pub fn open(db_path: &Path) -> Result<Self, Error> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL DEFAULT '',
                status      TEXT NOT NULL,
                assignee    TEXT,
                updated_at  TEXT NOT NULL,
                labels      TEXT NOT NULL,
                blocked_by  TEXT NOT NULL,
                depends_on  TEXT NOT NULL DEFAULT '[]',
                acceptance_count INTEGER NOT NULL DEFAULT 0,
                artifact_count INTEGER NOT NULL DEFAULT 0,
                latest_handoff_status TEXT,
                latest_handoff_at INTEGER,
                summary_preview TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_assignee ON tasks(assignee);
            "#,
        )?;
        ensure_column(&conn, "title", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "depends_on", "TEXT NOT NULL DEFAULT '[]'")?;
        ensure_column(&conn, "acceptance_count", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "artifact_count", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "latest_handoff_status", "TEXT")?;
        ensure_column(&conn, "latest_handoff_at", "INTEGER")?;
        ensure_column(&conn, "summary_preview", "TEXT NOT NULL DEFAULT ''")?;
        Ok(Self { conn })
    }

    pub fn upsert(
        &self,
        t: &Task,
        latest_handoff_status: Option<&str>,
        latest_handoff_at: Option<i64>,
    ) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO tasks(
                id,title,status,assignee,updated_at,labels,blocked_by,depends_on,
                acceptance_count,artifact_count,latest_handoff_status,latest_handoff_at,summary_preview
             )
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title,
                status=excluded.status,
                assignee=excluded.assignee,
                updated_at=excluded.updated_at,
                labels=excluded.labels,
                blocked_by=excluded.blocked_by,
                depends_on=excluded.depends_on,
                acceptance_count=excluded.acceptance_count,
                artifact_count=excluded.artifact_count,
                latest_handoff_status=excluded.latest_handoff_status,
                latest_handoff_at=excluded.latest_handoff_at,
                summary_preview=excluded.summary_preview",
            params![
                t.id,
                t.title,
                t.status.as_str(),
                t.assignee.as_deref(),
                t.updated_at.to_rfc3339(),
                serde_json::to_string(&t.labels).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&t.blocked_by).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&t.blocked_by).unwrap_or_else(|_| "[]".into()),
                t.acceptance.checks.len() as i64,
                t.artifacts.metadata.len() as i64,
                latest_handoff_status,
                latest_handoff_at,
                summary_preview(t),
            ],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete(&self, id: &str) -> Result<(), Error> {
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_ids(&self, filters: &ListFilters) -> Result<Vec<String>, Error> {
        let mut sql = "SELECT id FROM tasks WHERE 1=1".to_string();
        let mut args: Vec<String> = vec![];
        if let Some(s) = filters.status {
            sql.push_str(" AND status = ?");
            args.push(s.as_str().to_string());
        }
        if let Some(a) = &filters.assignee {
            sql.push_str(" AND assignee = ?");
            args.push(a.clone());
        }
        if let Some(l) = &filters.label {
            sql.push_str(" AND labels LIKE ?");
            args.push(format!("%\"{}\"%", l));
        }
        sql.push_str(" ORDER BY id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = vec![];
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_summaries(&self, filters: &ListFilters) -> Result<Vec<TaskSummary>, Error> {
        let mut sql = "SELECT id,title,status,assignee,updated_at,labels,blocked_by,depends_on,\
                       acceptance_count,artifact_count,latest_handoff_status,latest_handoff_at,\
                       summary_preview FROM tasks WHERE 1=1"
            .to_string();
        let mut args: Vec<String> = vec![];
        if let Some(s) = filters.status {
            sql.push_str(" AND status = ?");
            args.push(s.as_str().to_string());
        }
        if let Some(a) = &filters.assignee {
            sql.push_str(" AND assignee = ?");
            args.push(a.clone());
        }
        if let Some(l) = &filters.label {
            sql.push_str(" AND labels LIKE ?");
            args.push(format!("%\"{}\"%", l));
        }
        sql.push_str(" ORDER BY id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_summary)?;
        let mut out = vec![];
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn all_ids(&self) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare("SELECT id FROM tasks ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn clear(&self) -> Result<(), Error> {
        self.conn.execute("DELETE FROM tasks", [])?;
        Ok(())
    }
}

fn ensure_column(conn: &Connection, name: &str, definition: &str) -> Result<(), Error> {
    let mut stmt = conn.prepare("PRAGMA table_info(tasks)")?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == name {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE tasks ADD COLUMN {name} {definition}"),
        [],
    )?;
    Ok(())
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskSummary> {
    let status_raw: String = row.get(2)?;
    let updated_raw: String = row.get(4)?;
    let labels_raw: String = row.get(5)?;
    let blocked_raw: String = row.get(6)?;
    let depends_raw: String = row.get(7)?;
    let status = TaskStatus::from_str(&status_raw).map_err(|e| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e,
        )))
    })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(TaskSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        status,
        assignee: row.get(3)?,
        updated_at,
        labels: serde_json::from_str(&labels_raw).unwrap_or_default(),
        blocked_by: serde_json::from_str(&blocked_raw).unwrap_or_default(),
        depends_on: serde_json::from_str(&depends_raw).unwrap_or_default(),
        acceptance_count: row.get::<_, i64>(8)?.max(0) as usize,
        artifact_count: row.get::<_, i64>(9)?.max(0) as usize,
        latest_handoff_status: row.get(10)?,
        latest_handoff_at: row.get(11)?,
        summary_preview: row.get(12)?,
    })
}

fn summary_preview(task: &Task) -> String {
    let mut parts = Vec::new();
    if let Some(brief) = task.brief.as_ref() {
        if !brief.objective.trim().is_empty() {
            parts.push(brief.objective.trim());
        }
        if !brief.expected_result.trim().is_empty() {
            parts.push(brief.expected_result.trim());
        }
    }
    if parts.is_empty() {
        parts.push(task.title.trim());
    }
    truncate_preview(&parts.join(" | "))
}

fn truncate_preview(text: &str) -> String {
    const MAX_CHARS: usize = 220;
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_CHARS {
        return compact;
    }
    compact.chars().take(MAX_CHARS).collect::<String>() + "..."
}
