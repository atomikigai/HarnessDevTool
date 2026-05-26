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
        let extra_args = build_extra_args(kind, &opts);
        let session = AgentSession::spawn_with_id(
            id.clone(),
            kind,
            binary,
            thread_id,
            cwd,
            dir,
            extra_args,
            self.bus.clone(),
        )?;
        self.sessions.insert(id, session.clone());
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
}

/// Translate `SpawnOpts` into the CLI flags appended to the agent invocation.
///
/// - `Claude`: `--mcp-config <path> --strict-mcp-config` (validated by spike Q7).
/// - `Codex`:  no equivalent flag exists in this version; skipped. The MCP
///   config path is recorded but not injected. Codex MCP wiring is deferred to
///   a later phase (likely via `$CODEX_HOME/config.toml` or `-c` overrides).
fn build_extra_args(kind: AgentKind, opts: &SpawnOpts) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(path) = opts.mcp_config_path.as_ref() {
        match kind {
            AgentKind::Claude => {
                out.push("--mcp-config".to_string());
                out.push(path.display().to_string());
                out.push("--strict-mcp-config".to_string());
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
