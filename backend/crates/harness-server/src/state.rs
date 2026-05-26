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
    /// `$HARNESS_HOME` — needed by the sessions route to generate per-session
    /// MCP config files.
    pub harness_home: PathBuf,
    /// Path to the `harness-mcp-server` binary used by the bridge. `None` if
    /// it could not be located; spawn then proceeds without MCP injection.
    pub mcp_server_binary: Option<PathBuf>,
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
        let mcp_server_binary = detect_mcp_server_binary();
        let (tick_tx, _) = broadcast::channel(64);
        Ok(Self {
            store,
            manager,
            binaries,
            harness_home: cfg.home.clone(),
            mcp_server_binary,
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
            tick_tx,
        })
    }
}

/// Locate the `harness-mcp-server` binary.
///
/// Strategy (first hit wins):
///   1. `$HARNESS_MCP_SERVER` env var (explicit override).
///   2. `which harness-mcp-server` (PATH lookup; works once installed).
///   3. Sibling of the current executable (release/install layout).
///   4. Workspace `target/{release,debug}/harness-mcp-server` walking up
///      from the current exe (dev layout).
///
/// Returns `None` if none of the above resolves. The sessions route logs a
/// warning and spawns without MCP injection in that case.
fn detect_mcp_server_binary() -> Option<PathBuf> {
    const BIN: &str = "harness-mcp-server";

    if let Ok(p) = std::env::var("HARNESS_MCP_SERVER") {
        let p = PathBuf::from(p);
        if p.is_file() {
            tracing::info!(path = %p.display(), "mcp server: using HARNESS_MCP_SERVER");
            return Some(p);
        }
    }
    if let Ok(p) = which::which(BIN) {
        tracing::info!(path = %p.display(), "mcp server: found on PATH");
        return Some(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(BIN);
            if sibling.is_file() {
                tracing::info!(path = %sibling.display(), "mcp server: found as sibling of current exe");
                return Some(sibling);
            }
            // Walk up looking for a `target/` dir.
            let mut cur = Some(dir);
            while let Some(d) = cur {
                for profile in ["release", "debug"] {
                    let candidate = d.join("target").join(profile).join(BIN);
                    if candidate.is_file() {
                        tracing::info!(path = %candidate.display(), "mcp server: found in workspace target");
                        return Some(candidate);
                    }
                }
                cur = d.parent();
            }
        }
    }
    tracing::warn!(
        "harness-mcp-server binary not found; sessions will spawn without MCP injection"
    );
    None
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
