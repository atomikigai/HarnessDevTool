use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use dashmap::DashMap;
use harness_core::{
    ActiveSession, ActiveSessionsSource, AgentsRegistry, BudgetStore, BudgetWarning,
    BudgetWarningSink, BudgetWiring, ClaudeTranscriptReporter, CodexStubReporter, CostReporter,
    PauseFlag, RolesRegistry, Scheduler, SessionSpawner, SpawnRequest, SpawnResult, Store,
    StubReporter, TaskStore,
};
use harness_policy::PolicyEngine;
use harness_session::{AgentKind, Manager, SpawnOpts};
use serde_json::json;
use tokio::sync::broadcast;

use crate::approvals::ApprovalStore;
use crate::config::Config;
use crate::transcript::{TranscriptEvent, TranscriptStore, WatcherHandle};

/// Per-session transcript wiring. `bus` is cloned for each SSE subscriber;
/// `store` reads from disk for catch-up replay; `handle` aborts the tail
/// task when the session dies.
pub struct TranscriptSlot {
    pub store: Arc<TranscriptStore>,
    pub bus: broadcast::Sender<TranscriptEvent>,
    pub handle: WatcherHandle,
}

/// Shared application state.
pub struct AppState {
    pub store: Arc<Store>,
    pub manager: Arc<Manager>,
    pub tasks: Arc<TaskStore>,
    pub agents: Arc<AgentsRegistry>,
    pub roles: Arc<RolesRegistry>,
    pub pause: Arc<PauseFlag>,
    pub budgets: Arc<BudgetStore>,
    pub policy: Arc<PolicyEngine>,
    pub approvals: Arc<ApprovalStore>,
    pub db: Arc<module_db::Manager>,
    #[allow(dead_code)]
    pub scheduler: Arc<Scheduler>,
    /// Detected absolute paths for agent CLIs. Missing entries mean the binary
    /// was not on `PATH` at boot; spawn attempts for those kinds return 400.
    pub binaries: HashMap<AgentKind, PathBuf>,
    /// `$HARNESS_HOME` — needed by the sessions route to generate per-session
    /// MCP config files.
    pub harness_home: PathBuf,
    /// Active profile (workspace) id this AppState was built against. Used by
    /// the profiles routes to report current state. Switching profiles
    /// requires a backend restart today; see `routes/profiles.rs`.
    pub profile: String,
    pub autonomy_profile: harness_core::AutonomyProfile,
    /// Shared bearer token required by mutating HTTP routes when configured.
    pub api_token: Option<String>,
    /// Path to the `harness-mcp-server` binary used by the bridge. `None` if
    /// it could not be located; spawn then proceeds without MCP injection.
    pub mcp_server_binary: Option<PathBuf>,
    /// Base URL the spawned MCP server should call back into for delegating
    /// `task_create`. Derived from `Config::bind` at boot. Format:
    /// `http://<host>:<port>`.
    pub server_url: String,
    /// Per-session MCP config file paths, kept for cleanup on session kill.
    /// Keyed by session id; the file lives at `<harness_home>/.runtime/mcp-configs/<id>.json`
    /// where `<id>` is a UUID we generate at config-write time (NOT the session id).
    pub mcp_configs: Arc<DashMap<String, PathBuf>>,
    /// Per-session transcript stream wiring. Keyed by session id. Present
    /// only when the underlying CLI emits a JSONL transcript we can parse
    /// (Claude/Zeus today). Each entry holds the store (for replay), the
    /// live broadcast bus, and a handle to abort the watcher on kill.
    pub transcripts: Arc<DashMap<String, TranscriptSlot>>,
    pub start_time: Instant,
    pub version: &'static str,
    pub tick_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let profile = cfg.profile.as_str();
        tracing::info!(profile = %profile, "AppState init: using profile");

