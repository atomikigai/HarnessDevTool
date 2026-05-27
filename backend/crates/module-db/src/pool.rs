//! Lazy per-(connection, database) pool cache.
//!
//! We use sqlx `Any` so a single code path can run against SQLite, Postgres,
//! and MySQL. Drivers are installed once at process start via
//! `install_default_drivers`.
//!
//! The cache key is `(connection_id, Option<database>)` so the `/db` UI's
//! database dropdown actually routes queries to the chosen database on the
//! same server. SQLite ignores the override (one file = one DB) and always
//! lands in the `None` slot.

use std::sync::Arc;

use dashmap::DashMap;
use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;
use tokio::sync::Mutex;

use crate::error::{DbError, DbResult};
use crate::storage::{build_dsn, fetch_password, ConnectionsStore};
use crate::types::{Connection, Engine};

/// Make sure the sqlx Any driver registry contains all three engines. Safe to
/// call repeatedly — sqlx only installs once.
pub fn install_drivers() {
    // sqlx 0.7: install_default_drivers requires the cargo features
    // (sqlite/postgres/mysql) — they're all on in this crate.
    sqlx::any::install_default_drivers();
}

/// Compound key: `(connection_id, database_override)`. `None` means "use the
/// connection's saved default database".
type PoolKey = (String, Option<String>);

#[derive(Debug, Default)]
pub struct PoolCache {
    inner: DashMap<PoolKey, AnyPool>,
    // Serializes pool creation per key to avoid duplicate connect storms.
    locks: DashMap<PoolKey, Arc<Mutex<()>>>,
}

impl PoolCache {
    pub fn new() -> Self {
        install_drivers();
        Self::default()
    }

    /// Get-or-create the pool bound to the connection's saved default
    /// database. Convenience wrapper around `get_or_init_for(.., None)` for
    /// callers that don't care about per-database routing (e.g. listing
    /// databases on the server).
    pub async fn get_or_init(
        &self,
        store: &ConnectionsStore,
        connection_id: &str,
    ) -> DbResult<AnyPool> {
        self.get_or_init_for(store, connection_id, None).await
    }

    /// Get-or-create a pool for the given connection id, optionally routed to
    /// a specific database on that server (Postgres/MySQL only — SQLite
    /// ignores the override and always returns its single-file pool).
    pub async fn get_or_init_for(
        &self,
        store: &ConnectionsStore,
        connection_id: &str,
        database: Option<&str>,
    ) -> DbResult<AnyPool> {
        // For SQLite, collapse all (id, *) to (id, None) so we don't churn
        // pools — the override is meaningless there.
        let conn = store.get(connection_id)?;
        let key_db = match conn.engine {
            Engine::Sqlite => None,
            _ => database
                .filter(|d| !d.is_empty())
                .map(|d| d.to_string()),
        };
        let key: PoolKey = (connection_id.to_string(), key_db.clone());

        if let Some(p) = self.inner.get(&key) {
            return Ok(p.clone());
        }
        // Per-key mutex.
        let lock = self
            .locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        if let Some(p) = self.inner.get(&key) {
            return Ok(p.clone());
        }
        let pool = build_pool(&conn, key_db.as_deref()).await?;
        self.inner.insert(key, pool.clone());
        Ok(pool)
    }

    /// Drop every cached pool for `connection_id` regardless of which database
    /// override it was opened with — credentials/DSN may have changed for all
    /// of them.
    pub fn invalidate(&self, connection_id: &str) {
        let keys: Vec<PoolKey> = self
            .inner
            .iter()
            .filter(|e| e.key().0 == connection_id)
            .map(|e| e.key().clone())
            .collect();
        for k in keys {
            if let Some((_, pool)) = self.inner.remove(&k) {
                // Drop closes async; spawn so we don't block.
                tokio::spawn(async move { pool.close().await });
            }
        }
    }
}

pub async fn build_pool(conn: &Connection, database_override: Option<&str>) -> DbResult<AnyPool> {
    install_drivers();
    let password = if conn.password_ref.is_some() {
        fetch_password(&conn.id)?
    } else {
        None
    };
    let dsn = build_dsn(conn, password.as_deref(), database_override)?;
    let pool = AnyPoolOptions::new()
        .max_connections(8)
        .connect(&dsn)
        .await
        .map_err(DbError::from)?;
    Ok(pool)
}

/// One-off pool for a `ConnectionInput` (test-without-saving).
pub async fn build_pool_for_input(input: &crate::types::ConnectionInput) -> DbResult<AnyPool> {
    install_drivers();
    let conn = crate::storage::ephemeral_connection(input);
    let dsn = build_dsn(&conn, input.password.as_deref(), None)?;
    let pool = AnyPoolOptions::new()
        .max_connections(2)
        .connect(&dsn)
        .await
        .map_err(DbError::from)?;
    Ok(pool)
}
