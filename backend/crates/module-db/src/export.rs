//! Export tables / schemas to JSON, SQL INSERT, or CSV.
//!
//! High-level entry point: [`run_export`]. The manager forwards a
//! validated [`ExportRequest`] here; this module is engine-aware via the
//! `DbPool` it receives and emits a byte-blob plus suggested filename so
//! the REST layer can hand it to the browser as a download.
//!
//! Design decisions documented inline at the relevant code site:
//!  * CSV refuses schema-target exports — mixed-table CSVs are
//!    confusing in spreadsheet tools, and there's no header row that
//!    correctly describes more than one shape.
//!  * Data path streams in chunks of 5_000 rows; a hard 5_000_000-row
//!    cap protects the process from accidentally serialising a billion-
//!    row table.
//!  * `CREATE TABLE` emission is best-effort. We always reproduce
//!    columns + nullability + defaults + primary key. We reproduce
//!    indexes and foreign keys for SQLite/Postgres/MySQL when the
//!    schema introspector populated them. When a column subset is in
//!    play, indexes / FKs that touch dropped columns are silently
//!    omitted.

use std::collections::HashSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::error::{DbError, DbResult};
use crate::pool::DbPool;
use crate::schema::introspect;
use crate::types::{Column, Engine, Table};
use crate::value::{
    decode_mysql_row, decode_postgres_row, decode_sqlite_row, TaggedValue, Value,
};

/// Output format for an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    SqlInsert,
    Csv,
}

/// What parts of the table(s) to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum ExportScope {
    /// DDL only (CREATE TABLE + indexes/FKs where reproducible).
    SchemaOnly,
    /// CREATE TABLE + data.
    SchemaAndData,
    /// Data only (no DDL).
    DataOnly,
}

/// The thing being exported.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportTarget {
    Table {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<String>,
        name: String,
        /// Optional column whitelist; `None` means "all columns".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        columns: Option<Vec<String>>,
    },
    Schema {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ExportRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub target: ExportTarget,
    pub format: ExportFormat,
    pub scope: ExportScope,
}

/// Output of an export: bytes the caller is expected to ship to the user
/// as-is, plus a suggested filename and content-type for `Content-Disposition`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ExportResult {
    pub filename: String,
    pub content_type: String,
    /// Raw bytes (sent verbatim by the REST layer). Skipped from ts-rs so
    /// the frontend treats the response as a binary download rather than
    /// expecting a JSON-encoded blob.
    #[cfg_attr(feature = "ts-export", ts(skip))]
    #[serde(skip)]
    pub body: Vec<u8>,
}

/// Hard safety cap on total rows fetched per export. Past this point we
/// stop appending and add a `truncated_at` marker.
pub const ROW_LIMIT: usize = 5_000_000;
/// Per-fetch chunk size for the data path.
pub const CHUNK_SIZE: usize = 5_000;
/// Rows per `INSERT INTO ... VALUES (...), (...)` statement.
pub const SQL_INSERT_BATCH: usize = 500;

// ============================================================================
// Entry point
// ============================================================================

pub async fn run_export(
    pool: &DbPool,
    database: Option<&str>,
    req: &ExportRequest,
) -> DbResult<ExportResult> {
    let engine = pool.engine();
    let tree = introspect(pool, engine, database).await?;

    if let (ExportTarget::Schema { .. }, ExportFormat::Csv) = (&req.target, req.format) {
        return Err(DbError::Validation(
            "CSV export does not support schema targets; pick a single table.".into(),
        ));
    }

    let date = Utc::now().format("%Y-%m-%d").to_string();
    let db_label = database.unwrap_or("db").to_string();

    match &req.target {
        ExportTarget::Table {
            schema,
            name,
            columns,
        } => {
            let (sch_name, table) = locate_table(&tree, schema.as_deref(), name)?;
            let selected = resolve_columns(table, columns.as_deref())?;
            let ext = ext_for(req.format);
            let filename = format!("{db_label}.{sch_name}.{name}.{date}.{ext}");
            let content_type = content_type_for(req.format).to_string();
            let body = render_single_table(
                pool,
                engine,
                Some(&sch_name),
                table,
                &selected,
                req.format,
                req.scope,
            )
            .await?;
            Ok(ExportResult {
                filename,
                content_type,
                body,
            })
        }
        ExportTarget::Schema { name } => {
            let sch = tree
                .schemas
                .iter()
                .find(|s| s.name == *name)
                .ok_or_else(|| DbError::NotFound(format!("schema {name}")))?;
            let ext = ext_for(req.format);
            let filename = format!("{db_label}.{name}.{date}.{ext}");
            let content_type = content_type_for(req.format).to_string();
            let body = match req.format {
                ExportFormat::Json => {
                    render_schema_json(pool, engine, &sch.name, &sch.tables, req.scope).await?
                }
                ExportFormat::SqlInsert => {
                    render_schema_sql(pool, engine, &sch.name, &sch.tables, req.scope).await?
                }
                ExportFormat::Csv => unreachable!("guarded above"),
            };
            Ok(ExportResult {
                filename,
                content_type,
                body,
            })
        }
    }
}

