//! Lightweight planning rails for smart-loading agents.
//!
//! These tools return structured, deterministic guidance that helps an agent
//! decide which heavier capabilities to load instead of front-loading every
//! tool/schema into the prompt.

use std::collections::BTreeSet;

use serde_json::{json, Value};

pub fn pack(args: &Value) -> Result<Value, String> {
    let objective = required_str(args, "objective")?;
    let files = collect_files(args);
    let text = format!("{} {}", objective, files.join(" ")).to_ascii_lowercase();
    let signals = Signals::from(&text, &files);

    let mut groups = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    let mut skills = BTreeSet::new();
    let mut checks = Vec::new();
    let mut first_actions = Vec::new();
    let mut guardrails = Vec::new();

    groups.insert("repo");
    first_actions.push(action(
        "repo_analyze",
        "Get stack, scripts, git state, and key files before reading broadly.",
    ));

    if files.is_empty() {
        first_actions.push(action(
            "repo_find",
            "Locate likely implementation and test files from objective keywords.",
        ));
    } else {
        first_actions.push(action(
            "repo_read_file",
            "Read only the listed files and nearby tests before editing.",
        ));
    }

    if signals.frontend {
        skills.insert("agent-browser");
        checks.push(check(
            "pnpm --dir frontend check",
            "Static frontend validation for Svelte/TypeScript.",
        ));
        checks.push(check(
            "agent-browser",
            "Required real-user UI validation for frontend-visible changes.",
        ));
        guardrails.push("Frontend/UI changes require agent-browser validation as a real user.");
    }

    if signals.design {
        skills.insert("frontend-design");
        skills.insert("design-md");
        guardrails.push("Update frontend/DESIGN.md when tokens, global styles, layout patterns, or visual direction change.");
    }

    if signals.shadcn {
        skills.insert("shadcn-svelte");
    }

    if signals.backend {
        skills.insert("rust-tooling");
        checks.push(check(
            "cargo test -p harness-mcp-server",
            "Focused Rust tests for MCP/backend harness changes.",
        ));
    }

    if signals.contract {
        checks.push(check(
            "just gen-types",
            "Regenerate TypeScript types from Rust when shared API contracts change.",
        ));
        guardrails.push(
            "Keep X-Protocol-Version semantics explicit for frontend/backend HTTP contracts.",
        );
        guardrails.push("Do not edit generated files in frontend/src/lib/api/types/ by hand.");
    }

    if signals.db {
        groups.insert("db");
        capabilities.insert("db");
        first_actions.push(action(
            "db_schema",
            "Inspect relevant schema before writing SQL or data mutations.",
        ));
    }

    if signals.ssh {
        groups.insert("ssh");
        capabilities.insert("ssh");
        first_actions.push(action(
            "ssh_hosts",
            "List configured hosts before choosing a remote target.",
        ));
    }

    if signals.n8n {
        groups.insert("n8n");
        capabilities.insert("n8n");
        skills.insert("n8n-workflow-automation");
        first_actions.push(action(
            "n8n_validate_workflow",
            "Validate workflow shape and secret handling before import or activation.",
        ));
    }

    if signals.docs {
        groups.insert("docs");
        first_actions.push(action(
            "repo_read_file",
            "Read docs index and adjacent docs before editing documentation.",
        ));
    }

    if signals.context {
        groups.insert("context");
        capabilities.insert("context");
        first_actions.push(action(
            "session_context_pack",
            "Read compact session/task/handoff state before replaying transcript or asking broad status questions.",
        ));
    }

    if signals.external_docs {
        capabilities.insert("docs_web");
        skills.insert("crawl4ai-context");
        first_actions.push(action(
            "capability_describe",
            "Check docs_web availability before crawling external documentation.",
        ));
    }

    if signals.performance {
        skills.insert("performance-optimization");
        guardrails.push("Measure a baseline and a post-change number for performance work.");
    }

    if signals.security {
        skills.insert("security-tooling");
        guardrails
            .push("Do not expose raw secrets; reference env vars or configured credential stores.");
    }

    if signals.diagram {
        skills.insert("excalidraw-board");
        skills.insert("excalidraw-diagram");
    }

    if signals.code_graph {
        groups.insert("code_graph");
        capabilities.insert("code_graph");
        first_actions.push(action(
            "repo_code_graph_status",
            "Check whether deep code graph acceleration is available before broad architecture or impact analysis.",
        ));
        first_actions.push(action(
            "repo_symbol_search",
            "Use the lightweight local symbol index before opening many source files.",
        ));
    }

    if signals.refactor {
        skills.insert("code-simplification");
        guardrails.push("Keep behavior stable and scope the diff to the requested refactor.");
    }

    if signals.kiss {
        skills.insert("kiss");
        skills.insert("code-simplification");
        guardrails.push("Apply KISS: prefer deletion, stdlib/native behavior, installed dependencies, and the smallest local change before adding new code.");
    }

    if signals.review {
        first_actions.push(action(
            "evidence_pack",
            "Collect scoped diff, task/session metadata, artifacts, and evidence gaps before review or QA.",
        ));
    }

    skills.insert("code-review-and-quality");

    Ok(json!({
        "objective": objective,
        "files": files,
        "domains": signals.domains(),
        "recommended_tool_groups": groups.into_iter().collect::<Vec<_>>(),
        "recommended_capabilities": capabilities.into_iter().collect::<Vec<_>>(),
        "recommended_skills": skills.into_iter().collect::<Vec<_>>(),
        "first_actions": first_actions,
        "checks": dedupe_objects(checks),
        "guardrails": guardrails,
        "smart_loading": {
            "policy": "Load only the recommended non-core groups needed for the next step; unload groups when the task moves away from that domain.",
            "minimal_start": ["tools_search", "tools_load", "tools_unload", "planning_pack", "repo_analyze"],
        }
    }))
}

