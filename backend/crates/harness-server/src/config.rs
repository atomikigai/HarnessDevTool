use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use harness_core::{validate_profile_id, AutonomyProfile};

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
    /// Default autonomy profile for new threads. Project/thread overrides land
    /// in the A2 follow-up; env keeps this slice simple and explicit.
    pub autonomy_profile: AutonomyProfile,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind: SocketAddr = env::var("HARNESS_BIND")
            .or_else(|_| env::var("BACKEND_PORT").map(|port| format!("127.0.0.1:{port}")))
            .unwrap_or_else(|_| "127.0.0.1:7778".to_string())
            .parse()
            .context("parsing HARNESS_BIND")?;

        let home = match env::var("HARNESS_HOME") {
            Ok(v) => PathBuf::from(v),
            Err(_) => default_home()?,
        };

        let cors_origin = env::var("HARNESS_CORS_ORIGIN")
            .or_else(|_| env::var("FRONTEND_PORT").map(|port| format!("http://localhost:{port}")))
            .unwrap_or_else(|_| "http://localhost:8081".to_string());

        let profile = resolve_profile(&home);
        let autonomy_profile = resolve_autonomy_profile();

        Ok(Self {
            bind,
            home,
            cors_origin,
            profile,
            autonomy_profile,
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
        if validate_profile_id(trimmed).is_ok() {
            return trimmed.to_string();
        } else if !trimmed.is_empty() {
            tracing::warn!("ignoring invalid HARNESS_PROFILE");
        }
    }
    let pointer = home.join("active_profile");
    if let Ok(contents) = std::fs::read_to_string(&pointer) {
        let trimmed = contents.trim();
        if validate_profile_id(trimmed).is_ok() {
            return trimmed.to_string();
        } else if !trimmed.is_empty() {
            tracing::warn!(path = %pointer.display(), "ignoring invalid active_profile");
        }
    }
    "default".to_string()
}

fn resolve_autonomy_profile() -> AutonomyProfile {
    match env::var("HARNESS_AUTONOMY_PROFILE")
        .unwrap_or_else(|_| "assisted".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "manual" => AutonomyProfile::Manual,
        "autonomous" => AutonomyProfile::Autonomous,
        "ci" => AutonomyProfile::Ci,
        _ => AutonomyProfile::Assisted,
    }
}
