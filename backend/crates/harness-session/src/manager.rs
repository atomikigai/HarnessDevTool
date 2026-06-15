use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::errors::SessionError;
use crate::kind::AgentKind;
use crate::meta::{
    LoadedCapabilities, ProcessIdentity, SessionMeta, SessionRepoContext, SessionStatus,
};
use crate::output::{OutputReadChunk, OutputWriter};
use crate::session::{persist_meta, pid_alive, process_identity, AgentSession};

const DEFAULT_CLAUDE_MODEL: &str = "sonnet";
const DEFAULT_CLAUDE_EFFORT: &str = "medium";
const DEFAULT_CODEX_MODEL: &str = "gpt-5.5";
const DEFAULT_CODEX_EFFORT: &str = "medium";
const DELETED_MARKER: &str = ".deleted";

/// Broadcast event published by sessions onto the shared bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    #[serde(rename = "session.started")]
    Started { session_id: String, pid: u32 },
    #[serde(rename = "session.output")]
    Output {
        session_id: String,
        seq: u64,
        bytes: Vec<u8>,
    },
    #[serde(rename = "session.exit")]
    Exit {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },
    /// Heuristic state detector flipped to a different bucket. Fires on
    /// transitions only, not on every detection tick.
    #[serde(rename = "session.state_changed")]
    StateChanged {
        session_id: String,
        prev: crate::detect::AgentState,
        next: crate::detect::AgentState,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match self {
            SessionEvent::Started { session_id, .. }
            | SessionEvent::Output { session_id, .. }
            | SessionEvent::Exit { session_id, .. }
            | SessionEvent::StateChanged { session_id, .. } => session_id,
        }
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            SessionEvent::Started { .. } => "session.started",
            SessionEvent::Output { .. } => "session.output",
            SessionEvent::Exit { .. } => "session.exit",
            SessionEvent::StateChanged { .. } => "session.state_changed",
        }
    }
}

/// Owns all live sessions and the directory layout.
pub struct Manager {
    sessions_root: PathBuf,
    sessions: DashMap<String, Arc<AgentSession>>,
    detached: DashMap<String, SessionMeta>,
    bus: broadcast::Sender<SessionEvent>,
    shutting_down: AtomicBool,
    lifecycle_lock: Mutex<()>,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager")
            .field("sessions_root", &self.sessions_root)
            .field("live_sessions", &self.sessions.len())
            .field("detached_sessions", &self.detached.len())
            .finish()
    }
}

#[derive(Debug)]
pub struct KillTreeResult {
    pub affected: Vec<String>,
    pub tombstone_error: Option<SessionError>,
}

#[derive(Debug)]
pub struct StopTreeResult {
    pub affected: Vec<String>,
}

impl Manager {
    /// `sessions_root` is `<home>/profiles/<profile>/sessions`.
    pub fn new(sessions_root: impl Into<PathBuf>) -> Result<Self, SessionError> {
        let sessions_root = sessions_root.into();
        std::fs::create_dir_all(&sessions_root)?;
        let (bus, _) = broadcast::channel(1024);
        Ok(Self {
            sessions_root,
            sessions: DashMap::new(),
            detached: DashMap::new(),
            bus,
            shutting_down: AtomicBool::new(false),
            lifecycle_lock: Mutex::new(()),
        })
    }

    pub fn bus(&self) -> broadcast::Sender<SessionEvent> {
        self.bus.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.bus.subscribe()
    }

    pub fn sessions_root(&self) -> &Path {
        &self.sessions_root
    }

    pub fn get(&self, sid: &str) -> Option<Arc<AgentSession>> {
        self.sessions.get(sid).map(|e| e.value().clone())
    }

