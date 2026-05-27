//! `harness-core` — domain types, storage, task engine, agents registry and scheduler.
//!
//! See `docs/01-foundations/lessons-learned.md` §D1-D6 for the task design contract.

pub mod agents;
pub mod budget;
pub mod events;
pub mod pause;
pub mod roles;
pub mod scheduler;
pub mod store;
pub mod tasks;
pub mod threads;

pub use agents::{Agent, AgentDraft, AgentKind, AgentsRegistry};
pub use budget::{
    ActiveSession, ActiveSessionsSource, Budget, BudgetStore, BudgetWarning, BudgetWarningSink,
    ClaudeTranscriptReporter, CodexStubReporter, CostReporter, SessionCost, Usage,
};
pub use events::Event;
pub use pause::PauseFlag;
pub use roles::{Role, RolesRegistry};
pub use scheduler::{run_budget_pass, BudgetWiring, Scheduler, MAX_CONCURRENT_DEFAULT};
pub use store::{Store, StoreError};
pub use tasks::{
    AcceptanceCheck, Artifacts, ClaimResult, HistoryEvent, Lease, ListFilters, Task, TaskDraft,
    TaskEvent, TaskPatch, TaskStatus, TaskStore,
};
pub use threads::Thread;

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
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
