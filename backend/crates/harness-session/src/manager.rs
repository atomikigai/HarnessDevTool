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
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match self {
            SessionEvent::Started { session_id, .. }
            | SessionEvent::Output { session_id, .. }
            | SessionEvent::Exit { session_id, .. } => session_id,
        }
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            SessionEvent::Started { .. } => "session.started",
            SessionEvent::Output { .. } => "session.output",
            SessionEvent::Exit { .. } => "session.exit",
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
        let id = uuid::Uuid::new_v4().to_string();
        let dir = self.sessions_root.join(&id);
        let extra_args = build_extra_args(kind, &opts, &id);
        let session = AgentSession::spawn_with_id(
            id.clone(),
            kind,
            binary,
            thread_id,
            cwd,
            dir,
            extra_args,
            opts.role.clone(),
            self.bus.clone(),
        )?;
        self.sessions.insert(id, session.clone());

        // `auto_intro` is passed to claude as `--append-system-prompt` (CLI
        // flag, baked at spawn) so it never appears as user-typed input.
        // `role_prompt` IS user-typed: it's the "begin your role" kick that
        // tells the agent to start working, so it must appear in the
        // conversation. 200ms grace lets the CLI draw its prompt first.
        if let Some(mut payload) = opts.role_prompt {
            let s = session.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                payload.push('\n');
                if let Err(e) = s.write_input(payload.as_bytes()).await {
                    tracing::warn!(error = %e, "failed to inject role prompt");
                }
            });
        }

        Ok(session)
    }

    /// Forget a session (does NOT delete on-disk state).
    pub fn remove(&self, sid: &str) {
        self.sessions.remove(sid);
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
            AgentKind::Codex => {
                tracing::warn!(
                    path = %path.display(),
                    "codex MCP injection not implemented; skipping --mcp-config"
                );
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
        assert!(
            args.is_empty(),
            "codex must not get any extra flags yet, got {args:?}"
        );
    }
}