    /// Load persisted session metadata from disk as read-only detached
    /// sessions. Live handles always win; detached entries exist only so
    /// list/read-only views can survive a server restart.
    pub fn load_existing(&self) -> Result<(), SessionError> {
        if !self.sessions_root.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.sessions_root)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read session directory entry");
                    continue;
                }
            };
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            if dir.join(DELETED_MARKER).exists() {
                continue;
            }
            let meta_path = dir.join("meta.json");
            if !meta_path.exists() {
                continue;
            }

            let raw = match std::fs::read(&meta_path) {
                Ok(raw) => raw,
                Err(e) => {
                    tracing::warn!(path = %meta_path.display(), error = %e, "failed to read session meta");
                    continue;
                }
            };
            let mut meta: SessionMeta = match serde_json::from_slice(&raw) {
                Ok(meta) => meta,
                Err(e) => {
                    tracing::warn!(path = %meta_path.display(), error = %e, "failed to parse session meta");
                    continue;
                }
            };

            if meta.root_session_id.is_empty() {
                meta.root_session_id = meta.id.clone();
            }
            if meta.status == SessionStatus::Running {
                reap_orphan_if_identity_matches_in_background(&meta);
                // After a backend restart we only have persisted metadata, not
                // the PTY writer/killer/read tasks needed to control the
                // process, so expose it as non-live state while best-effort
                // orphan reaping proceeds in the background. Startup must not
                // block for up to 3s per orphan.
                meta.status = SessionStatus::Exited;
                if let Err(e) = persist_meta(&dir, &meta) {
                    tracing::warn!(
                        session_id = %meta.id,
                        path = %meta_path.display(),
                        error = %e,
                        "failed to persist reconciled detached session meta"
                    );
                }
            }

            if self.sessions.contains_key(&meta.id) {
                continue;
            }
            self.detached.insert(meta.id.clone(), meta);
        }

        Ok(())
    }

    /// Snapshot of all currently-tracked session handles.
    pub fn all(&self) -> Vec<Arc<AgentSession>> {
        self.sessions.iter().map(|e| e.value().clone()).collect()
    }

    /// Kill every live session in leaf-up order without removing or
    /// tombstoning them. Used for server reload/shutdown where persisted
    /// session state must remain available for detached replay after restart.
    pub async fn shutdown_all(&self) -> Vec<String> {
        let sessions = {
            let _guard = lock_or_recover(&self.lifecycle_lock);
            self.shutting_down.store(true, Ordering::SeqCst);
            let mut sessions = self.all();
            sessions.sort_by_key(|session| std::cmp::Reverse(self.tree_depth(session.id())));
            sessions
        };
        let mut ids = Vec::with_capacity(sessions.len());
        for session in sessions {
            let sid = session.id().to_string();
            if let Err(e) = session.kill().await {
                tracing::warn!(session = %sid, error = %e, "kill during manager shutdown");
            }
            ids.push(sid);
        }
        ids
    }

    /// Snapshot of all session metadata known to the manager. Includes live
    /// sessions plus detached read-only metadata loaded from disk.
    pub async fn list_metas(&self) -> Vec<SessionMeta> {
        let mut out = Vec::new();
        let mut live_ids = std::collections::HashSet::new();
        for entry in self.sessions.iter() {
            let session = entry.value().clone();
            live_ids.insert(entry.key().clone());
            out.push(session.meta().await);
        }
        for entry in self.detached.iter() {
            if !live_ids.contains(entry.key()) {
                out.push(entry.value().clone());
            }
        }
        out
    }

    /// Read the active `output.log` for a session straight from disk (used for
    /// SSE catch-up before the live bus tail). Available even for sessions
    /// that exited (as long as their dir still exists on disk).
    pub fn read_output(&self, sid: &str) -> Result<Vec<u8>, SessionError> {
        let dir = self.sessions_root.join(sid);
        if !dir.exists() {
            return Err(SessionError::NotFound(sid.to_string()));
        }
        // Reuse OutputWriter::open since it tolerates pre-existing files.
        let w = OutputWriter::open(&dir)?;
        w.read_active()
    }

    /// Read a bounded chunk from the active `output.log` for a session. Offsets
    /// are active-file offsets; see [`OutputWriter::read_active_chunk`] for
    /// rotation semantics.
    pub fn read_output_chunk(
        &self,
        sid: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<OutputReadChunk, SessionError> {
        let dir = self.sessions_root.join(sid);
        if !dir.exists() {
            return Err(SessionError::NotFound(sid.to_string()));
        }
        let w = OutputWriter::open(&dir)?;
        w.read_active_chunk(offset, max_bytes)
    }

    /// Spawn a new session. `binary` must be the absolute path to the agent CLI.
    pub fn spawn(
        &self,
        kind: AgentKind,
        binary: PathBuf,
        thread_id: String,
        cwd: PathBuf,
    ) -> Result<Arc<AgentSession>, SessionError> {
        self.spawn_with_opts(kind, binary, thread_id, cwd, SpawnOpts::default())
    }

    /// Spawn a new session with extra options (MCP config injection, etc).
    pub fn spawn_with_opts(
        &self,
        kind: AgentKind,
        binary: PathBuf,
        thread_id: String,
        cwd: PathBuf,
        opts: SpawnOpts,
    ) -> Result<Arc<AgentSession>, SessionError> {
        let _guard = lock_or_recover(&self.lifecycle_lock);
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(SessionError::Invalid(
                "session manager is shutting down".to_string(),
            ));
        }
        let id = opts
            .session_id_override
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let dir = self.sessions_root.join(&id);
        let extra_args = build_extra_args(kind, &opts, &id);

        // Resolve session-tree fields. Root sessions root themselves; children
        // inherit the parent's root and reject if the parent is gone.
        let (parent_session_id, root_session_id) = match opts.parent_session_id.as_deref() {
            None => (None, id.clone()),
            Some(pid) => {
                let parent = self
                    .get(pid)
                    .ok_or_else(|| SessionError::NotFound(pid.to_string()))?;
                let parent_root = parent.root_session_id_static().to_string();
                (Some(pid.to_string()), parent_root)
            }
        };

        let session = AgentSession::spawn_with_id(
            id.clone(),
            kind,
            binary,
            thread_id,
            cwd,
            dir,
            extra_args,
            opts.role.clone(),
            opts.owner_session_id.clone(),
            opts.task_id.clone(),
            opts.scopes.clone(),
            opts.repo.clone(),
            opts.loaded_capabilities.clone(),
            parent_session_id,
            root_session_id,
            opts.initial_size,
            self.bus.clone(),
        )?;
        self.detached.remove(&id);
        self.sessions.insert(id, session.clone());

        // `auto_intro` is passed to claude as `--append-system-prompt` and
        // to codex as `developer_instructions` (CLI config override), so it
        // never appears as user-typed input.
        // `role_prompt` IS user-typed: it's the "begin your role" kick that
        // tells the agent to start working, so it must appear in the
        // conversation.
        //
        // CLI-specific delivery:
        //   - **Codex**: role_prompt is passed as the positional `[PROMPT]`
        //     arg in `build_extra_args` — Codex submits it before its Ink TUI
        //     even mounts. Skip the PTY-injection task entirely.
        //   - **Claude/Cursor/Antigravity**: still go through the PTY using
        //     bracketed paste + delayed CR (`\r`). Claude's TUI accepts that
        //     cleanly; Cursor/Antigravity TBD when integrated.
        //
        // Bracketed paste mode (CSI 200~ ... CSI 201~) tells the TUI "this
        // is pasted content; don't interpret embedded LFs as Enter". Then
        // we send `\r` outside the paste envelope to submit.
        let inject_via_pty = !matches!(kind, AgentKind::Codex);
        if inject_via_pty {
            if let Some(payload) = opts.role_prompt {
                let payload = sanitize_pty_prompt(&payload);
                let s = session.clone();
                let sid = s.id().to_string();
                let injector = tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let mut buf: Vec<u8> = Vec::with_capacity(payload.len() + 16);
                    buf.extend_from_slice(b"\x1b[200~");
                    buf.extend_from_slice(payload.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    if let Err(e) = s.write_input(&buf).await {
                        tracing::warn!(spawn_id = %sid, error = %e, "failed to inject role prompt (paste)");
                        return;
                    }
                    // Spacer: let the TUI echo the pasted block before submit.
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    if let Err(e) = s.write_input(b"\r").await {
                        tracing::warn!(spawn_id = %sid, error = %e, "failed to submit role prompt");
                    }
                    tracing::info!(
                        spawn_id = %sid,
                        bytes_written = payload.len(),
                        "injected role prompt via bracketed paste + CR"
                    );
                });
                session.set_prompt_injector(injector);
            }
        }

        Ok(session)
    }

    /// Forget a session (does NOT delete on-disk state).
    pub fn remove(&self, sid: &str) {
        self.sessions.remove(sid);
        self.detached.remove(sid);
    }

    /// Mark a session as deleted for future manager loads. This preserves
    /// forensic artifacts such as `output.log` while hiding the session from
    /// UI/API listings after a restart.
    pub fn tombstone(&self, sid: &str) -> Result<(), SessionError> {
        let dir = self.sessions_root.join(sid);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(DELETED_MARKER), b"deleted\n")?;
        Ok(())
    }

    pub fn is_tombstoned(&self, sid: &str) -> bool {
        self.sessions_root.join(sid).join(DELETED_MARKER).exists()
    }

    // ── Session-tree helpers (Zeus orchestrator) ─────────────────────────

    /// Kill a session tree, remove it from the in-memory manager, and mark
    /// every affected session as tombstoned. Returned ids are ordered in the
    /// same leaf-up order used for killing so callers can clean runtime
    /// resources deterministically.
    pub async fn kill_tree_and_tombstone(&self, sid: &str) -> KillTreeResult {
        let (sessions_to_kill, affected, tombstone_error) = {
            let _guard = lock_or_recover(&self.lifecycle_lock);
            let mut affected = Vec::new();
            let mut sessions_to_kill = Vec::new();
            let mut tombstone_error = None;

            for child in self.descendants_of(sid).into_iter().rev() {
                let cid = child.id().to_string();
                self.remove(&cid);
                affected.push(cid);
                sessions_to_kill.push(child);
                if let Err(e) = self.tombstone(affected.last().expect("just pushed")) {
                    if tombstone_error.is_none() {
                        tombstone_error = Some(e);
                    }
                }
            }

            if let Some(session) = self.get(sid) {
                sessions_to_kill.push(session);
            }
            self.remove(sid);
            affected.push(sid.to_string());
            if let Err(e) = self.tombstone(sid) {
                if tombstone_error.is_none() {
                    tombstone_error = Some(e);
                }
            }

            (sessions_to_kill, affected, tombstone_error)
        };

        for session in sessions_to_kill {
            let cid = session.id().to_string();
            if let Err(e) = session.kill().await {
                tracing::warn!(
                    session = %cid,
                    parent = %sid,
                    error = %e,
                    "cascade kill: session returned error"
                );
            }
        }

        KillTreeResult {
            affected,
            tombstone_error,
        }
    }

    /// Stop a session tree without removing or tombstoning persisted metadata.
    /// Used by the UI Stop action so the killed session remains visible for
    /// transcript replay and for an explicit Restart action.
    pub async fn stop_tree(&self, sid: &str) -> StopTreeResult {
        let sessions_to_stop = {
            let _guard = lock_or_recover(&self.lifecycle_lock);
            let mut sessions = self.descendants_of(sid);
            sessions.reverse();
            if let Some(session) = self.get(sid) {
                sessions.push(session);
            }
            sessions
        };

        let mut affected = Vec::with_capacity(sessions_to_stop.len());
        for session in sessions_to_stop {
            let cid = session.id().to_string();
            if let Err(e) = session.kill().await {
                tracing::warn!(
                    session = %cid,
                    parent = %sid,
                    error = %e,
                    "stop tree: session returned error"
                );
            }
            affected.push(cid);
        }
        StopTreeResult { affected }
    }

    /// Direct children of `parent_sid` (one level only). Order is unspecified.
    pub fn children_of(&self, parent_sid: &str) -> Vec<Arc<AgentSession>> {
        self.sessions
            .iter()
            .filter_map(|e| {
                let s = e.value().clone();
                if s.parent_session_id_static() == Some(parent_sid) {
                    Some(s)
                } else {
                    None
                }
            })
            .collect()
    }

    /// All descendants of `parent_sid` (recursive, exclusive of the parent
    /// itself). Topological-ish order: a parent appears before its children
    /// in the returned vec — useful when callers want to kill children first
    /// they should reverse it.
    pub fn descendants_of(&self, parent_sid: &str) -> Vec<Arc<AgentSession>> {
        let mut out: Vec<Arc<AgentSession>> = Vec::new();
        let mut queue: Vec<String> = vec![parent_sid.to_string()];
        while let Some(pid) = queue.pop() {
            for child in self.children_of(&pid) {
                queue.push(child.id().to_string());
                out.push(child);
            }
        }
        out
    }

    /// Whether `maybe_descendant` is `ancestor` or transitively a child of
    /// it. Used by the MCP layer to ensure a session cannot cancel sessions
    /// outside its own tree.
    pub fn is_in_tree(&self, ancestor_sid: &str, maybe_descendant_sid: &str) -> bool {
        if ancestor_sid == maybe_descendant_sid {
            return true;
        }
        let Some(d) = self.get(maybe_descendant_sid) else {
            return false;
        };
        let mut current = d.parent_session_id_static().map(str::to_string);
        while let Some(pid) = current {
            if pid == ancestor_sid {
                return true;
            }
            current = self
                .get(&pid)
                .and_then(|p| p.parent_session_id_static().map(str::to_string));
        }
        false
    }

    fn tree_depth(&self, sid: &str) -> usize {
        let mut depth = 0;
        let mut current = self
            .get(sid)
            .and_then(|s| s.parent_session_id_static().map(str::to_string));
        while let Some(pid) = current {
            depth += 1;
            current = self
                .get(&pid)
                .and_then(|p| p.parent_session_id_static().map(str::to_string));
        }
        depth
    }
}

