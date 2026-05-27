//! TOML storage for saved connections + keyring helpers for passwords.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{DbError, DbResult};
use crate::types::{Connection, ConnectionInput};

const KEYRING_SERVICE: &str = "harness-db";

/// Build the keyring username form used for a connection. We use a single
/// service `harness-db` with one entry per connection id (`harness:db:<id>`).
pub fn keyring_user(id: &str) -> String {
    format!("harness:db:{id}")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConnectionsFile {
    #[serde(default)]
    connections: Vec<Connection>,
}

pub struct ConnectionsStore {
    path: PathBuf,
}

impl ConnectionsStore {
    /// `harness_home` is `~/.harness`. We persist to
    /// `<home>/profiles/<profile>/modules/db/connections.toml`.
    pub fn new(harness_home: &Path, profile: &str) -> DbResult<Self> {
        let dir = harness_home
            .join("profiles")
            .join(profile)
            .join("modules")
            .join("db");
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("connections.toml"),
        })
    }

    pub fn list(&self) -> DbResult<Vec<Connection>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path)?;
        let parsed: ConnectionsFile =
            toml_edit::de::from_str(&text).map_err(|e| DbError::Toml(e.to_string()))?;
        Ok(parsed.connections)
    }

    pub fn get(&self, id: &str) -> DbResult<Connection> {
        self.list()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| DbError::NotFound(format!("connection {id}")))
    }

    fn write_all(&self, conns: &[Connection]) -> DbResult<()> {
        let file = ConnectionsFile {
            connections: conns.to_vec(),
        };
        let text =
            toml_edit::ser::to_string_pretty(&file).map_err(|e| DbError::Toml(e.to_string()))?;
        // Atomic write: tmp + rename.
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn add(&self, input: ConnectionInput) -> DbResult<Connection> {
        validate_input(&input)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let password_ref = if input.password.as_deref().is_some_and(|p| !p.is_empty()) {
            let user = keyring_user(&id);
            store_password(&user, input.password.as_deref().unwrap())?;
            Some(user)
        } else {
            None
        };
        let conn = Connection {
            id,
            name: input.name,
            engine: input.engine,
            host: input.host,
            port: input.port,
            database: input.database,
            username: input.username,
            password_ref,
            ssl_mode: input.ssl_mode,
            params: input.params,
            created_at: now,
            updated_at: now,
        };
        let mut all = self.list()?;
        all.push(conn.clone());
        self.write_all(&all)?;
        Ok(conn)
    }

    pub fn update(&self, id: &str, input: ConnectionInput) -> DbResult<Connection> {
        validate_input(&input)?;
        let mut all = self.list()?;
        let pos = all
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| DbError::NotFound(format!("connection {id}")))?;
        let mut updated = all[pos].clone();
        updated.name = input.name;
        updated.engine = input.engine;
        updated.host = input.host;
        updated.port = input.port;
        updated.database = input.database;
        updated.username = input.username;
        updated.ssl_mode = input.ssl_mode;
        updated.params = input.params;
        updated.updated_at = Utc::now();
        // If a new password was provided (non-empty), replace keyring entry.
        if let Some(pw) = input.password.as_deref() {
            if !pw.is_empty() {
                let user = keyring_user(id);
                store_password(&user, pw)?;
                updated.password_ref = Some(user);
            }
        }
        all[pos] = updated.clone();
        self.write_all(&all)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> DbResult<()> {
        let mut all = self.list()?;
        let before = all.len();
        all.retain(|c| c.id != id);
        if all.len() == before {
            return Err(DbError::NotFound(format!("connection {id}")));
        }
        // Best-effort keyring cleanup (ignore errors — keyring may not exist).
        let _ = delete_password(&keyring_user(id));
        self.write_all(&all)
    }
}

fn validate_input(input: &ConnectionInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Validation("name is required".into()));
    }
    if input.database.trim().is_empty() {
        return Err(DbError::Validation("database is required".into()));
    }
    Ok(())
}

/// Try to fetch a password from the keyring. Returns `Ok(None)` if no entry
/// is set; `Ok(Some(...))` for an entry; `Err` only on backend failure.
pub fn fetch_password(id: &str) -> DbResult<Option<String>> {
    let user = keyring_user(id);
    let entry = match keyring::Entry::new(KEYRING_SERVICE, &user) {
        Ok(e) => e,
        Err(e) => return Err(DbError::Keyring(e.to_string())),
    };
    match entry.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        // On systems without a keyring service (CI, containers without
        // dbus/secret-service) treat as "no password configured" so the rest
        // of the manager keeps working for SQLite and password-less DBs.
        Err(keyring::Error::PlatformFailure(_)) | Err(keyring::Error::NoStorageAccess(_)) => {
            Ok(None)
        }
        Err(e) => Err(DbError::Keyring(e.to_string())),
    }
}

