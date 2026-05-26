use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use harness_core::Store;
use harness_session::{AgentKind, Manager};
use tokio::sync::broadcast;

use crate::config::Config;

/// Shared application state.
#[derive(Debug)]
pub struct AppState {
    pub store: Arc<Store>,
    pub manager: Arc<Manager>,
    /// Detected absolute paths for agent CLIs. Missing entries mean the binary
    /// was not on `PATH` at boot; spawn attempts for those kinds return 400.
    pub binaries: HashMap<AgentKind, PathBuf>,
    pub start_time: Instant,
    pub version: &'static str,
    pub tick_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let store = Arc::new(Store::new(&cfg.home)?);
        let sessions_root = cfg.home.join("profiles").join("default").join("sessions");
        let manager = Arc::new(Manager::new(sessions_root)?);
        let binaries = detect_binaries();
        let (tick_tx, _) = broadcast::channel(64);
        Ok(Self {
            store,
            manager,
            binaries,
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
            tick_tx,
        })
    }
}

fn detect_binaries() -> HashMap<AgentKind, PathBuf> {
    let mut out = HashMap::new();
    for (kind, name) in [(AgentKind::Claude, "claude"), (AgentKind::Codex, "codex")] {
        match which::which(name) {
            Ok(p) => {
                tracing::info!(binary = name, path = %p.display(), "agent binary detected");
                out.insert(kind, p);
            }
            Err(_) => {
                tracing::warn!(
                    binary = name,
                    "agent binary not found on PATH; spawn requests for this kind will fail"
                );
            }
        }
    }
    out
}
