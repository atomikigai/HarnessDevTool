//! `db.*` MCP tools. Thin wrappers around `module_db::Manager`.
//!
//! Approval policy (informational — enforcement lives in the harness's
//! approval layer): `db_query` is gated on the leading SQL keyword being
//! `SELECT` (or `EXPLAIN`/`SHOW`). Other keywords are flagged
//! `requires_approval: true` in the response so the harness can prompt.

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;
use module_db::{
    Engine, ExportFormat, ExportRequest, ExportScope, ExportTarget, Manager, QueryResult,
    SelectRequest, Table,
};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use zip::write::FileOptions;

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

fn opt_limit(args: &Value, default: usize, max: usize) -> usize {
    args.get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
        .clamp(1, max)
}

fn opt_string_array(args: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(items) = value.as_array() else {
        return Err(format!("arg `{key}` must be an array of strings"));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Some(s) = item.as_str() else {
            return Err(format!("arg `{key}` must be an array of strings"));
        };
        out.push(s.to_string());
    }
    Ok(Some(out))
}

fn required_string_array(args: &Value, key: &str) -> Result<Vec<String>, String> {
    opt_string_array(args, key)?.ok_or_else(|| format!("missing arg `{key}`"))
}

fn ident_quote(engine: Engine) -> char {
    match engine {
        Engine::Mysql => '`',
        _ => '"',
    }
}

fn quote_ident(engine: Engine, ident: &str) -> Result<String, String> {
    if ident.is_empty() {
        return Err("identifier must not be empty".into());
    }
    if ident.contains('\0') {
        return Err("identifier must not contain NUL".into());
    }
    let quote = ident_quote(engine);
    let escaped = ident.replace(quote, &format!("{quote}{quote}"));
    Ok(format!("{quote}{escaped}{quote}"))
}

fn qualify(engine: Engine, schema: Option<&str>, table: &str) -> Result<String, String> {
    let qtable = quote_ident(engine, table)?;
    match schema {
        Some(schema) if !schema.trim().is_empty() => {
            Ok(format!("{}.{}", quote_ident(engine, schema)?, qtable))
        }
        _ => Ok(qtable),
    }
}

fn sql_string(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "''"))
}

fn approved_arg(args: &Value) -> bool {
    args.get("approved")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn require_approved(args: &Value, action: &str) -> Result<(), Value> {
    if approved_arg(args) {
        Ok(())
    } else {
        Err(json!({
            "requires_approval": true,
            "reason": format!("{action} is a mutating/destructive database operation; create/confirm backup when appropriate and pass approved=true after explicit user confirmation")
        }))
    }
}

fn value_map_arg(args: &Value, key: &str) -> Result<HashMap<String, module_db::Value>, String> {
    let value = args
        .get(key)
        .ok_or_else(|| format!("missing arg `{key}`"))?
        .clone();
    serde_json::from_value(value).map_err(|e| format!("arg `{key}`: {e}"))
}

fn pk_list_args(args: &Value) -> Result<Vec<HashMap<String, module_db::Value>>, String> {
    if let Some(pks) = args.get("pks") {
        return serde_json::from_value(pks.clone()).map_err(|e| format!("arg `pks`: {e}"));
    }
    Ok(vec![value_map_arg(args, "pk")?])
}

fn validate_table_columns(
    mgr: &Manager,
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
) -> Result<String, String> {
    let tree = runtime()
        .block_on(mgr.schema_tree_filtered(connection_id, database, schema, Some(table)))
        .map_err(|e| e.to_string())?;
    let matched = tree
        .schemas
        .iter()
        .find(|schema_tree| schema.map(|s| s == schema_tree.name).unwrap_or(true))
        .and_then(|schema_tree| {
            schema_tree
                .tables
                .iter()
                .find(|candidate| candidate.name == table)
                .map(|table_info| (schema_tree.name.clone(), table_info))
        });
    let Some((schema_name, table_info)) = matched else {
        return Err(format!("table not found: {table}"));
    };
    for column in columns {
        if !table_info
            .columns
            .iter()
            .any(|candidate| candidate.name == *column)
        {
            return Err(format!(
                "unknown column `{column}` on {schema_name}.{table}"
            ));
        }
    }
    Ok(schema_name)
}

fn table_metadata(
    mgr: &Manager,
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> Result<(String, Table), String> {
    let tree = runtime()
        .block_on(mgr.schema_tree_filtered(connection_id, database, schema, Some(table)))
        .map_err(|e| e.to_string())?;
    tree.schemas
        .into_iter()
        .find(|schema_tree| schema.map(|s| s == schema_tree.name).unwrap_or(true))
        .and_then(|schema_tree| {
            schema_tree
                .tables
                .into_iter()
                .find(|candidate| candidate.name == table)
                .map(|table| (schema_tree.name, table))
        })
        .ok_or_else(|| format!("table not found: {table}"))
}

const READ_ONLY_KEYWORDS: &[&str] = &["SELECT", "EXPLAIN", "SHOW", "DESCRIBE", "DESC"];

fn is_read_only(sql: &str) -> bool {
    if has_multiple_statements(sql) {
        return false;
    }
    let kw = module_db::__leading_keyword(sql);
    READ_ONLY_KEYWORDS
        .iter()
        .any(|w| kw.eq_ignore_ascii_case(w))
}

fn has_multiple_statements(sql: &str) -> bool {
    let mut chars = sql.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_single {
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_single = false;
                }
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    in_double = false;
                }
            }
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '-' if chars.peek() == Some(&'-') => {
                chars.next();
                in_line_comment = true;
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block_comment = true;
            }
            ';' if chars.clone().any(|c| !c.is_whitespace()) => return true,
            _ => {}
        }
    }
    false
}

pub fn query(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
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
        .block_on(mgr.query_run(connection_id, database, sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

pub fn select(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let req: SelectRequest =
        serde_json::from_value(args.clone()).map_err(|e| format!("db_select args: {e}"))?;
    let response = runtime()
        .block_on(mgr.structured_select(connection_id, database, req))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "sql": response.sql,
        "result": response.result,
    }))
}

pub fn validate_query(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    if let Some(sql) = opt_str(args, "sql") {
        let leading_keyword = module_db::__leading_keyword(sql);
        let read_only = is_read_only(sql);
        let multiple_statements = has_multiple_statements(sql);
        let has_limit = sql.to_ascii_lowercase().contains(" limit ");
        return Ok(json!({
            "ok": read_only && !multiple_statements,
            "mode": "sql",
            "connection": connection_id,
            "database": database,
            "read_only": read_only,
            "multiple_statements": multiple_statements,
            "leading_keyword": leading_keyword,
            "warnings": if read_only && !has_limit && leading_keyword.eq_ignore_ascii_case("SELECT") {
                vec!["exploratory SELECT has no explicit LIMIT"]
            } else {
                Vec::<&str>::new()
            }
        }));
    }

    let req: SelectRequest =
        serde_json::from_value(args.clone()).map_err(|e| format!("structured select args: {e}"))?;
    let tree = runtime()
        .block_on(mgr.schema_tree_filtered(
            connection_id,
            database,
            req.schema.as_deref(),
            Some(req.table.as_str()),
        ))
        .map_err(|e| e.to_string())?;
    let table = tree
        .schemas
        .iter()
        .find(|schema| {
            req.schema
                .as_deref()
                .map(|s| s == schema.name)
                .unwrap_or(true)
        })
        .and_then(|schema| {
            schema
                .tables
                .iter()
                .find(|table| table.name == req.table)
                .map(|table| (schema.name.clone(), table))
        });
    let Some((schema_name, table)) = table else {
        return Ok(json!({
            "ok": false,
            "mode": "structured_select",
            "connection": connection_id,
            "database": database,
            "schema": req.schema,
            "table": req.table,
            "errors": ["table not found"]
        }));
    };
    let valid_columns = table
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut errors = Vec::new();
    if let Some(columns) = req.columns.as_ref() {
        if columns.is_empty() {
            errors.push("columns must not be empty".to_string());
        }
        for column in columns {
            if !valid_columns.contains(column.as_str()) {
                errors.push(format!("unknown selected column: {column}"));
            }
        }
    }
    for filter in &req.filters {
        if !valid_columns.contains(filter.column.as_str()) {
            errors.push(format!("unknown filter column: {}", filter.column));
        }
    }
    for order in &req.order_by {
        if !valid_columns.contains(order.column.as_str()) {
            errors.push(format!("unknown order_by column: {}", order.column));
        }
    }
    Ok(json!({
        "ok": errors.is_empty(),
        "mode": "structured_select",
        "connection": connection_id,
        "database": database,
        "schema": schema_name,
        "table": req.table,
        "errors": errors,
        "warnings": if req.limit.unwrap_or(20) > 100 {
            vec!["large limit requested; db_select caps at 500"]
        } else {
            Vec::<&str>::new()
        }
    }))
}