fn store_password(user: &str, password: &str) -> DbResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)?;
    // If the keyring is unavailable, fail loudly — we promised storage.
    entry.set_password(password)?;
    Ok(())
}

fn delete_password(user: &str) -> DbResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)?;
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(DbError::Keyring(e.to_string())),
    }
}

/// Build a sqlx DSN for a connection. Pulls password from keyring at call
/// time. Used by `pool::PoolCache` and by `connections_test`.
///
/// `database_override` replaces the connection's saved `database` for
/// Postgres/MySQL (e.g. when the UI's database dropdown picks a different
/// database on the same server). SQLite ignores the override because each
/// SQLite connection maps 1:1 to a file path.
pub fn build_dsn(
    conn: &Connection,
    password: Option<&str>,
    database_override: Option<&str>,
) -> DbResult<String> {
    use crate::types::Engine;
    // For Postgres/MySQL, prefer the override (if non-empty) over the saved
    // database. SQLite always uses the saved file path.
    let effective_db = |saved: &str| -> String {
        match database_override {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => saved.to_string(),
        }
    };
    match conn.engine {
        Engine::Sqlite => {
            // `database` is a filesystem path.
            // Pass the path verbatim; sqlx's sqlite driver expects `sqlite://<path>` or `sqlite:<path>`.
            // We use the URI form with `?mode=rwc` so the file is created if missing.
            let path = &conn.database;
            let mut extras = String::new();
            for (k, v) in &conn.params {
                extras.push('&');
                extras.push_str(&urlencoded(k));
                extras.push('=');
                extras.push_str(&urlencoded(v));
            }
            Ok(format!("sqlite://{path}?mode=rwc{extras}"))
        }
        Engine::Postgres => {
            let host = conn.host.as_deref().unwrap_or("localhost");
            let port = conn.port.unwrap_or(5432);
            let db = effective_db(&conn.database);
            let mut url = String::from("postgres://");
            if let Some(u) = &conn.username {
                url.push_str(&urlencoded(u));
                if let Some(p) = password {
                    url.push(':');
                    url.push_str(&urlencoded(p));
                }
                url.push('@');
            }
            url.push_str(host);
            url.push(':');
            url.push_str(&port.to_string());
            url.push('/');
            url.push_str(&urlencoded(&db));
            let mut sep = '?';
            if let Some(ssl) = conn.ssl_mode {
                url.push(sep);
                sep = '&';
                url.push_str(&format!("sslmode={}", ssl_str(ssl)));
            }
            for (k, v) in &conn.params {
                url.push(sep);
                sep = '&';
                url.push_str(&urlencoded(k));
                url.push('=');
                url.push_str(&urlencoded(v));
            }
            // Force UTF-8 client encoding unless the user set it explicitly,
            // so accented values round-trip in the result grid / exports.
            if !conn.params.keys().any(|k| k.eq_ignore_ascii_case("client_encoding")) {
                url.push(sep);
                sep = '&';
                url.push_str("client_encoding=UTF8");
            }
            let _ = sep;
            Ok(url)
        }
        Engine::Mysql => {
            let host = conn.host.as_deref().unwrap_or("localhost");
            let port = conn.port.unwrap_or(3306);
            let db = effective_db(&conn.database);
            let mut url = String::from("mysql://");
            if let Some(u) = &conn.username {
                url.push_str(&urlencoded(u));
                if let Some(p) = password {
                    url.push(':');
                    url.push_str(&urlencoded(p));
                }
                url.push('@');
            }
            url.push_str(host);
            url.push(':');
            url.push_str(&port.to_string());
            url.push('/');
            url.push_str(&urlencoded(&db));
            let mut sep = '?';
            for (k, v) in &conn.params {
                url.push(sep);
                sep = '&';
                url.push_str(&urlencoded(k));
                url.push('=');
                url.push_str(&urlencoded(v));
            }
            // Force utf8mb4 unless the user set charset explicitly.
            if !conn.params.keys().any(|k| k.eq_ignore_ascii_case("charset")) {
                url.push(sep);
                sep = '&';
                url.push_str("charset=utf8mb4");
            }
            let _ = sep;
            Ok(url)
        }
    }
}

