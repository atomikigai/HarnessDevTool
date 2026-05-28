use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub home: PathBuf,
    pub cors_origin: String,
    /// Active profile (workspace) id. Resolved at startup from:
    ///   1. `HARNESS_PROFILE` env var
    ///   2. `$HARNESS_HOME/active_profile` text file (one-line profile id)
    ///   3. fallback `"default"`
    ///
    /// All stores key into `$HARNESS_HOME/profiles/<profile>/…`. Switching
    /// profiles today requires a backend restart; hot-swap is a future slice.
    pub profile: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind: SocketAddr = env::var("HARNESS_BIND")
            .unwrap_or_else(|_| "127.0.0.1:7777".to_string())
            .parse()
            .context("parsing HARNESS_BIND")?;

        let home = match env::var("HARNESS_HOME") {
            Ok(v) => PathBuf::from(v),
            Err(_) => default_home()?,
        };

        let cors_origin =
            env::var("HARNESS_CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:8080".to_string());

        let profile = resolve_profile(&home);

        Ok(Self {
            bind,
            home,
            cors_origin,
            profile,
        })
    }
}

fn default_home() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME env var not set")?;
    Ok(PathBuf::from(home).join(".harness"))
}

/// Resolve the active profile id. Env var wins; otherwise read the
/// `active_profile` pointer file inside HARNESS_HOME; otherwise `default`.
fn resolve_profile(home: &std::path::Path) -> String {
    if let Ok(v) = env::var("HARNESS_PROFILE") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let pointer = home.join("active_profile");
    if let Ok(contents) = std::fs::read_to_string(&pointer) {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "default".to_string()
}
