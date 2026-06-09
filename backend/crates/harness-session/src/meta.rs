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
pub struct SessionRepoContext {
    pub repo_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub root_path: String,
    pub canonical_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct LoadedCapabilities {
    /// MCP servers injected into the agent process for this session.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Harness skills or skill bundles intentionally loaded for this session.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Non-MCP tool groups exposed or explicitly emphasized at spawn time.
    #[serde(default)]
    pub tool_groups: Vec<String>,
}

impl LoadedCapabilities {
    pub fn is_empty(&self) -> bool {
        self.mcp_servers.is_empty() && self.skills.is_empty() && self.tool_groups.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ProcessIdentity {
    /// Linux `/proc/<pid>/stat` field 22. Strong guard against PID reuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub linux_start_time_ticks: Option<u64>,
    /// NUL-separated `/proc/<pid>/cmdline`, normalized to spaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub cmdline: Option<String>,
    /// Process name from `/proc/<pid>/comm` or `/proc/<pid>/stat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub comm: Option<String>,
}

impl ProcessIdentity {
    pub fn is_empty(&self) -> bool {
        self.linux_start_time_ticks.is_none() && self.cmdline.is_none() && self.comm.is_none()
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub process_identity: Option<ProcessIdentity>,
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
    /// Repository/worktree identity detected by the harness when the session
    /// was spawned. Dynamic continuity lives in HARNESS_HOME; this only links
    /// the session back to the per-profile repo index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<SessionRepoContext>,
    /// Final capability set injected or emphasized for this spawn. This is
    /// recorded after heuristics such as documentation-url detection so later
    /// efficiency analysis can compare loaded context against outcomes.
    #[serde(default)]
    pub loaded_capabilities: LoadedCapabilities,

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
    /// Whether the harness is tailing a structured JSONL transcript for this
    /// session. False for CLIs whose transcript format isn't wired yet.
    #[serde(default)]
    pub has_transcript: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::AgentKind;

    #[test]
    fn session_meta_defaults_loaded_capabilities_for_old_files() {
        let raw = r#"{
            "id": "s1",
            "kind": "codex",
            "thread_id": "t1",
            "cwd": "/tmp",
            "pid": 42,
            "status": "running",
            "started_at": 1700000000000,
            "root_session_id": "s1",
            "has_transcript": false
        }"#;

        let meta: SessionMeta = serde_json::from_str(raw).expect("deserialize old meta");
        assert_eq!(meta.kind, AgentKind::Codex);
        assert!(meta.loaded_capabilities.is_empty());
        assert!(meta.process_identity.is_none());
    }

    #[test]
    fn empty_loaded_capabilities_are_serialized_with_empty_arrays() {
        let meta = SessionMeta {
            id: "s1".to_string(),
            kind: AgentKind::Codex,
            thread_id: "t1".to_string(),
            cwd: "/tmp".to_string(),
            pid: 42,
            process_identity: None,
            status: SessionStatus::Running,
            started_at: 1_700_000_000_000,
            exit_code: None,
            role: None,
            owner_session_id: None,
            task_id: None,
            scopes: Vec::new(),
            repo: None,
            loaded_capabilities: LoadedCapabilities::default(),
            parent_session_id: None,
            root_session_id: "s1".to_string(),
            detected_state: None,
            has_transcript: false,
        };

        let value = serde_json::to_value(&meta).expect("serialize meta");
        assert_eq!(
            value.get("loaded_capabilities"),
            Some(&serde_json::json!({
                "mcp_servers": [],
                "skills": [],
                "tool_groups": []
            }))
        );
    }
}