fn ssl_str(s: crate::types::SslMode) -> &'static str {
    match s {
        crate::types::SslMode::Disable => "disable",
        crate::types::SslMode::Prefer => "prefer",
        crate::types::SslMode::Require => "require",
    }
}

/// Minimal URL-encoder for DSN components. Avoids pulling in `url` just for
/// this — covers the unreserved ASCII set plus a few common chars in
/// usernames/db names.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the input shape from an existing connection (for the "test stored
/// connection" path, where we re-create the DSN from saved fields + keyring).
pub fn input_from_connection(conn: &Connection) -> ConnectionInput {
    ConnectionInput {
        name: conn.name.clone(),
        engine: conn.engine,
        host: conn.host.clone(),
        port: conn.port,
        database: conn.database.clone(),
        username: conn.username.clone(),
        password: None,
        ssl_mode: conn.ssl_mode,
        params: conn.params.clone(),
    }
}

/// Materialize a `Connection` (without persisting it) from a `ConnectionInput`
/// — used by `connections_test` for the unsaved-input path.
pub fn ephemeral_connection(input: &ConnectionInput) -> Connection {
    let now = Utc::now();
    Connection {
        id: "ephemeral".to_string(),
        name: input.name.clone(),
        engine: input.engine,
        host: input.host.clone(),
        port: input.port,
        database: input.database.clone(),
        username: input.username.clone(),
        password_ref: None,
        ssl_mode: input.ssl_mode,
        params: input.params.clone(),
        created_at: now,
        updated_at: now,
    }
}

/// Helper for tests / debug printing — list keys without secrets.
#[allow(dead_code)]
pub fn redacted_summary(conn: &Connection) -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    m.insert("id", conn.id.clone());
    m.insert("engine", conn.engine.as_str().to_string());
    m.insert(
        "host",
        conn.host.clone().unwrap_or_else(|| "-".to_string()),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Engine, SslMode};
    use chrono::Utc;

    fn base(engine: Engine, database: &str) -> Connection {
        let now = Utc::now();
        Connection {
            id: "test".into(),
            name: "t".into(),
            engine,
            host: Some("db.example.com".into()),
            port: None,
            database: database.into(),
            username: Some("alice".into()),
            password_ref: None,
            ssl_mode: None,
            params: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn postgres_dsn_uses_saved_database_without_override() {
        let c = base(Engine::Postgres, "appdb");
        let dsn = build_dsn(&c, Some("s3cret"), None).unwrap();
        assert!(
            dsn.starts_with("postgres://alice:s3cret@db.example.com:5432/appdb"),
            "got: {dsn}"
        );
        assert!(dsn.contains("client_encoding=UTF8"), "got: {dsn}");
    }

    #[test]
    fn postgres_dsn_honors_database_override() {
        let mut c = base(Engine::Postgres, "appdb");
        c.ssl_mode = Some(SslMode::Require);
        let dsn = build_dsn(&c, None, Some("analytics")).unwrap();
        assert!(
            dsn.starts_with("postgres://alice@db.example.com:5432/analytics"),
            "got: {dsn}"
        );
        assert!(dsn.contains("sslmode=require"), "got: {dsn}");
        // saved database name must not leak when overridden.
        assert!(!dsn.contains("/appdb"), "got: {dsn}");
    }

    #[test]
    fn mysql_dsn_honors_database_override() {
        let c = base(Engine::Mysql, "appdb");
        let dsn = build_dsn(&c, Some("pw"), Some("reports")).unwrap();
        assert!(
            dsn.starts_with("mysql://alice:pw@db.example.com:3306/reports"),
            "got: {dsn}"
        );
        assert!(dsn.contains("charset=utf8mb4"), "got: {dsn}");
    }

    #[test]
    fn mysql_dsn_uses_saved_when_override_is_empty() {
        let c = base(Engine::Mysql, "appdb");
        let dsn = build_dsn(&c, None, Some("")).unwrap();
        assert!(
            dsn.starts_with("mysql://alice@db.example.com:3306/appdb"),
            "got: {dsn}"
        );
        assert!(dsn.contains("charset=utf8mb4"), "got: {dsn}");
    }

    #[test]
    fn sqlite_dsn_ignores_database_override() {
        let c = base(Engine::Sqlite, "/tmp/app.sqlite");
        let with = build_dsn(&c, None, Some("other.sqlite")).unwrap();
        let without = build_dsn(&c, None, None).unwrap();
        assert_eq!(with, without);
        assert_eq!(with, "sqlite:///tmp/app.sqlite?mode=rwc");
    }
}
