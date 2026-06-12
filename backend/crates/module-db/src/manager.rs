//! Public facade — orchestrates storage + pool cache + execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use sqlx::Row as _;

use crate::error::{DbError, DbResult};
use crate::lease::{classify_txn, spawn_reaper, PinnedTab, TabLeases, TxnIntent};
use crate::pool::{build_pool_for_input, build_read_only_pool, DbPool, PoolCache};
use crate::query::{QueryRegistry, RunningQuery};
use crate::row as row_ops;
use crate::schema::introspect;
use crate::storage::ConnectionsStore;
use crate::structured::{self, SelectRequest, SelectResponse};
use crate::types::{
    Connection, ConnectionInput, Engine, QueryResult, Row, SchemaTree, SchemaTreeSchema, Table,
    TableKind, TestResult,
};
use crate::value::Value;

const SCHEMA_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const DB_CONTEXT_BRIEF_MAX_BYTES: usize = 12_000;
const SQLITE_CONTEXT_ROW_COUNT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SchemaCacheKey {
    connection_id: String,
    database: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedSchemaTree {
    inserted_at: Instant,
    tree: SchemaTree,
}

pub struct Manager {
    store: ConnectionsStore,
    pools: PoolCache,
    queries: Arc<QueryRegistry>,
    leases: Arc<TabLeases>,
    schema_cache: DashMap<SchemaCacheKey, CachedSchemaTree>,
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
            schema_cache: DashMap::new(),
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
        self.invalidate_schema_cache(id);
        Ok(c)
    }

    pub fn connections_remove(&self, id: &str) -> DbResult<()> {
        self.pools.invalidate(id);
        self.invalidate_schema_cache(id);
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
        self.cached_schema_tree(connection_id, database).await
    }

    async fn cached_schema_tree(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> DbResult<SchemaTree> {
        let key = SchemaCacheKey {
            connection_id: connection_id.to_string(),
            database: database.filter(|db| !db.is_empty()).map(str::to_string),
        };
        if let Some(entry) = self.schema_cache.get(&key) {
            if entry.inserted_at.elapsed() <= SCHEMA_CACHE_TTL {
                return Ok(entry.tree.clone());
            }
        }
        let conn = self.store.get(connection_id)?;
        let pool = build_read_only_pool(&conn, database).await?;
        let tree = introspect(&pool, conn.engine, database).await?;
        self.schema_cache.insert(
            key,
            CachedSchemaTree {
                inserted_at: Instant::now(),
                tree: tree.clone(),
            },
        );
        Ok(tree)
    }

    pub fn invalidate_schema_cache(&self, connection_id: &str) {
        let keys = self
            .schema_cache
            .iter()
            .filter(|entry| entry.key().connection_id == connection_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.schema_cache.remove(&key);
        }
    }

    pub async fn schema_tree_filtered(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: Option<&str>,
    ) -> DbResult<SchemaTree> {
        if schema.is_none() && table.is_none() {
            return self.cached_schema_tree(connection_id, database).await;
        }
        let tree = self.cached_schema_tree(connection_id, database).await?;
        Ok(filter_schema_tree(tree, schema, table))
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

    pub async fn query_run_read_only(
        &self,
        connection_id: &str,
        database: Option<&str>,
        sql: &str,
        _params: Option<Vec<Value>>,
        page_size: usize,
        page: usize,
    ) -> DbResult<QueryResult> {
        let conn = self.store.get(connection_id)?;
        let pool = build_read_only_pool(&conn, database).await?;
        crate::query::run_read_only(&pool, sql, page_size.min(10_000), page, &self.queries).await
    }

    pub async fn structured_select(
        &self,
        connection_id: &str,
        database: Option<&str>,
        req: SelectRequest,
    ) -> DbResult<SelectResponse> {
        let conn = self.store.get(connection_id)?;
        let pool = build_read_only_pool(&conn, database).await?;
        structured::select(&pool, req).await
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

    pub async fn context_refresh(&self, connection_id: &str) -> DbResult<String> {
        let conn = self.store.get(connection_id)?;
        let mut tree = self.cached_schema_tree(connection_id, None).await?;
        self.populate_context_row_counts(&conn, &mut tree).await?;
        let brief = build_db_context_brief(connection_id, &conn, &tree);
        self.write_context_cache(connection_id, &brief)?;
        Ok(brief)
    }

    pub async fn context(
        &self,
        connection_id: &str,
        max_age_hours: Option<u64>,
    ) -> DbResult<String> {
        let _ = self.store.get(connection_id)?;
        let max_age = Duration::from_secs(max_age_hours.unwrap_or(24) * 60 * 60);
        if let Some(cached) = self.cached_context_if_fresh(connection_id, max_age)? {
            return Ok(cached);
        }
        self.context_refresh(connection_id).await
    }

    pub fn cached_context_if_fresh(
        &self,
        connection_id: &str,
        max_age: Duration,
    ) -> DbResult<Option<String>> {
        let path = self.context_cache_path(connection_id);
        if !path.exists() || context_cache_is_stale(&path, max_age)? {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }

    pub fn context_cache_path(&self, connection_id: &str) -> PathBuf {
        self.context_dir()
            .join(format!("{}.md", safe_cache_key(connection_id)))
    }

    fn context_dir(&self) -> PathBuf {
        self.store.root().join("context")
    }

    fn write_context_cache(&self, connection_id: &str, brief: &str) -> DbResult<()> {
        let path = self.context_cache_path(connection_id);
        let dir = path
            .parent()
            .ok_or_else(|| DbError::Internal("context cache path has no parent".into()))?;
        create_private_dir_all(dir)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, brief)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    async fn populate_context_row_counts(
        &self,
        conn: &Connection,
        tree: &mut SchemaTree,
    ) -> DbResult<()> {
        if conn.engine != Engine::Sqlite {
            return Ok(());
        }
        let pool = build_read_only_pool(conn, None).await?;
        let DbPool::Sqlite(pool) = pool else {
            return Ok(());
        };
        for schema in &mut tree.schemas {
            for table in &mut schema.tables {
                if table.kind != TableKind::Table {
                    continue;
                }
                let qtable = quote_ident(conn.engine, &table.name);
                let sql = format!("SELECT count(*) FROM {qtable}");
                let count = tokio::time::timeout(SQLITE_CONTEXT_ROW_COUNT_TIMEOUT, async {
                    sqlx::query_scalar::<_, i64>(&sql).fetch_one(&pool).await
                })
                .await;
                if let Ok(Ok(count)) = count {
                    table.row_estimate = Some(count);
                } else {
                    table.row_estimate = None;
                }
            }
        }
        Ok(())
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

fn filter_schema_tree(
    tree: SchemaTree,
    schema_filter: Option<&str>,
    table_filter: Option<&str>,
) -> SchemaTree {
    let schemas = tree
        .schemas
        .into_iter()
        .filter_map(|schema| {
            if schema_filter.is_some_and(|filter| filter != schema.name) {
                return None;
            }
            let tables = schema
                .tables
                .into_iter()
                .filter(|table| table_filter.is_none_or(|filter| filter == table.name))
                .collect::<Vec<_>>();
            if tables.is_empty() && table_filter.is_some() {
                None
            } else {
                Some(SchemaTreeSchema {
                    name: schema.name,
                    tables,
                })
            }
        })
        .collect();
    SchemaTree { schemas }
}

fn build_db_context_brief(connection_id: &str, conn: &Connection, tree: &SchemaTree) -> String {
    let mut brief = String::new();
    brief.push_str("# Database Context Pack\n\n");
    brief.push_str(&format!(
        "- Connection id: `{}`\n",
        sanitize_context_text(connection_id)
    ));
    brief.push_str(&format!(
        "- Name: `{}`\n",
        sanitize_context_text(&conn.name)
    ));
    brief.push_str(&format!("- Engine: `{}`\n", conn.engine.as_str()));
    brief.push_str(&format!(
        "- Database: `{}`\n\n",
        sanitize_context_text(&conn.database)
    ));

    for schema in &tree.schemas {
        brief.push_str(&format!(
            "## Schema `{}`\n\n",
            sanitize_context_text(&schema.name)
        ));
        let tables = schema
            .tables
            .iter()
            .filter(|table| table.kind == TableKind::Table)
            .collect::<Vec<_>>();
        let views = schema
            .tables
            .iter()
            .filter(|table| table.kind == TableKind::View)
            .collect::<Vec<_>>();
        if !tables.is_empty() {
            brief.push_str("### Tables\n\n");
            for table in tables {
                append_table_context(&mut brief, conn.engine, table);
            }
        }
        if !views.is_empty() {
            brief.push_str("### Views\n\n");
            for view in views {
                append_table_context(&mut brief, conn.engine, view);
            }
        }
    }

    brief.push_str("## Relationships\n\n");
    let mut relationships = Vec::new();
    for schema in &tree.schemas {
        for table in &schema.tables {
            for fk in &table.foreign_keys {
                relationships.push(format!(
                    "- `{}`.`{}`({}) -> `{}`({})",
                    sanitize_context_text(&schema.name),
                    sanitize_context_text(&table.name),
                    fk.cols
                        .iter()
                        .map(|col| format!("`{}`", sanitize_context_text(col)))
                        .collect::<Vec<_>>()
                        .join(", "),
                    sanitize_context_text(&fk.ref_table),
                    fk.ref_cols
                        .iter()
                        .map(|col| format!("`{}`", sanitize_context_text(col)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    if relationships.is_empty() {
        brief.push_str("- No foreign keys reported by introspection.\n");
    } else {
        brief.push_str(&relationships.join("\n"));
        brief.push('\n');
    }

    truncate_brief(brief)
}

fn append_table_context(brief: &mut String, engine: Engine, table: &Table) {
    brief.push_str(&format!(
        "#### `{}` ({})\n\n",
        sanitize_context_text(&table.name),
        match table.kind {
            TableKind::Table => "table",
            TableKind::View => "view",
        }
    ));
    brief.push_str(&format!(
        "- Row count estimate: {}\n",
        table
            .row_estimate
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unknown".into())
    ));
    let pk_cols = table
        .columns
        .iter()
        .filter(|column| column.pk)
        .map(|column| format!("`{}`", sanitize_context_text(&column.name)))
        .collect::<Vec<_>>();
    brief.push_str(&format!(
        "- Primary key: {}\n",
        if pk_cols.is_empty() {
            "none reported".into()
        } else {
            pk_cols.join(", ")
        }
    ));
    brief.push_str("- Columns:\n");
    for column in &table.columns {
        brief.push_str(&format!(
            "  - `{}` {}{}{}\n",
            sanitize_context_text(&column.name),
            sanitize_context_text(&column.r#type),
            if column.nullable {
                " nullable"
            } else {
                " not null"
            },
            if column.pk { " pk" } else { "" }
        ));
    }
    if !table.foreign_keys.is_empty() {
        brief.push_str("- Foreign keys:\n");
        for fk in &table.foreign_keys {
            brief.push_str(&format!(
                "  - {} -> `{}`({})\n",
                fk.cols
                    .iter()
                    .map(|col| format!("`{}`", sanitize_context_text(col)))
                    .collect::<Vec<_>>()
                    .join(", "),
                sanitize_context_text(&fk.ref_table),
                fk.ref_cols
                    .iter()
                    .map(|col| format!("`{}`", sanitize_context_text(col)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let _ = engine;
    brief.push('\n');
}

fn context_cache_is_stale(path: &Path, max_age: Duration) -> DbResult<bool> {
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::ZERO)
        > max_age)
}

fn safe_cache_key(raw: &str) -> String {
    let mut safe = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    safe.truncate(48);
    let hash = short_value_hash(raw);
    if safe.is_empty() {
        format!("db-{hash}")
    } else {
        format!("{safe}-{hash}")
    }
}

fn short_value_hash(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    format!(
        "{:08x}",
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    )
}

fn sanitize_context_text(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let mut backticks = 0usize;
    for ch in value.chars() {
        if ch == '`' {
            backticks += 1;
            continue;
        }
        push_sanitized_backticks(&mut sanitized, backticks);
        backticks = 0;
        sanitized.push(ch);
    }
    push_sanitized_backticks(&mut sanitized, backticks);
    sanitized
        .replace("<!-- BEGIN", "<!-- BEGIN (sanitized)")
        .replace("<!-- END", "<!-- END (sanitized)")
}

fn push_sanitized_backticks(output: &mut String, count: usize) {
    if count == 0 {
        return;
    }
    for idx in 0..count {
        output.push('`');
        if (idx + 1) % 2 == 0 && idx + 1 < count {
            output.push('\u{200b}');
        }
    }
}

fn truncate_brief(mut brief: String) -> String {
    if brief.len() <= DB_CONTEXT_BRIEF_MAX_BYTES {
        return brief;
    }
    brief.truncate(DB_CONTEXT_BRIEF_MAX_BYTES);
    while !brief.is_char_boundary(brief.len()) {
        brief.pop();
    }
    brief.push_str("\n\n[truncated: database context pack exceeded 12KB]\n");
    brief
}

fn quote_ident(engine: Engine, ident: &str) -> String {
    let quote = if engine == Engine::Mysql { '`' } else { '"' };
    let escaped = ident.replace(quote, &format!("{quote}{quote}"));
    format!("{quote}{escaped}{quote}")
}

fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
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

    #[tokio::test]
    async fn schema_tree_uses_cache_until_invalidated() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let db_path = dir.path().join("cache.sqlite");
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "cache".to_string(),
                engine: Engine::Sqlite,
                database: db_path.display().to_string(),
                ..Default::default()
            })
            .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "CREATE TABLE first (id INTEGER PRIMARY KEY)",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        let first = mgr.schema_tree(&conn.id, None).await.unwrap();
        assert!(first.schemas[0]
            .tables
            .iter()
            .any(|table| table.name == "first"));

        mgr.query_run(
            &conn.id,
            None,
            "CREATE TABLE second (id INTEGER PRIMARY KEY)",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        let cached = mgr.schema_tree(&conn.id, None).await.unwrap();
        assert!(!cached.schemas[0]
            .tables
            .iter()
            .any(|table| table.name == "second"));

        mgr.invalidate_schema_cache(&conn.id);
        let refreshed = mgr.schema_tree(&conn.id, None).await.unwrap();
        assert!(refreshed.schemas[0]
            .tables
            .iter()
            .any(|table| table.name == "second"));
    }

    #[tokio::test]
    async fn context_refresh_writes_sanitized_capped_cache_and_context_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let db_path = dir.path().join("context.sqlite");
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "ctx ``` <!-- BEGIN bad -->".to_string(),
                engine: Engine::Sqlite,
                database: db_path.display().to_string(),
                ..Default::default()
            })
            .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id), note TEXT)",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "CREATE VIEW child_names AS SELECT note FROM child",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "INSERT INTO parent (name) VALUES ('a'), ('b')",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        mgr.invalidate_schema_cache(&conn.id);

        let brief = mgr.context_refresh(&conn.id).await.unwrap();
        assert!(brief.contains("# Database Context Pack"));
        assert!(brief.contains("#### `parent`"));
        assert!(brief.contains("#### `child_names`"));
        assert!(brief.contains("Row count estimate: 2"));
        assert!(brief.contains("parent_id"));
        assert!(!brief.contains("```"));
        assert!(!brief.contains("<!-- BEGIN bad -->"));
        assert!(brief.len() <= DB_CONTEXT_BRIEF_MAX_BYTES + 64);

        let cached = mgr.context(&conn.id, Some(24)).await.unwrap();
        assert_eq!(cached, brief);
        assert_eq!(
            std::fs::read_to_string(mgr.context_cache_path(&conn.id)).unwrap(),
            brief
        );
    }

    #[test]
    fn context_brief_has_global_size_cap() {
        let conn = Connection {
            id: "conn".into(),
            name: "big".into(),
            engine: Engine::Sqlite,
            host: None,
            port: None,
            database: "db".into(),
            username: None,
            password_ref: None,
            ssl_mode: None,
            params: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let columns = (0..1000)
            .map(|idx| crate::types::Column {
                name: format!("col_{idx}_{}", "x".repeat(20)),
                r#type: "TEXT".into(),
                nullable: true,
                pk: idx == 0,
                default: None,
                kind: None,
            })
            .collect();
        let tree = SchemaTree {
            schemas: vec![SchemaTreeSchema {
                name: "main".into(),
                tables: vec![Table {
                    name: "huge".into(),
                    kind: TableKind::Table,
                    row_estimate: Some(1),
                    columns,
                    indexes: Vec::new(),
                    foreign_keys: Vec::new(),
                }],
            }],
        };

        let brief = build_db_context_brief("conn", &conn, &tree);

        assert!(brief.len() <= DB_CONTEXT_BRIEF_MAX_BYTES + 64);
        assert!(brief.contains("truncated"));
    }

    #[tokio::test]
    async fn sqlite_read_only_mode_blocks_writing_cte_shape() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = Manager::new(dir.path(), "default").unwrap();
        let db_path = dir.path().join("readonly.sqlite");
        let conn = mgr
            .connections_add(ConnectionInput {
                name: "readonly".to_string(),
                engine: Engine::Sqlite,
                database: db_path.display().to_string(),
                ..Default::default()
            })
            .unwrap();
        mgr.query_run(
            &conn.id,
            None,
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            None,
            10,
            0,
        )
        .await
        .unwrap();
        mgr.query_run(&conn.id, None, "INSERT INTO t (id) VALUES (1)", None, 10, 0)
            .await
            .unwrap();

        let err = mgr
            .query_run_read_only(
                &conn.id,
                None,
                "WITH cte AS (DELETE FROM t RETURNING *) SELECT * FROM cte",
                None,
                10,
                0,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("syntax") || err.to_string().contains("readonly"));
        let count = mgr
            .query_run(&conn.id, None, "SELECT count(*) FROM t", None, 10, 0)
            .await
            .unwrap();
        assert_eq!(count.rows[0][0], Value::Int(1));
    }

    #[test]
    fn read_only_transaction_sql_for_pg_and_mysql_is_explicit() {
        assert_eq!(
            crate::query::postgres_read_only_begin_sql(),
            "BEGIN READ ONLY"
        );
        assert_eq!(
            crate::query::mysql_read_only_begin_sql(),
            "START TRANSACTION READ ONLY"
        );
    }
}
