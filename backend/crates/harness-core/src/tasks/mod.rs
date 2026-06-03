//! Task engine — see `docs/01-foundations/lessons-learned.md` §D1-D6.

mod events;
mod ids;
mod index;
mod model;
mod state_machine;
mod store;

pub use events::TaskEvent;
pub use model::{
    AcceptanceCheck, Artifacts, ClaimResult, HistoryEvent, Lease, ListFilters, Task, TaskBrief,
    TaskDraft, TaskPatch, TaskProposal, TaskProposalDraft, TaskProposalEvent, TaskProposalStatus,
    TaskStatus,
};
pub use store::TaskStore;
