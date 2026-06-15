use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use dashmap::DashMap;
use harness_core::RepoIndex;
use harness_core::{
    ActiveSession, ActiveSessionsSource, AgentsRegistry, BudgetStore, BudgetWarning,
    BudgetWarningSink, BudgetWiring, ClaudeTranscriptReporter, CodexStubReporter, CostReporter,
    Event, Item, PauseFlag, RolesRegistry, Scheduler, SessionSpawner, SpawnRequest, SpawnResult,
    Store, StubReporter, TaskStore,
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
    pub repos: Arc<RepoIndex>,
    pub policy: Arc<PolicyEngine>,
    pub approvals: Arc<ApprovalStore>,
    pub db: Arc<module_db::Manager>,
    pub ssh: Arc<module_ssh::Manager>,
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
    /// only when the underlying CLI emits a JSONL transcript we can parse.
    /// Each entry holds the store (for replay), the
    /// live broadcast bus, and a handle to abort the watcher on kill.
    pub transcripts: Arc<DashMap<String, TranscriptSlot>>,
    pub start_time: Instant,
    pub version: &'static str,
    pub tick_tx: broadcast::Sender<String>,
    pub sse_lagged_total: AtomicU64,
}

impl AppState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let profile = cfg.profile.as_str();
        tracing::info!(profile = %profile, "AppState init: using profile");

        let store = Arc::new(Store::with_profile(&cfg.home, profile)?);
        let sessions_root = cfg.home.join("profiles").join(profile).join("sessions");
        let manager = Arc::new(Manager::new(sessions_root)?);
        manager.load_existing()?;
        crate::context_governor::reconcile_persisted_governor_states(&manager);
        let task_store =
            TaskStore::with_profile(&cfg.home, profile)?.with_event_store(store.clone());
        let agents = Arc::new(AgentsRegistry::with_profile(&cfg.home, profile)?);
        let roles = Arc::new(RolesRegistry::load_for_profile(&cfg.home, profile)?);
        let pause = Arc::new(PauseFlag::load(&cfg.home)?);
        let budgets = Arc::new(BudgetStore::load_for_profile(&cfg.home, profile)?);
        let repos = Arc::new(RepoIndex::with_profile(&cfg.home, profile)?);
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
        let ssh = Arc::new(
            module_ssh::Manager::new(&cfg.home, profile)
                .map_err(|e| anyhow::anyhow!("module-ssh init: {e}"))?,
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
        // Zeus is virtual — no PTY of its own. Its underlying CLI is accounted
        // under the resolved concrete kind at spawn time.

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
            store: store.clone(),
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
            repos,
            policy,
            approvals,
            db,
            ssh,
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
            sse_lagged_total: AtomicU64::new(0),
        })
    }

    pub fn record_sse_lagged(&self) {
        self.sse_lagged_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sse_lagged_total(&self) -> u64 {
        self.sse_lagged_total.load(Ordering::Relaxed)
    }

    /// Best-effort cleanup of runtime artifacts associated with a session.
    /// Persisted transcript/output logs stay on disk for replay and forensics.
    pub fn cleanup_session_resources(&self, sid: &str) {
        if let Some((_, slot)) = self.transcripts.remove(sid) {
            slot.handle.stop();
        }
        if let Some((_, path)) = self.mcp_configs.remove(sid) {
            if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "could not remove mcp config"
                    );
                }
            }
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                let upstream_path = path.with_file_name(format!("{stem}.upstreams.json"));
                if upstream_path.exists() {
                    if let Err(e) = std::fs::remove_file(&upstream_path) {
                        tracing::warn!(
                            path = %upstream_path.display(),
                            error = %e,
                            "could not remove upstream mcp config"
                        );
                    }
                }
            }
        }
        let attach_dir = self.harness_home.join(".runtime/attach").join(sid);
        if attach_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&attach_dir) {
                tracing::warn!(dir = %attach_dir.display(), error = %e, "could not purge attach dir");
            }
        }
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
                    task_id: s.task_id_static().map(str::to_string),
                    owner_session_id: s.owner_session_id_static().map(str::to_string),
                    parent_session_id: s.parent_session_id_static().map(str::to_string),
                    root_session_id: Some(s.root_session_id_static().to_string()),
                    // TODO(Task 21): extend harness_core::budget::ActiveSession
                    // with scopes, then populate it here from s.scopes().
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
    store: Arc<Store>,
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

