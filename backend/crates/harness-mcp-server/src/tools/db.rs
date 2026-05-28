//! `db.*` MCP tools. Thin wrappers around `module_db::Manager`.
//!
//! Approval policy (informational — enforcement lives in the harness's
//! approval layer): `db_query` is gated on the leading SQL keyword being
//! `SELECT` (or `EXPLAIN`/`SHOW`/`WITH`). Other keywords are flagged
//! `requires_approval: true` in the response so the harness can prompt.

use std::sync::OnceLock;

use module_db::Manager;
use serde_json::{json, Value};
use tokio::runtime::Runtime;

use crate::tools::wrap_error;

/// Lazily-initialized shared tokio runtime — the MCP server is otherwise
/// fully synchronous; `db.*` ops need an async context.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

const READ_ONLY_KEYWORDS: &[&str] = &["SELECT", "EXPLAIN", "SHOW", "WITH", "DESCRIBE", "DESC"];

fn is_read_only(sql: &str) -> bool {
    let kw = module_db::__leading_keyword(sql);
    READ_ONLY_KEYWORDS
        .iter()
        .any(|w| kw.eq_ignore_ascii_case(w))
}

pub fn query(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let sql = str_arg(args, "sql")?;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let approved = args
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_read_only(sql) && !approved {
        return Ok(json!({
            "requires_approval": true,
            "reason": "non-SELECT statement; pass `approved: true` after user confirms",
            "leading_keyword": module_db::__leading_keyword(sql),
        }));
    }
    let result = runtime()
        .block_on(mgr.query_run(connection_id, None, sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

pub fn schema(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let tree = runtime()
        .block_on(mgr.schema_tree(connection_id, database))
        .map_err(|e| e.to_string())?;
    Ok(json!(tree))
}

pub fn explain(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let sql = str_arg(args, "sql")?;
    let res = runtime()
        .block_on(mgr.explain(connection_id, sql))
        .map_err(|e| e.to_string())?;
    Ok(json!(res))
}

// Keep wrap_error reachable so the dispatcher's `use ... wrap_error` is happy
// even when no error branch fires here.
#[allow(dead_code)]
fn _keep_imports() -> Value {
    wrap_error("noop")
}