pub fn test_selector(args: &Value) -> Result<Value, String> {
    let files = collect_files(args);
    if files.is_empty() {
        return Err("test_selector requires files, changed_files, or paths".into());
    }
    let objective = args.get("objective").and_then(Value::as_str).unwrap_or("");
    let text = format!("{} {}", objective, files.join(" ")).to_ascii_lowercase();
    let signals = Signals::from(&text, &files);
    let mut commands = Vec::new();

    if signals.backend {
        commands.push(check(
            "cargo test -p harness-mcp-server",
            "Rust MCP/backend code changed.",
        ));
    }
    if signals.frontend {
        commands.push(check(
            "pnpm --dir frontend check",
            "Frontend TypeScript/Svelte code changed.",
        ));
        commands.push(check(
            "agent-browser",
            "Frontend-visible behavior requires real browser validation.",
        ));
    }
    if signals.contract {
        commands.push(check(
            "just gen-types",
            "Shared Rust-to-TypeScript contract may have changed.",
        ));
    }
    if signals.n8n {
        commands.push(check(
            "n8n_validate_workflow",
            "Workflow JSON should be validated before import or activation.",
        ));
    }
    if signals.docs && !signals.frontend && !signals.backend {
        commands.push(check(
            "repo_git_diff",
            "Review documentation-only diff for accidental code changes.",
        ));
    }
    if commands.is_empty() {
        commands.push(check(
            "repo_git_diff",
            "Review scoped diff and select project-specific tests from repo_analyze scripts.",
        ));
    }

    Ok(json!({
        "files": files,
        "commands": dedupe_objects(commands),
        "notes": [
            "Prefer the narrowest command that exercises the touched contract.",
            "Escalate to just test when changes cross multiple packages or shared behavior."
        ]
    }))
}