#[derive(Debug, Clone)]
struct KindSelection {
    kind: AgentKind,
    binary: PathBuf,
    fallback: Option<FallbackSelection>,
}

#[derive(Debug, Clone)]
struct FallbackSelection {
    from: AgentKind,
    to: AgentKind,
    reason: &'static str,
    detail: String,
}

impl ManagerSpawner {
    fn kind_from_request(s: &str) -> Option<AgentKind> {
        match s {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "cursor" => Some(AgentKind::Cursor),
            "antigravity" => Some(AgentKind::Antigravity),
            "zeus" => Some(AgentKind::Zeus),
            _ => None,
        }
    }

    fn kind_for_role(
        role_cli: harness_core::agents::AgentKind,
        requested_kind: &str,
    ) -> Result<AgentKind, String> {
        match role_cli {
            harness_core::agents::AgentKind::Claude => Ok(AgentKind::Claude),
            harness_core::agents::AgentKind::Codex => Ok(AgentKind::Codex),
            harness_core::agents::AgentKind::Cursor => Ok(AgentKind::Cursor),
            harness_core::agents::AgentKind::Antigravity => Ok(AgentKind::Antigravity),
            harness_core::agents::AgentKind::Generic => Self::kind_from_request(requested_kind)
                .ok_or_else(|| format!("unknown kind: {requested_kind}")),
        }
    }

    fn select_kind_and_binary(
        &self,
        role_cli: harness_core::agents::AgentKind,
        role_name: &str,
        requested_kind: &str,
    ) -> Result<KindSelection, String> {
        let desired = Self::kind_for_role(role_cli, requested_kind)?.underlying_cli();
        if let Some(binary) = self.binaries.get(&desired).cloned() {
            return Ok(KindSelection {
                kind: desired,
                binary,
                fallback: None,
            });
        }

        if desired != AgentKind::Claude {
            if let Some(binary) = self.binaries.get(&AgentKind::Claude).cloned() {
                return Ok(KindSelection {
                    kind: AgentKind::Claude,
                    binary,
                    fallback: Some(FallbackSelection {
                        from: desired,
                        to: AgentKind::Claude,
                        reason: "binary_missing",
                        detail: format!(
                            "no binary detected for role {role_name} cli {desired} (requested agent kind {requested_kind})"
                        ),
                    }),
                });
            }
        }

        Err(format!(
            "no binary detected for role {role_name} cli {desired} (requested agent kind {requested_kind})"
        ))
    }

