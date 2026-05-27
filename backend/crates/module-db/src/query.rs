//! Query execution, decoding, and lightweight cancellation registry.
//!
//! Cancellation note: sqlx-Any does not expose engine-specific cancellation
//! handles uniformly. We register each running query under a `query_id` so
//! the manager can attempt a best-effort cancel via an auxiliary connection
//! (`pg_cancel_backend` / `KILL QUERY` / SQLite has no public hook from
//! sqlx-Any, so we return Unsupported on SQLite). The registry stores the
//! engine-specific backend pid when known; cancel becomes a no-op (ok=false)
//! when not known.

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::any::AnyRow;
use sqlx::{AnyPool, Column, Executor, Row, TypeInfo, ValueRef};

use crate::error::{DbError, DbResult};
use crate::types::{Engine, QueryResult, ResultColumn};
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct RunningQuery {
    pub engine: Engine,
    pub backend_pid: Option<i64>, // pg backend pid or mysql connection id
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

/// Decode a single sqlx Any column into our `Value` enum.
///
/// sqlx-Any only exposes a small set of native types via `try_get_raw`. We
/// probe a few candidate types in order, falling back to the JSON-friendly
/// `Text` representation. Per-engine subtleties (numerics, decimals, blobs)
/// are simplified: anything we cannot decode natively comes back as Text.
fn decode_cell(row: &AnyRow, idx: usize) -> Value {
    let value_ref = match row.try_get_raw(idx) {
        Ok(v) => v,
        Err(_) => return Value::Null,
    };
    if value_ref.is_null() {
        return Value::Null;
    }
    let type_name = value_ref.type_info().name().to_string();

    // Try typed extractions in a sensible order. We deliberately stay narrow
    // — sqlx-Any decoders are restrictive; anything fancy falls through to
    // Text via the type's stringification.
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Value::Bool(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Int(v);
    }
    if let Ok(v) = row.try_get::<i32, _>(idx) {
        return Value::Int(v as i64);
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return Value::Float(v);
    }
    if let Ok(v) = row.try_get::<f32, _>(idx) {
        return Value::Float(v as f64);
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        // Heuristic: if the column type screams "timestamp"/"date", tag it.
        let upper = type_name.to_ascii_uppercase();
        if upper.contains("TIMESTAMP") || upper.contains("DATETIME") {
            return Value::datetime(v);
        }
        if upper.contains("DATE") {
            return Value::date(v);
        }
        if upper.contains("TIME") {
            return Value::time(v);
        }
        if upper.contains("JSON") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&v) {
                return Value::json(parsed);
            }
        }
        if upper.contains("DECIMAL") || upper.contains("NUMERIC") {
            return Value::decimal(v);
        }
        return Value::Text(v);
    }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
        return Value::bytes(&v);
    }
    // Last-resort: opaque marker so the frontend sees *something*.
    Value::Text(format!("<unsupported:{type_name}>"))
}

/// Decode all cells of a row into a Vec<Value>.
pub fn decode_row(row: &AnyRow) -> Vec<Value> {
    (0..row.columns().len()).map(|i| decode_cell(row, i)).collect()
}

/// Run a SQL statement, returning a fully-buffered `QueryResult` capped by
/// `(page, page_size)`. Pagination is implemented client-side (offset/limit
/// appended to a wrapper subquery) only when the query looks like a single
/// SELECT; otherwise we run as-is and return all rows (truncated to page_size
/// if there are too many).
pub async fn run(
    pool: &AnyPool,
    engine: Engine,
    sql: &str,
    page_size: usize,
    page: usize,
    registry: &QueryRegistry,
) -> DbResult<QueryResult> {
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
        format!("SELECT * FROM ({trimmed}) AS _harness_sub LIMIT {} OFFSET {}", page_size + 1, offset)
    } else {
        sql.to_string()
    };

    let result = (async {
        let rows = pool
            .fetch_all(sqlx::query(&effective))
            .await
            .map_err(DbError::from)?;

        let columns: Vec<ResultColumn> = if let Some(r) = rows.first() {
            r.columns()
                .iter()
                .map(|c| ResultColumn {
                    name: c.name().to_string(),
                    r#type: c.type_info().name().to_string(),
                })
                .collect()
        } else {
            Vec::new()
        };

        let mut truncated = false;
        let mut decoded: Vec<Vec<Value>> = rows.iter().map(decode_row).collect();
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

/// Return the leading SQL keyword (after stripping `--` and `/* */` comments
/// and whitespace) of a statement. Used to gate write tools in MCP and decide
/// if pagination wrapping is safe.
pub fn leading_keyword(sql: &str) -> String {
    let mut s = sql.trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            // line comment to EOL
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