pub fn schema(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = opt_str(args, "table");
    let tree = runtime()
        .block_on(mgr.schema_tree_filtered(connection_id, database, schema, table))
        .map_err(|e| e.to_string())?;
    let tree_value = json!(tree);
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema_filter": schema,
        "table_filter": table,
        "filtered": schema.is_some() || table.is_some(),
        "schemas": tree_value["schemas"].clone(),
        "result": tree_value,
    }))
}

pub fn table_info(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let tree = runtime()
        .block_on(mgr.schema_tree_filtered(connection_id, database, schema, Some(table)))
        .map_err(|e| e.to_string())?;
    let matched_schema = tree
        .schemas
        .iter()
        .find(|schema_tree| schema.map(|s| s == schema_tree.name).unwrap_or(true))
        .and_then(|schema_tree| {
            schema_tree
                .tables
                .iter()
                .find(|candidate| candidate.name == table)
                .map(|table_info| (schema_tree.name.clone(), table_info.clone()))
        });
    let Some((schema_name, table_info)) = matched_schema else {
        return Ok(json!({
            "ok": false,
            "connection": connection_id,
            "database": database,
            "schema": schema,
            "table": table,
            "error": "table not found"
        }));
    };
    Ok(json!({
        "ok": true,
        "connection": connection_id,
        "database": database,
        "schema": schema_name,
        "table": table_info,
    }))
}

pub fn search_tables(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let q = str_arg(args, "q")?.trim();
    if q.is_empty() {
        return Err("arg `q` must not be empty".into());
    }
    let limit = opt_limit(args, 20, 100);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let needle = sql_string(&format!("%{}%", q.to_ascii_lowercase()));
    let sql = match conn.engine {
        Engine::Sqlite => format!(
            "SELECT 'main' AS schema_name, m.name AS table_name, m.type AS object_type, NULL AS column_name \
             FROM sqlite_master m \
             WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' AND lower(m.name) LIKE {needle} \
             ORDER BY m.name"
        ),
        Engine::Postgres => format!(
            "SELECT c.table_schema AS schema_name, c.table_name, t.table_type AS object_type, c.column_name \
             FROM information_schema.columns c \
             JOIN information_schema.tables t ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
             WHERE c.table_schema NOT IN ('pg_catalog','information_schema') \
               AND (lower(c.table_schema) LIKE {needle} OR lower(c.table_name) LIKE {needle} OR lower(c.column_name) LIKE {needle}) \
             ORDER BY c.table_schema, c.table_name, c.ordinal_position"
        ),
        Engine::Mysql => format!(
            "SELECT c.table_schema AS schema_name, c.table_name, t.table_type AS object_type, c.column_name \
             FROM information_schema.columns c \
             JOIN information_schema.tables t ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
             WHERE c.table_schema = DATABASE() \
               AND (lower(c.table_name) LIKE {needle} OR lower(c.column_name) LIKE {needle}) \
             ORDER BY c.table_schema, c.table_name, c.ordinal_position"
        ),
    };
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "q": q,
        "limit": limit,
        "result": result,
    }))
}

pub fn sample(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let limit = opt_limit(args, 20, 100);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let columns = opt_string_array(args, "columns")?;
    let select_list = match columns.as_ref() {
        Some(columns) if columns.is_empty() => return Err("arg `columns` must not be empty".into()),
        Some(columns) => columns
            .iter()
            .map(|column| quote_ident(conn.engine, column))
            .collect::<Result<Vec<_>, _>>()?
            .join(", "),
        None => "*".into(),
    };
    let qualified = qualify(conn.engine, schema, table)?;
    let sql = format!("SELECT {select_list} FROM {qualified}");
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "limit": limit,
        "sql": sql,
        "result": result,
    }))
}

pub fn count(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let qualified = qualify(conn.engine, schema, table)?;
    let sql = format!("SELECT count(*) AS total FROM {qualified}");
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, 1, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "sql": sql,
        "result": result,
    }))
}

pub fn distinct_values(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let column = str_arg(args, "column")?;
    let limit = opt_limit(args, 50, 200);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let schema_name = validate_table_columns(
        mgr,
        connection_id,
        database,
        schema,
        table,
        &[column.to_string()],
    )?;
    let qualified = qualify(conn.engine, Some(&schema_name), table)?;
    let qcol = quote_ident(conn.engine, column)?;
    let sql = format!(
        "SELECT {qcol} AS value, count(*) AS frequency \
         FROM {qualified} \
         GROUP BY {qcol} \
         ORDER BY frequency DESC, value \
         LIMIT {limit}"
    );
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema_name,
        "table": table,
        "column": column,
        "limit": limit,
        "sql": sql,
        "result": result,
    }))
}

