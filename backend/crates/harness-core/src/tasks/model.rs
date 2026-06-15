//! Domain types for the task engine (matches `task.v1` TOML schema).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// Canonical task lifecycle. See lessons-learned §D2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Proposed,
    Queued,
    InProgress,
    PendingVerify,
    Done,
    Paused,
    Blocked,
    Abandoned,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Proposed => "proposed",
            TaskStatus::Queued => "queued",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::PendingVerify => "pending_verify",
            TaskStatus::Done => "done",
            TaskStatus::Paused => "paused",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Abandoned => "abandoned",
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "proposed" => Ok(Self::Proposed),
            "queued" => Ok(Self::Queued),
            "in_progress" => Ok(Self::InProgress),
            "pending_verify" => Ok(Self::PendingVerify),
            "done" => Ok(Self::Done),
            "paused" => Ok(Self::Paused),
            "blocked" => Ok(Self::Blocked),
            "abandoned" => Ok(Self::Abandoned),
            other => Err(format!("unknown task status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct AcceptanceCheck {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Diff,
    TestOutput,
    Screenshot,
    Log,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::File => "file",
            ArtifactKind::Diff => "diff",
            ArtifactKind::TestOutput => "test_output",
            ArtifactKind::Screenshot => "screenshot",
            ArtifactKind::Log => "log",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Artifact {
    #[serde(default)]
    pub artifact_id: String,
    #[serde(default)]
    pub task_id: String,
    pub kind: ArtifactKind,
    pub path: String,
    #[serde(default)]
    pub produced_by: String,
    #[serde(default = "default_artifact_created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub summary: String,
}

fn default_artifact_created_at() -> DateTime<Utc> {
    Utc::now()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Artifacts {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub turns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default)]
    pub metadata: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Lease {
    pub holder: String,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Notes {
    #[serde(default)]
    pub why_paused: String,
    #[serde(default)]
    pub why_abandoned: String,
    #[serde(default)]
    pub blocked_reason: String,
    #[serde(default)]
    pub paused_reason: String,
    #[serde(default)]
    pub rejected_reason: String,
    #[serde(default)]
    pub last_failure: String,
    #[serde(default)]
    pub needs_human: bool,
    #[serde(default)]
    pub feedback: Vec<String>,
}

impl Notes {
    pub fn pause_reason(&self) -> &str {
        if self.paused_reason.trim().is_empty() {
            &self.why_paused
        } else {
            &self.paused_reason
        }
    }

    pub fn rejection_reason_present(&self) -> bool {
        !self.rejected_reason.trim().is_empty()
            || !self.last_failure.trim().is_empty()
            || !self.feedback.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct HistoryEvent {
    pub at: DateTime<Utc>,
    pub by: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TaskBrief {
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub expected_result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SpecRef {
    pub section: String,
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub version: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum SchedulerDecisionKind {
    Ready,
    AutoUnblocked,
    Assigned,
    AssignmentSkipped,
    ClaimBusy,
    CooldownAdded,
    CooldownSkipped,
    RoutedToEvaluator,
    EvaluatorSkipped,
    LeaseExpired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SchedulerExplanation {
    pub task_id: String,
    pub decision: SchedulerDecisionKind,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_holder: Option<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<usize>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum ReconcileSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReconcileEntity {
    pub kind: String,
    pub id: String,
}

impl ReconcileEntity {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReconcileIssue {
    pub kind: String,
    pub severity: ReconcileSeverity,
    pub entity: ReconcileEntity,
    pub message: String,
    #[serde(default)]
    pub related: Vec<ReconcileEntity>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReconcileSessionRef {
    pub session_id: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_session_id: Option<String>,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ReconcileReport {
    pub thread_id: String,
    pub generated_at: DateTime<Utc>,
    pub task_count: usize,
    pub session_count: usize,
    pub artifact_count: usize,
    #[serde(default)]
    pub issues: Vec<ReconcileIssue>,
}

/// Full task document — 1:1 with the on-disk TOML.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Task {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub status: TaskStatus,

    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,

    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub unblocks: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_lease: Option<Lease>,
    #[serde(default)]
    pub previous_assignees: Vec<String>,

    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub spec_refs: Vec<SpecRef>,
    /// Workspace-relative paths this task is allowed to write through harness
    /// repo tools. Empty means no repo writes are allowed for scoped workers.
    #[serde(default)]
    pub write_paths: Vec<String>,
    /// Workspace-relative paths this task must not write even if an allow path
    /// would otherwise match.
    #[serde(default)]
    pub forbidden_paths: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<TaskBrief>,
    #[serde(default)]
    pub acceptance: AcceptanceBlock,
    #[serde(default)]
    pub artifacts: Artifacts,
    #[serde(default)]
    pub notes: Notes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_explanation: Option<SchedulerExplanation>,
    #[serde(default)]
    pub history: HistoryBlock,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub acceptance_count: usize,
    pub artifact_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_handoff_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_handoff_at: Option<i64>,
    pub summary_preview: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct AcceptanceBlock {
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct HistoryBlock {
    #[serde(default)]
    pub events: Vec<HistoryEvent>,
}

/// Filters for [`TaskStore::list`].
#[derive(Debug, Clone, Default)]
pub struct ListFilters {
    pub status: Option<TaskStatus>,
    pub label: Option<String>,
    pub assignee: Option<String>,
}

/// Input to [`TaskStore::create`].
#[derive(Debug, Clone, Default)]
pub struct TaskDraft {
    pub title: String,
    pub parent: Option<String>,
    pub depends_on: Vec<String>,
    pub brief: Option<TaskBrief>,
    pub acceptance: Vec<AcceptanceCheck>,
    pub labels: Vec<String>,
    pub spec_refs: Vec<SpecRef>,
    pub write_paths: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub created_by: String,
}

/// Input to [`TaskStore::patch`] — all `Option<…>` so callers send sparse updates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub status: Option<TaskStatus>,
    pub assignee: Option<Option<String>>,
    pub labels: Option<Vec<String>>,
    pub spec_refs: Option<Vec<SpecRef>>,
    pub write_paths: Option<Vec<String>>,
    pub forbidden_paths: Option<Vec<String>>,
    pub blocked_by: Option<Vec<String>>,
    pub acceptance_checks: Option<Vec<AcceptanceCheck>>,
    pub artifacts: Option<Artifacts>,
    pub notes: Option<NotesPatch>,
    pub blocked_reason: Option<String>,
    pub paused_reason: Option<String>,
    pub rejected_reason: Option<String>,
    pub last_failure: Option<String>,
    pub needs_human: Option<bool>,
    pub why_paused: Option<String>,
    pub why_abandoned: Option<String>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotesPatch {
    pub why_paused: Option<String>,
    pub why_abandoned: Option<String>,
    pub blocked_reason: Option<String>,
    pub paused_reason: Option<String>,
    pub rejected_reason: Option<String>,
    pub last_failure: Option<String>,
    pub needs_human: Option<bool>,
    pub feedback: Option<Vec<String>>,
}

/// Outcome of a claim attempt.
#[derive(Debug, Clone)]
pub enum ClaimResult {
    Granted(Lease),
    Busy {
        holder: String,
        until: DateTime<Utc>,
    },
}
