use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub home: PathBuf,
    pub cors_origin: String,
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

        Ok(Self {
            bind,
            home,
            cors_origin,
        })
    }
}

fn default_home() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME env var not set")?;
    Ok(PathBuf::from(home).join(".harness"))
}