pub fn find_rows(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let q = str_arg(args, "q")?.trim();
    if q.is_empty() {
        return Err("arg `q` must not be empty".into());
    }
    let search_columns = required_string_array(args, "columns")?;
    if search_columns.is_empty() {
        return Err("arg `columns` must not be empty".into());
    }
    let return_columns = opt_string_array(args, "return_columns")?;
    let limit = opt_limit(args, 20, 100);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let mut columns_to_validate = search_columns.clone();
    if let Some(return_columns) = return_columns.as_ref() {
        if return_columns.is_empty() {
            return Err("arg `return_columns` must not be empty".into());
        }
        columns_to_validate.extend(return_columns.iter().cloned());
    }
    let schema_name = validate_table_columns(
        mgr,
        connection_id,
        database,
        schema,
        table,
        &columns_to_validate,
    )?;
    let qualified = qualify(conn.engine, Some(&schema_name), table)?;
    let select_list = match return_columns.as_ref() {
        Some(columns) => columns
            .iter()
            .map(|column| quote_ident(conn.engine, column))
            .collect::<Result<Vec<_>, _>>()?
            .join(", "),
        None => "*".into(),
    };
    let pattern = sql_string(&format!("%{q}%"));
    let like_op = if conn.engine == Engine::Postgres {
        "ILIKE"
    } else {
        "LIKE"
    };
    let predicates = search_columns
        .iter()
        .map(|column| {
            Ok(format!(
                "{} {like_op} {pattern}",
                quote_ident(conn.engine, column)?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(" OR ");
    let sql = format!("SELECT {select_list} FROM {qualified} WHERE ({predicates}) LIMIT {limit}");
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema_name,
        "table": table,
        "q": q,
        "columns": search_columns,
        "return_columns": return_columns,
        "limit": limit,
        "sql": sql,
        "result": result,
    }))
}

pub fn aggregate(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let group_by = opt_string_array(args, "group_by")?.unwrap_or_default();
    let metrics = args
        .get("metrics")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "missing or non-array arg: metrics".to_string())?;
    if metrics.is_empty() {
        return Err("arg `metrics` must not be empty".into());
    }
    if metrics.len() > 10 {
        return Err("too many metrics; max 10".into());
    }
    let limit = opt_limit(args, 50, 500);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;

    let mut columns_to_validate = group_by.clone();
    for metric in metrics {
        if let Some(column) = metric.get("column").and_then(|value| value.as_str()) {
            columns_to_validate.push(column.to_string());
        }
    }
    let schema_name = validate_table_columns(
        mgr,
        connection_id,
        database,
        schema,
        table,
        &columns_to_validate,
    )?;
    let qualified = qualify(conn.engine, Some(&schema_name), table)?;
    let mut select_parts = group_by
        .iter()
        .map(|column| quote_ident(conn.engine, column))
        .collect::<Result<Vec<_>, _>>()?;
    let mut metric_aliases = Vec::new();
    for metric in metrics {
        let fn_name = metric
            .get("fn")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "metric missing string field `fn`".to_string())?;
        let alias = metric
            .get("as")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "metric missing string field `as`".to_string())?;
        let qalias = quote_ident(conn.engine, alias)?;
        let expr = match fn_name {
            "count" => match metric.get("column").and_then(|value| value.as_str()) {
                Some(column) => format!("count({})", quote_ident(conn.engine, column)?),
                None => "count(*)".into(),
            },
            "count_distinct" => {
                let column = metric
                    .get("column")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| "count_distinct metric requires `column`".to_string())?;
                format!("count(DISTINCT {})", quote_ident(conn.engine, column)?)
            }
            "sum" | "avg" | "min" | "max" => {
                let column = metric
                    .get("column")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| format!("{fn_name} metric requires `column`"))?;
                format!("{fn_name}({})", quote_ident(conn.engine, column)?)
            }
            other => return Err(format!("unsupported aggregate fn: {other}")),
        };
        select_parts.push(format!("{expr} AS {qalias}"));
        metric_aliases.push(alias.to_string());
    }
    let select_list = select_parts.join(", ");
    let mut sql = format!("SELECT {select_list} FROM {qualified}");
    if !group_by.is_empty() {
        let group_list = group_by
            .iter()
            .map(|column| quote_ident(conn.engine, column))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        sql.push_str(&format!(" GROUP BY {group_list}"));
    }
    if let Some(first_alias) = metric_aliases.first() {
        sql.push_str(&format!(
            " ORDER BY {} DESC",
            quote_ident(conn.engine, first_alias)?
        ));
    }
    sql.push_str(&format!(" LIMIT {limit}"));
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema_name,
        "table": table,
        "group_by": group_by,
        "limit": limit,
        "sql": sql,
        "result": result,
    }))
}

pub fn extract_enriched(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let limit = opt_limit(args, 20, 200);
    let include_fk_labels = args
        .get("include_fk_labels")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let preferred_labels = opt_string_array(args, "label_columns")?.unwrap_or_else(|| {
        vec![
            "name".into(),
            "title".into(),
            "label".into(),
            "display_name".into(),
            "email".into(),
            "code".into(),
            "slug".into(),
            "description".into(),
        ]
    });

    let mut select_args = args.clone();
    if let Some(obj) = select_args.as_object_mut() {
        obj.insert("limit".into(), json!(limit));
    }
    let base = select(mgr, &select_args)?;
    let sql = base["sql"].clone();
    let result: QueryResult = serde_json::from_value(base["result"].clone())
        .map_err(|e| format!("db_extract_enriched result decode: {e}"))?;
    let mut rows = query_result_objects(&result);
    let mut enrichments = Vec::new();

    if include_fk_labels && !rows.is_empty() {
        let (schema_name, source_table) =
            table_metadata(mgr, connection_id, database, schema, table)?;
        let tree = runtime()
            .block_on(mgr.schema_tree(connection_id, database))
            .map_err(|e| e.to_string())?;
        for column in &source_table.columns {
            if !column.name.ends_with("_id")
                && !source_table
                    .foreign_keys
                    .iter()
                    .any(|fk| fk.cols.contains(&column.name))
            {
                continue;
            }
            let Some((ref_schema, ref_table, ref_column)) =
                resolve_reference(&tree, &schema_name, &source_table, &column.name)
            else {
                continue;
            };
            let Some(label_column) = choose_label_column(&ref_table, &preferred_labels) else {
                continue;
            };
            let ids = rows
                .iter()
                .filter_map(|row| row.get(&column.name).cloned())
                .filter(|value| !value.is_null())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                continue;
            }
            let conn = mgr
                .connections_get(connection_id)
                .map_err(|e| e.to_string())?;
            let qualified = qualify(conn.engine, Some(&ref_schema), &ref_table.name)?;
            let q_ref_col = quote_ident(conn.engine, &ref_column)?;
            let q_label_col = quote_ident(conn.engine, &label_column)?;
            let id_list = ids
                .iter()
                .map(json_value_sql_literal)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            let lookup_sql = format!(
                "SELECT {q_ref_col} AS id, {q_label_col} AS label FROM {qualified} WHERE {q_ref_col} IN ({id_list}) LIMIT 1000"
            );
            let lookup = runtime()
                .block_on(mgr.query_run(connection_id, database, &lookup_sql, None, 1000, 0))
                .map_err(|e| e.to_string())?;
            let labels = lookup
                .rows
                .iter()
                .filter_map(|row| {
                    if row.len() < 2 {
                        return None;
                    }
                    Some((serde_json::to_string(&row[0]).ok()?, row[1].to_json()))
                })
                .collect::<HashMap<_, _>>();
            let label_field = format!("{}_label", column.name);
            for row in &mut rows {
                if let Some(value) = row.get(&column.name) {
                    if let Ok(key) = serde_json::to_string(value) {
                        if let Some(label) = labels.get(&key) {
                            row.insert(label_field.clone(), label.clone());
                        }
                    }
                }
            }
            enrichments.push(json!({
                "source_column": column.name,
                "label_field": label_field,
                "ref_schema": ref_schema,
                "ref_table": ref_table.name,
                "ref_column": ref_column,
                "label_column": label_column,
                "lookup_sql": lookup_sql,
            }));
        }
    }

    Ok(json!({
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "sql": sql,
        "rows": rows,
        "enrichments": enrichments,
    }))
}

fn query_result_objects(result: &QueryResult) -> Vec<serde_json::Map<String, Value>> {
    result
        .rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (idx, value) in row.iter().enumerate() {
                if let Some(column) = result.columns.get(idx) {
                    obj.insert(column.name.clone(), value.to_json());
                }
            }
            obj
        })
        .collect()
}

