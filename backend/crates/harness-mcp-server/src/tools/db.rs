//! `db.*` MCP tools. Thin wrappers around `module_db::Manager`.
//!
//! Approval policy (informational — enforcement lives in the harness's
//! approval layer): `db_query` is gated on the leading SQL keyword being
//! `SELECT` (or `EXPLAIN`/`SHOW`). Other keywords are flagged
//! `requires_approval: true` in the response so the harness can prompt.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;
use module_db::{Engine, ExportFormat, ExportRequest, ExportScope, ExportTarget, Manager};
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
    let mut path = dir.join(filename);
    if !path.exists() {
        return path;
    }
    let stem = filename.trim_end_matches(".sql");
    for i in 1.. {
        path = dir.join(format!("{stem}-{i}.sql"));
        if !path.exists() {
            return path;
        }
    }
    unreachable!("unbounded suffix loop must return before overflow")
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
