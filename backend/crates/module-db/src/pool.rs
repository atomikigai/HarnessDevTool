//! Lazy per-connection pool cache.
//!
//! We use sqlx `Any` so a single code path can run against SQLite, Postgres,
//! and MySQL. Drivers are installed once at process start via
//! `install_default_drivers`.

use std::sync::Arc;

use dashmap::DashMap;
use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;
use tokio::sync::Mutex;

use crate::error::{DbError, DbResult};
use crate::storage::{build_dsn, fetch_password, ConnectionsStore};
use crate::types::Connection;

/// Make sure the sqlx Any driver registry contains all three engines. Safe to
/// call repeatedly — sqlx only installs once.
pub fn install_drivers() {
    // sqlx 0.7: install_default_drivers requires the cargo features
    // (sqlite/postgres/mysql) — they're all on in this crate.
    sqlx::any::install_default_drivers();
}

#[derive(Debug, Default)]
pub struct PoolCache {
    inner: DashMap<String, AnyPool>,
    // Serializes pool creation per id to avoid duplicate connect storms.
    locks: DashMap<String, Arc<Mutex<()>>>,
}

impl PoolCache {
    pub fn new() -> Self {
        install_drivers();
        Self::default()
    }

    /// Get-or-create a pool for the given connection id. Reads the saved
    /// connection and (if any) pulls the password from keyring.
    pub async fn get_or_init(
        &self,
        store: &ConnectionsStore,
        connection_id: &str,
    ) -> DbResult<AnyPool> {
        if let Some(p) = self.inner.get(connection_id) {
            return Ok(p.clone());
        }
        // Per-id mutex.
        let lock = self
            .locks
            .entry(connection_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        if let Some(p) = self.inner.get(connection_id) {
            return Ok(p.clone());
        }
        let conn = store.get(connection_id)?;
        let pool = build_pool(&conn).await?;
        self.inner.insert(connection_id.to_string(), pool.clone());
        Ok(pool)
    }

    pub fn invalidate(&self, connection_id: &str) {
        if let Some((_, pool)) = self.inner.remove(connection_id) {
            // Drop closes async; spawn so we don't block.
            tokio::spawn(async move { pool.close().await });
        }
    }
}

pub async fn build_pool(conn: &Connection) -> DbResult<AnyPool> {
    install_drivers();
    let password = if conn.password_ref.is_some() {
        fetch_password(&conn.id)?
    } else {
        None
    };
    let dsn = build_dsn(conn, password.as_deref())?;
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
    let dsn = build_dsn(&conn, input.password.as_deref())?;
    let pool = AnyPoolOptions::new()
        .max_connections(2)
        .connect(&dsn)
        .await
        .map_err(DbError::from)?;
    Ok(pool)
}