fn choose_label_column(table: &Table, preferred: &[String]) -> Option<String> {
    for name in preferred {
        if table.columns.iter().any(|column| column.name == *name) {
            return Some(name.clone());
        }
    }
    table
        .columns
        .iter()
        .find(|column| !column.pk && column.r#type.to_ascii_lowercase().contains("text"))
        .map(|column| column.name.clone())
}

fn resolve_reference(
    tree: &module_db::SchemaTree,
    source_schema: &str,
    source_table: &Table,
    source_column: &str,
) -> Option<(String, Table, String)> {
    if let Some(fk) = source_table
        .foreign_keys
        .iter()
        .find(|fk| fk.cols.len() == 1 && fk.cols[0] == source_column && fk.ref_cols.len() == 1)
    {
        for schema in &tree.schemas {
            if let Some(table) = schema
                .tables
                .iter()
                .find(|table| table.name == fk.ref_table)
            {
                return Some((schema.name.clone(), table.clone(), fk.ref_cols[0].clone()));
            }
        }
    }

    let base = source_column.strip_suffix("_id")?;
    let candidates = [
        base.to_string(),
        format!("{base}s"),
        format!("{base}ies"),
        if base == "user" {
            "users".into()
        } else {
            String::new()
        },
    ];
    for schema in &tree.schemas {
        if schema.name != source_schema {
            continue;
        }
        for candidate in candidates.iter().filter(|candidate| !candidate.is_empty()) {
            if let Some(table) = schema.tables.iter().find(|table| table.name == *candidate) {
                let ref_col = table
                    .columns
                    .iter()
                    .find(|column| column.pk)
                    .map(|column| column.name.clone())
                    .unwrap_or_else(|| "id".into());
                return Some((schema.name.clone(), table.clone(), ref_col));
            }
        }
    }
    None
}

fn json_value_sql_literal(value: &serde_json::Value) -> Result<String, String> {
    Ok(match value {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(value) => {
            if *value {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => sql_string(value),
        other => sql_string(&other.to_string()),
    })
}

pub fn relation_performance(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema").unwrap_or("public");
    let table = str_arg(args, "table")?;
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;

    if conn.engine != Engine::Postgres {
        return Ok(json!({
            "ok": false,
            "connection": connection_id,
            "database": database,
            "schema": schema,
            "table": table,
            "engine": conn.engine.as_str(),
            "reason": "db_relation_performance currently supports PostgreSQL only",
            "recommendation": "Use db_table_info, db_count, db_sample, db_explain, or targeted db_query for this engine."
        }));
    }

    let schema_lit = sql_string(schema);
    let table_lit = sql_string(table);
    let sql = format!(
        r#"
WITH relation AS (
  SELECT c.oid, n.nspname AS schema_name, c.relname AS relation_name, c.relkind
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = {schema_lit}
    AND c.relname = {table_lit}
    AND c.relkind IN ('r', 'p', 'v', 'm')
),
table_stats AS (
  SELECT
    schemaname,
    relname,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del,
    n_live_tup,
    n_dead_tup,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
  FROM pg_stat_user_tables
  WHERE schemaname = {schema_lit}
    AND relname = {table_lit}
),
index_stats AS (
  SELECT
    schemaname,
    relname,
    jsonb_agg(
      jsonb_build_object(
        'index_name', indexrelname,
        'idx_scan', idx_scan,
        'idx_tup_read', idx_tup_read,
        'idx_tup_fetch', idx_tup_fetch,
        'size_bytes', pg_relation_size(indexrelid),
        'size_pretty', pg_size_pretty(pg_relation_size(indexrelid))
      )
      ORDER BY idx_scan DESC, indexrelname
    ) AS indexes
  FROM pg_stat_user_indexes
  WHERE schemaname = {schema_lit}
    AND relname = {table_lit}
  GROUP BY schemaname, relname
)
SELECT
  r.schema_name,
  r.relation_name,
  CASE r.relkind
    WHEN 'r' THEN 'table'
    WHEN 'p' THEN 'partitioned_table'
    WHEN 'v' THEN 'view'
    WHEN 'm' THEN 'materialized_view'
    ELSE r.relkind::text
  END AS relation_kind,
  pg_total_relation_size(r.oid) AS total_size_bytes,
  pg_size_pretty(pg_total_relation_size(r.oid)) AS total_size_pretty,
  pg_relation_size(r.oid) AS heap_size_bytes,
  pg_size_pretty(pg_relation_size(r.oid)) AS heap_size_pretty,
  COALESCE(ts.seq_scan, 0) AS seq_scan,
  COALESCE(ts.seq_tup_read, 0) AS seq_tup_read,
  COALESCE(ts.idx_scan, 0) AS idx_scan,
  COALESCE(ts.idx_tup_fetch, 0) AS idx_tup_fetch,
  COALESCE(ts.n_live_tup, 0) AS estimated_live_rows,
  COALESCE(ts.n_dead_tup, 0) AS estimated_dead_rows,
  ts.last_vacuum,
  ts.last_autovacuum,
  ts.last_analyze,
  ts.last_autoanalyze,
  COALESCE(ix.indexes, '[]'::jsonb) AS indexes
FROM relation r
LEFT JOIN table_stats ts ON ts.schemaname = r.schema_name AND ts.relname = r.relation_name
LEFT JOIN index_stats ix ON ix.schemaname = r.schema_name AND ix.relname = r.relation_name
"#
    );
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, 50, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": !result.rows.is_empty(),
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "engine": "postgres",
        "sql": sql,
        "result": result,
    }))
}

pub fn row_insert(mgr: &Manager, args: &Value) -> Result<Value, String> {
    if let Err(response) = require_approved(args, "db_row_insert") {
        return Ok(response);
    }
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let values = value_map_arg(args, "values")?;
    let row = runtime()
        .block_on(mgr.row_insert(connection_id, database, schema, table, values))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "row": row,
    }))
}

pub fn row_delete(mgr: &Manager, args: &Value) -> Result<Value, String> {
    if let Err(response) = require_approved(args, "db_row_delete") {
        return Ok(response);
    }
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let pks = pk_list_args(args)?;
    let mut affected = 0u64;
    for pk in pks {
        affected += runtime()
            .block_on(mgr.row_delete(connection_id, database, schema, table, pk))
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({
        "ok": true,
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "affected": affected,
    }))
}

pub fn row_duplicate(mgr: &Manager, args: &Value) -> Result<Value, String> {
    if let Err(response) = require_approved(args, "db_row_duplicate") {
        return Ok(response);
    }
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let pks = pk_list_args(args)?;
    let mut rows = Vec::new();
    for pk in pks {
        let row = runtime()
            .block_on(mgr.row_duplicate(connection_id, database, schema, table, pk))
            .map_err(|e| e.to_string())?;
        rows.push(row);
    }
    Ok(json!({
        "ok": true,
        "connection": connection_id,
        "database": database,
        "schema": schema,
        "table": table,
        "rows": rows,
    }))
}

pub fn export_table(mgr: &Manager, harness_home: &Path, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database").map(str::to_string);
    let schema = opt_str(args, "schema").map(str::to_string);
    let table = str_arg(args, "table")?.to_string();
    let format = str_arg(args, "format")?;
    let columns = opt_string_array(args, "columns")?;
    let dir = harness_home.join("exports").join("db");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create export dir: {e}"))?;

    let (filename, content_type, body) = match format {
        "json" | "csv" | "sql_insert" => {
            let req = ExportRequest {
                database: database.clone(),
                target: ExportTarget::Table {
                    schema: schema.clone(),
                    name: table.clone(),
                    columns,
                },
                format: match format {
                    "json" => ExportFormat::Json,
                    "csv" => ExportFormat::Csv,
                    "sql_insert" => ExportFormat::SqlInsert,
                    _ => unreachable!(),
                },
                scope: if format == "sql_insert" {
                    ExportScope::SchemaAndData
                } else {
                    ExportScope::DataOnly
                },
            };
            let result = runtime()
                .block_on(mgr.export(connection_id, req))
                .map_err(|e| e.to_string())?;
            (result.filename, result.content_type, result.body)
        }
        "markdown" | "xlsx" => {
            let limit = opt_limit(args, 5_000, 100_000);
            let conn = mgr
                .connections_get(connection_id)
                .map_err(|e| e.to_string())?;
            if let Some(columns) = columns.as_ref() {
                validate_table_columns(
                    mgr,
                    connection_id,
                    database.as_deref(),
                    schema.as_deref(),
                    &table,
                    columns,
                )?;
            }
            let qualified = qualify(conn.engine, schema.as_deref(), &table)?;
            let select_list = match columns.as_ref() {
                Some(columns) if columns.is_empty() => {
                    return Err("arg `columns` must not be empty".into());
                }
                Some(columns) => columns
                    .iter()
                    .map(|column| quote_ident(conn.engine, column))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", "),
                None => "*".into(),
            };
            let sql = format!("SELECT {select_list} FROM {qualified} LIMIT {limit}");
            let result = runtime()
                .block_on(mgr.query_run(connection_id, database.as_deref(), &sql, None, limit, 0))
                .map_err(|e| e.to_string())?;
            let base = format!(
                "{}.{}.{}",
                database.as_deref().unwrap_or("db"),
                schema.as_deref().unwrap_or("main"),
                table
            );
            if format == "markdown" {
                (
                    format!("{base}.md"),
                    "text/markdown;charset=utf-8".into(),
                    render_markdown(&result).into_bytes(),
                )
            } else {
                (
                    format!("{base}.xlsx"),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
                    render_xlsx(&result)?,
                )
            }
        }
        other => return Err(format!("unsupported export format: {other}")),
    };
    let path = unique_export_path(&dir, &sanitize_filename(&filename));
    std::fs::write(&path, body).map_err(|e| format!("write export: {e}"))?;
    Ok(json!({
        "ok": true,
        "path": path,
        "filename": filename,
        "content_type": content_type,
    }))
}

