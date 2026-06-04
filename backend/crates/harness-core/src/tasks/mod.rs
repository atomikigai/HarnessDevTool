//! Task engine — see `docs/01-foundations/lessons-learned.md` §D1-D6.

mod events;
mod ids;
mod index;
mod model;
mod reconcile;
mod state_machine;
mod store;

pub use events::TaskEvent;
pub use model::{
    AcceptanceCheck, Artifact, ArtifactKind, Artifacts, ClaimResult, HistoryEvent, Lease,
    ListFilters, ReconcileEntity, ReconcileIssue, ReconcileReport, ReconcileSessionRef,
    ReconcileSeverity, SchedulerDecisionKind, SchedulerExplanation, SpecRef, Task, TaskBrief,
    TaskDraft, TaskPatch, TaskStatus,
};
pub use reconcile::reconcile_tasks;
pub use store::TaskStore;