        let store = Arc::new(Store::with_profile(&cfg.home, profile)?);
        let sessions_root = cfg.home.join("profiles").join(profile).join("sessions");
        let manager = Arc::new(Manager::new(sessions_root)?);
        let task_store =
            TaskStore::with_profile(&cfg.home, profile)?.with_event_store(store.clone());
        let agents = Arc::new(AgentsRegistry::with_profile(&cfg.home, profile)?);
        let roles = Arc::new(RolesRegistry::load_for_profile(&cfg.home, profile)?);
        let pause = Arc::new(PauseFlag::load(&cfg.home)?);
        let budgets = Arc::new(BudgetStore::load_for_profile(&cfg.home, profile)?);
        let policy_path = cfg.home.join("profiles").join(profile).join("policy.toml");
        let policy = Arc::new(PolicyEngine::load(policy_path.clone()).unwrap_or_else(|e| {
            tracing::warn!(
                path = %policy_path.display(),
                error = %e,
                "failed to load policy, using default policy"
            );
            PolicyEngine::default_at(policy_path)
        }));
        let approvals = Arc::new(ApprovalStore::new());
        let db = Arc::new(
            module_db::Manager::new(&cfg.home, profile)
                .map_err(|e| anyhow::anyhow!("module-db init: {e}"))?,
        );
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
        reporters.insert(
            AgentKind::Cursor.as_str().to_string(),
            Arc::new(StubReporter::new("cursor")),
        );
        reporters.insert(
            AgentKind::Antigravity.as_str().to_string(),
            Arc::new(StubReporter::new("antigravity")),
        );
        // Zeus is virtual — no PTY of its own. Its underlying CLI today is
        // Claude, so the Claude reporter already accounts for its usage.

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

        let binaries = detect_binaries();
        let mcp_server_binary = detect_mcp_server_binary();
        // Render a loopback-friendly URL even when bind is 0.0.0.0; the MCP
        // child runs on the same machine and will be unable to reach 0.0.0.0
        // as a dest, so substitute 127.0.0.1.
        let host = if cfg.bind.ip().is_unspecified() {
            "127.0.0.1".to_string()
        } else {
            cfg.bind.ip().to_string()
        };
        let server_url = format!("http://{}:{}", host, cfg.bind.port());

        // Sub-agent spawner. The scheduler asks this for a PTY whenever it
        // claims a task or routes one to an evaluator. Without it the
        // scheduler would silently set the assignee without ever launching
        // the agent (the "phantom assignee" bug).
        let spawner = Arc::new(ManagerSpawner {
            manager: manager.clone(),
            roles: roles.clone(),
            tasks: Arc::new(task_store.clone()),
            binaries: binaries.clone(),
            mcp_server_binary: mcp_server_binary.clone(),
            harness_home: cfg.home.clone(),
            server_url: server_url.clone(),
            api_token: cfg.api_token.clone(),
            mcp_configs: Arc::new(DashMap::new()),
        });

