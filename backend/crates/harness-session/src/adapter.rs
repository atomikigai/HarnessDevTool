use crate::kind::AgentKind;
use crate::manager::SpawnOpts;

pub(crate) const DEFAULT_CLAUDE_MODEL: &str = "sonnet";
pub(crate) const DEFAULT_CLAUDE_EFFORT: &str = "medium";
pub(crate) const DEFAULT_CODEX_MODEL: &str = "gpt-5.5";
pub(crate) const DEFAULT_CODEX_EFFORT: &str = "medium";

/// Translate `SpawnOpts` into the CLI flags appended to the agent invocation.
///
/// - `Claude`: pins `--session-id <id>` so the harness UUID matches the on-disk
///   transcript filename (`~/.claude/projects/{cwd-slug}/{id}.jsonl`); the
///   budget reporter relies on this mapping. It also always skips Claude's
///   permission prompts with `--dangerously-skip-permissions`; the harness owns
///   supervision and audit for these child processes. When MCP injection is on, it adds
///   `--mcp-config <path> --strict-mcp-config` when MCP injection is on, plus
///   `--disallowed-tools TodoWrite` so claude can't satisfy task-
///   shaped requests with its in-process todo list (which never reaches the
///   harness TaskStore and so leaves the right-side Tasks panel empty).
/// - `Codex`: injects the harness MCP with per-invocation `-c
///   mcp_servers.harness.*` overrides. Codex does not have a `--mcp-config`
///   file flag, so we avoid mutating `~/.codex/config.toml`.
pub(crate) fn build_extra_args(kind: AgentKind, opts: &SpawnOpts, session_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    if matches!(kind, AgentKind::Claude) {
        out.push("--session-id".to_string());
        out.push(session_id.to_string());
        out.push("--model".to_string());
        out.push(
            opts.model
                .as_deref()
                .unwrap_or(DEFAULT_CLAUDE_MODEL)
                .to_string(),
        );
        out.push("--effort".to_string());
        out.push(
            opts.effort
                .as_deref()
                .unwrap_or(DEFAULT_CLAUDE_EFFORT)
                .to_string(),
        );
        out.push("--dangerously-skip-permissions".to_string());
    }

    match kind {
        AgentKind::Codex => {
            out.push("--dangerously-bypass-approvals-and-sandbox".to_string());
            out.push("--model".to_string());
            out.push(
                opts.model
                    .as_deref()
                    .unwrap_or(DEFAULT_CODEX_MODEL)
                    .to_string(),
            );
            out.push("-c".to_string());
            out.push(format!(
                "model_reasoning_effort={}",
                toml_string(opts.effort.as_deref().unwrap_or(DEFAULT_CODEX_EFFORT))
            ));
            if let Some(command) = opts.mcp_server_command.as_ref() {
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.harness.command={}",
                    toml_string(command)
                ));
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.harness.args={}",
                    toml_string_array(&opts.mcp_server_args)
                ));
            }
            for server in &opts.extra_mcp_servers {
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.{}.command={}",
                    server.name,
                    toml_string(&server.command)
                ));
                out.push("-c".to_string());
                out.push(format!(
                    "mcp_servers.{}.args={}",
                    server.name,
                    toml_string_array(&server.args)
                ));
            }
            if let Some(intro) = opts.auto_intro.as_ref() {
                out.push("-c".to_string());
                out.push(format!("developer_instructions={}", toml_string(intro)));
            }
            if let Some(prompt) = opts.role_prompt.as_ref() {
                out.push(prompt.clone());
            }
        }
        AgentKind::Cursor | AgentKind::Antigravity => {}
        AgentKind::Claude | AgentKind::Zeus => {}
    }

    if let Some(path) = opts.mcp_config_path.as_ref() {
        match kind {
            AgentKind::Claude => {
                out.push("--mcp-config".to_string());
                out.push(path.display().to_string());
                out.push("--strict-mcp-config".to_string());
                out.push("--disallowed-tools".to_string());
                out.push("TodoWrite".to_string());
                if let Some(intro) = opts.auto_intro.as_ref() {
                    out.push("--append-system-prompt".to_string());
                    out.push(intro.clone());
                }
            }
            AgentKind::Codex => {}
            AgentKind::Cursor | AgentKind::Antigravity => {
                tracing::warn!(
                    kind = %kind,
                    path = %path.display(),
                    "MCP injection not implemented for this CLI; skipping --mcp-config"
                );
            }
            AgentKind::Zeus => {
                tracing::warn!("build_extra_args called with Zeus kind directly; this is a bug");
            }
        }
    }
    out
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn toml_string_array(values: &[String]) -> String {
    let parts = values
        .iter()
        .map(|v| toml_string(v))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{parts}]")
}
