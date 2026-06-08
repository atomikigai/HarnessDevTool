//! `harness-core` — domain types, storage, task engine, agents registry and scheduler.
//!
//! See `docs/01-foundations/lessons-learned.md` §D1-D6 for the task design contract.

pub mod agents;
pub mod budget;
pub mod events;
pub mod ids;
pub mod knowledge;
pub mod pause;
pub mod repos;
pub mod roles;
pub mod scheduler;
pub mod store;
pub mod tasks;
pub mod threads;

pub use agents::{Agent, AgentDraft, AgentKind, AgentsRegistry};
pub use budget::{
    ActiveSession, ActiveSessionsSource, AgentCost, Budget, BudgetBreakdown, BudgetLedgerView,
    BudgetObservation, BudgetStore, BudgetWarning, BudgetWarningSink, ClaudeTranscriptReporter,
    CodexStubReporter, CostReporter, RoleCost, SessionCost, SessionCostView, StubReporter,
    TaskCost, Usage,
};
pub use events::{Event, Item, TimelineEntity, TimelineItem, TimelineReport};
pub use ids::{validate_path_id, validate_profile_id, validate_task_id, validate_thread_id};
pub use knowledge::{
    check_pdftotext, ingest_pdf, ingest_text, KnowledgeIngestRequest, KnowledgeIngestResult,
    KnowledgeShard, PdfTextToolStatus,
};
pub use pause::PauseFlag;
pub use repos::{
    CurrentRepoReport, RepoContext, RepoError, RepoIdentity, RepoIndex, RepoRecord,
    RepoThreadRecord,
};
pub use roles::{Role, RolesRegistry};
pub use scheduler::{
    run_budget_pass, BudgetWiring, NoopSpawner, Scheduler, SessionSpawner, SpawnRequest,
    SpawnResult, MAX_CONCURRENT_DEFAULT,
};
pub use store::{Store, StoreError};
pub use tasks::{
    AcceptanceCheck, Artifact, ArtifactKind, Artifacts, ClaimResult, HistoryEvent, Lease,
    ListFilters, ReconcileEntity, ReconcileIssue, ReconcileReport, ReconcileSessionRef,
    ReconcileSeverity, SchedulerDecisionKind, SchedulerExplanation, SpecRef, Task, TaskBrief,
    TaskDraft, TaskEvent, TaskPatch, TaskStatus, TaskStore,
};
pub use threads::{
    AutonomyProfile, ExecutionMode, Handoff, ReadinessIssue, ReadinessReport, ReadinessStatus,
    Thread,
};

/// Crate-level result type for task/agent operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Crate-level error type, surfaced to `harness-server` and mapped to HTTP.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("task not found: {0}")]
    NotFound(String),
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: tasks::TaskStatus,
        to: tasks::TaskStatus,
    },
    #[error("task is busy (held by {holder} until {until})")]
    Busy {
        holder: String,
        until: chrono::DateTime<chrono::Utc>,
    },
    #[error("lease not held by {0}")]
    LeaseNotHeld(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