fn ext_for(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Json => "json",
        ExportFormat::SqlInsert => "sql",
        ExportFormat::Csv => "csv",
    }
}

fn content_type_for(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Json => "application/json",
        ExportFormat::SqlInsert => "application/sql",
        ExportFormat::Csv => "text/csv",
    }
}

fn locate_table<'a>(
    tree: &'a crate::types::SchemaTree,
    schema: Option<&str>,
    name: &str,
) -> DbResult<(String, &'a Table)> {
    for s in &tree.schemas {
        if let Some(sn) = schema {
            if s.name != sn {
                continue;
            }
        }
        if let Some(t) = s.tables.iter().find(|t| t.name == name) {
            return Ok((s.name.clone(), t));
        }
    }
    Err(DbError::NotFound(format!("table {name}")))
}

/// Resolve an optional column whitelist against the table's introspected
/// columns. Preserves the order requested by the caller so downstream CSV
/// / SQL / JSON output is deterministic. Returns the validated `Column`
/// slice references in that order.
fn resolve_columns<'a>(
    table: &'a Table,
    requested: Option<&[String]>,
) -> DbResult<Vec<&'a Column>> {
    match requested {
        None => Ok(table.columns.iter().collect()),
        Some(list) => {
            let by_name: std::collections::HashMap<&str, &Column> =
                table.columns.iter().map(|c| (c.name.as_str(), c)).collect();
            let mut unknown = Vec::new();
            let mut out = Vec::with_capacity(list.len());
            for n in list {
                match by_name.get(n.as_str()) {
                    Some(c) => out.push(*c),
                    None => unknown.push(n.clone()),
                }
            }
            if !unknown.is_empty() {
                return Err(DbError::Validation(format!(
                    "unknown columns: {}",
                    unknown.join(", ")
                )));
            }
            if out.is_empty() {
                return Err(DbError::Validation(
                    "column subset is empty after validation".into(),
                ));
            }
            Ok(out)
        }
    }
}

// ============================================================================
// Single-table rendering
// ============================================================================

async fn render_single_table(
    pool: &DbPool,
    engine: Engine,
    schema: Option<&str>,
    table: &Table,
    selected: &[&Column],
    format: ExportFormat,
    scope: ExportScope,
) -> DbResult<Vec<u8>> {
    match format {
        ExportFormat::Json => {
            render_table_json(pool, engine, schema, table, selected, scope).await
        }
        ExportFormat::SqlInsert => {
            render_table_sql(pool, engine, schema, table, selected, scope).await
        }
        ExportFormat::Csv => render_table_csv(pool, engine, schema, table, selected).await,
    }
}