pub fn contract_guard(args: &Value) -> Result<Value, String> {
    let files = collect_files(args);
    if files.is_empty() {
        return Err("contract_guard requires files, changed_files, or paths".into());
    }
    let text = files.join(" ").to_ascii_lowercase();
    let signals = Signals::from(&text, &files);
    let mut required = Vec::new();
    let mut warnings = Vec::new();

    if signals.contract {
        required.push(check(
            "just gen-types",
            "Shared Rust API/type changes must export TypeScript from ts-rs.",
        ));
        warnings.push("Verify X-Protocol-Version handling for any HTTP contract change.");
    }
    if signals.frontend {
        required.push(check(
            "pnpm --dir frontend check",
            "Frontend contract consumer changed.",
        ));
        required.push(check(
            "agent-browser",
            "UI flow must be validated as a real user.",
        ));
    }
    if signals.design {
        required.push(check(
            "DESIGN.md review",
            "Update the design source of truth if visual direction changed.",
        ));
    }
    if files
        .iter()
        .any(|file| file.starts_with("frontend/src/lib/api/types/"))
    {
        warnings.push("Generated frontend API types should not be edited manually.");
    }
    if text.contains(".env") {
        warnings.push("The repo intentionally versions .env; do not delete, rename, or replace it unless explicitly asked.");
    }

    Ok(json!({
        "files": files,
        "requires": dedupe_objects(required),
        "warnings": warnings,
        "contract_risk": if signals.contract { "elevated" } else { "low" },
    }))
}

#[derive(Default)]
struct Signals {
    frontend: bool,
    backend: bool,
    contract: bool,
    db: bool,
    ssh: bool,
    n8n: bool,
    docs: bool,
    context: bool,
    external_docs: bool,
    design: bool,
    shadcn: bool,
    performance: bool,
    security: bool,
    diagram: bool,
    code_graph: bool,
    refactor: bool,
    review: bool,
    kiss: bool,
}

impl Signals {
    fn from(text: &str, files: &[String]) -> Self {
        let file_match = |pred: fn(&str) -> bool| files.iter().any(|file| pred(file));
        let frontend = file_match(is_frontend_file)
            || contains_any(
                text,
                &["frontend", "svelte", "vite", "ui", "browser", "css"],
            );
        let backend = file_match(is_backend_file)
            || contains_any(text, &["backend", "rust", "cargo", "mcp", "server", "api"]);
        let contract = (frontend && backend)
            || contains_any(
                text,
                &[
                    "contract",
                    "protocol",
                    "x-protocol-version",
                    "ts-rs",
                    "generated types",
                    "api/types",
                ],
            );
        Self {
            frontend,
            backend,
            contract,
            db: file_match(is_db_file)
                || contains_any(text, &["database", "sql", "schema", "table", "query"]),
            ssh: contains_any(text, &["ssh", "sftp", "remote host", "server command"]),
            n8n: contains_any(text, &["n8n", "workflow", "webhook automation"]),
            docs: file_match(is_docs_file) || contains_any(text, &["docs", "documentation"]),
            context: contains_any(
                text,
                &[
                    "handoff",
                    "resume",
                    "continuity",
                    "context pack",
                    "next action",
                    "subagent",
                    "child summary",
                    "ledger",
                    "retomar",
                ],
            ),
            external_docs: text.contains("http://")
                || text.contains("https://")
                || contains_any(text, &["latest docs", "official docs", "external docs"]),
            design: files.iter().any(|file| file.ends_with("DESIGN.md"))
                || contains_any(text, &["design", "layout", "visual", "tailwind", "theme"]),
            shadcn: contains_any(text, &["shadcn", "bits ui", "component library"]),
            performance: contains_any(text, &["performance", "slow", "latency", "benchmark"]),
            security: contains_any(text, &["security", "secret", "token", "auth", "permission"]),
            diagram: contains_any(text, &["diagram", "excalidraw", "wireframe", "board"]),
            code_graph: contains_any(
                text,
                &[
                    "architecture",
                    "arquitectura",
                    "symbol",
                    "simbolo",
                    "símbolo",
                    "callers",
                    "callees",
                    "call graph",
                    "code graph",
                    "impact",
                    "blast radius",
                    "routes",
                    "rutas",
                    "large refactor",
                    "refactor amplio",
                ],
            ),
            refactor: contains_any(text, &["refactor", "simplify", "cleanup", "clarity"]),
            review: contains_any(
                text,
                &[
                    "review",
                    "qa",
                    "evidence",
                    "handoff",
                    "verify",
                    "verification",
                    "before merge",
                    "merge",
                    "pr",
                ],
            ),
            kiss: contains_any(
                text,
                &[
                    "kiss",
                    "yagni",
                    "keep it simple",
                    "simplest solution",
                    "minimal solution",
                    "over engineered",
                    "over-engineered",
                    "boilerplate",
                    "less code",
                    "delete code",
                    "standard library",
                    "native platform",
                ],
            ),
        }
    }

