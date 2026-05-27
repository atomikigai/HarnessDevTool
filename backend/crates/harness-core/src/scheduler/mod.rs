//! Scheduler — see lessons-learned §D5. F2 scope is reduced to: emit
//! `task.ready`, auto-unblock when deps complete, and expire stale leases. No
//! auto-claim (deferred to F3).

mod spawner;
mod tick;

pub use spawner::{NoopSpawner, SessionSpawner, SpawnRequest, SpawnResult};
pub use tick::{run_budget_pass, BudgetWiring, Scheduler};

/// Default cap on concurrent in-progress tasks per thread when none is
/// configured. Budget-aware override is a later slice.
pub const MAX_CONCURRENT_DEFAULT: usize = 3;
