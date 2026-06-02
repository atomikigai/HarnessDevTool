//! Public facade — orchestrates storage + pool cache + execution.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sqlx::Row as _;

use crate::error::{DbError, DbResult};
use crate::lease::{classify_txn, spawn_reaper, PinnedTab, TabLeases, TxnIntent};
use crate::pool::{build_pool_for_input, build_read_only_pool, DbPool, PoolCache};
use crate::query::{QueryRegistry, RunningQuery};
use crate::row as row_ops;
use crate::schema::introspect;
use crate::storage::ConnectionsStore;
use crate::types::{Connection, ConnectionInput, Engine, QueryResult, Row, SchemaTree, TestResult};
use crate::value::Value;

pub struct Manager {
    store: ConnectionsStore,
    pools: PoolCache,
    queries: Arc<QueryRegistry>,
    leases: Arc<TabLeases>,
}

impl Manager {
    pub fn new(harness_home: &Path, profile: &str) -> DbResult<Self> {
        let store = ConnectionsStore::new(harness_home, profile)?;
        let leases = Arc::new(TabLeases::new());
        // Start the idle reaper — but only if a Tokio runtime is available.
        // The harness-server has one (axum); the harness-mcp-server is a
        // sync stdio loop and would panic on `tokio::spawn`. Without the
        // reaper, leases simply never auto-expire — fine for short-lived
        // MCP child processes that never pin tabs anyway.
        if tokio::runtime::Handle::try_current().is_ok() {
            std::mem::drop(spawn_reaper(leases.clone()));
        } else {
            tracing::debug!(
                "module-db Manager: no tokio runtime detected, lease idle reaper disabled"
            );
        }
        Ok(Self {
            store,
            pools: PoolCache::new(),
            queries: QueryRegistry::new(),
            leases,
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
        if matches!(conn.engine, Engine::Sqlite) {
            if !std::path::Path::new(&conn.database).exists() {
                return Err(DbError::NotFound(format!("sqlite file {}", conn.database)));
            }
            return Ok(vec![conn.database]);
        }
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
        let pool = build_read_only_pool(&conn, database).await?;
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
        self.query_run_with_tab(connection_id, database, None, sql, _params, page_size, page)
            .await
    }

    /// Variant that participates in the per-tab lease system (Q13).
    ///
    /// When `tab_id` is `Some(_)`:
    /// - If the SQL begins a transaction (`BEGIN` / `START TRANSACTION`), the
    ///   tab is auto-pinned to a dedicated single-connection pool BEFORE the
    ///   statement runs, so the transaction sticks to that connection.
    /// - If the SQL ends a transaction (`COMMIT` / `ROLLBACK` / `END`), it
    ///   runs on the leased pool (if any) and the lease is dropped AFTER.
    /// - Any other SQL runs on the leased pool if one exists, else falls back
    ///   to the shared pool.
    ///
    /// When `tab_id` is `None`, behaviour is identical to the legacy path —
    /// always shared pool, no auto-pin.
    #[allow(clippy::too_many_arguments)]
    pub async fn query_run_with_tab(
        &self,
        connection_id: &str,
        database: Option<&str>,
        tab_id: Option<&str>,
        sql: &str,
        _params: Option<Vec<Value>>,
        page_size: usize,
        page: usize,
    ) -> DbResult<QueryResult> {
        let ps = if page_size == 0 {
            100
        } else {
            page_size.min(10_000)
        };
        let intent = classify_txn(sql);

        // Auto-pin on BEGIN, before running the statement.
        if let Some(tid) = tab_id {
            if intent == TxnIntent::Begin && !self.leases.is_pinned(tid) {
                self.leases
                    .pin(&self.store, tid, connection_id, database)
                    .await?;
                tracing::info!(tab_id = tid, connection_id, "auto-pinned tab on BEGIN");
            }
        }

        // Pick the pool: leased if the tab has one, else shared.
        let pool = if let Some(tid) = tab_id {
            match self.leases.pool_for(tid).await {
                Some(p) => p,
                None => {
                    self.pools
                        .get_or_init_for(&self.store, connection_id, database)
                        .await?
                }
            }
        } else {
            self.pools
                .get_or_init_for(&self.store, connection_id, database)
                .await?
        };

        let result = crate::query::run(&pool, sql, ps, page, &self.queries).await;

        // Auto-unpin on COMMIT/ROLLBACK, regardless of result — the txn is
        // closed either way.
        if let Some(tid) = tab_id {
            if intent == TxnIntent::End && self.leases.is_pinned(tid) {
                self.leases.unpin(tid);
                tracing::info!(tab_id = tid, "auto-unpinned tab on COMMIT/ROLLBACK");
            }
        }

        result
    }

    // ---- Tab leases (Q13) --------------------------------------------------

    /// Manually pin a tab to a dedicated single-connection pool. Idempotent —
    /// re-pinning the same `tab_id` replaces the old lease.
    pub async fn tab_pin(
        &self,
        tab_id: &str,
        connection_id: &str,
        database: Option<&str>,
    ) -> DbResult<()> {
        self.leases
            .pin(&self.store, tab_id, connection_id, database)
            .await
    }

    /// Manually release a tab lease. Returns whether something was released.
    pub fn tab_unpin(&self, tab_id: &str) -> bool {
        self.leases.unpin(tab_id)
    }

    pub fn tabs_pinned(&self) -> Vec<PinnedTab> {
        self.leases.snapshot()
    }

    pub fn tab_is_pinned(&self, tab_id: &str) -> bool {
        self.leases.is_pinned(tab_id)
    }

    /// Best-effort query cancel. Returns Ok(true) if a cancel was attempted,
    /// Ok(false) if nothing to do (unknown id), Err on backend failure.
    pub async fn query_cancel(&self, query_id: &str) -> DbResult<bool> {
        let Some((_, rq)) = self.queries.inner.remove(query_id) else {
            return Ok(false);
        };
        let RunningQuery {
            engine,
            backend_pid,
            pool,
        } = rq;
        match (engine, backend_pid) {
            (Engine::Postgres, Some(pid)) => {
                let DbPool::Postgres(p) = pool else {
                    return Ok(false);
                };
                let pid = i32::try_from(pid)
                    .map_err(|_| DbError::Validation("invalid postgres backend pid".into()))?;
                sqlx::query("SELECT pg_cancel_backend($1)")
                    .bind(pid)
                    .execute(&p)
                    .await?;
                Ok(true)
            }
            (Engine::Mysql, Some(pid)) => {
                let DbPool::Mysql(p) = pool else {
                    return Ok(false);
                };
                if pid < 0 {
                    return Err(DbError::Validation("invalid mysql connection id".into()));
                }
                let sql = format!("KILL QUERY {pid}");
                sqlx::query(&sql).execute(&p).await?;
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

    /// Export a table or schema to JSON / SQL INSERT / CSV. See
    /// `module_db::export` for the request/response shapes and the
    /// per-format rules (CSV refuses schema targets, SQL batches 500
    /// rows/statement, hard 5M-row safety cap on the data path).
    pub async fn export(
        &self,
        connection_id: &str,
        req: crate::export::ExportRequest,
    ) -> DbResult<crate::export::ExportResult> {
        let conn = self.store.get(connection_id)?;
        let pool = build_read_only_pool(&conn, req.database.as_deref()).await?;
        crate::export::run_export(&pool, req.database.as_deref(), &req).await
    }

    /// Run an `EXPLAIN`-style query. Engine-specific prefix.
    pub async fn explain(
        &self,
        connection_id: &str,
        database: Option<&str>,
        sql: &str,
    ) -> DbResult<QueryResult> {
        let conn = self.store.get(connection_id)?;
        let prefix = match conn.engine {
            Engine::Sqlite => "EXPLAIN QUERY PLAN",
            Engine::Postgres => "EXPLAIN",
            Engine::Mysql => "EXPLAIN",
        };
        let wrapped = format!("{prefix} {sql}");
        self.query_run(connection_id, database, &wrapped, None, 1000, 0)
            .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn query_cancel_unknown_id_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();

        assert!(!mgr.query_cancel("missing").await.unwrap());
    }

    #[tokio::test]
    async fn sqlite_databases_list_does_not_create_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let db_path = dir.path().join("missing.sqlite");
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "missing".to_string(),
                engine: Engine::Sqlite,
                host: None,
                port: None,
                database: db_path.display().to_string(),
                username: None,
                password: None,
                ssl_mode: None,
                params: Default::default(),
            })
            .unwrap();

        let err = mgr.databases_list(&conn.id).await.unwrap_err();

        assert!(matches!(err, DbError::NotFound(_)));
        assert!(!db_path.exists());
    }

    #[tokio::test]
    async fn query_cancel_removes_registered_sqlite_query() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        mgr.queries.inner.insert(
            "q1".to_string(),
            RunningQuery {
                engine: Engine::Sqlite,
                backend_pid: Some(1),
                pool: DbPool::Sqlite(pool),
            },
        );

        let err = mgr.query_cancel("q1").await.unwrap_err();

        assert!(matches!(err, DbError::Unsupported(_)));
        assert!(!mgr.queries.inner.contains_key("q1"));
    }
}