/// Per-spawn options.
#[derive(Debug, Clone, Default)]
pub struct SpawnOpts {
    /// Absolute path to a JSON file consumed by the agent's `--mcp-config`
    /// flag (or its kind-specific equivalent). `None` disables MCP injection.
    pub mcp_config_path: Option<PathBuf>,
    /// Stdio MCP command for CLIs that accept per-invocation config overrides
    /// instead of a config-file flag (Codex).
    pub mcp_server_command: Option<String>,
    /// Stdio MCP args paired with [`Self::mcp_server_command`].
    pub mcp_server_args: Vec<String>,
    /// Additional stdio MCP servers exposed to CLIs that support per-spawn
    /// config. Claude gets these through the JSON config file; Codex gets
    /// equivalent `-c mcp_servers.<name>.*` overrides.
    pub extra_mcp_servers: Vec<McpServerConfig>,
    /// Optional initial prompt to write into the PTY after spawn. Used by the
    /// role-template system to seed the agent.
    pub role_prompt: Option<String>,
    /// Optional role name to record in [`SessionMeta`] for inspection. Does
    /// NOT affect runtime behavior on its own; pair with `role_prompt`.
    pub role: Option<String>,
    /// Session that owns this worker's lifecycle/output. For child spawns the
    /// server normally sets this to the parent session id.
    pub owner_session_id: Option<String>,
    /// Harness task id this session is scoped to.
    pub task_id: Option<String>,
    /// Resource/work scopes granted to this session.
    pub scopes: Vec<String>,
    /// Optional repository identity attached by the server from the per-profile
    /// repo index.
    pub repo: Option<SessionRepoContext>,
    /// Capability set actually loaded or emphasized for this session. Stored
    /// in SessionMeta so later efficiency analysis can compare spawn shape
    /// against transcript/tool outcomes.
    pub loaded_capabilities: LoadedCapabilities,
    /// Optional CLI model override for this spawn.
    pub model: Option<String>,
    /// Optional CLI reasoning/effort override for this spawn.
    pub effort: Option<String>,
    /// Briefing appended to claude's system prompt via `--append-system-prompt`
    /// (NOT typed into the PTY). Used to tell the agent about the harness MCP
    /// tools so it doesn't fall back to its built-in todo list. Silent to the
    /// user — never shows up as a chat message.
    pub auto_intro: Option<String>,
    /// Parent session id when this spawn is a child in the session tree
    /// (e.g. a Zeus worker). `None` for root spawns. The manager will look
    /// the parent up and inherit its `root_session_id`.
    pub parent_session_id: Option<String>,
    /// Caller-pre-minted session id. When `Some`, the manager uses this id
    /// instead of generating one — lets the server build the MCP config
    /// (which embeds `--session-id`) before the session is actually spawned.
    pub session_id_override: Option<String>,
    /// Optional `(cols, rows)` to pass to `openpty()` instead of the 80x24
    /// default. When the frontend knows its terminal viewport ahead of time
    /// (it measures the container before POSTing), forwarding the real size
    /// lets the TUI render its first frame at the correct dimensions —
    /// otherwise the catch-up SSE replays bytes calibrated for 80 cols into
    /// a wider terminal and the user sees a mangled first frame.
    pub initial_size: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Translate `SpawnOpts` into the CLI flags appended to the agent invocation.
///
/// - `Claude`: pins `--session-id <id>` so the harness UUID matches the on-disk
///   transcript filename (`~/.claude/projects/{cwd-slug}/{id}.jsonl`); the
///   budget reporter relies on this mapping. Also adds
///   `--mcp-config <path> --strict-mcp-config` when MCP injection is on, plus
///   `--disallowed-tools TodoWrite` so claude can't satisfy task-
///   shaped requests with its in-process todo list (which never reaches the
///   harness TaskStore and so leaves the right-side Tasks panel empty).
/// - `Codex`: injects the harness MCP with per-invocation `-c
///   mcp_servers.harness.*` overrides. Codex does not have a `--mcp-config`
///   file flag, so we avoid mutating `~/.codex/config.toml`.
fn build_extra_args(kind: AgentKind, opts: &SpawnOpts, session_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    if matches!(kind, AgentKind::Claude) {
        out.push("--session-id".to_string());
        out.push(session_id.to_string());
        out.push("--model".to_string());
        out.push(
            opts.model
                .as_deref()
                .unwrap_or(DEFAULT_CLAUDE_MODEL)
                .to_string(),
        );
        out.push("--effort".to_string());
        out.push(
            opts.effort
                .as_deref()
                .unwrap_or(DEFAULT_CLAUDE_EFFORT)
                .to_string(),
        );
    }

    // ── Per-CLI autonomous-mode flags ──────────────────────────────────────
    // Harness sessions run under our supervision (scheduler, pause flag,
    // budget caps, audit log) — per-call approval prompts are noise. Each
    // CLI is opted into "autonomous" mode at spawn time. Tools that don't
    // have a documented flag for this skip silently.
    match kind {
        AgentKind::Codex => {
            // Codex harness workers run behind the harness' own policy,
            // budget and audit rails. Avoid per-call Codex approval prompts
            // for harness MCP tools.
            out.push("--dangerously-bypass-approvals-and-sandbox".to_string());
            out.push("--model".to_string());
            out.push(
                opts.model
                    .as_deref()
                    .unwrap_or(DEFAULT_CODEX_MODEL)
                    .to_string(),
            );
            out.push("-c".to_string());
            out.push(format!(
                "model_reasoning_effort={}",
                toml_string(opts.effort.as_deref().unwrap_or(DEFAULT_CODEX_EFFORT))
            ));
            if let Some(command) = opts.mcp_server_command.as_ref() {
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.harness.command={}",
                    toml_string(command)
                ));
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.harness.args={}",
                    toml_string_array(&opts.mcp_server_args)
                ));
            }
            for server in &opts.extra_mcp_servers {
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.{}.command={}",
                    server.name,
                    toml_string(&server.command)
                ));
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.{}.args={}",
                    server.name,
                    toml_string_array(&server.args)
                ));
            }
            if let Some(intro) = opts.auto_intro.as_ref() {
                // Codex supports developer instructions through config
                // overrides. Unlike the positional `[PROMPT]`, this is model
                // instruction context and does not appear as the first user
                // turn in the TUI/transcript.
                out.push("-c".to_string());
                out.push(format!("developer_instructions={}", toml_string(intro)));
            }
            // Pass only the actual role/initial work prompt as Codex's
            // positional `[PROMPT]` arg. That's how Codex's CLI accepts the
            // first user turn — typing into its Ink TUI via bracketed paste is
            // racey because the TUI takes ~1s to mount and Ink doesn't always
            // honor the paste-end + CR sequence. As a positional arg it's
            // submitted before the TUI even renders.
            if let Some(prompt) = opts.role_prompt.as_ref() {
                out.push(prompt.clone());
            }
        }
        AgentKind::Cursor => {
            // cursor-agent flag for non-interactive autonomous mode is not
            // verified yet; leave a TODO. Worst case the user accepts each
            // tool in the TUI manually until we wire it.
        }
        AgentKind::Antigravity => {
            // Same as cursor — unverified. Leave a TODO.
        }
        AgentKind::Claude | AgentKind::Zeus => {
            // Claude's `--dangerously-skip-permissions` lives in the MCP-
            // injection arm below (needs MCP config to be meaningful).
            // A direct Zeus kind here would be a bug; routes resolve Zeus to
            // its underlying CLI before spawning.
        }
    }

    if let Some(path) = opts.mcp_config_path.as_ref() {
        match kind {
            AgentKind::Claude => {
                out.push("--mcp-config".to_string());
                out.push(path.display().to_string());
                out.push("--strict-mcp-config".to_string());
                // Disable claude's built-in todo tools so it routes task-
                // shaped requests through the harness MCP `task_*` tools
                // (which fire the `task.created` SSE the UI listens for).
                // Flag confirmed via `claude --help`: `--disallowed-tools`
                // accepts a space- or comma-separated list of tool names.
                out.push("--disallowed-tools".to_string());
                out.push("TodoWrite".to_string());
                // Harness sessions run under our supervision (scheduler, pause
                // flag, budget caps, role-typed prompts) so the per-call
                // permission prompts are noise — claude should treat the
                // harness MCP tools as native operations. Prefer the explicit
                // mode flag supported by current Claude Code builds.
                out.push("--permission-mode".to_string());
                out.push("bypassPermissions".to_string());
                if let Some(intro) = opts.auto_intro.as_ref() {
                    // Silent system-prompt addendum — invisible to the user
                    // and not counted as a turn.
                    out.push("--append-system-prompt".to_string());
                    out.push(intro.clone());
                }
            }
            AgentKind::Codex => {
                // Codex consumes `mcp_server_command` via `-c` above. It has
                // no `--mcp-config <file>` equivalent.
            }
            AgentKind::Cursor | AgentKind::Antigravity => {
                tracing::warn!(
                    kind = %kind,
                    path = %path.display(),
                    "MCP injection not implemented for this CLI; skipping --mcp-config"
                );
            }
            AgentKind::Zeus => {
                // Unreachable in practice: routes/sessions.rs swaps Zeus for
                // its `underlying_cli()` before this matches. Keep the arm to satisfy
                // exhaustiveness without a `_` wildcard.
                tracing::warn!("build_extra_args called with Zeus kind directly; this is a bug");
            }
        }
    }
    out
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn toml_string_array(values: &[String]) -> String {
    let parts = values
        .iter()
        .map(|v| toml_string(v))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{parts}]")
}

