//! Per-tab connection lease for transactions and session-local state (Q13).
//!
//! By default the manager hands out connections from a shared pool — every
//! query may use a different physical connection. That's fine for SELECT-only
//! editors, but breaks any flow that depends on session state: transactions
//! (`BEGIN .. COMMIT`), `SET search_path`, temp tables, `LISTEN/NOTIFY`.
//!
//! When a tab needs that, the harness "leases" it a dedicated pool of size 1
//! with idle/max-lifetime disabled. All queries through that pool share the
//! same physical connection until the lease is released (manually, by
//! `COMMIT`/`ROLLBACK`, or by the idle reaper).
//!
//! Trigger:
//! - Manual: `POST /api/db/tabs/:tab_id/pin` from the UI toggle.
//! - Automatic: the manager intercepts `BEGIN` / `START TRANSACTION` and
//!   pins the tab; `COMMIT` / `ROLLBACK` unpins it.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Mutex;

use crate::error::{DbError, DbResult};
use crate::pool::DbPool;
use crate::storage::{build_dsn, fetch_password, ConnectionsStore};
use crate::types::{Connection, Engine};

/// How long a lease can sit idle (no queries) before the reaper drops it.
pub const LEASE_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 min

#[derive(Debug)]
struct Lease {
    connection_id: String,
    database: Option<String>,
    pool: DbPool,
    last_used: Mutex<Instant>,
}

/// One-pool-per-tab map keyed by an opaque `tab_id` minted by the frontend.
#[derive(Debug, Default)]
pub struct TabLeases {
    inner: DashMap<String, Arc<Lease>>,
}

impl TabLeases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a dedicated single-connection pool for this tab. If a lease for
    /// the same `tab_id` already exists, it's replaced (the previous one is
    /// dropped — closing its connection asynchronously).
    pub async fn pin(
        &self,
        store: &ConnectionsStore,
        tab_id: &str,
        connection_id: &str,
        database: Option<&str>,
    ) -> DbResult<()> {
        let conn = store.get(connection_id)?;
        let database_key = match conn.engine {
            Engine::Sqlite => None,
            _ => database.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        };
        let pool = build_lease_pool(&conn, database_key.as_deref()).await?;
        let lease = Arc::new(Lease {
            connection_id: connection_id.to_string(),
            database: database_key,
            pool,
            last_used: Mutex::new(Instant::now()),
        });
        if let Some(prev) = self.inner.insert(tab_id.to_string(), lease) {
            drop_lease_async(prev);
        }
        Ok(())
    }

    /// Drop the lease (closes its pool's single connection).
    pub fn unpin(&self, tab_id: &str) -> bool {
        if let Some((_, prev)) = self.inner.remove(tab_id) {
            drop_lease_async(prev);
            true
        } else {
            false
        }
    }

    /// Whether this tab is currently pinned.
    pub fn is_pinned(&self, tab_id: &str) -> bool {
        self.inner.contains_key(tab_id)
    }

    /// Returns the leased pool plus a touch on its `last_used` timestamp.
    pub async fn pool_for(&self, tab_id: &str) -> Option<DbPool> {
        let lease = self.inner.get(tab_id)?.clone();
        let mut t = lease.last_used.lock().await;
        *t = Instant::now();
        Some(lease.pool.clone())
    }

    /// Snapshot of all active leases — used by the `GET /api/db/tabs` route.
    pub fn snapshot(&self) -> Vec<PinnedTab> {
        self.inner
            .iter()
            .map(|e| PinnedTab {
                tab_id: e.key().clone(),
                connection_id: e.value().connection_id.clone(),
                database: e.value().database.clone(),
            })
            .collect()
    }

    /// Reaper pass — drop leases idle for longer than [`LEASE_IDLE_TIMEOUT`].
    /// Returns the tab ids that were reaped.
    pub async fn reap_idle(&self) -> Vec<String> {
        let now = Instant::now();
        let mut victims = Vec::new();
        for entry in self.inner.iter() {
            let last = *entry.value().last_used.lock().await;
            if now.duration_since(last) >= LEASE_IDLE_TIMEOUT {
                victims.push(entry.key().clone());
            }
        }
        for v in &victims {
            if let Some((_, prev)) = self.inner.remove(v) {
                drop_lease_async(prev);
            }
        }
        victims
    }
}

/// Public shape of an active lease (returned by routes).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct PinnedTab {
    pub tab_id: String,
    pub connection_id: String,
    pub database: Option<String>,
}