    fn append_spawn_fallback_event(&self, req: &SpawnRequest, fallback: &FallbackSelection) {
        let payload = json!({
            "agent_id": req.agent_id,
            "role": req.role,
            "task_id": req.task_id,
            "requested_kind": req.kind,
            "from": fallback.from.as_str(),
            "to": fallback.to.as_str(),
            "reason": fallback.reason,
            "detail": fallback.detail,
        });
        let event = Event {
            seq: 0,
            at: chrono::Utc::now().timestamp_millis(),
            event_type: "scheduler.spawn.fallback".to_string(),
            items: vec![Item::Text {
                text: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
            }],
            thread_id: Some(req.thread_id.clone()),
            actor: Some("scheduler".to_string()),
            payload: Some(payload),
        };
        if let Err(e) = self.store.append_event(&req.thread_id, &event) {
            tracing::warn!(
                thread_id = %req.thread_id,
                agent_id = %req.agent_id,
                error = %e,
                "failed to append scheduler spawn fallback event"
            );
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

    fn classify_spawn_failure(message: &str) -> Option<&'static str> {
        let lower = message.to_ascii_lowercase();
        if lower.contains("unknown role") || lower.contains("unknown kind") {
            return None;
        }
        if lower.contains("quota")
            || lower.contains("rate limit")
            || lower.contains("rate_limit")
            || lower.contains("429")
            || lower.contains("too many requests")
        {
            return Some("quota_exceeded");
        }
        if lower.contains("panic")
            || lower.contains("crash")
            || lower.contains("runtime")
            || lower.contains("pty error")
            || lower.contains("spawn")
            || lower.contains("exited")
        {
            return Some("runtime_error");
        }
        None
    }

    fn fallback_for_spawn_failure(
        &self,
        kind: AgentKind,
        error: &str,
    ) -> Option<(PathBuf, FallbackSelection)> {
        if kind == AgentKind::Claude {
            return None;
        }
        let reason = Self::classify_spawn_failure(error)?;
        let binary = self.binaries.get(&AgentKind::Claude).cloned()?;
        Some((
            binary,
            FallbackSelection {
                from: kind,
                to: AgentKind::Claude,
                reason,
                detail: format!("primary CLI spawn failed: {error}"),
            },
        ))
    }
}

impl SessionSpawner for ManagerSpawner {
    fn spawn(&self, req: SpawnRequest) -> SpawnResult {
        if let Some(sid) = self.find_existing(&req.agent_id, &req.thread_id) {
            return SpawnResult::AlreadyRunning { session_id: sid };
        }

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
        let selection = match self.select_kind_and_binary(role.cli, &req.role, &req.kind) {
            Ok(selection) => selection,
            Err(e) => return SpawnResult::Failed(e),
        };
        if let Some(fallback) = selection.fallback.as_ref() {
            self.append_spawn_fallback_event(&req, fallback);
        }
        let kind = selection.kind;
        let binary = selection.binary;

        let cwd = req
            .cwd
            .clone()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        // Build SpawnOpts with the role prompt. We tag SessionMeta.role with
        // the AGENT id (not the role tag) so `find_existing` can de-dupe.
        let mut opts = SpawnOpts {
            role_prompt: Some(role.prompt_template.clone()),
            role: Some(req.agent_id.clone()),
            task_id: req.task_id.clone(),
            scopes: task_scopes(req.task_id.as_deref()),
            ..SpawnOpts::default()
        };

        let active_task = self.tasks.latest_active(&req.thread_id).ok().flatten();
        let load_crawl4ai = active_task
            .as_ref()
            .map(crate::routes::sessions::task_mentions_documentation_url)
            .unwrap_or(false);
        let task_skill_text = active_task
            .as_ref()
            .map(crate::routes::sessions::task_capability_text);
        let smart_skills = crate::routes::sessions::resolve_smart_skills(
            load_crawl4ai,
            Some(&req.role),
            Some(&cwd),
            [
                opts.auto_intro.as_deref(),
                opts.role_prompt.as_deref(),
                task_skill_text.as_deref(),
            ],
            &opts.scopes,
            crate::routes::sessions::CapabilityProfile::Auto,
        );
        let smart_tool_groups = crate::routes::sessions::resolve_smart_tool_groups(
            Some(&req.role),
            Some(&cwd),
            [
                opts.auto_intro.as_deref(),
                opts.role_prompt.as_deref(),
                task_skill_text.as_deref(),
            ],
            &opts.scopes,
            crate::routes::sessions::CapabilityProfile::Auto,
        );

        // Per-session MCP config (mirrors the REST `create_session` path so
        // sub-agents see the same `task_*` tool surface as user-spawned
        // sessions).
        let mut config_path: Option<PathBuf> = None;
        if matches!(kind, AgentKind::Claude | AgentKind::Codex) {
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
                let load_code_graph = smart_tool_groups.iter().any(|group| group == "code_graph");
                let upstreams =
                    crate::routes::sessions::upstream_mcp_configs(load_crawl4ai, load_code_graph);
                if !upstreams.is_empty() {
                    let upstream_path = configs_dir.join(format!("{mcp_id}.upstreams.json"));
                    if let Err(e) = crate::routes::sessions::write_private_json(
                        &upstream_path,
                        &serde_json::Value::Array(upstreams),
                    ) {
                        return SpawnResult::Failed(format!("write upstream MCP config: {e}"));
                    }
                    mcp_args.push("--upstream-config".to_string());
                    mcp_args.push(upstream_path.display().to_string());
                }
                mcp_args.push("--role".to_string());
                mcp_args.push(req.role.clone());
                if let Some(task_id) = req.task_id.as_deref() {
                    mcp_args.push("--task-id".to_string());
                    mcp_args.push(task_id.to_string());
                }
                for scope in &opts.scopes {
                    mcp_args.push("--scope".to_string());
                    mcp_args.push(scope.to_string());
                }
                if let Some(token) = self.api_token.as_ref() {
                    mcp_args.push("--api-token".to_string());
                    mcp_args.push(token.clone());
                }
                let mut mcp_servers = serde_json::Map::new();
                mcp_servers.insert(
                    "harness".to_string(),
                    json!({
                        "command": mcp_bin.display().to_string(),
                        "args": mcp_args.clone()
                    }),
                );
                let loaded_capabilities =
                    crate::routes::sessions::loaded_mcp_capabilities_with_skills(
                        load_crawl4ai,
                        smart_skills,
                        smart_tool_groups,
                    );
                let capability_intro = crate::routes::sessions::spawn_capability_intro(
                    load_crawl4ai,
                    &loaded_capabilities.skills,
                    &loaded_capabilities.tool_groups,
                );
                let config = json!({ "mcpServers": serde_json::Value::Object(mcp_servers) });
                if let Err(e) = crate::routes::sessions::write_private_json(&path, &config) {
                    return SpawnResult::Failed(format!("write mcp config: {e}"));
                }
                opts.mcp_config_path = Some(path.clone());
                opts.mcp_server_command = Some(mcp_bin.display().to_string());
                opts.mcp_server_args = mcp_args;
                opts.loaded_capabilities = loaded_capabilities;
                opts.auto_intro = Some(capability_intro);
                config_path = Some(path);
            }
        }