pub fn export_query(mgr: &Manager, harness_home: &Path, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let sql = str_arg(args, "sql")?;
    let format = str_arg(args, "format")?;
    let limit = opt_limit(args, 5_000, 100_000);
    if !is_read_only(sql) {
        return Ok(json!({
            "requires_approval": true,
            "reason": "db_export_query only exports single read-only SQL statements"
        }));
    }
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, sql, None, limit, 0))
        .map_err(|e| e.to_string())?;
    let (ext, content_type, body) = match format {
        "json" => (
            "json",
            "application/json",
            render_query_json(sql, &result).into_bytes(),
        ),
        "csv" => ("csv", "text/csv", render_csv(&result).into_bytes()),
        "markdown" => (
            "md",
            "text/markdown;charset=utf-8",
            render_markdown(&result).into_bytes(),
        ),
        "xlsx" => (
            "xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            render_xlsx(&result)?,
        ),
        other => return Err(format!("unsupported export format: {other}")),
    };
    let filename = args
        .get("filename")
        .and_then(|value| value.as_str())
        .map(sanitize_filename)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            format!(
                "query-{}.{}",
                chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
                ext
            )
        });
    let dir = harness_home.join("exports").join("db");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create export dir: {e}"))?;
    let path = unique_export_path(&dir, &filename);
    std::fs::write(&path, body).map_err(|e| format!("write export: {e}"))?;
    Ok(json!({
        "ok": true,
        "path": path,
        "filename": filename,
        "content_type": content_type,
        "rows": result.rows.len(),
        "truncated": result.truncated,
    }))
}

pub fn generate_view_sql(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let schema = opt_str(args, "schema");
    let view = str_arg(args, "view")?;
    let sql = str_arg(args, "sql")?.trim().trim_end_matches(';').trim();
    let replace = args
        .get("replace")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let materialized = args
        .get("materialized")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !is_read_only(sql) || module_db::__leading_keyword(sql).eq_ignore_ascii_case("EXPLAIN") {
        return Err("view SQL must be a single read-only SELECT-like statement".into());
    }
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    if materialized && conn.engine != Engine::Postgres {
        return Err("materialized views are supported only for PostgreSQL".into());
    }
    let qualified = qualify(conn.engine, schema, view)?;
    let create_sql = match (conn.engine, replace, materialized) {
        (Engine::Postgres, true, false) => {
            format!("CREATE OR REPLACE VIEW {qualified} AS\n{sql};")
        }
        (Engine::Postgres, false, false) => format!("CREATE VIEW {qualified} AS\n{sql};"),
        (Engine::Postgres, _, true) => {
            let drop = if replace {
                format!("DROP MATERIALIZED VIEW IF EXISTS {qualified};\n")
            } else {
                String::new()
            };
            format!("{drop}CREATE MATERIALIZED VIEW {qualified} AS\n{sql};")
        }
        (Engine::Mysql, true, false) => format!("CREATE OR REPLACE VIEW {qualified} AS\n{sql};"),
        (_, _, _) => {
            let drop = if replace {
                format!("DROP VIEW IF EXISTS {qualified};\n")
            } else {
                String::new()
            };
            format!("{drop}CREATE VIEW {qualified} AS\n{sql};")
        }
    };
    let drop_sql = if materialized {
        format!("DROP MATERIALIZED VIEW IF EXISTS {qualified};")
    } else {
        format!("DROP VIEW IF EXISTS {qualified};")
    };
    Ok(json!({
        "ok": true,
        "engine": conn.engine.as_str(),
        "schema": schema,
        "view": view,
        "create_sql": create_sql,
        "drop_sql": drop_sql,
        "migration": {
            "up": create_sql,
            "down": drop_sql
        }
    }))
}

pub fn drop_table(mgr: &Manager, args: &Value) -> Result<Value, String> {
    if let Err(response) = require_approved(args, "db_drop_table") {
        return Ok(response);
    }
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = opt_str(args, "schema");
    let table = str_arg(args, "table")?;
    let cascade = args
        .get("cascade")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let qualified = qualify(conn.engine, schema, table)?;
    let sql = format!(
        "DROP TABLE {qualified}{}",
        if cascade && conn.engine == Engine::Postgres {
            " CASCADE"
        } else {
            ""
        }
    );
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, 1, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "sql": sql, "result": result }))
}

pub fn drop_schema(mgr: &Manager, args: &Value) -> Result<Value, String> {
    if let Err(response) = require_approved(args, "db_drop_schema") {
        return Ok(response);
    }
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let schema = str_arg(args, "schema")?;
    let cascade = args
        .get("cascade")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;
    let sql = match conn.engine {
        Engine::Sqlite => {
            return Err("db_drop_schema is not supported for sqlite".into());
        }
        Engine::Postgres => format!(
            "DROP SCHEMA {}{}",
            quote_ident(conn.engine, schema)?,
            if cascade { " CASCADE" } else { "" }
        ),
        Engine::Mysql => format!("DROP DATABASE {}", quote_ident(conn.engine, schema)?),
    };
    let result = runtime()
        .block_on(mgr.query_run(connection_id, database, &sql, None, 1, 0))
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "sql": sql, "result": result }))
}

pub fn explain(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let sql = str_arg(args, "sql")?;
    let res = runtime()
        .block_on(mgr.explain(connection_id, database, sql))
        .map_err(|e| e.to_string())?;
    Ok(json!(res))
}

