use serde::{Deserialize, Serialize};

use crate::detect::AgentState;
use crate::kind::AgentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum SessionStatus {
    Running,
    Exited,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SessionMeta {
    pub id: String,
    pub kind: AgentKind,
    pub thread_id: String,
    pub cwd: String,
    pub pid: u32,
    pub status: SessionStatus,
    /// Unix epoch ms.
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Name of the role template that seeded this session, if any. Carried as
    /// metadata only — the prompt itself is written to the PTY at spawn time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Session that owns this worker's output and lifecycle decisions. For
    /// orchestrated child sessions this is the direct parent; root sessions
    /// leave it unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    /// Harness task this session is expected to work on, if the spawn was
    /// scoped to a task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Coarse resource/work scopes granted to the session. Examples:
    /// `backend`, `frontend`, `db:connection:<id>`, `task:T-0001`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,

    // ── Session tree (Zeus / orchestrator) ────────────────────────────────
    /// Parent session id when this session was spawned as a child of another
    /// (e.g. a Zeus worker). `None` for root sessions created directly by
    /// the user. Stable for the lifetime of the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// Topmost ancestor in the session tree — the root supervisor. For a
    /// root session this equals `id`. Always present so cost / lifecycle
    /// queries can group by tree without traversing the full parent chain.
    #[serde(default)]
    pub root_session_id: String,
    /// Heuristic interaction state derived from the PTY scrollback tail
    /// (working / blocked / idle / unknown). Updated periodically by a
    /// background task; `None` until the first detection pass runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_state: Option<AgentState>,
    /// Whether the harness is tailing a structured JSONL transcript for
    /// this session (Chat view available). True for Claude/Zeus today;
    /// false for CLIs whose transcript format isn't wired yet.
    #[serde(default)]
    pub has_transcript: bool,
}