        // Scheduler takes the store by value; we hand it a clone so AppState
        // can also expose it through `tasks`.
        let mcp_configs = spawner.mcp_configs.clone();
        let scheduler = Arc::new(Scheduler::spawn_full(
            task_store.clone(),
            agents.clone(),
            pause.clone(),
            None,
            Some(budget_wiring),
            spawner as Arc<dyn SessionSpawner>,
        ));
        let tasks = Arc::new(task_store);
        Ok(Self {
            store,
            manager,
            tasks,
            agents,
            roles,
            pause,
            budgets,
            policy,
            approvals,
            db,
            scheduler,
            binaries,
            harness_home: cfg.home.clone(),
            profile: cfg.profile.clone(),
            autonomy_profile: cfg.autonomy_profile,
            api_token: cfg.api_token.clone(),
            mcp_server_binary,
            server_url,
            mcp_configs,
            transcripts: Arc::new(DashMap::new()),
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
            .map(|s| {
                let role_value = s.role();
                let (agent_id, role) = match role_value {
                    Some(value) if value.starts_with("agent:") => {
                        (Some(value["agent:".len()..].to_string()), None)
                    }
                    Some(value) => (None, Some(value)),
                    None => (None, None),
                };
                ActiveSession {
                    thread_id: s.thread_id().to_string(),
                    session_id: s.id().to_string(),
                    cwd: s.cwd().to_path_buf(),
                    kind: s.kind().as_str().to_string(),
                    agent_id,
                    role,
                }
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

/// `SessionSpawner` impl that delegates to `harness_session::Manager`.
///
/// De-duping: if the agent already has a live session attached to this thread
/// we return `AlreadyRunning` instead of launching another PTY. The scheduler
/// will keep calling on every tick (cheap), so this guard is critical.
///
/// Role-prompt: when the role resolves in the registry we seed `SpawnOpts`
/// with the template. `Manager` injects it into the PTY after a short grace
/// period — see `manager.rs`.
struct ManagerSpawner {
    manager: Arc<Manager>,
    roles: Arc<RolesRegistry>,
    tasks: Arc<TaskStore>,
    binaries: HashMap<AgentKind, PathBuf>,
    mcp_server_binary: Option<PathBuf>,
    harness_home: PathBuf,
    /// Base URL we pass to the MCP child as `--server-url` so it can delegate
    /// `task_create` back into our HTTP store (drives the SSE `task.created`).
    server_url: String,
    api_token: Option<String>,
    mcp_configs: Arc<DashMap<String, PathBuf>>,
}

impl ManagerSpawner {
    /// Parse `agent:claude-3` / `agent:codex-1` back into an [`AgentKind`].
    fn kind_from_str(s: &str) -> Option<AgentKind> {
        match s {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "cursor" => Some(AgentKind::Cursor),
            "antigravity" => Some(AgentKind::Antigravity),
            "zeus" => Some(AgentKind::Zeus),
            _ => None,
        }
    }

    /// Find a live session for `(agent_id, thread_id)`. We tag sessions with
    /// the agent id via `SessionMeta.role` today; in lieu of a dedicated
    /// `agent_id` field we use the role-string convention `agent:<id>` set at
    /// spawn time. Falls back to thread+kind matching for legacy sessions.
    fn find_existing(&self, agent_id: &str, thread_id: &str) -> Option<String> {
        for s in self.manager.all() {
            if s.thread_id() != thread_id {
                continue;
            }
            // Block on the async meta lock briefly; this is fine in the
            // scheduler context (called at most a few times per tick).
            let role_match = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    s.meta()
                        .await
                        .role
                        .as_deref()
                        .map(|r| r == agent_id)
                        .unwrap_or(false)
                })
            });
            if role_match {
                return Some(s.id().to_string());
            }
        }
        None
    }
}

impl SessionSpawner for ManagerSpawner {
    fn spawn(&self, req: SpawnRequest) -> SpawnResult {
        if let Some(sid) = self.find_existing(&req.agent_id, &req.thread_id) {
            return SpawnResult::AlreadyRunning { session_id: sid };
        }

        let kind = match Self::kind_from_str(&req.kind) {
            Some(k) => k,
            None => return SpawnResult::Failed(format!("unknown kind: {}", req.kind)),
        };

        let binary = match self.binaries.get(&kind).cloned() {
            Some(b) => b,
            None => {
                return SpawnResult::Failed(format!("no binary detected for kind {}", req.kind))
            }
        };

        let role = match self.roles.get(&req.role) {
            Some(r) => r,
            None => {
                // We surface this loudly because the previous behavior (per
                // the bug brief) was "manager logs error but returns OK" —
                // resulting in invisible failures. With this guard we return
                // Failed and the scheduler logs at warn level.
                return SpawnResult::Failed(format!("unknown role: {}", req.role));
            }
        };

        let cwd = req
            .cwd
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        // Build SpawnOpts with the role prompt. We tag SessionMeta.role with
        // the AGENT id (not the role tag) so `find_existing` can de-dupe.
        let mut opts = SpawnOpts {
            role_prompt: Some(role.prompt_template.clone()),
            role: Some(req.agent_id.clone()),
            ..SpawnOpts::default()
        };

        let load_crawl4ai = self
            .tasks
            .latest_active(&req.thread_id)
            .ok()
            .flatten()
            .map(|task| crate::routes::sessions::task_mentions_documentation_url(&task))
            .unwrap_or(false);

        // Per-session MCP config (mirrors the REST `create_session` path so
        // sub-agents see the same `task_*` tool surface as user-spawned
        // sessions).
        let mut config_path: Option<PathBuf> = None;
        if matches!(kind, AgentKind::Claude) {
            if let Some(mcp_bin) = self.mcp_server_binary.as_ref() {
                let mcp_id = uuid::Uuid::new_v4().to_string();
                let agent_id = format!("agent:{}-{}", kind.as_str(), &mcp_id[..8]);
                let configs_dir = self.harness_home.join(".runtime").join("mcp-configs");
                if let Err(e) = std::fs::create_dir_all(&configs_dir) {
                    return SpawnResult::Failed(format!("create mcp-configs dir: {e}"));
                }
                let path = configs_dir.join(format!("{mcp_id}.json"));
                let mut mcp_args = vec![
                    "--thread".to_string(),
                    req.thread_id.clone(),
                    "--agent-id".to_string(),
                    agent_id.clone(),
                    "--harness-home".to_string(),
                    self.harness_home.display().to_string(),
                    "--server-url".to_string(),
                    self.server_url.clone(),
                    "--cwd".to_string(),
                    cwd.display().to_string(),
                ];
                mcp_args.push("--role".to_string());
                mcp_args.push(req.role.clone());
                if let Some(token) = self.api_token.as_ref() {
                    mcp_args.push("--api-token".to_string());
                    mcp_args.push(token.clone());
                }
                let mut mcp_servers = serde_json::Map::new();
                mcp_servers.insert(
                    "harness".to_string(),
                    json!({
                        "command": mcp_bin.display().to_string(),
                        "args": mcp_args
                    }),
                );
                if load_crawl4ai {
                    let crawl = crate::routes::sessions::crawl4ai_mcp_server();
                    mcp_servers.insert(
                        crawl.name,
                        json!({
                            "command": crawl.command,
                            "args": crawl.args,
                        }),
                    );
                }
                let config = json!({ "mcpServers": serde_json::Value::Object(mcp_servers) });
                if let Err(e) = crate::routes::sessions::write_private_json(&path, &config) {
                    return SpawnResult::Failed(format!("write mcp config: {e}"));
                }
                opts.mcp_config_path = Some(path.clone());
                opts.auto_intro = Some(if load_crawl4ai {
                    format!(
                        "{}\n\n{}",
                        crate::routes::sessions::harness_mcp_intro(),
                        crate::routes::sessions::crawl4ai_context_intro()
                    )
                } else {
                    crate::routes::sessions::harness_mcp_intro().to_string()
                });
                config_path = Some(path);
            }
        }

        match self
            .manager
            .spawn_with_opts(kind, binary, req.thread_id, cwd, opts)
        {
            Ok(session) => {
                let sid = session.id().to_string();
                if let Some(p) = config_path {
                    self.mcp_configs.insert(sid.clone(), p);
                }
                SpawnResult::Launched { session_id: sid }
            }
            Err(e) => SpawnResult::Failed(format!("manager.spawn: {e}")),
        }
    }
}

fn detect_binaries() -> HashMap<AgentKind, PathBuf> {
    let mut out = HashMap::new();
    let kinds = [
        AgentKind::Claude,
        AgentKind::Codex,
        AgentKind::Cursor,
        AgentKind::Antigravity,
        // Zeus is virtual — skip discovery entirely.
    ];
    for kind in kinds {
        let name = kind.default_binary();
        match which::which(name) {
            Ok(p) => {
                tracing::info!(binary = name, path = %p.display(), "agent binary detected");
                out.insert(kind, p);
            }
            Err(_) => {
                tracing::warn!(
                    binary = name,
                    kind = %kind,
                    "agent binary not found on PATH; spawn requests for this kind will fail"
                );
            }
        }
    }
    out
}