async fn render_table_json(
    pool: &DbPool,
    engine: Engine,
    schema: Option<&str>,
    table: &Table,
    selected: &[&Column],
    scope: ExportScope,
) -> DbResult<Vec<u8>> {
    let columns_meta: Vec<_> = selected
        .iter()
        .map(|c| json!({ "name": c.name, "data_type": c.r#type }))
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("table".to_string(), json!(table.name));
    if let Some(s) = schema {
        obj.insert("schema".to_string(), json!(s));
    }
    obj.insert("columns".to_string(), json!(columns_meta));
    if matches!(scope, ExportScope::SchemaAndData | ExportScope::DataOnly) {
        let (rows, truncated) = fetch_all_rows(pool, engine, schema, &table.name, selected).await?;
        let row_vals: Vec<_> = rows
            .into_iter()
            .map(|r| serde_json::Value::Array(r.into_iter().map(|v| v.into()).collect()))
            .collect();
        obj.insert("rows".to_string(), serde_json::Value::Array(row_vals));
        if truncated {
            obj.insert("truncated_at".to_string(), json!(ROW_LIMIT));
        }
    }
    let s = serde_json::to_vec_pretty(&serde_json::Value::Object(obj))
        .map_err(|e| DbError::Internal(format!("json encode: {e}")))?;
    Ok(s)
}

async fn render_table_sql(
    pool: &DbPool,
    engine: Engine,
    schema: Option<&str>,
    table: &Table,
    selected: &[&Column],
    scope: ExportScope,
) -> DbResult<Vec<u8>> {
    let mut out = String::new();
    if matches!(scope, ExportScope::SchemaOnly | ExportScope::SchemaAndData) {
        out.push_str(&render_create_table(engine, schema, table, selected));
        out.push('\n');
    }
    if matches!(scope, ExportScope::SchemaAndData | ExportScope::DataOnly) {
        let (rows, truncated) = fetch_all_rows(pool, engine, schema, &table.name, selected).await?;
        render_inserts_into(&mut out, engine, schema, &table.name, selected, &rows);
        if truncated {
            out.push_str(&format!("-- truncated at {ROW_LIMIT} rows\n"));
        }
    }
    Ok(out.into_bytes())
}

async fn render_table_csv(
    pool: &DbPool,
    engine: Engine,
    schema: Option<&str>,
    table: &Table,
    selected: &[&Column],
) -> DbResult<Vec<u8>> {
    let mut out = String::new();
    // Header row.
    let header: Vec<String> = selected.iter().map(|c| csv_field(&c.name)).collect();
    out.push_str(&header.join(","));
    out.push_str("\r\n");
    let (rows, truncated) = fetch_all_rows(pool, engine, schema, &table.name, selected).await?;
    for row in rows {
        let fields: Vec<String> = row.iter().map(value_to_csv_field).collect();
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    if truncated {
        out.push_str(&format!("# truncated at {ROW_LIMIT} rows\r\n"));
    }
    Ok(out.into_bytes())
}

// ============================================================================
// Schema-target rendering (JSON / SQL only)
// ============================================================================

async fn render_schema_json(
    pool: &DbPool,
    engine: Engine,
    schema: &str,
    tables: &[Table],
    scope: ExportScope,
) -> DbResult<Vec<u8>> {
    let mut tables_out: Vec<serde_json::Value> = Vec::with_capacity(tables.len());
    for t in tables {
        let selected: Vec<&Column> = t.columns.iter().collect();
        let columns_meta: Vec<_> = selected
            .iter()
            .map(|c| json!({ "name": c.name, "data_type": c.r#type }))
            .collect();
        let mut obj = serde_json::Map::new();
        obj.insert("table".to_string(), json!(t.name));
        obj.insert("columns".to_string(), json!(columns_meta));
        if matches!(scope, ExportScope::SchemaAndData | ExportScope::DataOnly) {
            let (rows, truncated) =
                fetch_all_rows(pool, engine, Some(schema), &t.name, &selected).await?;
            let row_vals: Vec<_> = rows
                .into_iter()
                .map(|r| serde_json::Value::Array(r.into_iter().map(|v| v.into()).collect()))
                .collect();
            obj.insert("rows".to_string(), serde_json::Value::Array(row_vals));
            if truncated {
                obj.insert("truncated_at".to_string(), json!(ROW_LIMIT));
            }
        }
        tables_out.push(serde_json::Value::Object(obj));
    }
    let doc = json!({ "schema": schema, "tables": tables_out });
    let s = serde_json::to_vec_pretty(&doc)
        .map_err(|e| DbError::Internal(format!("json encode: {e}")))?;
    Ok(s)
}

async fn render_schema_sql(
    pool: &DbPool,
    engine: Engine,
    schema: &str,
    tables: &[Table],
    scope: ExportScope,
) -> DbResult<Vec<u8>> {
    let mut out = String::new();
    for t in tables {
        let selected: Vec<&Column> = t.columns.iter().collect();
        if matches!(scope, ExportScope::SchemaOnly | ExportScope::SchemaAndData) {
            out.push_str(&render_create_table(engine, Some(schema), t, &selected));
            out.push('\n');
        }
        if matches!(scope, ExportScope::SchemaAndData | ExportScope::DataOnly) {
            let (rows, truncated) =
                fetch_all_rows(pool, engine, Some(schema), &t.name, &selected).await?;
            render_inserts_into(&mut out, engine, Some(schema), &t.name, &selected, &rows);
            if truncated {
                out.push_str(&format!("-- truncated at {ROW_LIMIT} rows\n"));
            }
        }
    }
    Ok(out.into_bytes())
}

// ============================================================================
// CREATE TABLE
// ============================================================================

fn render_create_table(
    engine: Engine,
    schema: Option<&str>,
    table: &Table,
    selected: &[&Column],
) -> String {
    let q = ident_quote(engine);
    let qtable = qualify(engine, schema, &table.name);
    let mut lines: Vec<String> = Vec::new();
    let kept: HashSet<&str> = selected.iter().map(|c| c.name.as_str()).collect();

    for c in selected {
        let mut line = format!("  {q}{name}{q} {ty}", name = c.name, ty = c.r#type);
        if !c.nullable {
            line.push_str(" NOT NULL");
        }
        if let Some(d) = &c.default {
            // Schema introspectors sometimes surface Some("") for "no
            // default"; treat that as no clause.
            if !d.is_empty() {
                line.push_str(&format!(" DEFAULT {d}"));
            }
        }
        lines.push(line);
    }

    // PK across all selected PK columns.
    let pk_cols: Vec<String> = selected
        .iter()
        .filter(|c| c.pk)
        .map(|c| format!("{q}{}{q}", c.name))
        .collect();
    if !pk_cols.is_empty() {
        lines.push(format!("  PRIMARY KEY ({})", pk_cols.join(", ")));
    }

    // FKs entirely contained within the selected subset.
    for fk in &table.foreign_keys {
        if fk.cols.iter().all(|c| kept.contains(c.as_str())) {
            let cols = fk
                .cols
                .iter()
                .map(|c| format!("{q}{c}{q}"))
                .collect::<Vec<_>>()
                .join(", ");
            let ref_cols = fk
                .ref_cols
                .iter()
                .map(|c| format!("{q}{c}{q}"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "  CONSTRAINT {q}{}{q} FOREIGN KEY ({cols}) REFERENCES {q}{}{q} ({ref_cols})",
                fk.name, fk.ref_table
            ));
        }
    }

    let mut out = format!("CREATE TABLE {qtable} (\n{}\n);\n", lines.join(",\n"));

    // Indexes (separate statements). Skip primary-key implied indexes
    // since the PRIMARY KEY clause above already covers them.
    for ix in &table.indexes {
        if ix.cols.iter().any(|c| !kept.contains(c.as_str())) {
            continue;
        }
        // SQLite & friends auto-generate names like "sqlite_autoindex_*";
        // skip those, they're implementation details.
        if ix.name.starts_with("sqlite_autoindex_") {
            continue;
        }
        let cols = ix
            .cols
            .iter()
            .map(|c| format!("{q}{c}{q}"))
            .collect::<Vec<_>>()
            .join(", ");
        let unique = if ix.unique { "UNIQUE " } else { "" };
        out.push_str(&format!(
            "CREATE {unique}INDEX {q}{}{q} ON {qtable} ({cols});\n",
            ix.name
        ));
    }
    out
}

// ============================================================================
// INSERT batching
// ============================================================================

fn render_inserts_into(
    out: &mut String,
    engine: Engine,
    schema: Option<&str>,
    table_name: &str,
    selected: &[&Column],
    rows: &[Vec<Value>],
) {
    if rows.is_empty() {
        return;
    }
    let q = ident_quote(engine);
    let qtable = qualify(engine, schema, table_name);
    let cols = selected
        .iter()
        .map(|c| format!("{q}{}{q}", c.name))
        .collect::<Vec<_>>()
        .join(", ");

    for chunk in rows.chunks(SQL_INSERT_BATCH) {
        out.push_str(&format!("INSERT INTO {qtable} ({cols}) VALUES\n"));
        let parts: Vec<String> = chunk
            .iter()
            .map(|row| {
                let vals = row
                    .iter()
                    .map(|v| sql_literal(engine, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("  ({vals})")
            })
            .collect();
        out.push_str(&parts.join(",\n"));
        out.push_str(";\n");
    }
}

// ============================================================================
// Data path — chunked SELECT with safety cap
// ============================================================================

async fn fetch_all_rows(
    pool: &DbPool,
    engine: Engine,
    schema: Option<&str>,
    table: &str,
    selected: &[&Column],
) -> DbResult<(Vec<Vec<Value>>, bool)> {
    let q = ident_quote(engine);
    let qtable = qualify(engine, schema, table);
    let col_list = selected
        .iter()
        .map(|c| format!("{q}{}{q}", c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out: Vec<Vec<Value>> = Vec::new();
    let mut offset: usize = 0;
    let mut truncated = false;
    loop {
        if out.len() >= ROW_LIMIT {
            truncated = true;
            break;
        }
        let remaining = ROW_LIMIT - out.len();
        let limit = CHUNK_SIZE.min(remaining + 1); // +1 to detect overflow next loop
        let sql = format!("SELECT {col_list} FROM {qtable} LIMIT {limit} OFFSET {offset}");
        let fetched: Vec<Vec<Value>> = match pool {
            DbPool::Sqlite(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                rows.iter().map(decode_sqlite_row).collect()
            }
            DbPool::Postgres(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                rows.iter().map(decode_postgres_row).collect()
            }
            DbPool::Mysql(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                rows.iter().map(decode_mysql_row).collect()
            }
        };
        let got = fetched.len();
        if got == 0 {
            break;
        }
        out.extend(fetched);
        if got < limit {
            break;
        }
        offset += got;
        let _ = engine; // engine carried only for clarity
        let _ = pool.engine();
        let _ = engine;
        // Continue the loop.
        if out.len() > ROW_LIMIT {
            out.truncate(ROW_LIMIT);
            truncated = true;
            break;
        }
    }
    Ok((out, truncated))
}

// ============================================================================
// Quoting helpers
// ============================================================================

fn ident_quote(engine: Engine) -> char {
    match engine {
        Engine::Mysql => '`',
        _ => '"',
    }
}

fn qualify(engine: Engine, schema: Option<&str>, table: &str) -> String {
    let q = ident_quote(engine);
    match schema {
        // SQLite's only schema is "main"; including it in CREATE TABLE
        // statements would actually be wrong (you can't say
        // `CREATE TABLE "main"."t"`), so we drop it for SQLite.
        Some(s) if !(s.is_empty() || matches!(engine, Engine::Sqlite) && s == "main") => {
            format!("{q}{s}{q}.{q}{table}{q}")
        }
        _ => format!("{q}{table}{q}"),
    }
}

fn sql_literal(engine: Engine, v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => match engine {
            Engine::Mysql => {
                if *b {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            _ => {
                if *b {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
        },
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_finite() {
                format!("{f}")
            } else {
                "NULL".into()
            }
        }
        Value::Text(s) => quote_sql_string(s),
        Value::Tagged(t) => match t {
            TaggedValue::Decimal(s) => s.clone(),
            TaggedValue::Date(s)
            | TaggedValue::Time(s)
            | TaggedValue::DateTime(s) => quote_sql_string(s),
            TaggedValue::Bytes(b64) => {
                use base64::Engine as _;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .unwrap_or_default();
                let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
                // X'..' works in Postgres / SQLite / MySQL.
                format!("X'{hex}'")
            }
            TaggedValue::Json(j) => quote_sql_string(&j.to_string()),
        },
    }
}

fn quote_sql_string(s: &str) -> String {
    let escaped = s.replace('\'', "''");
    format!("'{escaped}'")
}

// ============================================================================
// CSV helpers (RFC 4180)
// ============================================================================

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn value_to_csv_field(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => csv_field(s),
        Value::Tagged(t) => match t {
            TaggedValue::Decimal(s)
            | TaggedValue::Date(s)
            | TaggedValue::Time(s)
            | TaggedValue::DateTime(s)
            | TaggedValue::Bytes(s) => csv_field(s),
            TaggedValue::Json(j) => csv_field(&j.to_string()),
        },
    }
}
