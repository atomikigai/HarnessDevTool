use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
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
    /// Shared bearer token for mutating HTTP routes. Optional for loopback
    /// development, required when the backend listens on a non-loopback IP.
    pub api_token: Option<String>,
    /// Daily background evolution scheduler. Disabled by default.
    pub evolution: EvolutionConfig,
}

#[derive(Debug, Clone)]
pub struct EvolutionConfig {
    pub enabled: bool,
    pub panama_hour: u32,
    pub panama_minute: u32,
    pub idle_only: bool,
    pub observation_limit: usize,
    pub curator_dry_run: bool,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            panama_hour: 7,
            panama_minute: 0,
            idle_only: true,
            observation_limit: 100,
            curator_dry_run: true,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind: SocketAddr = env::var("HARNESS_BIND")
            .or_else(|_| env::var("BACKEND_PORT").map(|port| format!("127.0.0.1:{port}")))
            .unwrap_or_else(|_| "127.0.0.1:43177".to_string())
            .parse()
            .context("parsing HARNESS_BIND")?;

        let home = match env::var("HARNESS_HOME") {
            Ok(v) => PathBuf::from(v),
            Err(_) => default_home()?,
        };

        let cors_origin = env::var("HARNESS_CORS_ORIGIN")
            .or_else(|_| env::var("FRONTEND_PORT").map(|port| format!("http://localhost:{port}")))
            .unwrap_or_else(|_| "http://localhost:43178".to_string());

        let profile = resolve_profile(&home);
        let autonomy_profile = resolve_autonomy_profile();
        let api_token = resolve_api_token(bind)?;
        let evolution = resolve_evolution_config()?;

        Ok(Self {
            bind,
            home,
            cors_origin,
            profile,
            autonomy_profile,
            api_token,
            evolution,
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

fn resolve_api_token(bind: SocketAddr) -> Result<Option<String>> {
    validate_api_token(bind, env::var("HARNESS_API_TOKEN").ok())
}

fn resolve_evolution_config() -> Result<EvolutionConfig> {
    let (panama_hour, panama_minute) = parse_hhmm(
        &env::var("HARNESS_EVOLVE_TIME_PANAMA").unwrap_or_else(|_| "07:00".to_string()),
    )?;
    Ok(EvolutionConfig {
        enabled: env_bool("HARNESS_EVOLVE_ENABLED", true),
        panama_hour,
        panama_minute,
        idle_only: env_bool("HARNESS_EVOLVE_IDLE_ONLY", true),
        observation_limit: env_usize("HARNESS_EVOLVE_OBSERVATION_LIMIT", 100).clamp(1, 200),
        curator_dry_run: env_bool("HARNESS_CURATOR_DRY_RUN", true),
    })
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_hhmm(raw: &str) -> Result<(u32, u32)> {
    let (hour, minute) = raw
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("HARNESS_EVOLVE_TIME_PANAMA must be HH:MM"))?;
    let hour = hour
        .parse::<u32>()
        .context("parsing HARNESS_EVOLVE_TIME_PANAMA hour")?;
    let minute = minute
        .parse::<u32>()
        .context("parsing HARNESS_EVOLVE_TIME_PANAMA minute")?;
    if hour > 23 || minute > 59 {
        bail!("HARNESS_EVOLVE_TIME_PANAMA must be a valid HH:MM");
    }
    Ok((hour, minute))
}

fn validate_api_token(bind: SocketAddr, raw: Option<String>) -> Result<Option<String>> {
    let token = raw
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if !bind.ip().is_loopback() && token.is_none() {
        bail!("HARNESS_API_TOKEN is required when HARNESS_BIND is not loopback");
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    #[test]
    fn api_token_optional_for_loopback_bind() {
        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777);

        assert!(validate_api_token(bind, None).unwrap().is_none());
    }

    #[test]
    fn api_token_required_for_non_loopback_bind() {
        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7777);

        assert!(validate_api_token(bind, None).is_err());
    }

    #[test]
    fn parse_hhmm_accepts_daily_schedule() {
        assert_eq!(parse_hhmm("07:00").unwrap(), (7, 0));
        assert_eq!(parse_hhmm("23:59").unwrap(), (23, 59));
        assert!(parse_hhmm("24:00").is_err());
        assert!(parse_hhmm("7am").is_err());
    }
}
