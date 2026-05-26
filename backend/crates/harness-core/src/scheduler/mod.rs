//! Scheduler — see lessons-learned §D5. F2 scope is reduced to: emit
//! `task.ready`, auto-unblock when deps complete, and expire stale leases. No
//! auto-claim (deferred to F3).

mod tick;

pub use tick::Scheduler;