    fn domains(&self) -> Vec<&'static str> {
        let mut domains = Vec::new();
        if self.frontend {
            domains.push("frontend");
        }
        if self.backend {
            domains.push("backend");
        }
        if self.contract {
            domains.push("frontend_backend_contract");
        }
        if self.db {
            domains.push("db");
        }
        if self.ssh {
            domains.push("ssh");
        }
        if self.n8n {
            domains.push("n8n");
        }
        if self.docs {
            domains.push("docs");
        }
        if self.context {
            domains.push("context");
        }
        if self.external_docs {
            domains.push("external_docs");
        }
        if self.design {
            domains.push("design");
        }
        if self.performance {
            domains.push("performance");
        }
        if self.security {
            domains.push("security");
        }
        if self.diagram {
            domains.push("diagram");
        }
        if self.code_graph {
            domains.push("code_graph");
        }
        if self.refactor {
            domains.push("refactor");
        }
        if self.kiss {
            domains.push("kiss");
        }
        domains
    }
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn collect_files(args: &Value) -> Vec<String> {
    ["files", "changed_files", "paths"]
        .iter()
        .filter_map(|key| args.get(*key).and_then(Value::as_array))
        .flat_map(|items| items.iter().filter_map(Value::as_str))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn action(tool: &str, reason: &str) -> Value {
    json!({ "tool": tool, "reason": reason })
}

fn check(command: &str, reason: &str) -> Value {
    json!({ "command": command, "reason": reason })
}

fn dedupe_objects(items: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.to_string()))
        .collect()
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn is_frontend_file(file: &str) -> bool {
    file.starts_with("frontend/")
        || file.ends_with(".svelte")
        || file.ends_with(".ts")
        || file.ends_with(".tsx")
        || file.ends_with(".css")
}

fn is_backend_file(file: &str) -> bool {
    file.starts_with("backend/") || file.ends_with(".rs") || file.ends_with("Cargo.toml")
}

fn is_docs_file(file: &str) -> bool {
    file.starts_with("docs/") || file.ends_with(".md") || file.ends_with(".mdx")
}

fn is_db_file(file: &str) -> bool {
    file.ends_with(".sql") || file.contains("/migrations/") || file.contains("/schema/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planning_pack_keeps_recommendations_scoped() {
        let result = pack(&json!({
            "objective": "Fix frontend/backend API contract bug",
            "files": [
                "backend/crates/harness-server/src/routes/sessions.rs",
                "frontend/src/lib/api/client.ts"
            ]
        }))
        .unwrap();

        assert!(result["recommended_tool_groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "repo"));
        assert!(result["recommended_skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "agent-browser"));
        assert!(result["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["command"] == "just gen-types"));
    }

    #[test]
    fn test_selector_recommends_browser_for_frontend_files() {
        let result = test_selector(&json!({
            "files": ["frontend/src/lib/components/app/ChatView.svelte"]
        }))
        .unwrap();
        assert!(result["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["command"] == "agent-browser"));
    }

    #[test]
    fn planning_pack_recommends_code_graph_for_architecture() {
        let result = pack(&json!({
            "objective": "Analyze architecture impact and callers before a large refactor"
        }))
        .unwrap();
        assert!(result["recommended_tool_groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "code_graph"));
        assert!(result["first_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["tool"] == "repo_code_graph_status"));
    }

    #[test]
    fn planning_pack_recommends_kiss_for_simplicity_work() {
        let result = pack(&json!({
            "objective": "Apply KISS: remove boilerplate and avoid over-engineered abstractions"
        }))
        .unwrap();

        assert!(result["recommended_skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "kiss"));
        assert!(result["domains"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "kiss"));
        assert!(result["guardrails"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str().unwrap_or("").contains("Apply KISS")));
    }
}
