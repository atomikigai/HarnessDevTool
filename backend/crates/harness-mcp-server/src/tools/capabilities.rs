use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityCategory {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub use_when: &'static [&'static str],
    pub mentions: &'static [&'static str],
    pub tools: &'static [&'static str],
    pub skills: &'static [&'static str],
    pub status: CapabilityStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Always,
    Loaded,
    AvailableOnRequest,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilityRuntime {
    pub docs_web_loaded: bool,
    pub requested: Vec<String>,
}

pub fn list(runtime: CapabilityRuntime) -> Value {
    let categories = categories(runtime)
        .into_iter()
        .map(|category| {
            json!({
                "id": category.id,
                "title": category.title,
                "description": category.description,
                "use_when": category.use_when,
                "mentions": category.mentions,
                "status": category.status,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "guidance": "Pick a category by use_when/mentions, then call capability_describe with its id before scanning many tools.",
        "categories": categories,
    })
}

pub fn describe(runtime: CapabilityRuntime, args: &Value) -> Result<Value, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id is required".to_string())?;
    let category = categories(runtime)
        .into_iter()
        .find(|category| category.id == id)
        .ok_or_else(|| format!("unknown capability category: {id}"))?;
    Ok(json!({
        "id": category.id,
        "title": category.title,
        "description": category.description,
        "use_when": category.use_when,
        "mentions": category.mentions,
        "status": category.status,
        "tools": category.tools,
        "skills": category.skills,
        "next_step": next_step(category.status, category.id),
    }))
}

pub fn request(runtime: CapabilityRuntime, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id is required".to_string())?;
    let category = categories(runtime)
        .into_iter()
        .find(|category| category.id == id)
        .ok_or_else(|| format!("unknown capability category: {id}"))?;
    match category.status {
        CapabilityStatus::Always | CapabilityStatus::Loaded => Ok(id.to_string()),
        CapabilityStatus::AvailableOnRequest if id != "docs_web" => Ok(id.to_string()),
        CapabilityStatus::AvailableOnRequest => Err(
            "docs_web is not hot-loaded in this session yet; start a docs/web-capable spawn or include an official docs URL in the task so the smart loader grants it."
                .to_string(),
        ),
    }
}

fn next_step(status: CapabilityStatus, id: &str) -> String {
    match status {
        CapabilityStatus::Always | CapabilityStatus::Loaded => {
            format!("Use the listed tools directly for `{id}` work.")
        }
        CapabilityStatus::AvailableOnRequest => {
            format!("Ask the harness to start a session with `{id}` or use the matching built-in rails if listed.")
        }
    }
}

fn categories(runtime: CapabilityRuntime) -> Vec<CapabilityCategory> {
    let requested = |id: &str| runtime.requested.iter().any(|item| item == id);
    vec![
        CapabilityCategory {
            id: "tasks",
            title: "Tasks and handoff",
            description: "Create, inspect, claim, update, and submit Harness tasks.",
            use_when: &["task tracking", "handoff", "planner work", "claim work", "submit for verification"],
            mentions: &["task", "todo", "handoff", "acceptance", "pending_verify"],
            tools: &[
                "task_create",
                "task_propose",
                "task_list",
                "task_get",
                "task_claim",
                "task_update",
                "task_submit",
            ],
            skills: &[],
            status: CapabilityStatus::Always,
        },
        CapabilityCategory {
            id: "repo",
            title: "Repository intelligence",
            description: "Analyze the workspace, find files/text, read files, inspect git state, and make scoped repo writes.",
            use_when: &["unknown repo", "find code", "read file", "change code", "git diff", "project structure"],
            mentions: &["repo", "codebase", "file", "rg", "git", "diff", "commit"],
            tools: &[
                "repo_analyze",
                "repo_find",
                "repo_scan",
                "repo_read_file",
                "repo_git_status",
                "repo_git_diff",
                "repo_write_file",
            ],
            skills: &["rust-tooling", "ast-grep", "code-simplification"],
            status: CapabilityStatus::Always,
        },
        CapabilityCategory {
            id: "docs_web",
            title: "External docs and web context",
            description: "Fetch and extract relevant external documentation or API reference pages through the gateway.",
            use_when: &["official docs URL", "API reference", "latest docs", "web page", "external documentation"],
            mentions: &["http", "https", "docs", "documentation", "reference", "crawl", "website"],
            tools: &["crawl4ai__*"],
            skills: &["crawl4ai-context"],
            status: if runtime.docs_web_loaded {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "db",
            title: "Database manager",
            description: "Inspect schemas, validate/query data, explain plans, export data, and perform guarded row operations.",
            use_when: &["database", "SQL", "schema", "table rows", "query", "export"],
            mentions: &["db", "sql", "sqlite", "postgres", "mysql", "schema", "table"],
            tools: &[
                "db_schema",
                "db_query",
                "db_validate_query",
                "db_table_info",
                "db_explain",
                "db_export_query",
            ],
            skills: &[],
            status: if requested("db") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "ssh",
            title: "SSH and SFTP",
            description: "List known hosts, test SSH access, run guarded remote commands, and move files over SFTP.",
            use_when: &["remote host", "SSH", "SFTP", "remote file", "server command"],
            mentions: &["ssh", "sftp", "remote", "host", "server"],
            tools: &[
                "ssh_hosts",
                "ssh_test",
                "ssh_exec",
                "ssh_context_refresh",
                "ssh_context",
                "sftp_list",
                "sftp_get",
                "sftp_put",
            ],
            skills: &[],
            status: if requested("ssh") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "n8n",
            title: "n8n workflow automation",
            description: "Generate, validate, save, import, activate, and smoke-test n8n workflow automations against a configured or local n8n instance.",
            use_when: &[
                "n8n workflow",
                "automation",
                "webhook workflow",
                "import workflow",
                "test workflow",
            ],
            mentions: &["n8n", "workflow", "automation", "webhook", "zapier", "make.com"],
            tools: &[
                "n8n_validate_workflow",
                "n8n_save_workflow",
                "n8n_import_workflow",
                "n8n_activate_workflow",
                "n8n_webhook_request",
                "n8n_local_start",
            ],
            skills: &[],
            status: if requested("n8n") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "document_extract",
            title: "Document extraction",
            description:
                "Convert local PDF, DOCX, PPTX, CSV, and XLSX files into searchable Markdown knowledge shards optimized for agent reading.",
            use_when: &[
                "PDF manual",
                "Word document",
                "PowerPoint deck",
                "DOCX",
                "PPTX",
                "extract document to Markdown",
            ],
            mentions: &[
                "pdf",
                "docx",
                "pptx",
                "word",
                "powerpoint",
                "document",
                "manual",
                "slides",
                "knowledge_search",
            ],
            tools: &[
                "knowledge_pdf_ingest",
                "knowledge_office_ingest",
                "knowledge_data_ingest",
                "knowledge_search",
            ],
            skills: &[],
            status: if requested("document_extract") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "data_loader",
            title: "CSV/XLSX data loader",
            description: "Inspect, write, and ingest CSV/XLSX files with deterministic parsing and searchable knowledge summaries.",
            use_when: &[
                "CSV",
                "Excel",
                "spreadsheet",
                "tabular file",
                "normalize rows",
                "ingest data to knowledge",
            ],
            mentions: &["csv", "xlsx", "excel", "spreadsheet", "rows", "columns"],
            tools: &[
                "POST /api/data/inspect",
                "POST /api/data/write",
                "knowledge_data_ingest",
                "knowledge_search",
            ],
            skills: &[],
            status: if requested("data_loader") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "project_memory",
            title: "Project memory",
            description: "Use persisted project context, continuity, repo bindings, and codebase-memory status.",
            use_when: &["resume context", "prior decisions", "project memory", "codebase-memory", "continuity"],
            mentions: &["memory", "continuity", "decision", "pending", "codebase-memory"],
            tools: &["repo_codebase_memory_status"],
            skills: &[],
            status: if requested("project_memory") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "sessions",
            title: "Subagents and mailbox",
            description:
                "Spawn child sessions, inspect descendants, send input, exchange mailbox messages, and cancel children.",
            use_when: &[
                "subagent",
                "parallel worker",
                "child session",
                "mailbox",
                "delegate work",
            ],
            mentions: &["agent", "subagent", "child", "worker", "mailbox", "delegate"],
            tools: &[
                "session_spawn_child",
                "session_list_children",
                "session_read_child_summary",
                "session_send_input",
                "session_mailbox_send",
                "session_mailbox_list",
                "session_mailbox_ack",
                "session_cancel_child",
            ],
            skills: &[],
            status: if requested("sessions") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
        CapabilityCategory {
            id: "docs_build",
            title: "Documentation build",
            description: "Generate a docs site scaffold from Markdown and run a local docs build when dependencies exist.",
            use_when: &["docs site", "Starlight", "mdBook", "VitePress", "documentation build"],
            mentions: &["docs", "starlight", "mdbook", "vitepress", "site"],
            tools: &["docs_build"],
            skills: &[],
            status: if requested("docs_build") {
                CapabilityStatus::Loaded
            } else {
                CapabilityStatus::AvailableOnRequest
            },
        },
    ]
}