pub fn performance_audit(mgr: &Manager, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database");
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let conn = mgr
        .connections_get(connection_id)
        .map_err(|e| e.to_string())?;

    if conn.engine != Engine::Postgres {
        return Ok(json!({
            "ok": false,
            "engine": conn.engine.as_str(),
            "reason": "db_performance_audit currently supports PostgreSQL only",
            "recommendation": "Use db_schema/db_query manually for this engine until an engine-specific audit is implemented."
        }));
    }

    let checks = [
        AuditCheck {
            name: "table_activity_and_size",
            description: "Largest and most active user tables, including dead tuples and vacuum/analyze timestamps.",
            sql: r#"
SELECT
  s.schemaname,
  s.relname AS table_name,
  s.seq_scan,
  s.idx_scan,
  s.n_live_tup,
  s.n_dead_tup,
  pg_size_pretty(pg_total_relation_size(format('%I.%I', s.schemaname, s.relname)::regclass)) AS total_size,
  s.last_vacuum,
  s.last_autovacuum,
  s.last_analyze,
  s.last_autoanalyze
FROM pg_stat_user_tables s
ORDER BY pg_total_relation_size(format('%I.%I', s.schemaname, s.relname)::regclass) DESC
"#,
        },
        AuditCheck {
            name: "missing_fk_indexes",
            description: "Foreign keys whose referencing columns do not appear as the leftmost columns of any valid index.",
            sql: r#"
WITH fk AS (
  SELECT
    c.oid AS constraint_oid,
    n.nspname AS schema_name,
    t.relname AS table_name,
    c.conname AS constraint_name,
    c.conrelid,
    c.conkey,
    array_agg(a.attname ORDER BY u.ord) AS columns
  FROM pg_constraint c
  JOIN pg_class t ON t.oid = c.conrelid
  JOIN pg_namespace n ON n.oid = t.relnamespace
  JOIN unnest(c.conkey) WITH ORDINALITY AS u(attnum, ord) ON true
  JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = u.attnum
  WHERE c.contype = 'f'
    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
  GROUP BY c.oid, n.nspname, t.relname, c.conname, c.conrelid, c.conkey
),
indexed AS (
  SELECT
    fk.constraint_oid,
    EXISTS (
      SELECT 1
      FROM pg_index i
      WHERE i.indrelid = fk.conrelid
        AND i.indisvalid
        AND i.indpred IS NULL
        AND (i.indkey::smallint[])[1:array_length(fk.conkey, 1)] = fk.conkey
    ) AS has_left_prefix_index
  FROM fk
)
SELECT fk.schema_name, fk.table_name, fk.constraint_name, fk.columns
FROM fk
JOIN indexed i USING (constraint_oid)
WHERE NOT i.has_left_prefix_index
ORDER BY fk.schema_name, fk.table_name, fk.constraint_name
"#,
        },
        AuditCheck {
            name: "unused_or_low_usage_indexes",
            description: "Non-unique indexes with zero scans, sorted by size. Review before dropping.",
            sql: r#"
SELECT
  schemaname,
  relname AS table_name,
  indexrelname AS index_name,
  idx_scan,
  pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
WHERE idx_scan = 0
  AND indexrelid NOT IN (
    SELECT indexrelid FROM pg_index WHERE indisunique OR indisprimary
  )
ORDER BY pg_relation_size(indexrelid) DESC
"#,
        },
        AuditCheck {
            name: "index_usage_ratio",
            description: "Tables with high sequential scan pressure compared to index scans.",
            sql: r#"
SELECT
  schemaname,
  relname AS table_name,
  seq_scan,
  idx_scan,
  n_live_tup,
  CASE
    WHEN seq_scan + idx_scan = 0 THEN 0
    ELSE round((idx_scan::numeric / (seq_scan + idx_scan)) * 100, 2)
  END AS index_scan_pct
FROM pg_stat_user_tables
WHERE n_live_tup > 0
ORDER BY seq_scan DESC, n_live_tup DESC
"#,
        },
        AuditCheck {
            name: "duplicate_indexes",
            description: "Indexes with identical table, key columns, predicates, and expressions.",
            sql: r#"
SELECT
  ni.nspname AS schema_name,
  ct.relname AS table_name,
  array_agg(ci.relname ORDER BY ci.relname) AS duplicate_indexes,
  pg_size_pretty(sum(pg_relation_size(ci.oid))) AS total_size
FROM pg_index i
JOIN pg_class ci ON ci.oid = i.indexrelid
JOIN pg_class ct ON ct.oid = i.indrelid
JOIN pg_namespace ni ON ni.oid = ct.relnamespace
WHERE ni.nspname NOT IN ('pg_catalog', 'information_schema')
GROUP BY ni.nspname, ct.relname, i.indrelid, i.indkey, i.indclass, i.indcollation, i.indoption, i.indexprs, i.indpred
HAVING count(*) > 1
ORDER BY sum(pg_relation_size(ci.oid)) DESC
"#,
        },
        AuditCheck {
            name: "pg_stat_statements_available",
            description: "Whether pg_stat_statements is installed for slow query analysis.",
            sql: r#"
SELECT EXISTS (
  SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements'
) AS pg_stat_statements_available
"#,
        },
    ];

    let sections = checks
        .iter()
        .map(|check| {
            let result = runtime().block_on(mgr.query_run(
                connection_id,
                database,
                check.sql,
                None,
                limit,
                0,
            ));
            match result {
                Ok(result) => json!({
                    "name": check.name,
                    "description": check.description,
                    "sql": check.sql.trim(),
                    "result": result,
                }),
                Err(error) => json!({
                    "name": check.name,
                    "description": check.description,
                    "sql": check.sql.trim(),
                    "error": error.to_string(),
                }),
            }
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "ok": true,
        "engine": conn.engine.as_str(),
        "connection": connection_id,
        "database": database,
        "sections": sections,
        "next_steps": [
            "Use EXPLAIN on specific slow queries before proposing DDL.",
            "Treat unused index findings as candidates, not drop instructions.",
            "Update db_memory_write with stable findings and open questions."
        ]
    }))
}

struct AuditCheck {
    name: &'static str,
    description: &'static str,
    sql: &'static str,
}

pub fn backup(mgr: &Manager, harness_home: &Path, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database").map(str::to_string);
    let schema = opt_str(args, "schema").map(str::to_string);
    let table = opt_str(args, "table").map(str::to_string);
    let targets = match (schema, table) {
        (Some(schema), Some(table)) => vec![ExportTarget::Table {
            schema: Some(schema),
            name: table,
            columns: None,
        }],
        (Some(schema), None) => vec![ExportTarget::Schema { name: schema }],
        (None, Some(table)) => vec![ExportTarget::Table {
            schema: None,
            name: table,
            columns: None,
        }],
        (None, None) => {
            let tree = runtime()
                .block_on(mgr.schema_tree(connection_id, database.as_deref()))
                .map_err(|e| e.to_string())?;
            tree.schemas
                .into_iter()
                .map(|schema| ExportTarget::Schema { name: schema.name })
                .collect()
        }
    };

    if targets.is_empty() {
        return Err("cannot backup an empty schema tree".into());
    }

    let dir = harness_home.join("backups").join("db");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut files = Vec::new();

    for target in targets {
        let req = ExportRequest {
            database: database.clone(),
            target,
            format: ExportFormat::SqlInsert,
            scope: ExportScope::SchemaAndData,
        };
        let result = runtime()
            .block_on(mgr.export(connection_id, req))
            .map_err(|e| e.to_string())?;
        let filename = format!(
            "{}-{}",
            ts,
            sanitize_filename(result.filename.trim_end_matches(".sql"))
        );
        let path = unique_backup_path(&dir, &format!("{filename}.sql"));
        std::fs::write(&path, &result.body).map_err(|e| e.to_string())?;
        files.push(json!({
            "path": path,
            "bytes": result.body.len(),
            "content_type": result.content_type,
        }));
    }

    Ok(json!({
        "ok": true,
        "files": files,
    }))
}

pub fn memory_read(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database").unwrap_or("default");
    let path = memory_path(harness_home, profile, connection_id, database);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("db_memory_read: {e}")),
    };
    Ok(json!({
        "content": content,
        "path": path,
    }))
}

pub fn memory_write(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let connection_id = str_arg(args, "connection")?;
    let database = opt_str(args, "database").unwrap_or("default");
    let content = str_arg(args, "content")?;
    if content.len() > 1_048_576 {
        return Err("db_memory_write: content exceeds 1048576 bytes".into());
    }
    let path = memory_path(harness_home, profile, connection_id, database);
    let parent = path
        .parent()
        .ok_or_else(|| "db_memory_write: invalid memory path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("db_memory_write: create parent: {e}"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("db_memory_write: temp file: {e}"))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("db_memory_write: write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("db_memory_write: flush temp file: {e}"))?;
    tmp.persist(&path)
        .map_err(|e| format!("db_memory_write: persist: {}", e.error))?;
    Ok(json!({
        "ok": true,
        "path": path,
        "bytes": content.len(),
    }))
}

