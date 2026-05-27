//! Sub-agent spawn plumbing for the scheduler.
//!
//! The scheduler claims tasks for registered agents (planner/generator/
//! evaluator), but the actual PTY launch lives in `harness-session`. To keep
//! `harness-core` free of a hard dependency on `harness-session`, the scheduler
//! talks to a [`SessionSpawner`] trait that the binary (`harness-server`)
//! implements by delegating to its `Manager`.
//!
//! Semantics:
//!   * `spawn` is fire-and-forget from the scheduler's perspective — it's
//!     expected to be cheap (record-keeping + tokio::spawn) and never block.
//!   * Implementations are responsible for de-duping: if the agent already has
//!     a live session on this thread, return [`SpawnResult::AlreadyRunning`].
//!   * Failures are logged by the implementation and reported back as
//!     [`SpawnResult::Failed`] so the scheduler can decide whether to retry.

use std::path::PathBuf;

/// Request handed to a [`SessionSpawner`] when the scheduler decides an agent
/// needs a live PTY (e.g. it just claimed a queued task or was reassigned to
/// verify a submission).
#[derive(Debug, Clone)]
pub struct SpawnRequest {
    /// `agent:<kind>-<n>` from the registry. Used by the spawner to de-dupe
    /// against existing live sessions for this `(thread, agent)` pair.
    pub agent_id: String,
    /// Free-form role tag ("planner" / "generator" / "evaluator" / ...). The
    /// spawner resolves this against the roles registry to find the prompt
    /// template to inject into the PTY.
    pub role: String,
    /// Kind tag (`claude` / `codex` / ...) — matches `AgentKind::as_str()`
    /// without forcing `harness-core` to depend on `harness-session`.
    pub kind: String,
    /// Thread the agent is working on. The spawner uses this both to attach
    /// the session to the thread and to resolve a working directory (if the
    /// thread has one recorded).
    pub thread_id: String,
    /// Optional override cwd. When `None`, the spawner falls back to its
    /// default (typically `$HOME`).
    pub cwd: Option<PathBuf>,
}

/// Outcome of a spawn attempt. Cheap so the scheduler can log without
/// allocating in the success path.
#[derive(Debug, Clone)]
pub enum SpawnResult {
    /// A new session is being launched.
    Launched { session_id: String },
    /// The agent already has a live session attached to this thread; no
    /// action taken.
    AlreadyRunning { session_id: String },
    /// Spawn failed for a known reason (missing binary, missing role
    /// template, PTY error, ...). The message is intended for logs only.
    Failed(String),
}

/// Plug-in interface the scheduler uses to materialize claimed agents as live
/// PTY sessions. Implementations must be cheap and non-blocking — see module
/// docs.
pub trait SessionSpawner: Send + Sync {
    fn spawn(&self, req: SpawnRequest) -> SpawnResult;
}

/// Default no-op spawner used when the scheduler is constructed without a
/// real `harness-session::Manager` (e.g. from unit tests that only care about
/// task-state transitions).
#[derive(Default)]
pub struct NoopSpawner;

impl SessionSpawner for NoopSpawner {
    fn spawn(&self, _req: SpawnRequest) -> SpawnResult {
        SpawnResult::Failed("noop spawner".into())
    }
}
