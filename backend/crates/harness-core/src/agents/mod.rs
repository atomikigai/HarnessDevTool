//! Agents registry — persisted at `$HARNESS_HOME/profiles/default/agents/registry.toml`.
//!
//! "Agent" here is the *runtime persona* that can claim tasks (claude/codex/…),
//! not to be confused with the CLI session under `harness-session`.

mod registry;

pub use registry::{Agent, AgentDraft, AgentKind, AgentsRegistry};
