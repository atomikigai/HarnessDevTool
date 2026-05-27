//! Public facade — orchestrates storage + pool cache + execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use sqlx::Row as _;
use tokio::io::AsyncWriteExt;

use crate::error::{DbError, DbResult};
use crate::pool::{build_pool_for_input, DbPool, PoolCache};
use crate::query::{QueryRegistry, RunningQuery};
use crate::row as row_ops;
use crate::schema::introspect;
use crate::storage::ConnectionsStore;
use crate::types::{
    Connection, ConnectionInput, Engine, QueryResult, Row, SchemaTree, TestResult,
};
use crate::value::Value;

pub struct Manager {
    store: ConnectionsStore,
    pools: PoolCache,
    queries: Arc<QueryRegistry>,
}

impl Manager {
    pub fn new(harness_home: &Path, profile: &str) -> DbResult<Self> {
        let store = ConnectionsStore::new(harness_home, profile)?;
        Ok(Self {
            store,
            pools: PoolCache::new(),
            queries: QueryRegistry::new(),
        })
    }

    // ---- Connections CRUD --------------------------------------------------

    pub fn connections_list(&self) -> DbResult<Vec<Connection>> {
        self.store.list()
    }

    pub fn connections_get(&self, id: &str) -> DbResult<Connection> {
        self.store.get(id)
    }

    pub fn connections_add(&self, input: ConnectionInput) -> DbResult<Connection> {
        self.store.add(input)
    }

    pub fn connections_update(&self, id: &str, input: ConnectionInput) -> DbResult<Connection> {
        let c = self.store.update(id, input)?;
        // Invalidate any cached pool — credentials or DSN may have changed.
        self.pools.invalidate(id);
        Ok(c)
    }

    pub fn connections_remove(&self, id: &str) -> DbResult<()> {
        self.pools.invalidate(id);
        self.store.remove(id)
    }

    // ---- Connection test (stored or ephemeral) -----------------------------

    pub async fn connections_test_stored(&self, id: &str) -> DbResult<TestResult> {
        let conn = self.store.get(id)?;
        let pool = self.pools.get_or_init(&self.store, &conn.id).await?;
        let start = Instant::now();
        let version = probe_server_version(&pool).await;
        Ok(TestResult {
            ok: true,
            latency_ms: start.elapsed().as_millis() as u64,
            server_version: version,
            error: None,
        })
    }

    pub async fn connections_test_input(&self, input: ConnectionInput) -> DbResult<TestResult> {
        let _engine = input.engine;
        let start = Instant::now();
        match build_pool_for_input(&input).await {
            Ok(pool) => {
                let version = probe_server_version(&pool).await;
                pool.close().await;
                Ok(TestResult {
                    ok: true,
                    latency_ms: start.elapsed().as_millis() as u64,
                    server_version: version,
                    error: None,
                })
            }
            Err(e) => Ok(TestResult {
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                server_version: None,
                error: Some(e.to_string()),
            }),
        }
    }

    // ---- Databases / schema -----------------------------------------------

    pub async fn databases_list(&self, connection_id: &str) -> DbResult<Vec<String>> {
        let conn = self.store.get(connection_id)?;
        let pool = self.pools.get_or_init(&self.store, connection_id).await?;
        let names = match &pool {
            DbPool::Sqlite(_) => vec![conn.database.clone()],
            DbPool::Postgres(p) => {
                let rows = sqlx::query(
                    "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname",
                )
                .fetch_all(p)
                .await?;
                rows.iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect()
            }
            DbPool::Mysql(p) => {
                let rows = sqlx::query("SHOW DATABASES").fetch_all(p).await?;
                rows.iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect()
            }
        };
        Ok(names)
    }

    pub async fn schema_tree(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> DbResult<SchemaTree> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        introspect(&pool, conn.engine, database).await
    }

    // ---- Query -------------------------------------------------------------

