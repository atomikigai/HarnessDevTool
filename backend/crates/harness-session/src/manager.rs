use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::errors::SessionError;
use crate::kind::AgentKind;
use crate::output::OutputWriter;
use crate::session::AgentSession;

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
        b64: String,
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
    bus: broadcast::Sender<SessionEvent>,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager")
            .field("sessions_root", &self.sessions_root)
            .field("live_sessions", &self.sessions.len())
            .finish()
    }
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
            bus,
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

    /// Snapshot of all currently-tracked session handles.
    pub fn all(&self) -> Vec<Arc<AgentSession>> {
        self.sessions.iter().map(|e| e.value().clone()).collect()
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
            parent_session_id,
            root_session_id,
            opts.initial_size,
            self.bus.clone(),
        )?;
        self.sessions.insert(id, session.clone());

        // `auto_intro` is passed to claude as `--append-system-prompt` (CLI
        // flag, baked at spawn) so it never appears as user-typed input.
        // `role_prompt` IS user-typed: it's the "begin your role" kick that
        // tells the agent to start working, so it must appear in the
        // conversation.
        //
        // CLI-specific delivery:
        //   - **Codex**: prompt is passed as the positional `[PROMPT]` arg
        //     in `build_extra_args` — Codex submits it before its Ink TUI
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
                let s = session.clone();
                let sid = s.id().to_string();
                tokio::spawn(async move {
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
            }
        }

        Ok(session)
    }

    /// Forget a session (does NOT delete on-disk state).
    pub fn remove(&self, sid: &str) {
        self.sessions.remove(sid);
    }

    // ── Session-tree helpers (Zeus orchestrator) ─────────────────────────

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
}

/// Per-spawn options.
#[derive(Debug, Clone, Default)]
pub struct SpawnOpts {
    /// Absolute path to a JSON file consumed by the agent's `--mcp-config`
    /// flag (or its kind-specific equivalent). `None` disables MCP injection.
    pub mcp_config_path: Option<PathBuf>,
    /// Optional initial prompt to write into the PTY after spawn. Used by the
    /// role-template system to seed the agent.
    pub role_prompt: Option<String>,
    /// Optional role name to record in [`SessionMeta`] for inspection. Does
    /// NOT affect runtime behavior on its own; pair with `role_prompt`.
    pub role: Option<String>,
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

/// Translate `SpawnOpts` into the CLI flags appended to the agent invocation.
///
/// - `Claude`: pins `--session-id <id>` so the harness UUID matches the on-disk
///   transcript filename (`~/.claude/projects/{cwd-slug}/{id}.jsonl`); the
///   budget reporter relies on this mapping. Also adds
///   `--mcp-config <path> --strict-mcp-config` when MCP injection is on, plus
///   `--disallowed-tools TodoWrite TodoRead` so claude can't satisfy task-
///   shaped requests with its in-process todo list (which never reaches the
///   harness TaskStore and so leaves the right-side Tasks panel empty).
/// - `Codex`: no equivalent flags exist in this version; skipped. Codex
///   integration is deferred (likely via `$CODEX_HOME/config.toml` or `-c`).
fn build_extra_args(kind: AgentKind, opts: &SpawnOpts, session_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    if matches!(kind, AgentKind::Claude) {
        out.push("--session-id".to_string());
        out.push(session_id.to_string());
    }

    // ── Per-CLI autonomous-mode flags ──────────────────────────────────────
    // Harness sessions run under our supervision (scheduler, pause flag,
    // budget caps, audit log) — per-call approval prompts are noise. Each
    // CLI is opted into "autonomous" mode at spawn time. Tools that don't
    // have a documented flag for this skip silently.
    match kind {
        AgentKind::Codex => {
            // Codex autonomous mode: never ask for approval, sandbox writes
            // to the workspace dir. `--ask-for-approval never` + `-s
            // workspace-write` is the documented combo (the older
            // `--full-auto` flag was renamed). For Zeus workers this is
            // what we want — the orchestrator is the one approving /
            // validating, not a per-tool prompt.
            out.push("--ask-for-approval".to_string());
            out.push("never".to_string());
            out.push("--sandbox".to_string());
            out.push("workspace-write".to_string());
            // Pass the role/initial prompt as Codex's positional `[PROMPT]`
            // arg. That's how Codex's CLI accepts the first user turn —
            // typing into its Ink TUI via bracketed paste is racey because
            // the TUI takes ~1s to mount and Ink doesn't always honor the
            // paste-end + CR sequence. As a positional arg it's submitted
            // before the TUI even renders.
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
            // Zeus runs as Claude under the hood, same path.
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
                out.push("TodoWrite TodoRead".to_string());
                // Harness sessions run under our supervision (scheduler, pause
                // flag, budget caps, role-typed prompts) so the per-call
                // permission prompts are noise — claude should treat the
                // harness MCP tools as native operations.
                out.push("--dangerously-skip-permissions".to_string());
                if let Some(intro) = opts.auto_intro.as_ref() {
                    // Silent system-prompt addendum — invisible to the user
                    // and not counted as a turn.
                    out.push("--append-system-prompt".to_string());
                    out.push(intro.clone());
                }
            }
            AgentKind::Codex | AgentKind::Cursor | AgentKind::Antigravity => {
                tracing::warn!(
                    kind = %kind,
                    path = %path.display(),
                    "MCP injection not implemented for this CLI; skipping --mcp-config"
                );
            }
            AgentKind::Zeus => {
                // Unreachable in practice: routes/sessions.rs swaps Zeus for
                // its `underlying_cli()` (Claude) before this matches, so
                // we'd hit the Claude arm above. Keep the arm to satisfy
                // exhaustiveness without a `_` wildcard.
                tracing::warn!("build_extra_args called with Zeus kind directly; this is a bug");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn claude_without_mcp_only_pins_session_id() {
        let opts = SpawnOpts::default();
        let args = build_extra_args(AgentKind::Claude, &opts, "sid-123");
        assert_eq!(
            args,
            vec!["--session-id".to_string(), "sid-123".to_string()]
        );
        assert!(!args.iter().any(|a| a == "--disallowed-tools"));
        assert!(!args.iter().any(|a| a == "--mcp-config"));
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

        // MCP wiring
        let mcp_idx = args.iter().position(|a| a == "--mcp-config").unwrap();
        assert_eq!(args[mcp_idx + 1], "/tmp/cfg.json");
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));

        // Todo tools disabled
        let dis_idx = args.iter().position(|a| a == "--disallowed-tools").unwrap();
        assert_eq!(args[dis_idx + 1], "TodoWrite TodoRead");

        // Permission prompts skipped — harness supervises the session.
        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));

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
    fn codex_never_gets_disallowed_tools() {
        let opts = SpawnOpts {
            mcp_config_path: Some(PathBuf::from("/tmp/cfg.json")),
            ..SpawnOpts::default()
        };
        let args = build_extra_args(AgentKind::Codex, &opts, "sid-c");
        assert_eq!(
            args,
            vec![
                "--ask-for-approval".to_string(),
                "never".to_string(),
                "--sandbox".to_string(),
                "workspace-write".to_string(),
            ]
        );
        assert!(
            !args.iter().any(|a| a == "--mcp-config"),
            "codex MCP injection is still deferred"
        );
        assert!(
            !args.iter().any(|a| a == "--disallowed-tools"),
            "Todo tool disabling is Claude-only"
        );
    }
}
