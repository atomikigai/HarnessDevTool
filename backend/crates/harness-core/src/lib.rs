//! `harness-core` — task engine, agents registry and scheduler.
//!
//! See `docs/01-foundations/lessons-learned.md` §D1-D6 for the design contract.
//! Public surface is intentionally narrow: callers build a [`TaskStore`] and an
//! [`AgentsRegistry`] over a `$HARNESS_HOME`, subscribe to per-thread broadcast
//! events, and optionally spin a [`Scheduler`] to keep the loop alive.

pub mod agents;
pub mod scheduler;
pub mod tasks;

pub use agents::{Agent, AgentDraft, AgentKind, AgentsRegistry};
pub use scheduler::Scheduler;
pub use tasks::{
    AcceptanceCheck, Artifacts, ClaimResult, HistoryEvent, Lease, ListFilters, Task, TaskDraft,
    TaskEvent, TaskPatch, TaskStatus, TaskStore,
};

/// Crate-level result type.
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
