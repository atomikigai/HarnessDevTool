//! Query execution, decoding, and lightweight cancellation registry.
//!
//! Cancellation note: engine-specific cancellation handles are not exposed
//! uniformly. We register each running query under a `query_id` so the
//! manager can attempt a best-effort cancel via an auxiliary connection
//! (`pg_cancel_backend` / `KILL QUERY` / SQLite has no public hook, so we
//! return Unsupported on SQLite). The registry stores the engine-specific
//! backend pid when known; cancel becomes a no-op (ok=false) when not known.

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::{Column, Row, TypeInfo};

use crate::error::{DbError, DbResult};
use crate::pool::DbPool;
use crate::types::{Engine, QueryResult, ResultColumn};
use crate::value::{decode_mysql_row, decode_postgres_row, decode_sqlite_row};

#[derive(Debug, Clone)]
pub struct RunningQuery {
    pub engine: Engine,
    pub backend_pid: Option<i64>,
}

#[derive(Debug, Default)]
pub struct QueryRegistry {
    pub inner: DashMap<String, RunningQuery>,
}

impl QueryRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

/// Run a SQL statement, returning a fully-buffered `QueryResult` capped by
/// `(page, page_size)`.
pub async fn run(
    pool: &DbPool,
    sql: &str,
    page_size: usize,
    page: usize,
    registry: &QueryRegistry,
) -> DbResult<QueryResult> {
    let engine = pool.engine();
    let query_id = uuid::Uuid::new_v4().to_string();
    registry.inner.insert(
        query_id.clone(),
        RunningQuery {
            engine,
            backend_pid: None,
        },
    );

    let start = Instant::now();
    let trimmed = sql.trim_end_matches(';').trim();
    let is_select = leading_keyword(trimmed).eq_ignore_ascii_case("SELECT");
    let effective = if is_select {
        let offset = page.saturating_mul(page_size);
        format!(
            "SELECT * FROM ({trimmed}) AS _harness_sub LIMIT {} OFFSET {}",
            page_size + 1,
            offset
        )
    } else {
        sql.to_string()
    };

    let result = (async {
        let (columns, mut decoded) = match pool {
            DbPool::Sqlite(p) => {
                let rows = sqlx::query(&effective)
                    .fetch_all(p)
                    .await
                    .map_err(DbError::from)?;
                let cols = first_row_cols_sqlite(rows.first());
                let dec: Vec<Vec<crate::Value>> = rows.iter().map(decode_sqlite_row).collect();
                (cols, dec)
            }
            DbPool::Postgres(p) => {
                let rows = sqlx::query(&effective)
                    .fetch_all(p)
                    .await
                    .map_err(DbError::from)?;
                let cols = first_row_cols_pg(rows.first());
                let dec: Vec<Vec<crate::Value>> = rows.iter().map(decode_postgres_row).collect();
                (cols, dec)
            }
            DbPool::Mysql(p) => {
                let rows = sqlx::query(&effective)
                    .fetch_all(p)
                    .await
                    .map_err(DbError::from)?;
                let cols = first_row_cols_mysql(rows.first());
                let dec: Vec<Vec<crate::Value>> = rows.iter().map(decode_mysql_row).collect();
                (cols, dec)
            }
        };

        let mut truncated = false;
        if decoded.len() > page_size {
            decoded.truncate(page_size);
            truncated = true;
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        Ok::<_, DbError>(QueryResult {
            columns,
            rows: decoded,
            total_rows: None,
            truncated,
            elapsed_ms,
            query_id: query_id.clone(),
        })
    })
    .await;

    registry.inner.remove(&query_id);
    result
}

fn first_row_cols_sqlite(r: Option<&sqlx::sqlite::SqliteRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn first_row_cols_pg(r: Option<&sqlx::postgres::PgRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn first_row_cols_mysql(r: Option<&sqlx::mysql::MySqlRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Return the leading SQL keyword.
pub fn leading_keyword(sql: &str) -> String {
    let mut s = sql.trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            if let Some(nl) = rest.find('\n') {
                s = rest[nl + 1..].trim_start();
                continue;
            } else {
                return String::new();
            }
        }
        if let Some(rest) = s.strip_prefix("/*") {
            if let Some(end) = rest.find("*/") {
                s = rest[end + 2..].trim_start();
                continue;
            } else {
                return String::new();
            }
        }
        break;
    }
    s.split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_keyword_handles_comments() {
        assert_eq!(leading_keyword("SELECT 1"), "SELECT");
        assert_eq!(leading_keyword("  select * from t"), "select");
        assert_eq!(leading_keyword("-- hello\nUPDATE t SET x=1"), "UPDATE");
        assert_eq!(leading_keyword("/* c */ DELETE FROM t"), "DELETE");
        assert_eq!(leading_keyword(""), "");
    }
}
