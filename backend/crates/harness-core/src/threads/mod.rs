use serde::{Deserialize, Serialize};

use crate::repos::RepoContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum ExecutionMode {
    Quick,
    Standard,
    Project,
    Exploratory,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum AutonomyProfile {
    Manual,
    Assisted,
    Autonomous,
    Ci,
}

impl AutonomyProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Assisted => "assisted",
            Self::Autonomous => "autonomous",
            Self::Ci => "ci",
        }
    }
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Project => "project",
            Self::Exploratory => "exploratory",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum ReadinessStatus {
    Ready,
    ReadyWithWarnings,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReadinessIssue {
    pub id: String,
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to_fix: Option<String>,
}

impl ReadinessIssue {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        message: impl Into<String>,
        how_to_fix: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            message: message.into(),
            how_to_fix,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReadinessReport {
    pub status: ReadinessStatus,
    pub checked_at: i64,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub blocking: Vec<ReadinessIssue>,
    #[serde(default)]
    pub warnings: Vec<ReadinessIssue>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub facts: serde_json::Value,
    pub suggested_execution_mode: ExecutionMode,
}

impl ReadinessReport {
    pub fn new(
        checked_at: i64,
        cwd: impl Into<String>,
        blocking: Vec<ReadinessIssue>,
        warnings: Vec<ReadinessIssue>,
        facts: serde_json::Value,
        suggested_execution_mode: ExecutionMode,
    ) -> Self {
        let status = if !blocking.is_empty() {
            ReadinessStatus::Blocked
        } else if !warnings.is_empty() {
            ReadinessStatus::ReadyWithWarnings
        } else {
            ReadinessStatus::Ready
        };
        Self {
            status,
            checked_at,
            cwd: cwd.into(),
            blocking,
            warnings,
            facts,
            suggested_execution_mode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Handoff {
    pub at: i64,
    pub from: String,
    pub to_role: String,
    pub task_id: String,
    pub status: String,
    pub goal: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(default)]
    pub commands_run: Vec<String>,
    #[serde(default)]
    pub verification_passed: Vec<String>,
    #[serde(default)]
    pub verification_not_run: Vec<String>,
    #[serde(default)]
    pub blocked_on: Vec<String>,
    pub next_agent_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Thread {
    /// UUID v4 of the thread.
    pub id: String,
    /// Optional human-readable title.
    pub title: Option<String>,
    /// Unix timestamp (ms) of creation.
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy_profile: Option<AutonomyProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoContext>,
}

impl Thread {
    pub fn new(id: String, title: Option<String>, created_at: i64) -> Self {
        Self {
            id,
            title,
            created_at,
            execution_mode: None,
            autonomy_profile: None,
            repo: None,
        }
    }
}