        let primary = self.manager.spawn_with_opts(
            kind,
            binary,
            req.thread_id.clone(),
            cwd.clone(),
            opts.clone(),
        );
        match primary {
            Ok(session) => {
                let sid = session.id().to_string();
                if let Some(p) = config_path {
                    self.mcp_configs.insert(sid.clone(), p);
                }
                SpawnResult::Launched { session_id: sid }
            }
            Err(e) => {
                let error = format!("manager.spawn: {e}");
                if let Some((fallback_binary, fallback)) =
                    self.fallback_for_spawn_failure(kind, &error)
                {
                    self.append_spawn_fallback_event(&req, &fallback);
                    return match self.manager.spawn_with_opts(
                        AgentKind::Claude,
                        fallback_binary,
                        req.thread_id,
                        cwd,
                        opts,
                    ) {
                        Ok(session) => {
                            let sid = session.id().to_string();
                            if let Some(p) = config_path {
                                self.mcp_configs.insert(sid.clone(), p);
                            }
                            SpawnResult::Launched { session_id: sid }
                        }
                        Err(fallback_err) => SpawnResult::Failed(format!(
                            "{error}; fallback manager.spawn: {fallback_err}"
                        )),
                    };
                }
                SpawnResult::Failed(error)
            }
        }
    }
}

