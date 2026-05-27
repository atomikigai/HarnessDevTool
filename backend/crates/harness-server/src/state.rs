use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use dashmap::DashMap;
use harness_core::{
    ActiveSession, ActiveSessionsSource, AgentsRegistry, BudgetStore, BudgetWarning,
    BudgetWarningSink, BudgetWiring, ClaudeTranscriptReporter, CodexStubReporter, CostReporter,
    PauseFlag, RolesRegistry, Scheduler, Store, TaskStore,
};
use harness_session::{AgentKind, Manager};
use serde_json::json;
use tokio::sync::broadcast;

use crate::config::Config;

/// Shared application state.
pub struct AppState {
    pub store: Arc<Store>,
    pub manager: Arc<Manager>,
    pub tasks: Arc<TaskStore>,
    pub agents: Arc<AgentsRegistry>,
    pub roles: Arc<RolesRegistry>,
    pub pause: Arc<PauseFlag>,
    pub budgets: Arc<BudgetStore>,
    #[allow(dead_code)]
    pub scheduler: Arc<Scheduler>,
    /// Detected absolute paths for agent CLIs. Missing entries mean the binary
    /// was not on `PATH` at boot; spawn attempts for those kinds return 400.
    pub binaries: HashMap<AgentKind, PathBuf>,
    /// `$HARNESS_HOME` — needed by the sessions route to generate per-session
    /// MCP config files.
    pub harness_home: PathBuf,
    /// Path to the `harness-mcp-server` binary used by the bridge. `None` if
    /// it could not be located; spawn then proceeds without MCP injection.
    pub mcp_server_binary: Option<PathBuf>,
    /// Per-session MCP config file paths, kept for cleanup on session kill.
    /// Keyed by session id; the file lives at `<harness_home>/.runtime/mcp-configs/<id>.json`
    /// where `<id>` is a UUID we generate at config-write time (NOT the session id).
    pub mcp_configs: DashMap<String, PathBuf>,
    pub start_time: Instant,
    pub version: &'static str,
    pub tick_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let store = Arc::new(Store::new(&cfg.home)?);
        let sessions_root = cfg.home.join("profiles").join("default").join("sessions");
        let manager = Arc::new(Manager::new(sessions_root)?);
        let task_store = TaskStore::new(&cfg.home)?;
        let agents = Arc::new(AgentsRegistry::new(&cfg.home)?);
        let roles = Arc::new(RolesRegistry::load(&cfg.home)?);
        let pause = Arc::new(PauseFlag::load(&cfg.home)?);
        let budgets = Arc::new(BudgetStore::load(&cfg.home)?);
        let (tick_tx, _) = broadcast::channel(64);

        // Per-agent-kind cost reporter wiring. Keyed by `AgentKind::as_str()`
        // so `harness-core` stays free of a dependency on `harness-session`.
        let mut reporters: HashMap<String, Arc<dyn CostReporter>> = HashMap::new();
        reporters.insert(
            AgentKind::Claude.as_str().to_string(),
            Arc::new(ClaudeTranscriptReporter::new()),
        );
        reporters.insert(
            AgentKind::Codex.as_str().to_string(),
            Arc::new(CodexStubReporter),
        );

        let budget_wiring = BudgetWiring {
            store: (*budgets).clone(),
            reporters,
            sessions: Arc::new(ManagerSessionsSource {
                manager: manager.clone(),
            }),
            sink: Arc::new(TickWarningSink {
                tx: tick_tx.clone(),
            }),
        };

        // Scheduler takes the store by value; we hand it a clone so AppState
        // can also expose it through `tasks`.
        let scheduler = Arc::new(Scheduler::spawn_with_budget(
            task_store.clone(),
            agents.clone(),
            pause.clone(),
            None,
            Some(budget_wiring),
        ));
        let tasks = Arc::new(task_store);
        let binaries = detect_binaries();
        let mcp_server_binary = detect_mcp_server_binary();
        Ok(Self {
            store,
            manager,
            tasks,
            agents,
            roles,
            pause,
            budgets,
            scheduler,
            binaries,
            harness_home: cfg.home.clone(),
            mcp_server_binary,
            mcp_configs: DashMap::new(),
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
            tick_tx,
        })
    }
}

/// Bridges `harness-session::Manager` to the scheduler's budget pass without
/// requiring the pass to await an async lock — we cache the immutable
/// per-session identity on `AgentSession` itself.
struct ManagerSessionsSource {
    manager: Arc<Manager>,
}

impl ActiveSessionsSource for ManagerSessionsSource {
    fn snapshot(&self) -> Vec<ActiveSession> {
        self.manager
            .all()
            .into_iter()
            .map(|s| ActiveSession {
                thread_id: s.thread_id().to_string(),
                session_id: s.id().to_string(),
                cwd: s.cwd().to_path_buf(),
                kind: s.kind().as_str().to_string(),
            })
            .collect()
    }
}

/// Forwards `budget.warning` events onto the shared tick broadcast so they
/// reach any SSE subscriber on `/api/events` (no filter).
struct TickWarningSink {
    tx: broadcast::Sender<String>,
}

impl BudgetWarningSink for TickWarningSink {
    fn emit(&self, w: BudgetWarning) {
        let payload = json!({
            "type": "budget.warning",
            "thread_id": w.thread_id,
            "spent_usd": w.spent_usd,
            "limit_usd": w.limit_usd,
            "pct": w.pct,
        })
        .to_string();
        let _ = self.tx.send(payload);
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