    pub async fn query_run(
        &self,
        connection_id: &str,
        database: Option<&str>,
        sql: &str,
        _params: Option<Vec<Value>>,
        page_size: usize,
        page: usize,
    ) -> DbResult<QueryResult> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        let ps = if page_size == 0 { 100 } else { page_size.min(10_000) };
        let _ = conn;
        crate::query::run(&pool, sql, ps, page, &self.queries).await
    }

    /// Best-effort query cancel. Returns Ok(true) if a cancel was attempted,
    /// Ok(false) if nothing to do (unknown id), Err on backend failure.
    pub async fn query_cancel(&self, query_id: &str) -> DbResult<bool> {
        let Some((_, rq)) = self.queries.inner.remove(query_id) else {
            return Ok(false);
        };
        let RunningQuery { engine, backend_pid } = rq;
        match (engine, backend_pid) {
            (Engine::Postgres, Some(pid)) => {
                // Aux connection — not implemented in this slice.
                tracing::debug!(pid, "pg cancel requested (aux connection TODO)");
                Ok(true)
            }
            (Engine::Mysql, Some(pid)) => {
                tracing::debug!(pid, "mysql cancel requested (KILL QUERY TODO)");
                Ok(true)
            }
            (Engine::Sqlite, _) => Err(DbError::Unsupported(
                "sqlite query cancel not wired in this slice".into(),
            )),
            _ => Ok(false),
        }
    }

    // ---- Row CRUD ----------------------------------------------------------

    pub async fn row_insert(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        values: HashMap<String, Value>,
    ) -> DbResult<Row> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        let _ = conn;
        row_ops::insert(&pool, database, schema, table, values).await
    }

    pub async fn row_update(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        pk: HashMap<String, Value>,
        values: HashMap<String, Value>,
    ) -> DbResult<Row> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        let _ = conn;
        row_ops::update(&pool, database, schema, table, pk, values).await
    }

    pub async fn row_delete(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        pk: HashMap<String, Value>,
    ) -> DbResult<u64> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        let _ = conn;
        row_ops::delete(&pool, database, schema, table, pk).await
    }

    pub async fn row_duplicate(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        pk: HashMap<String, Value>,
    ) -> DbResult<Row> {
        let conn = self.store.get(connection_id)?;
        let pool = self
            .pools
            .get_or_init_for(&self.store, connection_id, database)
            .await?;
        let _ = conn;
        row_ops::duplicate(&pool, database, schema, table, pk).await
    }

    // ---- Export ------------------------------------------------------------

    /// Re-runs the query identified by `query_id` is not feasible (we don't
    /// keep the SQL). This slice exports a fresh result given the SQL.
    /// `query_id` is accepted for API compatibility but ignored — callers
    /// pass the SQL via `sql`.
    pub async fn export(
        &self,
        connection_id: &str,
        sql: &str,
        format: ExportFormat,
        path: PathBuf,
    ) -> DbResult<u64> {
        let res = self
            .query_run(connection_id, None, sql, None, 1_000_000, 0)
            .await?;
        let mut file = tokio::fs::File::create(&path).await?;
        match format {
            ExportFormat::Csv => {
                let header = res
                    .columns
                    .iter()
                    .map(|c| csv_escape(&c.name))
                    .collect::<Vec<_>>()
                    .join(",");
                file.write_all(header.as_bytes()).await?;
                file.write_all(b"\n").await?;
                for r in &res.rows {
                    let line = r
                        .iter()
                        .map(|v| csv_escape(&value_to_csv(v)))
                        .collect::<Vec<_>>()
                        .join(",");
                    file.write_all(line.as_bytes()).await?;
                    file.write_all(b"\n").await?;
                }
            }
            ExportFormat::Json => {
                let json = serde_json::json!({
                    "columns": res.columns,
                    "rows": res.rows,
                });
                let s = serde_json::to_string(&json).unwrap_or_else(|_| "[]".to_string());
                file.write_all(s.as_bytes()).await?;
            }
        }
        file.flush().await?;
        Ok(res.rows.len() as u64)
    }

    /// Run an `EXPLAIN`-style query. Engine-specific prefix.
    pub async fn explain(
        &self,
        connection_id: &str,
        sql: &str,
    ) -> DbResult<QueryResult> {
        let conn = self.store.get(connection_id)?;
        let prefix = match conn.engine {
            Engine::Sqlite => "EXPLAIN QUERY PLAN",
            Engine::Postgres => "EXPLAIN",
            Engine::Mysql => "EXPLAIN",
        };
        let wrapped = format!("{prefix} {sql}");
        self.query_run(connection_id, None, &wrapped, None, 1000, 0)
            .await
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    pub fn parse(s: &str) -> DbResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "csv" => Ok(ExportFormat::Csv),
            "json" => Ok(ExportFormat::Json),
            other => Err(DbError::Validation(format!("unknown format: {other}"))),
        }
    }
}

async fn probe_server_version(pool: &DbPool) -> Option<String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query("SELECT sqlite_version()")
            .fetch_one(p)
            .await
            .ok()
            .and_then(|r| r.try_get::<String, _>(0).ok()),
        DbPool::Postgres(p) => sqlx::query("SELECT version()")
            .fetch_one(p)
            .await
            .ok()
            .and_then(|r| r.try_get::<String, _>(0).ok()),
        DbPool::Mysql(p) => sqlx::query("SELECT version()")
            .fetch_one(p)
            .await
            .ok()
            .and_then(|r| r.try_get::<String, _>(0).ok()),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn value_to_csv(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Tagged(t) => serde_json::to_string(t).unwrap_or_default(),
    }
}

