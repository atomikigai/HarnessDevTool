pub mod db;
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
                          task.created SSE event so the UI updates immediately. Returns \
                          the created Task object."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["title"],
                "properties": {
                    "thread_id":  { "type": "string" },
                    "title":      { "type": "string" },
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
            description: "Fetch a single task by id within a thread.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_claim".into(),
            description: "Claim a lease on a task. Returns busy info if another agent holds it."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id", "agent_id"],
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
            description: "Renew the lease the caller holds on a task.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id", "agent_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "agent_id":  { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_update".into(),
            description: "Patch a task's metadata (status, label, assignee, title, notes)."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id", "patch"],
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
            description: "Release the lease the caller holds on a task.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id", "agent_id"],
                "properties": {
                    "thread_id": { "type": "string" },
                    "task_id":   { "type": "string" },
                    "agent_id":  { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "task_submit".into(),
            description: "Submit task artifacts (files, turns, diff). Marks task as submitted."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "task_id", "artifacts"],
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
            name: "db_query".into(),
            description: "Run a SQL query against a saved DB connection. Non-SELECT statements \
                require `approved: true`."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "sql"],
                "properties": {
                    "connection": { "type": "string", "description": "connection id" },
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
                    "sql":        { "type": "string" }
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