pub fn memory_path(
    harness_home: &Path,
    profile: &str,
    connection_id: &str,
    database: &str,
) -> PathBuf {
    harness_home
        .join("profiles")
        .join(sanitize_filename(profile))
        .join("db-memory")
        .join(sanitize_filename(connection_id))
        .join(format!("{}.md", sanitize_filename(database)))
}

fn sanitize_filename(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    sanitized.trim_matches('_').to_string()
}

fn unique_backup_path(dir: &Path, filename: &str) -> PathBuf {
    let stem = filename.trim_end_matches(".sql");
    unique_path_with_ext(dir, stem, "sql")
}

fn unique_export_path(dir: &Path, filename: &str) -> PathBuf {
    let (stem, ext) = filename
        .rsplit_once('.')
        .map(|(stem, ext)| (stem, ext))
        .unwrap_or((filename, "dat"));
    unique_path_with_ext(dir, stem, ext)
}

fn unique_path_with_ext(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut path = dir.join(format!("{stem}.{ext}"));
    if !path.exists() {
        return path;
    }
    for i in 1.. {
        path = dir.join(format!("{stem}-{i}.{ext}"));
        if !path.exists() {
            return path;
        }
    }
    unreachable!("unbounded suffix loop must return before overflow")
}

fn render_query_json(sql: &str, result: &QueryResult) -> String {
    serde_json::to_string_pretty(&json!({
        "sql": sql,
        "columns": result.columns,
        "rows": query_result_objects(result),
        "truncated": result.truncated,
        "elapsed_ms": result.elapsed_ms,
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn render_csv(result: &QueryResult) -> String {
    let mut out = String::new();
    out.push_str(
        &result
            .columns
            .iter()
            .map(|column| csv_cell(&column.name))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in &result.rows {
        out.push_str(
            &row.iter()
                .map(|value| csv_cell(&cell_text(value)))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn csv_cell(raw: &str) -> String {
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw.to_string()
    }
}

fn render_markdown(result: &module_db::QueryResult) -> String {
    let mut out = String::new();
    out.push('|');
    for column in &result.columns {
        out.push(' ');
        out.push_str(&markdown_cell(&column.name));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in &result.columns {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in &result.rows {
        out.push('|');
        for value in row {
            out.push(' ');
            out.push_str(&markdown_cell(&cell_text(value)));
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

fn markdown_cell(raw: &str) -> String {
    raw.replace('|', "\\|").replace('\n', "<br>")
}

fn render_xlsx(result: &module_db::QueryResult) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("[Content_Types].xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#).map_err(|e| e.to_string())?;
        zip.add_directory("_rels/", options)
            .map_err(|e| e.to_string())?;
        zip.start_file("_rels/.rels", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).map_err(|e| e.to_string())?;
        zip.add_directory("xl/_rels/", options)
            .map_err(|e| e.to_string())?;
        zip.start_file("xl/_rels/workbook.xml.rels", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).map_err(|e| e.to_string())?;
        zip.start_file("xl/workbook.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Export" sheetId="1" r:id="rId1"/></sheets></workbook>"#).map_err(|e| e.to_string())?;
        zip.add_directory("xl/worksheets/", options)
            .map_err(|e| e.to_string())?;
        zip.start_file("xl/worksheets/sheet1.xml", options)
            .map_err(|e| e.to_string())?;
        let mut sheet = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        sheet.push_str("<row r=\"1\">");
        for (idx, column) in result.columns.iter().enumerate() {
            sheet.push_str(&xlsx_text_cell(idx + 1, 1, &column.name));
        }
        sheet.push_str("</row>");
        for (ridx, row) in result.rows.iter().enumerate() {
            let row_num = ridx + 2;
            sheet.push_str(&format!("<row r=\"{row_num}\">"));
            for (cidx, value) in row.iter().enumerate() {
                sheet.push_str(&xlsx_text_cell(cidx + 1, row_num, &cell_text(value)));
            }
            sheet.push_str("</row>");
        }
        sheet.push_str("</sheetData></worksheet>");
        zip.write_all(sheet.as_bytes()).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(cursor.into_inner())
}

fn xlsx_text_cell(col: usize, row: usize, value: &str) -> String {
    format!(
        r#"<c r="{}{}" t="inlineStr"><is><t>{}</t></is></c>"#,
        xlsx_col(col),
        row,
        xml_escape(value)
    )
}

fn xlsx_col(mut n: usize) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    out
}

fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cell_text(value: &module_db::Value) -> String {
    match value {
        module_db::Value::Null => String::new(),
        module_db::Value::Bool(value) => value.to_string(),
        module_db::Value::Int(value) => value.to_string(),
        module_db::Value::Float(value) => value.to_string(),
        module_db::Value::Text(value) => value.clone(),
        module_db::Value::Tagged(tagged) => serde_json::to_string(tagged).unwrap_or_default(),
    }
}

// Keep wrap_error reachable so the dispatcher's `use ... wrap_error` is happy
// even when no error branch fires here.
#[allow(dead_code)]
fn _keep_imports() -> Value {
    wrap_error("noop")
}

#[cfg(test)]
mod tests {
    use super::*;
    use module_db::{ConnectionInput, Engine};

    #[test]
    fn backup_writes_sql_file() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO users (name) VALUES ('Ada')",
                None,
                10,
                0,
            ))
            .unwrap();

        let result = backup(
            &mgr,
            dir.path(),
            &json!({ "connection": conn.id, "schema": "main", "table": "users" }),
        )
        .unwrap();
        let path = result["files"][0]["path"].as_str().unwrap();
        let sql = std::fs::read_to_string(path).unwrap();
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("INSERT INTO"));
    }

    #[test]
    fn db_memory_round_trips_by_connection_and_database() {
        let dir = tempfile::tempdir().unwrap();
        let args = json!({
            "connection": "conn-1",
            "database": "main",
            "content": "# DB Memory\n\n## Overview\nKnown structure."
        });

        let written = memory_write(dir.path(), "default", &args).unwrap();
        assert_eq!(written["ok"], true);

        let read = memory_read(
            dir.path(),
            "default",
            &json!({ "connection": "conn-1", "database": "main" }),
        )
        .unwrap();
        assert!(read["content"]
            .as_str()
            .unwrap()
            .contains("Known structure"));
        assert!(read["path"].as_str().unwrap().contains("db-memory"));
    }

    #[test]
    fn db_schema_can_return_one_table() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();

        let result = schema(
            &mgr,
            &json!({ "connection": conn.id, "schema": "main", "table": "users" }),
        )
        .unwrap();
        assert_eq!(result["filtered"], true);
        let tables = result["result"]["schemas"][0]["tables"].as_array().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0]["name"], "users");
    }

    #[test]
    fn semantic_db_helpers_handle_common_table_questions() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT, name TEXT, role TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO users (email, name, role) VALUES ('ada@example.test', 'Ada', 'admin'), ('bob@example.test', 'Bob', 'member'), ('bea@example.test', 'Bea', 'member')",
                None,
                10,
                0,
            ))
            .unwrap();

        let info = table_info(
            &mgr,
            &json!({ "connection": conn.id, "schema": "main", "table": "users" }),
        )
        .unwrap();
        assert_eq!(info["ok"], true);
        assert_eq!(info["schema"], "main");
        assert_eq!(info["table"]["name"], "users");

        let matches = search_tables(&mgr, &json!({ "connection": conn.id, "q": "user" })).unwrap();
        assert_eq!(matches["q"], "user");
        assert!(matches["result"]["rows"].as_array().unwrap().len() >= 1);

        let sample_rows = sample(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "columns": ["email"],
                "limit": 20
            }),
        )
        .unwrap();
        assert!(sample_rows["sql"].as_str().unwrap().contains("\"email\""));
        assert_eq!(sample_rows["result"]["rows"].as_array().unwrap().len(), 3);

        let total = count(
            &mgr,
            &json!({ "connection": conn.id, "schema": "main", "table": "users" }),
        )
        .unwrap();
        assert!(total["sql"]
            .as_str()
            .unwrap()
            .contains("SELECT count(*) AS total"));
        assert_eq!(total["result"]["rows"][0][0], 3);

        let roles = distinct_values(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "column": "role"
            }),
        )
        .unwrap();
        assert_eq!(roles["result"]["rows"].as_array().unwrap().len(), 2);

        let found = find_rows(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "q": "example",
                "columns": ["email"],
                "return_columns": ["email", "name"],
                "limit": 20
            }),
        )
        .unwrap();
        assert_eq!(found["result"]["rows"].as_array().unwrap().len(), 3);

        let grouped = aggregate(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "group_by": ["role"],
                "metrics": [{ "fn": "count", "as": "total" }],
                "limit": 20
            }),
        )
        .unwrap();
        assert!(grouped["sql"].as_str().unwrap().contains("GROUP BY"));
        assert_eq!(grouped["result"]["rows"].as_array().unwrap().len(), 2);

        let perf = relation_performance(
            &mgr,
            &json!({ "connection": conn.id, "schema": "main", "table": "users" }),
        )
        .unwrap();
        assert_eq!(perf["ok"], false);
        assert_eq!(perf["engine"], "sqlite");
        assert!(perf["reason"].as_str().unwrap().contains("PostgreSQL only"));
    }

    #[test]
    fn db_select_and_validate_query_use_structured_filters() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT, name TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO users (email, name) VALUES ('ada@example.test', 'Ada'), ('bob@example.test', 'Bob')",
                None,
                10,
                0,
            ))
            .unwrap();

        let validation = validate_query(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "columns": ["email"],
                "filters": [{ "column": "name", "op": "eq", "value": "Ada" }],
                "limit": 20
            }),
        )
        .unwrap();
        assert_eq!(validation["ok"], true);
        assert_eq!(validation["mode"], "structured_select");

        let selected = select(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "columns": ["email"],
                "filters": [{ "column": "name", "op": "eq", "value": "Ada" }],
                "order_by": [{ "column": "id", "dir": "asc" }],
                "limit": 20
            }),
        )
        .unwrap();
        assert!(selected["sql"].as_str().unwrap().contains("\"email\""));
        assert_eq!(selected["result"]["rows"].as_array().unwrap().len(), 1);

        let invalid = validate_query(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "columns": ["missing"],
                "limit": 20
            }),
        )
        .unwrap();
        assert_eq!(invalid["ok"], false);
        assert!(invalid["errors"][0]
            .as_str()
            .unwrap()
            .contains("unknown selected column"));

        let raw = validate_query(
            &mgr,
            &json!({ "connection": conn.id, "sql": "SELECT * FROM users; DROP TABLE users" }),
        )
        .unwrap();
        assert_eq!(raw["ok"], false);
        assert_eq!(raw["multiple_statements"], true);
    }

    #[test]
    fn db_extract_enriched_adds_fk_like_labels() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO users (id, name) VALUES (1, 'Ada')",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO posts (user_id, title) VALUES (1, 'Hello')",
                None,
                10,
                0,
            ))
            .unwrap();

        let result = extract_enriched(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "posts",
                "columns": ["id", "user_id", "title"],
                "limit": 20
            }),
        )
        .unwrap();
        assert_eq!(result["rows"][0]["user_id_label"], "Ada");
        assert_eq!(result["enrichments"][0]["ref_table"], "users");
    }

    #[test]
    fn db_export_query_and_generate_view_sql_handle_complex_selects() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, firstname TEXT, lastname TEXT, email TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE events (id INTEGER PRIMARY KEY, created_by INTEGER, title TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO users VALUES (1, 'Ada', 'Lovelace', 'ada@example.test')",
                None,
                10,
                0,
            ))
            .unwrap();
        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "INSERT INTO events VALUES (1, 1, 'Launch')",
                None,
                10,
                0,
            ))
            .unwrap();
        let sql = "SELECT ev.id, ev.title, us.email, us.firstname || ' ' || us.lastname AS user_name FROM events ev JOIN users us ON us.id = ev.created_by";

        let exported = export_query(
            &mgr,
            dir.path(),
            &json!({
                "connection": conn.id,
                "sql": sql,
                "format": "csv",
                "filename": "events.csv",
                "limit": 20
            }),
        )
        .unwrap();
        let path = exported["path"].as_str().unwrap();
        assert!(std::fs::read_to_string(path).unwrap().contains("user_name"));

        let view = generate_view_sql(
            &mgr,
            &json!({
                "connection": conn.id,
                "view": "vw_events",
                "sql": sql
            }),
        )
        .unwrap();
        assert!(view["create_sql"].as_str().unwrap().contains("CREATE VIEW"));
        assert!(view["migration"]["down"]
            .as_str()
            .unwrap()
            .contains("DROP VIEW"));
    }

    #[test]
    fn mutating_db_tools_require_approval_and_export_files() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        runtime()
            .block_on(mgr.query_run(
                &conn.id,
                None,
                "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT, name TEXT)",
                None,
                10,
                0,
            ))
            .unwrap();

        let blocked = row_insert(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "values": { "email": "ada@example.test", "name": "Ada" }
            }),
        )
        .unwrap();
        assert_eq!(blocked["requires_approval"], true);

        let inserted = row_insert(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "values": { "email": "ada@example.test", "name": "Ada" },
                "approved": true
            }),
        )
        .unwrap();
        assert_eq!(inserted["ok"], true);

        let duplicated = row_duplicate(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "pk": { "id": 1 },
                "approved": true
            }),
        )
        .unwrap();
        assert_eq!(duplicated["rows"].as_array().unwrap().len(), 1);

        let markdown = export_table(
            &mgr,
            dir.path(),
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "format": "markdown",
                "limit": 20
            }),
        )
        .unwrap();
        let md_path = markdown["path"].as_str().unwrap();
        assert!(std::fs::read_to_string(md_path).unwrap().contains("|"));

        let xlsx = export_table(
            &mgr,
            dir.path(),
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "format": "xlsx",
                "limit": 20
            }),
        )
        .unwrap();
        let xlsx_path = xlsx["path"].as_str().unwrap();
        assert!(std::fs::metadata(xlsx_path).unwrap().len() > 0);

        let deleted = row_delete(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users",
                "pks": [{ "id": 1 }, { "id": 2 }],
                "approved": true
            }),
        )
        .unwrap();
        assert_eq!(deleted["affected"], 2);

        let drop_blocked = drop_table(
            &mgr,
            &json!({
                "connection": conn.id,
                "schema": "main",
                "table": "users"
            }),
        )
        .unwrap();
        assert_eq!(drop_blocked["requires_approval"], true);
    }

    #[test]
    fn performance_audit_reports_unsupported_engine() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "sqlite".into(),
                engine: Engine::Sqlite,
                database: dir.path().join("app.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();

        let result = performance_audit(&mgr, &json!({ "connection": conn.id })).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["engine"], "sqlite");
        assert!(result["reason"]
            .as_str()
            .unwrap()
            .contains("PostgreSQL only"));
    }

    #[test]
    fn read_only_gate_rejects_ctes_and_stacked_statements() {
        assert!(is_read_only("SELECT * FROM users"));
        assert!(is_read_only("SELECT ';' AS semi"));
        assert!(is_read_only("SELECT 1;   "));
        assert!(!is_read_only(
            "WITH x AS (DELETE FROM users RETURNING *) SELECT * FROM x"
        ));
        assert!(!is_read_only("SELECT * FROM users; DROP TABLE users"));
    }
}