fn sanitize_pty_prompt(prompt: &str) -> String {
    prompt
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect::<String>()
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn reap_orphan_if_identity_matches_in_background(meta: &SessionMeta) {
    let meta = meta.clone();
    std::thread::spawn(move || reap_orphan_if_identity_matches(&meta));
}

fn reap_orphan_if_identity_matches(meta: &SessionMeta) {
    let pid = meta.pid as i32;
    if !pid_alive(pid) {
        return;
    }
    let Some(expected) = meta.process_identity.as_ref() else {
        tracing::warn!(
            session_id = %meta.id,
            pid = meta.pid,
            "not reaping orphan PTY child because process identity is missing"
        );
        return;
    };
    let Some(actual) = process_identity(meta.pid) else {
        tracing::warn!(
            session_id = %meta.id,
            pid = meta.pid,
            "not reaping orphan PTY child because current process identity could not be read"
        );
        return;
    };
    if !process_identity_matches(expected, &actual) {
        tracing::warn!(
            session_id = %meta.id,
            pid = meta.pid,
            expected = ?expected,
            actual = ?actual,
            "not reaping orphan PTY child because PID identity changed"
        );
        return;
    }

    tracing::warn!(
        session_id = %meta.id,
        pid = meta.pid,
        "reaping orphan PTY child from persisted running session"
    );
    terminate_pid(pid);
}

/// True when the persisted session PID is alive **and** still the same
/// process recorded in `meta` (start-time/cmdline identity guard against PID
/// recycling — same check the orphan reaper uses). Metas without a recorded
/// identity fall back to plain liveness.
pub fn pid_alive_and_identity_matches(meta: &SessionMeta) -> bool {
    if !pid_alive(meta.pid as i32) {
        return false;
    }
    let Some(expected) = meta.process_identity.as_ref() else {
        return true;
    };
    match process_identity(meta.pid) {
        Some(actual) => process_identity_matches(expected, &actual),
        // Identity was recorded but can no longer be read — conservatively
        // treat the PID as recycled/dead.
        None => false,
    }
}

fn process_identity_matches(expected: &ProcessIdentity, actual: &ProcessIdentity) -> bool {
    if let Some(expected_start) = expected.linux_start_time_ticks {
        return actual.linux_start_time_ticks == Some(expected_start);
    }
    if let Some(expected_cmdline) = expected.cmdline.as_deref() {
        return actual.cmdline.as_deref() == Some(expected_cmdline);
    }
    if let Some(expected_comm) = expected.comm.as_deref() {
        return actual.comm.as_deref() == Some(expected_comm);
    }
    false
}

fn terminate_pid(pid: i32) {
    #[cfg(unix)]
    {
        if pid <= 0 {
            return;
        }
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if !pid_alive(pid) {
                return;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{SessionMeta, SessionStatus};
    use std::path::PathBuf;

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("harness-session-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    fn test_meta(id: &str, thread_id: &str, status: SessionStatus, pid: u32) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            kind: AgentKind::Cursor,
            thread_id: thread_id.to_string(),
            cwd: "/tmp".to_string(),
            pid,
            process_identity: None,
            status,
            started_at: 1_700_000_000_000,
            exit_code: None,
            result: None,
            role: None,
            owner_session_id: None,
            task_id: None,
            scopes: Vec::new(),
            repo: None,
            loaded_capabilities: LoadedCapabilities::default(),
            parent_session_id: None,
            root_session_id: id.to_string(),
            detected_state: None,
            has_transcript: false,
        }
    }

    fn write_meta(root: &std::path::Path, meta: &SessionMeta) {
        let dir = root.join(&meta.id);
        std::fs::create_dir_all(&dir).expect("create session dir");
        std::fs::write(
            dir.join("meta.json"),
            serde_json::to_vec_pretty(meta).expect("serialize meta"),
        )
        .expect("write meta");
    }

    #[test]
    fn claude_without_mcp_only_pins_session_id() {
        let opts = SpawnOpts::default();
        let args = build_extra_args(AgentKind::Claude, &opts, "sid-123");
        assert_eq!(
            args,
            vec![
                "--session-id".to_string(),
                "sid-123".to_string(),
                "--model".to_string(),
                DEFAULT_CLAUDE_MODEL.to_string(),
                "--effort".to_string(),
                DEFAULT_CLAUDE_EFFORT.to_string()
            ]
        );
        assert!(!args.iter().any(|a| a == "--disallowed-tools"));
        assert!(!args.iter().any(|a| a == "--mcp-config"));
    }

    #[test]
    fn claude_model_and_effort_can_be_overridden_per_spawn() {
        let opts = SpawnOpts {
            model: Some("opus".into()),
            effort: Some("high".into()),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Claude, &opts, "sid-123");

        assert!(args.windows(2).any(|w| w[0] == "--model" && w[1] == "opus"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--effort" && w[1] == "high"));
    }

    #[test]
    fn claude_with_mcp_disables_todo_tools() {
        let opts = SpawnOpts {
            mcp_config_path: Some(PathBuf::from("/tmp/cfg.json")),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Claude, &opts, "sid-xyz");

        // session-id is always present for claude
        let sid_idx = args.iter().position(|a| a == "--session-id").unwrap();
        assert_eq!(args[sid_idx + 1], "sid-xyz");
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--model" && w[1] == DEFAULT_CLAUDE_MODEL));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--effort" && w[1] == DEFAULT_CLAUDE_EFFORT));

        // MCP wiring
        let mcp_idx = args.iter().position(|a| a == "--mcp-config").unwrap();
        assert_eq!(args[mcp_idx + 1], "/tmp/cfg.json");
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));

        // Todo tools disabled
        let dis_idx = args.iter().position(|a| a == "--disallowed-tools").unwrap();
        assert_eq!(args[dis_idx + 1], "TodoWrite");

        // Permission prompts skipped — harness supervises the session.
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--permission-mode" && w[1] == "bypassPermissions"));

        // No auto_intro set → no --append-system-prompt
        assert!(!args.iter().any(|a| a == "--append-system-prompt"));
    }

    #[test]
    fn claude_with_intro_appends_system_prompt() {
        let opts = SpawnOpts {
            mcp_config_path: Some(PathBuf::from("/tmp/cfg.json")),
            auto_intro: Some("harness MCP available: task_create, ...".to_string()),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Claude, &opts, "sid-i");
        let idx = args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap();
        assert_eq!(args[idx + 1], "harness MCP available: task_create, ...");
    }

    #[test]
    fn claude_intro_without_mcp_is_not_appended() {
        // auto_intro is only meaningful when the harness MCP is wired; if
        // mcp_config_path is None, the intro would describe tools the agent
        // can't see — better to skip it than confuse the model.
        let opts = SpawnOpts {
            auto_intro: Some("some intro".to_string()),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Claude, &opts, "sid");
        assert!(!args.iter().any(|a| a == "--append-system-prompt"));
    }

    #[test]
    fn pty_prompt_sanitizer_strips_terminal_escape_bytes() {
        let prompt = "before\x1b[201~\n\x1b]0;title\x07after";
        let sanitized = sanitize_pty_prompt(prompt);

        assert_eq!(sanitized, "before[201~\n]0;titleafter");
        assert!(!sanitized.contains('\u{1b}'));
    }

    #[test]
    fn codex_gets_mcp_overrides_developer_instructions_and_prompt() {
        let opts = SpawnOpts {
            mcp_config_path: Some(PathBuf::from("/tmp/cfg.json")),
            mcp_server_command: Some("/tmp/harness-mcp-server".into()),
            mcp_server_args: vec![
                "--thread".into(),
                "t1".into(),
                "--agent-id".into(),
                "agent:codex-1".into(),
            ],
            extra_mcp_servers: vec![McpServerConfig {
                name: "crawl4ai".into(),
                command: "npx".into(),
                args: vec![
                    "-y".into(),
                    "mcp-remote".into(),
                    "http://localhost:11235/mcp/sse".into(),
                ],
            }],
            auto_intro: Some("Harness tools are available.".into()),
            role_prompt: Some("Inspect the database.".into()),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Codex, &opts, "sid-c");
        assert!(args
            .iter()
            .any(|a| a == "--dangerously-bypass-approvals-and-sandbox"));
        assert!(args
            .iter()
            .all(|a| a != "--ask-for-approval" && a != "--sandbox"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--model" && w[1] == DEFAULT_CODEX_MODEL));
        assert!(args
            .iter()
            .any(|a| a == "model_reasoning_effort=\"medium\""));
        assert!(
            !args.iter().any(|a| a == "--mcp-config"),
            "Codex uses -c config overrides, not --mcp-config"
        );
        assert!(args
            .iter()
            .any(|a| a == "mcp_servers.harness.command=\"/tmp/harness-mcp-server\""));
        assert!(args.iter().any(|a| {
            a
            == "mcp_servers.harness.args=[\"--thread\", \"t1\", \"--agent-id\", \"agent:codex-1\"]"
        }));
        assert!(args
            .iter()
            .any(|a| a == "mcp_servers.crawl4ai.command=\"npx\""));
        assert!(args.iter().any(|a| {
            a == "mcp_servers.crawl4ai.args=[\"-y\", \"mcp-remote\", \"http://localhost:11235/mcp/sse\"]"
        }));
        assert!(args
            .iter()
            .any(|a| a == "developer_instructions=\"Harness tools are available.\""));
        assert!(args.last().unwrap().contains("Inspect the database."));
        assert!(!args
            .last()
            .unwrap()
            .contains("Harness tools are available."));
        assert!(
            !args.iter().any(|a| a == "--disallowed-tools"),
            "Todo tool disabling is Claude-only"
        );
    }

    #[test]
    fn codex_auto_intro_only_is_not_a_user_prompt() {
        let opts = SpawnOpts {
            auto_intro: Some("You are Zeus.".into()),
            ..SpawnOpts::default()
        };

        let args = build_extra_args(AgentKind::Codex, &opts, "sid-c");

        assert!(args
            .iter()
            .any(|a| a == "developer_instructions=\"You are Zeus.\""));
        assert!(
            !args.iter().any(|a| a == "You are Zeus."),
            "auto_intro must not become Codex's positional user prompt"
        );
    }

    #[test]
    fn codex_model_and_effort_can_be_overridden_per_spawn() {
        let opts = SpawnOpts {
            model: Some("gpt-5.4".into()),
            effort: Some("xhigh".into()),
            ..SpawnOpts::default()
        };

        let args = build_extra_args(AgentKind::Codex, &opts, "sid-c");

        assert!(args
            .windows(2)
            .any(|w| w[0] == "--model" && w[1] == "gpt-5.4"));
        assert!(args.iter().any(|a| a == "model_reasoning_effort=\"xhigh\""));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn session_tree_tracks_active_and_exited_direct_children() {
        let root = temp_test_dir("tree");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");

        // Cursor currently contributes no extra CLI args, so these ordinary
        // POSIX binaries can stand in for real agent processes in the PTY.
        let shell = PathBuf::from("/bin/sh");
        let true_bin = PathBuf::from("/bin/true");

        let parent = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                shell.clone(),
                "thread-1".to_string(),
                cwd.clone(),
                SpawnOpts {
                    session_id_override: Some("parent".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn parent");
        let active_child = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                shell,
                "thread-1".to_string(),
                cwd.clone(),
                SpawnOpts {
                    session_id_override: Some("active-child".to_string()),
                    parent_session_id: Some(parent.id().to_string()),
                    role: Some("worker".to_string()),
                    owner_session_id: Some(parent.id().to_string()),
                    task_id: Some("T-0001".to_string()),
                    scopes: vec!["backend".to_string(), "task:T-0001".to_string()],
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn active child");
        let exited_child = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                true_bin,
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("exited-child".to_string()),
                    parent_session_id: Some(parent.id().to_string()),
                    role: Some("quick-worker".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn exited child");

        for _ in 0..20 {
            if exited_child.meta().await.status != crate::meta::SessionStatus::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        let active_meta = active_child.meta().await;
        assert_eq!(active_meta.parent_session_id.as_deref(), Some(parent.id()));
        assert_eq!(active_meta.root_session_id, parent.id());
        assert_eq!(active_meta.role.as_deref(), Some("worker"));
        assert_eq!(active_meta.owner_session_id.as_deref(), Some(parent.id()));
        assert_eq!(active_meta.task_id.as_deref(), Some("T-0001"));
        assert_eq!(
            active_meta.scopes,
            vec!["backend".to_string(), "task:T-0001".to_string()]
        );

        let exited_meta = exited_child.meta().await;
        assert_eq!(exited_meta.parent_session_id.as_deref(), Some(parent.id()));
        assert_eq!(exited_meta.root_session_id, parent.id());
        assert_eq!(exited_meta.status, crate::meta::SessionStatus::Exited);

        let child_ids: std::collections::HashSet<String> = manager
            .children_of(parent.id())
            .into_iter()
            .map(|s| s.id().to_string())
            .collect();
        assert_eq!(child_ids.len(), 2);
        assert!(child_ids.contains(active_child.id()));
        assert!(child_ids.contains(exited_child.id()));
        assert!(manager.is_in_tree(parent.id(), active_child.id()));
        assert!(manager.is_in_tree(parent.id(), exited_child.id()));
        assert!(!manager.is_in_tree(active_child.id(), parent.id()));

        let _ = active_child.kill().await;
        let _ = parent.kill().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn kill_aborts_state_detector_and_prompt_injector() {
        let root = temp_test_dir("kill-aborts-runtime");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");

        let session = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                PathBuf::from("/bin/sh"),
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("interruptible".to_string()),
                    role_prompt: Some("stay alive until killed".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn session");

        let handles = session.interruptible_abort_handles_for_test();
        let state_detector = handles
            .state_detector
            .expect("state detector should be registered");
        let prompt_injector = handles
            .prompt_injector
            .expect("prompt injector should be registered");

        session.kill().await.expect("kill session");

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if state_detector.is_finished() && prompt_injector.is_finished() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("interruptible tasks should abort after kill");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn load_existing_rehydrates_exited_session_from_disk() {
        let root = temp_test_dir("rehydrate-exited");
        let sessions_root = root.join("sessions");
        let meta = test_meta("detached-1", "thread-1", SessionStatus::Exited, 123);
        write_meta(&sessions_root, &meta);
        std::fs::write(
            sessions_root.join("detached-1").join("output.log"),
            b"hello",
        )
        .expect("write output");

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");

        assert!(manager.get("detached-1").is_none());
        let metas = manager.list_metas().await;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id, "detached-1");
        assert_eq!(metas[0].status, SessionStatus::Exited);
        assert_eq!(manager.read_output("detached-1").expect("output"), b"hello");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn remove_forgets_detached_session_metadata() {
        let root = temp_test_dir("remove-detached");
        let sessions_root = root.join("sessions");
        write_meta(
            &sessions_root,
            &test_meta("detached-1", "thread-1", SessionStatus::Exited, 123),
        );

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");
        assert_eq!(manager.list_metas().await.len(), 1);

        manager.remove("detached-1");

        assert!(manager.list_metas().await.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn kill_tree_and_tombstone_is_idempotent_for_missing_session() {
        let root = temp_test_dir("kill-tree-missing");
        let sessions_root = root.join("sessions");
        let manager = Manager::new(&sessions_root).expect("manager");

        let result = manager.kill_tree_and_tombstone("missing").await;

        assert_eq!(result.affected, vec!["missing".to_string()]);
        assert!(result.tombstone_error.is_none());
        assert!(manager.is_tombstoned("missing"));
        assert!(sessions_root.join("missing").join(DELETED_MARKER).exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_all_kills_leaf_up_without_tombstones() {
        let root = temp_test_dir("shutdown-all");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");
        let shell = PathBuf::from("/bin/sh");

        let parent = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                shell.clone(),
                "thread-1".to_string(),
                cwd.clone(),
                SpawnOpts {
                    session_id_override: Some("parent".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn parent");
        let child = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                shell.clone(),
                "thread-1".to_string(),
                cwd.clone(),
                SpawnOpts {
                    session_id_override: Some("child".to_string()),
                    parent_session_id: Some(parent.id().to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn child");
        let grandchild = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                shell,
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("grandchild".to_string()),
                    parent_session_id: Some(child.id().to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn grandchild");

        let affected = manager.shutdown_all().await;

        assert_eq!(
            affected,
            vec![
                grandchild.id().to_string(),
                child.id().to_string(),
                parent.id().to_string()
            ]
        );
        assert!(manager.get(parent.id()).is_some());
        assert!(manager.get(child.id()).is_some());
        assert!(manager.get(grandchild.id()).is_some());
        assert!(!manager.is_tombstoned(parent.id()));
        assert!(!manager.is_tombstoned(child.id()));
        assert!(!manager.is_tombstoned(grandchild.id()));
        assert_eq!(parent.meta().await.status, SessionStatus::Killed);
        assert_eq!(child.meta().await.status, SessionStatus::Killed);
        assert_eq!(grandchild.meta().await.status, SessionStatus::Killed);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_all_rejects_late_spawns() {
        let root = temp_test_dir("shutdown-all-gate");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");

        assert!(manager.shutdown_all().await.is_empty());

        let result = manager.spawn_with_opts(
            AgentKind::Cursor,
            PathBuf::from("/bin/sh"),
            "thread-1".to_string(),
            cwd,
            SpawnOpts::default(),
        );

        assert!(matches!(
            result,
            Err(SessionError::Invalid(msg)) if msg.contains("shutting down")
        ));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exit_event_is_emitted_after_meta_is_persisted() {
        let root = temp_test_dir("exit-meta-before-event");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");
        let mut events = manager.subscribe();

        let session = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                PathBuf::from("/bin/true"),
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("quick-exit".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn quick exit");

        let code = loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                .await
                .expect("exit event timeout")
                .expect("receive event");
            if let SessionEvent::Exit {
                session_id, code, ..
            } = event
            {
                if session_id == session.id() {
                    break code;
                }
            }
        };

        let meta = session.meta().await;
        assert_eq!(code, Some(0));
        assert_eq!(meta.status, SessionStatus::Exited);
        assert_eq!(meta.exit_code, Some(0));
        let result = meta.result.expect("session result");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.process_success);
        assert!(result.completed_at >= meta.started_at);

        let persisted: SessionMeta = serde_json::from_slice(
            &std::fs::read(sessions_root.join("quick-exit").join("meta.json"))
                .expect("read persisted meta"),
        )
        .expect("parse persisted meta");
        assert_eq!(persisted.status, SessionStatus::Exited);
        assert_eq!(persisted.exit_code, Some(0));
        let persisted_result = persisted.result.expect("persisted result");
        assert_eq!(persisted_result.exit_code, Some(0));
        assert!(persisted_result.process_success);
        assert!(persisted_result.completed_at >= persisted.started_at);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn load_existing_skips_tombstoned_session() {
        let root = temp_test_dir("skip-tombstoned");
        let sessions_root = root.join("sessions");
        write_meta(
            &sessions_root,
            &test_meta("deleted-1", "thread-1", SessionStatus::Exited, 123),
        );
        write_meta(
            &sessions_root,
            &test_meta("visible-1", "thread-1", SessionStatus::Exited, 124),
        );

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.tombstone("deleted-1").expect("tombstone");
        manager.load_existing().expect("load existing");

        let ids: Vec<String> = manager
            .list_metas()
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["visible-1".to_string()]);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn load_existing_reconciles_orphan_running_to_exited() {
        let root = temp_test_dir("rehydrate-orphan");
        let sessions_root = root.join("sessions");
        let meta = test_meta("orphan-1", "thread-1", SessionStatus::Running, 0);
        write_meta(&sessions_root, &meta);

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");

        let metas = manager.list_metas().await;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].status, SessionStatus::Exited);

        let persisted: SessionMeta = serde_json::from_slice(
            &std::fs::read(sessions_root.join("orphan-1").join("meta.json"))
                .expect("read persisted meta"),
        )
        .expect("parse persisted meta");
        assert_eq!(persisted.status, SessionStatus::Exited);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn load_existing_reconciles_running_even_if_pid_is_alive() {
        let root = temp_test_dir("rehydrate-live-pid-detached");
        let sessions_root = root.join("sessions");
        let meta = test_meta(
            "detached-live-pid",
            "thread-1",
            SessionStatus::Running,
            std::process::id(),
        );
        write_meta(&sessions_root, &meta);

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");

        assert!(manager.get("detached-live-pid").is_none());
        let metas = manager.list_metas().await;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].status, SessionStatus::Exited);

        let persisted: SessionMeta = serde_json::from_slice(
            &std::fs::read(sessions_root.join("detached-live-pid").join("meta.json"))
                .expect("read persisted meta"),
        )
        .expect("parse persisted meta");
        assert_eq!(persisted.status, SessionStatus::Exited);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn spawn_persists_pid_identity_in_meta() {
        let root = temp_test_dir("spawn-pid-identity");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        let manager = Manager::new(&sessions_root).expect("manager");

        let session = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                PathBuf::from("/bin/sh"),
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("identity-session".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn session");

        let persisted: SessionMeta = serde_json::from_slice(
            &std::fs::read(sessions_root.join("identity-session").join("meta.json"))
                .expect("read persisted meta"),
        )
        .expect("parse persisted meta");
        assert_eq!(persisted.pid, session.pid());
        let identity = persisted
            .process_identity
            .as_ref()
            .expect("process identity should be persisted");
        assert!(identity.linux_start_time_ticks.is_some());
        assert!(!identity.is_empty());

        let _ = session.kill().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn load_existing_does_not_kill_pid_with_mismatched_identity() {
        let root = temp_test_dir("rehydrate-pid-mismatch");
        let sessions_root = root.join("sessions");
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let mut meta = test_meta(
            "pid-mismatch",
            "thread-1",
            SessionStatus::Running,
            child.id(),
        );
        meta.process_identity = Some(ProcessIdentity {
            linux_start_time_ticks: Some(u64::MAX),
            cmdline: Some("definitely-not-this-process".to_string()),
            comm: Some("not-sleep".to_string()),
        });
        write_meta(&sessions_root, &meta);

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");

        assert!(child.try_wait().expect("poll sleep").is_none());
        let persisted: SessionMeta = serde_json::from_slice(
            &std::fs::read(sessions_root.join("pid-mismatch").join("meta.json"))
                .expect("read persisted meta"),
        )
        .expect("parse persisted meta");
        assert_eq!(persisted.status, SessionStatus::Exited);

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_metas_merges_live_and_detached_without_duplicates() {
        let root = temp_test_dir("list-metas");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        write_meta(
            &sessions_root,
            &test_meta("detached-1", "thread-1", SessionStatus::Exited, 123),
        );

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");
        let live = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                PathBuf::from("/bin/sh"),
                "thread-1".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("live-1".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn live");

        let ids: std::collections::HashSet<String> = manager
            .list_metas()
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("detached-1"));
        assert!(ids.contains("live-1"));

        let _ = live.kill().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn live_session_shadows_detached_same_id() {
        let root = temp_test_dir("shadow-detached");
        let sessions_root = root.join("sessions");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&cwd).expect("create cwd");
        write_meta(
            &sessions_root,
            &test_meta("same-id", "old-thread", SessionStatus::Exited, 123),
        );

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");
        let live = manager
            .spawn_with_opts(
                AgentKind::Cursor,
                PathBuf::from("/bin/sh"),
                "new-thread".to_string(),
                cwd,
                SpawnOpts {
                    session_id_override: Some("same-id".to_string()),
                    ..SpawnOpts::default()
                },
            )
            .expect("spawn live");

        let metas: Vec<SessionMeta> = manager
            .list_metas()
            .await
            .into_iter()
            .filter(|m| m.id == "same-id")
            .collect();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].thread_id, "new-thread");
        assert_eq!(metas[0].status, SessionStatus::Running);

        let _ = live.kill().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn load_existing_skips_bad_meta_without_failing() {
        let root = temp_test_dir("bad-meta");
        let sessions_root = root.join("sessions");
        write_meta(
            &sessions_root,
            &test_meta("valid", "thread-1", SessionStatus::Exited, 123),
        );
        let bad_dir = sessions_root.join("bad");
        std::fs::create_dir_all(&bad_dir).expect("create bad dir");
        std::fs::write(bad_dir.join("meta.json"), b"{not json").expect("write bad meta");

        let manager = Manager::new(&sessions_root).expect("manager");
        manager.load_existing().expect("load existing");

        let metas = manager.list_metas().await;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id, "valid");

        let _ = std::fs::remove_dir_all(root);
    }
}
