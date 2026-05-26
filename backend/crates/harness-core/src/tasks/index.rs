//! Derived SQLite index over the canonical TOML files. Reconstructable at any
//! time from the on-disk task files.

use std::path::Path;

use rusqlite::{params, Connection};

use super::model::{ListFilters, Task};
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
                status      TEXT NOT NULL,
                assignee    TEXT,
                updated_at  TEXT NOT NULL,
                labels      TEXT NOT NULL,
                blocked_by  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_assignee ON tasks(assignee);
            "#,
        )?;
        Ok(Self { conn })
    }

    pub fn upsert(&self, t: &Task) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO tasks(id,status,assignee,updated_at,labels,blocked_by)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET
                status=excluded.status,
                assignee=excluded.assignee,
                updated_at=excluded.updated_at,
                labels=excluded.labels,
                blocked_by=excluded.blocked_by",
            params![
                t.id,
                t.status.as_str(),
                t.assignee.as_deref(),
                t.updated_at.to_rfc3339(),
                serde_json::to_string(&t.labels).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&t.blocked_by).unwrap_or_else(|_| "[]".into()),
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