/// Classification of a query w.r.t. session-state semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnIntent {
    /// `BEGIN`, `START TRANSACTION` — the tab must be pinned BEFORE we run it
    /// so the transaction lives on the leased connection.
    Begin,
    /// `COMMIT`, `ROLLBACK`, `END` — run on the leased pool, then unpin.
    End,
    /// Anything else.
    Plain,
}

/// Classify by leading SQL keyword. Comment-aware via [`crate::query::leading_keyword`].
pub fn classify_txn(sql: &str) -> TxnIntent {
    let kw = crate::query::leading_keyword(sql).to_ascii_uppercase();
    match kw.as_str() {
        "BEGIN" | "START" => TxnIntent::Begin,
        "COMMIT" | "ROLLBACK" | "END" => TxnIntent::End,
        _ => TxnIntent::Plain,
    }
}

/// Build a pool of `max=1, min=1` with idle/lifetime timeouts disabled.
async fn build_lease_pool(conn: &Connection, database_override: Option<&str>) -> DbResult<DbPool> {
    let password = if conn.password_ref.is_some() {
        fetch_password(&conn.id)?
    } else {
        None
    };
    let dsn = build_dsn(conn, password.as_deref(), database_override)?;
    match conn.engine {
        Engine::Sqlite => {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .min_connections(1)
                .idle_timeout(None)
                .max_lifetime(None)
                .connect(&dsn)
                .await
                .map_err(DbError::from)?;
            Ok(DbPool::Sqlite(pool))
        }
        Engine::Postgres => {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .min_connections(1)
                .idle_timeout(None)
                .max_lifetime(None)
                .connect(&dsn)
                .await
                .map_err(DbError::from)?;
            Ok(DbPool::Postgres(pool))
        }
        Engine::Mysql => {
            let pool = MySqlPoolOptions::new()
                .max_connections(1)
                .min_connections(1)
                .idle_timeout(None)
                .max_lifetime(None)
                .connect(&dsn)
                .await
                .map_err(DbError::from)?;
            Ok(DbPool::Mysql(pool))
        }
    }
}

/// Drop the lease's pool out-of-band. `DbPool::close` is async — we spawn so
/// the caller doesn't block on connection teardown.
fn drop_lease_async(lease: Arc<Lease>) {
    let pool = lease.pool.clone();
    tokio::spawn(async move {
        pool.close().await;
    });
}

/// Spawn the reaper task — call once at manager init. Returns the
/// [`tokio::task::JoinHandle`] only for tests; production code can drop it
/// and let the task run for the manager's lifetime.
pub fn spawn_reaper(leases: Arc<TabLeases>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let tick = Duration::from_secs(60);
        loop {
            tokio::time::sleep(tick).await;
            let victims = leases.reap_idle().await;
            if !victims.is_empty() {
                tracing::info!(?victims, "reaped idle DB tab leases");
            }
        }
    })
}

// Re-export so `crate::lease::Path` resolves cleanly when tests want it.
#[allow(dead_code)]
fn _path_marker(_p: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_txn_begin_variants() {
        assert_eq!(classify_txn("BEGIN"), TxnIntent::Begin);
        assert_eq!(classify_txn("begin transaction"), TxnIntent::Begin);
        assert_eq!(classify_txn("  START TRANSACTION"), TxnIntent::Begin);
        assert_eq!(classify_txn("-- c\nBEGIN"), TxnIntent::Begin);
    }

    #[test]
    fn classify_txn_end_variants() {
        assert_eq!(classify_txn("COMMIT"), TxnIntent::End);
        assert_eq!(classify_txn("rollback"), TxnIntent::End);
        assert_eq!(classify_txn("END"), TxnIntent::End);
    }

    #[test]
    fn classify_txn_plain() {
        assert_eq!(classify_txn("SELECT 1"), TxnIntent::Plain);
        assert_eq!(classify_txn("UPDATE t SET x=1"), TxnIntent::Plain);
        assert_eq!(classify_txn(""), TxnIntent::Plain);
    }

    #[tokio::test]
    async fn drop_lease_closes_pool_even_when_arc_is_shared() {
        let sqlite = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let lease = Arc::new(Lease {
            connection_id: "conn".into(),
            database: None,
            pool: DbPool::Sqlite(sqlite.clone()),
            last_used: Mutex::new(Instant::now()),
        });
        let _shared = lease.clone();

        drop_lease_async(lease);

        for _ in 0..20 {
            if sqlite.is_closed() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert!(sqlite.is_closed(), "lease pool should be closed");
    }
}
