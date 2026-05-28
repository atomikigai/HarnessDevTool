pub mod db;
pub mod session;
pub mod skills;
pub mod spec;
pub mod tasks;

use serde_json::{json, Value};

use crate::protocol::ToolDescriptor;

/// Descriptors returned by `tools/list`. Names use underscores (claude requires
/// `[a-zA-Z0-9_-]+`); the brief's `task.list` is the conceptual name.
pub fn list_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "task_create".into(),
            description: "Create a new task in the current (or named) thread. Emits a \
                          task.created SSE event so the UI updates immediately. Orchestrators \
                          should pass `brief` using Objetivo/Contexto/Tarea/Reglas/Resultado \
                          esperado; the brief is persisted in acceptance checks so workers can \
                          recover it with task_get across sessions. Returns the created Task object."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["title"],
                "properties": {
                    "thread_id":  { "type": "string" },
                    "title":      { "type": "string" },
                    "brief": {
                        "oneOf": [
                            { "type": "string" },
                            {
                                "type": "object",
                                "properties": {
                                    "objetivo": { "type": "string" },
                                    "contexto": { "type": "string" },
                                    "tarea": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "reglas": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "resultado_esperado": { "type": "string" }
                                }
                            }
                        ]
                    },
                    "parent":     { "type": "string" },
                    "depends_on": { "type": "array", "items": { "type": "string" } },
                    "labels":     { "type": "array", "items": { "type": "string" } },
                    "acceptance": {
                        "type": "object",
                        "properties": {
                            "checks": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "required": ["text"],
                                    "properties": {
                                        "id":   { "type": "string" },
                                        "text": { "type": "string" }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "task_list".into(),
            description: "List tasks for a thread, with optional status/label/assignee filters."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "status": { "type": "string" },
                    "label": { "type": "string" },
                    "assignee": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_get".into(),
            description: "Fetch a single task by id within a thread. `thread_id` defaults to \
                          the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_claim".into(),
            description: "Claim a lease on a task. Returns busy info if another agent holds it. \
                          `thread_id` defaults to the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id", "agent_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "agent_id":  { "type": "string" },
                    "ttl_s":     { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "task_renew".into(),
            description: "Renew the lease the caller holds on a task. `thread_id` defaults to \
                          the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id", "agent_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "agent_id":  { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_update".into(),
            description: "Patch a task's metadata (status, label, assignee, title, notes). \
                          `thread_id` defaults to the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id", "patch"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "patch":     {
                        "type": "object",
                        "properties": {
                            "status":   { "type": "string" },
                            "label":    { "type": "string" },
                            "assignee": { "type": "string" },
                            "title":    { "type": "string" },
                            "notes":    { "type": "string" }
                        }
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "task_release".into(),
            description: "Release the lease the caller holds on a task. `thread_id` defaults to \
                          the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id", "agent_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "agent_id":  { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_submit".into(),
            description: "Submit task artifacts (files, turns, diff). Marks task as submitted. \
                          `thread_id` defaults to the caller's thread when omitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["task_id", "artifacts"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "artifacts": {
                        "type": "object",
                        "properties": {
                            "files": { "type": "array", "items": { "type": "string" } },
                            "turns": { "type": "integer" },
                            "diff":  { "type": "string" }
                        },
                        "required": ["files"]
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "spec_read".into(),
            description:
                "Read the thread spec markdown (profiles/default/threads/<tid>/spec.md). Empty if missing."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "scope":     { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "spec_write".into(),
            description:
                "Overwrite the thread spec markdown (profiles/default/threads/<tid>/spec.md)."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "content"],
                "properties": {
                    "thread_id": { "type": "string", "pattern": "^[A-Za-z0-9_-]+$" },
                    "content":   { "type": "string", "maxLength": 1048576 },
                    "etag":      { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_query".into(),
            description: "Run a SQL query against a saved DB connection. Non-SELECT statements \
                require `approved: true`."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "sql"],
                "properties": {
                    "connection": { "type": "string", "description": "connection id" },
                    "database":   { "type": "string" },
                    "sql":        { "type": "string" },
                    "limit":      { "type": "integer", "minimum": 1 },
                    "approved":   { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_schema".into(),
            description: "Return the schema tree (schemas/tables/columns) of a connection."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_explain".into(),
            description: "EXPLAIN a SQL statement on a connection (engine-specific prefix).".into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "sql"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "sql":        { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_performance_audit".into(),
            description: "Run a read-only PostgreSQL performance audit over saved DB connection stats: table activity/size, FK indexes, unused indexes, scan ratios, duplicate indexes, and pg_stat_statements availability."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "limit":      { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "db_backup".into(),
            description: "Write a SQL backup for a DB connection before approved modifications. \
                With schema+table it backs up that table; with schema only it backs up the schema; \
                with no target it backs up every schema from the current schema tree."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_memory_read".into(),
            description: "Read the persistent architecture/structure memory for a saved DB connection and database."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_memory_write".into(),
            description: "Overwrite the persistent architecture/structure memory for a saved DB connection and database. Use it to improve indexed DB documentation across sessions."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "content"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "content":    { "type": "string", "maxLength": 1048576 }
                }
            }),
        },
        ToolDescriptor {
            name: "skills_search".into(),
            description: "Search skills (stub until F5 — currently returns []).".into(),
            input_schema: json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "top_k": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        // ── Session tree (Zeus orchestrator) ────────────────────────────
        ToolDescriptor {
            name: "session_spawn_child".into(),
            description:
                "Create a child session under the CURRENT session. Used by orchestrators \
                 (Zeus) to delegate scoped work to a CLI specialised for the role. The \
                 child inherits the current session as its root and as its parent."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["kind", "role", "initial_prompt"],
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["claude", "codex", "cursor", "antigravity"],
                        "description": "Which CLI backs the child PTY."
                    },
                    "role": {
                        "type": "string",
                        "description": "Free-form role label (backend/frontend/db/qa/refactor/etc.)."
                    },
                    "initial_prompt": {
                        "type": "string",
                        "description": "First user turn typed into the child PTY. Include scope, \
                                        forbidden areas, expected output, test requirements."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional cwd override; defaults to $HOME."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "session_list_children".into(),
            description:
                "List direct children of the current session (one level only). Returns \
                 [{ session_id, kind, role, status, ... }]."
                    .into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "session_read_child_summary".into(),
            description:
                "Read the current meta/status of a child session by id. Pre-F3 this is a \
                 meta snapshot; richer handoff summaries land with F3."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["child_session_id"],
                "properties": { "child_session_id": { "type": "string" } }
            }),
        },
        ToolDescriptor {
            name: "session_send_input".into(),
            description:
                "Write raw input bytes into a descendant session's PTY. Use this to unstick \
                 a worker that's waiting for Enter (`text: \"\\r\"`), or to send a follow-up \
                 message into an existing child session. The text is sent verbatim — embed \
                 `\\r` to submit at the end."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["child_session_id", "text"],
                "properties": {
                    "child_session_id": { "type": "string" },
                    "text":             { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "session_cancel_child".into(),
            description:
                "Kill a descendant of the current session. Errors if the target is not \
                 inside the caller's session tree."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["child_session_id"],
                "properties": {
                    "child_session_id": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }),
        },
    ]
}

/// Wrap a JSON value into the MCP `tools/call` result envelope.
/// MCP expects: `{ content: [{ type: "text", text: "..." }] }`.
pub fn wrap_text(payload: &Value) -> Value {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "null".to_string());
    json!({
        "content": [ { "type": "text", "text": text } ]
    })
}

/// Wrap an error result so the agent sees a structured failure without
/// dropping the JSON-RPC call.
pub fn wrap_error(message: &str) -> Value {
    json!({
        "content": [ { "type": "text", "text": format!("error: {message}") } ],
        "isError": true
    })
}
