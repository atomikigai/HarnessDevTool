//! Scheduler — see lessons-learned §D5. F2 scope is reduced to: emit
//! `task.ready`, auto-unblock when deps complete, and expire stale leases. No
//! auto-claim (deferred to F3).

mod tick;

pub use tick::Scheduler;

/// Default cap on concurrent in-progress tasks per thread when none is
/// configured. Budget-aware override is a later slice.
pub const MAX_CONCURRENT_DEFAULT: usize = 3;