fn task_scopes(task_id: Option<&str>) -> Vec<String> {
    task_id
        .map(|task_id| vec![format!("task:{task_id}")])
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::agents::AgentKind as RoleAgentKind;
    use tempfile::TempDir;

    fn test_spawner(binaries: HashMap<AgentKind, PathBuf>) -> (ManagerSpawner, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let profile = "default";
        let store = Arc::new(Store::with_profile(dir.path(), profile).unwrap());
        let manager = Arc::new(Manager::new(dir.path().join("profiles/default/sessions")).unwrap());
        let roles = Arc::new(RolesRegistry::load_for_profile(dir.path(), profile).unwrap());
        let tasks = Arc::new(TaskStore::with_profile(dir.path(), profile).unwrap());
        (
            ManagerSpawner {
                manager,
                store,
                roles,
                tasks,
                binaries,
                mcp_server_binary: None,
                harness_home: dir.path().to_path_buf(),
                server_url: "http://127.0.0.1:7777".to_string(),
                api_token: None,
                mcp_configs: Arc::new(DashMap::new()),
            },
            dir,
        )
    }

    #[test]
    fn role_cli_overrides_scheduler_requested_kind() {
        assert_eq!(
            ManagerSpawner::kind_for_role(RoleAgentKind::Codex, "claude").unwrap(),
            AgentKind::Codex
        );
        assert_eq!(
            ManagerSpawner::kind_for_role(RoleAgentKind::Claude, "codex").unwrap(),
            AgentKind::Claude
        );
    }

    #[test]
    fn generic_role_uses_scheduler_requested_kind() {
        assert_eq!(
            ManagerSpawner::kind_for_role(RoleAgentKind::Generic, "cursor").unwrap(),
            AgentKind::Cursor
        );
        assert_eq!(
            ManagerSpawner::kind_for_role(RoleAgentKind::Generic, "zeus")
                .unwrap()
                .underlying_cli(),
            AgentKind::Codex
        );
        assert!(ManagerSpawner::kind_for_role(RoleAgentKind::Generic, "missing").is_err());
    }

    #[test]
    fn missing_role_cli_binary_falls_back_to_claude() {
        let mut binaries = HashMap::new();
        binaries.insert(AgentKind::Claude, PathBuf::from("/bin/claude"));
        let (spawner, _dir) = test_spawner(binaries);

        let selected = spawner
            .select_kind_and_binary(RoleAgentKind::Codex, "generator", "codex")
            .unwrap();

        assert_eq!(selected.kind, AgentKind::Claude);
        assert_eq!(selected.binary, PathBuf::from("/bin/claude"));
        let fallback = selected.fallback.expect("fallback metadata");
        assert_eq!(fallback.from, AgentKind::Codex);
        assert_eq!(fallback.to, AgentKind::Claude);
        assert_eq!(fallback.reason, "binary_missing");
    }

    #[test]
    fn classifies_spawn_failures_for_fallback_audit() {
        assert_eq!(
            ManagerSpawner::classify_spawn_failure("manager.spawn: API quota exceeded"),
            Some("quota_exceeded")
        );
        assert_eq!(
            ManagerSpawner::classify_spawn_failure("manager.spawn: pty error: child exited"),
            Some("runtime_error")
        );
        assert_eq!(
            ManagerSpawner::classify_spawn_failure("manager.spawn: unknown role"),
            None
        );
    }

    #[test]
    fn runtime_spawn_failure_selects_claude_fallback() {
        let mut binaries = HashMap::new();
        binaries.insert(AgentKind::Claude, PathBuf::from("/bin/claude"));
        binaries.insert(AgentKind::Codex, PathBuf::from("/bin/codex"));
        let (spawner, _dir) = test_spawner(binaries);

        let (_binary, fallback) = spawner
            .fallback_for_spawn_failure(AgentKind::Codex, "manager.spawn: pty error: crash")
            .expect("fallback");

        assert_eq!(fallback.from, AgentKind::Codex);
        assert_eq!(fallback.to, AgentKind::Claude);
        assert_eq!(fallback.reason, "runtime_error");
    }

    #[test]
    fn spawn_fallback_event_is_append_only() {
        let mut binaries = HashMap::new();
        binaries.insert(AgentKind::Claude, PathBuf::from("/bin/claude"));
        let (spawner, _dir) = test_spawner(binaries);
        let thread = spawner
            .store
            .create_thread(Some("fallback test".to_string()))
            .unwrap();
        let req = SpawnRequest {
            thread_id: thread.id.clone(),
            agent_id: "agent:generator-1".to_string(),
            kind: "codex".to_string(),
            role: "generator".to_string(),
            task_id: Some("T-0001".to_string()),
            cwd: None,
        };
        let fallback = FallbackSelection {
            from: AgentKind::Codex,
            to: AgentKind::Claude,
            reason: "binary_missing",
            detail: "codex missing".to_string(),
        };

        spawner.append_spawn_fallback_event(&req, &fallback);

        let events = spawner.store.read_events(&thread.id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "scheduler.spawn.fallback");
        assert_eq!(events[0].actor.as_deref(), Some("scheduler"));
        let payload = events[0].payload.as_ref().unwrap();
        assert_eq!(payload["from"], "codex");
        assert_eq!(payload["to"], "claude");
        assert_eq!(payload["reason"], "binary_missing");
        assert_eq!(payload["task_id"], "T-0001");
    }

    #[test]
    fn runtime_fallback_event_is_append_only() {
        let mut binaries = HashMap::new();
        binaries.insert(AgentKind::Claude, PathBuf::from("/bin/claude"));
        binaries.insert(AgentKind::Codex, PathBuf::from("/bin/codex"));
        let (spawner, _dir) = test_spawner(binaries);
        let thread = spawner
            .store
            .create_thread(Some("runtime fallback test".to_string()))
            .unwrap();
        let req = SpawnRequest {
            thread_id: thread.id.clone(),
            agent_id: "agent:generator-1".to_string(),
            kind: "codex".to_string(),
            role: "generator".to_string(),
            task_id: Some("T-0002".to_string()),
            cwd: None,
        };
        let (_binary, fallback) = spawner
            .fallback_for_spawn_failure(AgentKind::Codex, "manager.spawn: pty error: crash")
            .expect("runtime fallback");

        spawner.append_spawn_fallback_event(&req, &fallback);

        let events = spawner.store.read_events(&thread.id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "scheduler.spawn.fallback");
        assert_eq!(events[0].actor.as_deref(), Some("scheduler"));
        let payload = events[0].payload.as_ref().unwrap();
        assert_eq!(payload["from"], "codex");
        assert_eq!(payload["to"], "claude");
        assert_eq!(payload["reason"], "runtime_error");
        assert_eq!(payload["task_id"], "T-0002");
    }

    #[test]
    fn synthetic_goal_routes_roles_to_expected_cli_and_audits_blocked_primary_fallback() {
        let mut binaries = HashMap::new();
        binaries.insert(AgentKind::Claude, PathBuf::from("/bin/claude"));
        binaries.insert(AgentKind::Codex, PathBuf::from("/bin/codex"));
        binaries.insert(AgentKind::Cursor, PathBuf::from("/bin/cursor-agent"));
        let (spawner, _dir) = test_spawner(binaries);

        let role_matrix = [
            ("planner", AgentKind::Claude),
            ("generator", AgentKind::Claude),
            ("evaluator", AgentKind::Claude),
            ("frontend-visual", AgentKind::Cursor),
        ];
        for (role_name, expected) in role_matrix {
            let role = spawner.roles.get(role_name).expect("baseline role");
            let selected = spawner
                .select_kind_and_binary(role.cli, role_name, "codex")
                .expect("role resolves to a launchable CLI");
            assert_eq!(
                selected.kind, expected,
                "role {role_name} should route to {expected}"
            );
            assert!(
                selected.fallback.is_none(),
                "role {role_name} should not fallback while its primary binary is present"
            );
        }

        let (fallback_binary, fallback) = spawner
            .fallback_for_spawn_failure(
                AgentKind::Codex,
                "manager.spawn: API quota exceeded while booting synthetic worker",
            )
            .expect("blocked codex primary should fallback to Claude");
        assert_eq!(fallback_binary, PathBuf::from("/bin/claude"));
        assert_eq!(fallback.from, AgentKind::Codex);
        assert_eq!(fallback.to, AgentKind::Claude);
        assert_eq!(fallback.reason, "quota_exceeded");

        let thread = spawner
            .store
            .create_thread(Some("synthetic TODO goal".to_string()))
            .unwrap();
        let req = SpawnRequest {
            thread_id: thread.id.clone(),
            agent_id: "agent:codex-1".to_string(),
            kind: "codex".to_string(),
            role: "generator".to_string(),
            task_id: Some("T-0001".to_string()),
            cwd: None,
        };

        spawner.append_spawn_fallback_event(&req, &fallback);

        let events = spawner.store.read_events(&thread.id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "scheduler.spawn.fallback");
        let payload = events[0].payload.as_ref().unwrap();
        assert_eq!(payload["from"], "codex");
        assert_eq!(payload["to"], "claude");
        assert_eq!(payload["reason"], "quota_exceeded");
        assert_eq!(payload["task_id"], "T-0001");
    }
}
