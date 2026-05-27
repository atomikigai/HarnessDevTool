use serde::{Deserialize, Serialize};

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
}
