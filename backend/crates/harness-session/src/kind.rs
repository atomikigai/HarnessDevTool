use serde::{Deserialize, Serialize};

/// CLI flavor the harness knows how to spawn. Closed set — extending requires
/// a new variant here plus updates to `build_extra_args` and the frontend
/// selector. See [[agents/supported-clis]].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum AgentKind {
    Claude,
    Codex,
    Cursor,
    /// Antigravity (`agy`). Covers the cloud / Workspace / external-context
    /// worker role inside Zeus orchestration; usable on its own too.
    Antigravity,
    /// Zeus is **not a CLI** — it's a virtual orchestrator session. Spawning
    /// a Zeus session asks the harness to plan + delegate sub-tasks to
    /// role-typed workers backed by the other CLIs (see [[agents/zeus-orchestrator]]).
    /// The role-to-CLI mapping is the canonical Zeus matrix; every role
    /// falls back to Claude on quota / failure. Today it runs under a
    /// single Codex PTY with a Zeus orchestrator system prompt; the real
    /// multi-CLI delegation lands with F3.
    Zeus,
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Cursor => "cursor",
            AgentKind::Antigravity => "antigravity",
            AgentKind::Zeus => "zeus",
        }
    }

    /// Default binary name searched on `$PATH` when no explicit override is
    /// provided. Some CLIs ship under a name that differs from the kind label.
    /// `Zeus` is virtual — it has no binary; callers must special-case it.
    pub fn default_binary(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Cursor => "cursor-agent",
            AgentKind::Antigravity => "agy",
            AgentKind::Zeus => "", // virtual; no binary
        }
    }

    /// Whether this kind backs a real CLI process. `false` for `Zeus`, which
    /// is an orchestration session synthesised by the harness from the other
    /// CLIs.
    pub fn is_real_cli(self) -> bool {
        !matches!(self, AgentKind::Zeus)
    }

    /// Which CLI binary actually backs this `AgentKind`. `Zeus` runs under
    /// Codex today (matrix orchestrator role) until F3 wires the real
    /// multi-CLI delegation. All other kinds back themselves.
    pub fn underlying_cli(self) -> AgentKind {
        match self {
            AgentKind::Zeus => AgentKind::Codex,
            other => other,
        }
    }

    /// Install hint shown when the binary is not found.
    pub fn install_hint(self) -> &'static str {
        match self {
            AgentKind::Claude => "Install: https://docs.claude.com/en/docs/claude-code/setup",
            AgentKind::Codex => "Install: npm i -g @openai/codex",
            AgentKind::Cursor => "Install Cursor and ensure `cursor-agent` is on $PATH",
            AgentKind::Antigravity => "Install Antigravity and ensure `agy` is on $PATH",
            AgentKind::Zeus => "Zeus is a virtual orchestrator — it has no binary to install",
        }
    }
}
