pub mod capabilities;
pub mod db;
pub mod docs;
pub mod knowledge;
pub mod repo;
pub mod session;
pub mod skills;
pub mod spec;
pub mod ssh;
pub mod tasks;

use serde_json::{json, Value};

use crate::protocol::ToolDescriptor;

/// Descriptors returned by `tools/list`. Names use underscores (claude requires
/// `[a-zA-Z0-9_-]+`); the brief's `task.list` is the conceptual name.
pub fn list_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "capability_list".into(),
            description: "List short Harness capability categories with when-to-use cues and common mentions. Call this before scanning many individual tools."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDescriptor {
            name: "capability_describe".into(),
            description: "Describe one Harness capability category, including concise use_when cues, mentions, status, relevant tools, and skills."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Capability category id from capability_list, for example repo, tasks, docs_web, db, ssh, data_loader, project_memory."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "capability_request".into(),
            description: "Request that one capability category be expanded in subsequent tools/list responses for this MCP session."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Capability category id from capability_list."
                    },
                    "reason": {
                        "type": "string",
                        "description": "Short reason this category is needed now."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "knowledge_pdf_ingest".into(),
            description: "Extract a local technical PDF into compact Markdown shards under HARNESS_HOME/profiles/<profile>/knowledge/pdf/<document>. Returns index and shard paths for future agent sessions."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["source_path"],
                "properties": {
                    "source_path": {
                        "type": "string",
                        "description": "Absolute or working-directory-relative path to a local .pdf file."
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional human-readable document title used for the output folder."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "knowledge_office_ingest".into(),
            description: "Extract a local DOCX or PPTX into compact agent-readable Markdown shards under HARNESS_HOME/profiles/<profile>/knowledge/office/<document>. Returns index and shard paths for future sessions."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["source_path"],
                "properties": {
                    "source_path": {
                        "type": "string",
                        "description": "Absolute or working-directory-relative path to a local .docx or .pptx file."
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional human-readable document title used for the output folder."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "task_create".into(),
            description: "Create a new task in the current (or named) thread. Emits a \
                          task.created SSE event so the UI updates immediately. Orchestrators \
                          should pass `brief` using Objetivo/Contexto/Tarea/Reglas/Resultado \
                          esperado; the brief is persisted as first-class task context so workers can \
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
                    "spec_refs": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["section", "version"],
                            "properties": {
                                "section": { "type": "string" },
                                "version": { "type": "integer", "minimum": 1 }
                            }
                        }
                    },
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
            name: "task_propose".into(),
            description: "Propose a new task in the current (or named) thread. Uses the same \
                          arguments as task_create, but stores the task as proposed so it is not \
                          claimable or scheduled until a planner promotes it to queued."
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
                    "spec_refs": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["section", "version"],
                            "properties": {
                                "section": { "type": "string" },
                                "version": { "type": "integer", "minimum": 1 }
                            }
                        }
                    },
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
            description: "Patch a task's metadata (status, labels, assignee, title, reasons, notes). \
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
                            "status":          { "type": "string" },
                            "labels":          { "type": "array", "items": { "type": "string" } },
                            "assignee":        { "type": ["string", "null"] },
                            "title":           { "type": "string" },
                            "blocked_by":      { "type": "array", "items": { "type": "string" } },
                            "blocked_reason":  { "type": "string" },
                            "paused_reason":   { "type": "string" },
                            "rejected_reason": { "type": "string" },
                            "last_failure":    { "type": "string" },
                            "needs_human":     { "type": "boolean" },
                            "why_paused":      { "type": "string" },
                            "why_abandoned":   { "type": "string" },
                            "feedback":        { "type": "string" },
                            "notes": {
                                "type": "object",
                                "properties": {
                                    "why_paused":      { "type": "string" },
                                    "why_abandoned":   { "type": "string" },
                                    "blocked_reason":  { "type": "string" },
                                    "paused_reason":   { "type": "string" },
                                    "rejected_reason": { "type": "string" },
                                    "last_failure":    { "type": "string" },
                                    "needs_human":     { "type": "boolean" },
                                    "feedback":        { "type": "array", "items": { "type": "string" } }
                                }
                            }
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
                            "turns": { "type": "array", "items": { "type": "string" } },
                            "diff":  { "type": "string" },
                            "metadata": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "required": ["kind", "path"],
                                    "properties": {
                                        "artifact_id": { "type": "string" },
                                        "task_id": { "type": "string" },
                                        "kind": {
                                            "type": "string",
                                            "enum": ["file", "diff", "test_output", "screenshot", "log"]
                                        },
                                        "path": { "type": "string" },
                                        "produced_by": { "type": "string" },
                                        "created_at": { "type": "string" },
                                        "summary": { "type": "string" }
                                    }
                                }
                            }
                        }
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
            name: "spec_set_section".into(),
            description:
                "Set one named section of the thread spec. Requires the caller's current \
                 spec version when `spec_version_required` is provided and increments the \
                 append-only spec version."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["thread_id", "section", "content"],
                "properties": {
                    "thread_id": { "type": "string", "pattern": "^[A-Za-z0-9_-]+$" },
                    "section":   { "type": "string" },
                    "content":   { "type": "string", "maxLength": 1048576 },
                    "spec_version_required": { "type": "integer", "minimum": 0 },
                    "by":        { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_query".into(),
            description: "Run a targeted SQL query against a saved DB connection. Prefer small read-only SELECTs with exact filters and LIMIT for inspection; non-SELECT statements require `approved: true`."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "sql"],
                "properties": {
                    "connection": { "type": "string", "description": "connection id" },
                    "database":   { "type": "string" },
                    "sql":        { "type": "string", "description": "single SQL statement; use exact schema/table filters and avoid broad scans for exploration" },
                    "limit":      { "type": "integer", "minimum": 1, "description": "maximum rows returned; use 20 for exploratory samples unless more is requested" },
                    "approved":   { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_select".into(),
            description: "Run a structured read-only SELECT. The agent fills table, columns, filters, ordering, and limit; the backend validates identifiers and builds SQL safely."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "columns":    { "type": "array", "items": { "type": "string" } },
                    "filters":    {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["column", "op"],
                            "properties": {
                                "column": { "type": "string" },
                                "op": { "type": "string", "enum": ["eq", "neq", "gt", "gte", "lt", "lte", "like", "ilike", "contains", "starts_with", "in", "is_null", "is_not_null"] },
                                "value": {},
                                "values": { "type": "array", "items": {} }
                            }
                        }
                    },
                    "order_by":   {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["column"],
                            "properties": {
                                "column": { "type": "string" },
                                "dir": { "type": "string", "enum": ["asc", "desc"] }
                            }
                        }
                    },
                    "limit":      { "type": "integer", "minimum": 1, "description": "default 20, capped at 500" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_validate_query".into(),
            description: "Validate a raw SQL or structured db_select request before execution. Checks read-only/single-statement SQL or table/column/operator validity for structured selects."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "sql":        { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "columns":    { "type": "array", "items": { "type": "string" } },
                    "filters":    { "type": "array", "items": { "type": "object" } },
                    "order_by":   { "type": "array", "items": { "type": "object" } },
                    "limit":      { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "db_schema".into(),
            description: "Inspect database schema. For a specific table, pass `schema` and `table` to return only that table instead of the whole schema tree."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string", "description": "schema name, e.g. public" },
                    "table":      { "type": "string", "description": "table/view name, e.g. users" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_table_info".into(),
            description: "Return compact metadata for one table/view: columns, primary key flags, indexes, and foreign keys. Prefer this over broad schema introspection when the user names a table."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string", "description": "schema name, e.g. public" },
                    "table":      { "type": "string", "description": "table/view name, e.g. users" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_search_tables".into(),
            description: "Search schemas, tables, and columns by name using catalog queries filtered by the search term. Use this when the exact table name is unknown."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "q"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "q":          { "type": "string", "description": "case-insensitive table/column search text" },
                    "limit":      { "type": "integer", "minimum": 1, "description": "maximum matches; default 20" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_sample".into(),
            description: "Return a small read-only sample from one table using quoted identifiers and LIMIT. Prefer this over hand-written SELECTs for simple examples."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "columns":    { "type": "array", "items": { "type": "string" }, "description": "optional columns to return; omit for all columns" },
                    "limit":      { "type": "integer", "minimum": 1, "description": "default 20, capped at 100" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_count".into(),
            description: "Return COUNT(*) for one table using quoted identifiers. Prefer this over hand-written count queries for simple row counts."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_distinct_values".into(),
            description: "Return distinct values for one column with frequency counts. Use for enum-like/status/category columns before filtering or summarizing."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table", "column"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "column":     { "type": "string" },
                    "limit":      { "type": "integer", "minimum": 1, "description": "default 50, capped at 200" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_find_rows".into(),
            description: "Search text across selected columns of one table and return a small sample. Prefer this over hand-written LIKE queries."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table", "q", "columns"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "q":          { "type": "string" },
                    "columns":    { "type": "array", "items": { "type": "string" }, "description": "columns to search" },
                    "return_columns": { "type": "array", "items": { "type": "string" }, "description": "optional projected columns; omit for all columns" },
                    "limit":      { "type": "integer", "minimum": 1, "description": "default 20, capped at 100" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_aggregate".into(),
            description: "Run a structured aggregate query with group_by and metrics such as count, count_distinct, sum, avg, min, max. Prefer this over raw GROUP BY SQL."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table", "metrics"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string" },
                    "table":      { "type": "string" },
                    "group_by":   { "type": "array", "items": { "type": "string" } },
                    "metrics":    {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["fn", "as"],
                            "properties": {
                                "fn": { "type": "string", "enum": ["count", "count_distinct", "sum", "avg", "min", "max"] },
                                "column": { "type": "string", "description": "required except for count(*)" },
                                "as": { "type": "string" }
                            }
                        }
                    },
                    "limit":      { "type": "integer", "minimum": 1, "description": "default 50, capped at 500" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_extract_enriched".into(),
            description: "Extract rows from one table and enrich foreign-key/id columns with readable labels from referenced tables. Use when the user asks to extract information with FK texts."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "columns": { "type": "array", "items": { "type": "string" }, "description": "base columns to return; omit for all columns" },
                    "filters": { "type": "array", "items": { "type": "object" }, "description": "same structured filters as db_select" },
                    "include_fk_labels": { "type": "boolean", "description": "default true" },
                    "label_columns": { "type": "array", "items": { "type": "string" }, "description": "preferred columns for labels, e.g. name,title,email,code" },
                    "limit": { "type": "integer", "minimum": 1, "description": "default 20, capped at 200" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_relation_performance".into(),
            description: "Return performance and maintenance stats for one table/view. PostgreSQL returns size, tuple estimates, scan counters, vacuum/analyze timestamps, and index stats. Prefer this over broad performance audits when the user names a relation."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database":   { "type": "string" },
                    "schema":     { "type": "string", "description": "schema name, e.g. public; defaults to public for PostgreSQL" },
                    "table":      { "type": "string", "description": "table/view name" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_row_insert".into(),
            description: "Insert one row into a table. Mutating tool: requires approved=true after explicit user confirmation."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table", "values"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "values": { "type": "object" },
                    "approved": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_row_delete".into(),
            description: "Delete one or more rows by primary-key maps. Mutating tool: requires approved=true after backup/explicit user confirmation."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "pk": { "type": "object" },
                    "pks": { "type": "array", "items": { "type": "object" } },
                    "approved": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_row_duplicate".into(),
            description: "Duplicate one or more rows by primary-key maps. Mutating tool: requires approved=true after explicit user confirmation."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "pk": { "type": "object" },
                    "pks": { "type": "array", "items": { "type": "object" } },
                    "approved": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_export_table".into(),
            description: "Export one table to a file under HARNESS_HOME/exports/db. Formats: json, csv, markdown, xlsx, sql_insert."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table", "format"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "columns": { "type": "array", "items": { "type": "string" } },
                    "format": { "type": "string", "enum": ["json", "csv", "markdown", "xlsx", "sql_insert"] },
                    "limit": { "type": "integer", "minimum": 1, "description": "used for markdown/xlsx; default 5000, capped at 100000" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_export_query".into(),
            description: "Export the result of a read-only SQL query to a file under HARNESS_HOME/exports/db. Use for complex SELECTs with joins/laterals/JSON aggregates. Formats: json, csv, markdown, xlsx."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "sql", "format"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "sql": { "type": "string", "description": "single read-only SQL statement" },
                    "format": { "type": "string", "enum": ["json", "csv", "markdown", "xlsx"] },
                    "filename": { "type": "string", "description": "optional filename without path" },
                    "limit": { "type": "integer", "minimum": 1, "description": "default 5000, capped at 100000" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_generate_view_sql".into(),
            description: "Generate CREATE VIEW or CREATE MATERIALIZED VIEW SQL from a read-only SELECT. Does not execute DDL; returns migration-style SQL text."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "view", "sql"],
                "properties": {
                    "connection": { "type": "string" },
                    "schema": { "type": "string" },
                    "view": { "type": "string" },
                    "sql": { "type": "string", "description": "single read-only SELECT statement" },
                    "replace": { "type": "boolean", "description": "default true" },
                    "materialized": { "type": "boolean", "description": "PostgreSQL only" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_drop_table".into(),
            description: "Drop a table. Dangerous mutating tool: requires approved=true and should only be called after db_backup."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "table"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "table": { "type": "string" },
                    "cascade": { "type": "boolean" },
                    "approved": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "db_drop_schema".into(),
            description: "Drop a schema. Dangerous mutating tool: requires approved=true and should only be called after db_backup."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["connection", "schema"],
                "properties": {
                    "connection": { "type": "string" },
                    "database": { "type": "string" },
                    "schema": { "type": "string" },
                    "cascade": { "type": "boolean" },
                    "approved": { "type": "boolean" }
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
            description: "Search active/proposed Harness skills learned for this profile. Use before inventing a workflow from scratch."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "top_k": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "skill_propose".into(),
            description: "Create a proposed skill Markdown file from a repeated successful workflow or recovery pattern. This never activates the skill."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["title", "body", "reason"],
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string", "description": "Markdown body for the proposed skill." },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "reason": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "skill_promote".into(),
            description: "Promote a reviewed proposed skill to active. Creates a snapshot before changing files."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["id", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "skill_archive".into(),
            description: "Archive an active or proposed skill without deleting it. Creates a snapshot before changing files."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["id", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "skill_record_usage".into(),
            description: "Append usage telemetry for a loaded skill so curator can keep useful skills and flag unused ones."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["skill_id", "outcome"],
                "properties": {
                    "skill_id": { "type": "string" },
                    "outcome": { "type": "string" },
                    "session_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "loaded": { "type": "boolean" },
                    "used": { "type": "boolean" },
                    "duration_ms": { "type": "integer", "minimum": 0 }
                }
            }),
        },
        ToolDescriptor {
            name: "evolve_observe".into(),
            description: "Append an evolution observation about repeated work, recovery, failed tools, or useful patterns. Observations are proposals input, not active behavior."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["kind", "summary"],
                "properties": {
                    "kind": { "type": "string" },
                    "summary": { "type": "string" },
                    "thread_id": { "type": "string" },
                    "session_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "signals": { "type": "array", "items": { "type": "string" } },
                    "evidence": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolDescriptor {
            name: "evolve_run".into(),
            description: "Run the deterministic learner batch over recent observations and write proposed skills only."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }
            }),
        },
        ToolDescriptor {
            name: "curator_run".into(),
            description: "Run deterministic skill corpus maintenance. Defaults to dry_run=true; non-dry-run archives unused active skills with snapshots."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dry_run": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_analyze".into(),
            description: "Analyze the current workspace deterministically: stack signals, package manager, key files, scripts, env files, git state, and codebase-memory-mcp availability. Use before planning in unknown repos."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional workspace-relative subpath." }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_scan".into(),
            description: "List files under the current workspace with depth and result limits. Paths are workspace-relative and never escape cwd."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_depth": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_find".into(),
            description: "Deterministically find files by name, extension, or bounded text content search under the current workspace. Prefer this over ad-hoc shell find/grep/rg for repository discovery."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "name_contains": { "type": "string" },
                    "content_contains": { "type": "string" },
                    "extensions": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "max_depth": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_read_file".into(),
            description: "Read a text file from the workspace with size limits. Refuses paths outside cwd."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1 },
                    "head_lines": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_write_file".into(),
            description: "Write a UTF-8 text file under the workspace. Requires a task-scoped MCP session and the target path must be allowed by that task's write_paths."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["path", "content"],
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_git_status".into(),
            description: "Return `git status --short --branch` for the workspace.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "repo_git_log".into(),
            description: "Return recent git commits for the workspace.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1 },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_git_diff".into(),
            description: "Return git diff for the workspace, optionally scoped to a path.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "staged": { "type": "boolean" },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_git_create_branch".into(),
            description: "Create a git branch in the workspace and optionally check it out. Sensitive: requires policy approval."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["branch"],
                "properties": {
                    "branch": { "type": "string" },
                    "checkout": { "type": "boolean", "description": "Defaults to true." },
                    "start_point": { "type": "string", "description": "Optional git ref to branch from." }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_git_commit".into(),
            description: "Stage task-scoped paths and create a git commit. Requires a task-scoped MCP session; only paths allowed by the task can be staged."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["message", "paths"],
                "properties": {
                    "message": { "type": "string" },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1
                    },
                    "allow_empty": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_git_push".into(),
            description: "Push the current or selected branch to a remote. Sensitive: requires policy approval."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "remote": { "type": "string", "description": "Defaults to origin." },
                    "branch": { "type": "string", "description": "Defaults to the current branch." },
                    "set_upstream": { "type": "boolean", "description": "Defaults to true." }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_github_pr_create".into(),
            description: "Create a GitHub pull request using the local `gh` CLI. Sensitive: requires policy approval and an authenticated gh installation."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["title"],
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "base": { "type": "string" },
                    "head": { "type": "string" },
                    "draft": { "type": "boolean" }
                }
            }),
        },
        ToolDescriptor {
            name: "repo_codebase_memory_status".into(),
            description: "Report whether codebase-memory-mcp is installed and whether a local index marker exists for this workspace. The harness treats it as an optional code-intelligence accelerator."
                .into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "docs_build".into(),
            description: "Build or scaffold a static documentation site from workspace Markdown. Conceptual capability: docs.build. Backend can be auto, starlight, mdbook, or vitepress."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "backend": {
                        "type": "string",
                        "enum": ["auto", "starlight", "mdbook", "vitepress"],
                        "description": "Docs backend. Defaults to auto: mdbook for Rust-only repos, Starlight otherwise."
                    },
                    "source_dir": {
                        "type": "string",
                        "description": "Workspace-relative directory containing .md/.mdx files. Defaults to docs."
                    },
                    "output_dir": {
                        "type": "string",
                        "description": "Workspace-relative generated docs-site directory. Defaults to docs-site."
                    },
                    "title": { "type": "string" },
                    "install": {
                        "type": "boolean",
                        "description": "When true, run pnpm install before Node-backed builds."
                    },
                    "run_build": {
                        "type": "boolean",
                        "description": "When false, only scaffold/copy files. Defaults to true."
                    }
                }
            }),
        },
        ToolDescriptor {
            name: "ssh_hosts".into(),
            description: "List saved SSH hosts for the active profile.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "ssh_test".into(),
            description: "Test a saved SSH host. Network client wiring is incremental; returns a structured readiness message when unavailable."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host"],
                "properties": { "host": { "type": "string", "description": "saved SSH host id" } }
            }),
        },
        ToolDescriptor {
            name: "ssh_exec".into(),
            description: "Run a non-interactive command on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "cmd"],
                "properties": {
                    "host": { "type": "string" },
                    "cmd": { "type": "string" },
                    "env": { "type": "object" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_list".into(),
            description: "List a remote directory on a saved SSH host.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["host"],
                "properties": {
                    "host": { "type": "string" },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_get".into(),
            description: "Copy a remote file from a saved SSH host to a local path on the harness server. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "remote_path", "local_path"],
                "properties": {
                    "host": { "type": "string" },
                    "remote_path": { "type": "string" },
                    "local_path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_put".into(),
            description: "Copy a local file from the harness server to a remote path on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "local_path", "remote_path"],
                "properties": {
                    "host": { "type": "string" },
                    "local_path": { "type": "string" },
                    "remote_path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_mkdir".into(),
            description: "Create a remote directory on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "path"],
                "properties": {
                    "host": { "type": "string" },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_rmdir".into(),
            description: "Remove an empty remote directory on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "path"],
                "properties": {
                    "host": { "type": "string" },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_unlink".into(),
            description: "Remove a remote file on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "path"],
                "properties": {
                    "host": { "type": "string" },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "sftp_rename".into(),
            description: "Rename or move a remote path on a saved SSH host. Requires approval by default."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["host", "from_path", "to_path"],
                "properties": {
                    "host": { "type": "string" },
                    "from_path": { "type": "string" },
                    "to_path": { "type": "string" }
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
                "required": ["role", "initial_prompt"],
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["claude", "codex", "cursor", "antigravity"],
                        "description": "Optional CLI backing the child PTY. For Zeus roots with a role matrix, omit this and the harness resolves it from the selected role."
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
                    "task_id": {
                        "type": "string",
                        "description": "Optional harness task id this child is assigned to."
                    },
                    "scopes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional resource/work scopes granted to this child."
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
            name: "session_mailbox_send".into(),
            description:
                "Append an auditable mailbox message for a descendant session. This does not \
                 write into the PTY; the child reads it with session_mailbox_list and can ack it."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["to_session_id", "body"],
                "properties": {
                    "to_session_id": { "type": "string" },
                    "body":          { "type": "string" },
                    "task_id":       { "type": "string" },
                    "scopes":        { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolDescriptor {
            name: "session_mailbox_list".into(),
            description:
                "List mailbox messages addressed to the current session, including ack state."
                    .into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "session_mailbox_ack".into(),
            description:
                "Acknowledge a mailbox message addressed to the current session. Ack is \
                 append-only; the original message is not rewritten."
                    .into(),
            input_schema: json!({
                "type": "object",
                "required": ["message_id"],
                "properties": { "message_id": { "type": "string" } }
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
