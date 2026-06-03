//! Maps incoming JSON-RPC requests to handlers.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tracing::warn;

use crate::protocol::{
    error_response, error_response_with, result_response, Request, RpcError, PROTOCOL_VERSION,
    SERVER_NAME, SERVER_VERSION,
};
use crate::tools::{
    self, db as db_tools, knowledge as knowledge_tools, repo, session as session_tools, skills,
    spec, tasks, wrap_error, wrap_text,
};
use harness_core::TaskStore;
use harness_policy::{capability_default, Decision};
use module_db::Manager as DbManager;

pub struct Dispatcher {
    store: TaskStore,
    db: DbManager,
    harness_home: PathBuf,
    profile: String,
    thread_id: String,
    agent_id: String,
    role: Option<String>,
    /// Stable session id owning this MCP instance. Used to attribute
    /// `session.spawn_child` calls to the right parent session in the tree.
    /// `None` for legacy callers that pre-date the `--session-id` flag.
    session_id: Option<String>,
    /// Base URL of the harness-server (e.g. `http://127.0.0.1:8787`). When
    /// `Some`, `task_create` delegates to the REST endpoint so the in-process
    /// broadcast bus emits `task.created` and the SSE stream pushes the new
    /// task into the right panel without the user having to refresh.
    server_url: Option<String>,
    api_token: Option<String>,
    cwd: PathBuf,
}

impl Dispatcher {
    #[allow(dead_code)]
    pub fn new(harness_home: PathBuf, thread_id: String, agent_id: String) -> Result<Self, String> {
        Self::new_with_server(
            harness_home,
            thread_id,
            agent_id,
            None,
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_server(
        harness_home: PathBuf,
        thread_id: String,
        agent_id: String,
        session_id: Option<String>,
        profile: String,
        server_url: Option<String>,
        cwd: PathBuf,
        api_token: Option<String>,
        role: Option<String>,
    ) -> Result<Self, String> {
        let store = TaskStore::with_profile(&harness_home, &profile).map_err(|e| e.to_string())?;
        let db = DbManager::new(&harness_home, &profile).map_err(|e| e.to_string())?;
        Ok(Self {
            store,
            db,
            harness_home,
            profile,
            thread_id,
            agent_id,
            role,
            session_id,
            server_url,
            api_token,
            cwd,
        })
    }

    /// Handle a request. Returns `None` for notifications (no id).
    pub fn handle(&self, req: Request) -> Option<Value> {
        let id = req.id.clone();

        // Notifications have no id and never produce a response.
        let is_notification = id.is_none();
        let id = id.unwrap_or(Value::Null);

        match req.method.as_str() {
            "initialize" => Some(result_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
                }),
            )),
            "notifications/initialized" | "initialized" => None,
            "notifications/cancelled" => None,
            "ping" => Some(result_response(id, json!({}))),
            "tools/list" => Some(result_response(
                id,
                json!({ "tools": tools::list_descriptors() }),
            )),
            "tools/call" => {
                if is_notification {
                    return None;
                }
                Some(self.handle_tool_call(id, req.params))
            }
            other => {
                if is_notification {
                    warn!(method = %other, "unhandled notification");
                    return None;
                }
                Some(error_response_with(
                    id,
                    RpcError::MethodNotFound,
                    &format!("method not found: {other}"),
                ))
            }
        }
    }

    fn handle_tool_call(&self, id: Value, params: Value) -> Value {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return error_response_with(id, RpcError::InvalidParams, "missing tool name");
            }
        };
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        if let Some(msg) = self.check_tool_policy(&name, &args) {
            return result_response(id, wrap_error(&msg));
        }

        let outcome: Result<Value, String> = match name.as_str() {
            "task_create" => tasks::create(
                &self.store,
                &self.thread_id,
                &self.agent_id,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "task_propose" => tasks::propose(&self.store, &self.thread_id, &self.agent_id, &args),
            "task_list" => tasks::list(&self.store, &self.thread_id, &args),
            "task_get" => tasks::get(&self.store, &self.thread_id, &args),
            "task_claim" => tasks::claim(&self.store, &self.thread_id, &args),
            "task_renew" => tasks::renew(&self.store, &self.thread_id, &args),
            "task_update" => tasks::update(&self.store, &self.thread_id, &self.agent_id, &args),
            "task_release" => tasks::release(&self.store, &self.thread_id, &args),
            "task_submit" => tasks::submit(&self.store, &self.thread_id, &self.agent_id, &args),
            "spec_read" => spec::read(&self.harness_home, &self.thread_id, &args),
            "spec_write" => spec::write(
                &self.harness_home,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "knowledge_pdftotext_check" => Ok(knowledge_tools::pdftotext_check()),
            "knowledge_pdf_ingest" => {
                knowledge_tools::pdf_ingest(&self.harness_home, &self.profile, &args)
            }
            "skills_search" => skills::search(&args),
            "repo_analyze" => repo::analyze(&self.cwd, &args),
            "repo_scan" => repo::scan(&self.cwd, &args),
            "repo_read_file" => repo::read_file(&self.cwd, &args),
            "repo_git_status" => repo::git_status(&self.cwd, &args),
            "repo_git_log" => repo::git_log(&self.cwd, &args),
            "repo_git_diff" => repo::git_diff(&self.cwd, &args),
            "repo_codebase_memory_status" => repo::codebase_memory_status(&self.cwd, &args),
            "db_query" => db_tools::query(&self.db, &args),
            "db_schema" => db_tools::schema(&self.db, &args),
            "db_explain" => db_tools::explain(&self.db, &args),
            "db_performance_audit" => db_tools::performance_audit(&self.db, &args),
            "db_backup" => db_tools::backup(&self.db, &self.harness_home, &args),
            "db_memory_read" => db_tools::memory_read(&self.harness_home, &self.profile, &args),
            "db_memory_write" => db_tools::memory_write(&self.harness_home, &self.profile, &args),
            "session_spawn_child" => session_tools::spawn_child(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_list_children" => session_tools::list_children(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
            ),
            "session_read_child_summary" => session_tools::read_child_summary(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_send_input" => session_tools::send_input(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_cancel_child" => session_tools::cancel_child(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            other => {
                return error_response_with(
                    id,
                    RpcError::MethodNotFound,
                    &format!("unknown tool: {other}"),
                );
            }
        };

        match outcome {
            Ok(payload) => result_response(id, wrap_text(&payload)),
            Err(msg) => {
                // Per MCP spec, tool errors should be returned as a normal
                // result with isError=true, NOT as a JSON-RPC error (those are
                // reserved for protocol-level failures). This keeps the agent
                // loop alive and surfaces a structured message to the model.
                result_response(id, wrap_error(&msg))
            }
        }
    }

    fn check_tool_policy(&self, tool_name: &str, tool_args: &Value) -> Option<String> {
        let Some(server_url) = self.server_url.as_deref() else {
            return match capability_default(tool_name, self.role.as_deref()) {
                Some(Decision::Deny) => Some(policy_denied_message(tool_name)),
                // capability_default only produces None or Deny; anything else is unrestricted.
                _ => None,
            };
        };
        let payload = json!({
            "tool": tool_name,
            "args": tool_args,
            "thread_id": self.thread_id,
            "agent_id": self.agent_id,
            "role": self.role.as_deref(),
        });
        let url = format!("{}/api/approvals/check", server_url.trim_end_matches('/'));
        let mut req = ureq::post(&url).timeout(Duration::from_secs(120));
        if let Some(token) = self.api_token.as_deref() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        match req.send_json(payload) {
            Ok(resp) => match resp.into_json::<Value>() {
                Ok(value) => match value.get("decision").and_then(|v| v.as_str()) {
                    Some("allow") => None,
                    Some("deny") => Some(policy_denied_message(tool_name)),
                    Some(other) => Some(format!(
                        "approval check returned unknown decision `{other}` for {tool_name}; failing closed"
                    )),
                    None => {
                        Some(format!(
                            "approval check response missing decision for {tool_name}; failing closed"
                        ))
                    }
                },
                Err(e) => {
                    warn!(error = %e, "approval check response parse failed");
                    Some(format!(
                        "approval check response parse failed for {tool_name}; failing closed"
                    ))
                }
            },
            Err(e) => {
                warn!(error = %e, "approval check failed");
                Some(format!(
                    "approval check failed for {tool_name}; failing closed"
                ))
            }
        }
    }
}

fn policy_denied_message(tool_name: &str) -> String {
    match tool_name {
        "task_create" => "tool call denied by policy: task_create; usa task_propose".to_string(),
        _ => format!("tool call denied by policy: {tool_name}"),
    }
}

// Keep error_response symbol used so importers don't get warnings.
#[allow(dead_code)]
fn _unused() -> Value {
    error_response(Value::Null, RpcError::InternalError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_request;

    fn tmp_home() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "harness-mcp-disp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn mk(thread: &str, agent: &str) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new(home.clone(), thread.to_string(), agent.to_string()).unwrap();
        (d, home)
    }

    fn mk_with_cwd(thread: &str, agent: &str, cwd: PathBuf) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home.clone(),
            thread.to_string(),
            agent.to_string(),
            None,
            "default".into(),
            None,
            cwd,
            None,
            None,
        )
        .unwrap();
        (d, home)
    }

    fn mk_with_role(thread: &str, agent: &str, role: Option<&str>) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home.clone(),
            thread.to_string(),
            agent.to_string(),
            None,
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            role.map(String::from),
        )
        .unwrap();
        (d, home)
    }

    #[test]
    fn initialize_then_list_tools() {
        let (d, _) = mk("t1", "agent:1");

        let init_line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let req = parse_request(init_line).unwrap();
        let resp = d.handle(req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);

        let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req = parse_request(initialized).unwrap();
        assert!(d.handle(req).is_none(), "notifications produce no response");

        let list_line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let req = parse_request(list_line).unwrap();
        let resp = d.handle(req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in [
            "task_create",
            "task_propose",
            "task_list",
            "task_get",
            "task_claim",
            "task_renew",
            "task_update",
            "task_release",
            "task_submit",
            "spec_read",
            "spec_write",
            "knowledge_pdftotext_check",
            "knowledge_pdf_ingest",
            "db_performance_audit",
            "db_memory_read",
            "db_memory_write",
            "skills_search",
        ] {
            assert!(names.contains(&expected), "missing tool: {expected}");
        }
    }

    #[test]
    fn task_list_default_thread_empty() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"task_list","arguments":{}}}"#;
        let req = parse_request(line).unwrap();
        let resp = d.handle(req).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "[]");
    }

    #[test]
    fn task_create_with_brief_persists_worker_contract() {
        let (d, _home) = mk("t-brief", "agent:planner");
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":31,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Wire task brief",
                    "brief":{
                        "objetivo":"Permitir handoff claro al worker.",
                        "contexto":"MCP task_create debe conservar memoria entre sesiones.",
                        "tarea":["Crear task con brief","Recuperar task con task_get"],
                        "reglas":["No romper","Cambios mínimos","Seguir estilo existente","Agregar test"],
                        "resultado_esperado":"El worker puede leer el contrato completo."
                    },
                    "labels":["backend","brief"]
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(created["title"], "Wire task brief");
        assert_eq!(
            created["brief"]["objective"],
            "Permitir handoff claro al worker."
        );
        assert_eq!(
            created["brief"]["context"],
            "MCP task_create debe conservar memoria entre sesiones."
        );
        assert_eq!(created["brief"]["tasks"][0], "Crear task con brief");
        assert_eq!(created["brief"]["tasks"][1], "Recuperar task con task_get");
        assert_eq!(created["brief"]["rules"][0], "No romper");
        assert_eq!(
            created["brief"]["expected_result"],
            "El worker puede leer el contrato completo."
        );
        assert_eq!(created["acceptance"]["checks"].as_array().unwrap().len(), 0);

        let task_id = created["id"].as_str().unwrap();
        let get_line = format!(
            r#"{{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{{"name":"task_get","arguments":{{"task_id":"{task_id}"}}}}}}"#
        );
        let resp = d.handle(parse_request(&get_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let fetched: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(fetched["id"], task_id);
        assert_eq!(fetched["brief"], created["brief"]);
    }

    #[test]
    fn task_create_rejects_incomplete_structured_brief() {
        let (d, _home) = mk("t-brief-invalid", "agent:planner");
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":33,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Vague task",
                    "brief":{
                        "objetivo":"Arreglar cosas"
                    }
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("brief incomplete"));
        assert!(text.contains("contexto"));
        assert!(text.contains("tarea"));
        assert!(text.contains("reglas"));
        assert!(text.contains("resultado_esperado"));
        assert!(text.contains("Retry task_create with brief using this exact shape"));
        assert!(text.contains("\"objetivo\""));
        assert!(text.contains("\"contexto\""));
    }

    #[test]
    fn task_create_accepts_legacy_string_brief() {
        let (d, _home) = mk("t-brief-string", "agent:planner");
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":34,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Legacy brief",
                    "brief":"Plain text brief from an older caller."
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            created["brief"]["objective"],
            "Plain text brief from an older caller."
        );
        assert_eq!(created["acceptance"]["checks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn task_propose_creates_proposed_task() {
        let (d, _home) = mk_with_role("t-propose", "agent:worker", Some("worker"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":35,
            "method":"tools/call",
            "params":{
                "name":"task_propose",
                "arguments":{
                    "title":"Suggested follow-up",
                    "brief":"Worker found more work."
                }
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(created["status"], "proposed");
    }

    #[test]
    fn task_create_rejects_worker_role_with_hint() {
        let (d, _home) = mk_with_role("t-worker-create", "agent:worker", Some("worker"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":36,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{"title":"Should be proposed"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(
            text,
            "error: tool call denied by policy: task_create; usa task_propose"
        );
    }

    #[test]
    fn offline_capability_matrix_rejects_generator_spec_write() {
        let (d, _home) = mk_with_role("t-generator-spec", "agent:generator", Some("generator"));
        let line = r##"{
            "jsonrpc":"2.0",
            "id":39,
            "method":"tools/call",
            "params":{
                "name":"spec_write",
                "arguments":{"thread_id":"t-generator-spec","content":"# Should not write"}
            }
        }"##;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: tool call denied by policy: spec_write");
    }

    #[test]
    fn offline_capability_matrix_rejects_evaluator_sensitive_tool() {
        let (d, _home) = mk_with_role("t-evaluator-db", "agent:evaluator", Some("evaluator"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":40,
            "method":"tools/call",
            "params":{
                "name":"db_query",
                "arguments":{"sql":"select 1"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: tool call denied by policy: db_query");
    }

    #[test]
    fn task_create_allows_unknown_roles_by_default_matrix() {
        for role in ["super-planner-worker", "planner-worker", "not-orchestrator"] {
            let (d, _home) = mk_with_role("t-stuffed", "agent:worker", Some(role));
            let line = r#"{
                "jsonrpc":"2.0",
                "id":38,
                "method":"tools/call",
                "params":{
                    "name":"task_create",
                    "arguments":{"title":"Sneaky create"}
                }
            }"#;
            let resp = d.handle(parse_request(line).unwrap()).unwrap();
            assert_ne!(resp["result"]["isError"], true);
            let text = resp["result"]["content"][0]["text"].as_str().unwrap();
            let created: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(created["status"], "queued", "role `{role}` is unknown");
        }
    }

    #[test]
    fn task_create_allows_planner_and_legacy_none_role() {
        for (role, thread) in [
            (Some("planner"), "t-planner-create"),
            (None, "t-none-create"),
        ] {
            let (d, _home) = mk_with_role(thread, "agent:planner", role);
            let line = r#"{
                "jsonrpc":"2.0",
                "id":37,
                "method":"tools/call",
                "params":{
                    "name":"task_create",
                    "arguments":{"title":"Allowed create"}
                }
            }"#;
            let resp = d.handle(parse_request(line).unwrap()).unwrap();
            assert_ne!(resp["result"]["isError"], true);
            let text = resp["result"]["content"][0]["text"].as_str().unwrap();
            let created: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(created["status"], "queued");
        }
    }

    #[test]
    fn unknown_tool_returns_structured_error() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        // We use JSON-RPC error for unknown tools (protocol-level).
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn missing_args_returns_is_error_payload() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"task_get","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn spec_read_missing_returns_empty_string() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"t1"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"content\":\"\""));
    }

    #[test]
    fn spec_write_then_spec_read_returns_written_content() {
        let (d, _) = mk("t1", "agent:1");
        let write_line = r##"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"spec_write","arguments":{"thread_id":"t1","content":"# Spec\nBody"}}}"##;
        let resp = d.handle(parse_request(write_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains("\"created\":true"));

        let read_line = r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"t1"}}}"#;
        let resp = d.handle(parse_request(read_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("# Spec\\nBody"));
    }

    #[test]
    fn repo_analyze_reports_stack_and_codebase_memory_status() {
        let cwd = tmp_home();
        let (d, _home) = mk_with_cwd("t-repo", "agent:planner", cwd.clone());
        std::fs::write(cwd.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(
            cwd.join("package.json"),
            r#"{"scripts":{"test":"vitest"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        std::fs::write(cwd.join("pnpm-lock.yaml"), "").unwrap();
        let line = r#"{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"repo_analyze","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let stack = value["stack"].as_array().unwrap();
        assert!(stack.iter().any(|v| v == "rust"));
        assert!(stack.iter().any(|v| v == "node"));
        assert_eq!(value["package_manager"], "pnpm");
        assert!(value["codebase_memory"]["recommended"].as_bool().unwrap());
    }

    #[test]
    fn repo_read_file_rejects_parent_escape() {
        let (d, _) = mk("t-repo-safe", "agent:planner");
        let line = r#"{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"repo_read_file","arguments":{"path":"../secret.txt"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("must not escape"));
    }

    #[test]
    fn spec_read_rejects_invalid_thread_id() {
        let (d, _) = mk("t-spec-safe", "agent:planner");
        let line = r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"../escape"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("thread_id"));
    }

    #[test]
    fn task_tools_reject_invalid_path_ids() {
        let (d, _) = mk("t-task-safe", "agent:planner");
        let bad_thread = r#"{"jsonrpc":"2.0","id":44,"method":"tools/call","params":{"name":"task_list","arguments":{"thread_id":"../escape"}}}"#;
        let resp = d.handle(parse_request(bad_thread).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("thread_id"));

        let bad_task = r#"{"jsonrpc":"2.0","id":45,"method":"tools/call","params":{"name":"task_get","arguments":{"task_id":"../T-0001"}}}"#;
        let resp = d.handle(parse_request(bad_task).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("task_id"));
    }

    #[test]
    fn repo_read_file_truncates_on_utf8_boundary() {
        let cwd = tmp_home();
        let (d, _home) = mk_with_cwd("t-repo-utf8", "agent:planner", cwd.clone());
        std::fs::write(cwd.join("note.txt"), "aéz").unwrap();

        let line = r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"repo_read_file","arguments":{"path":"note.txt","max_bytes":2}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["content"], "a");
        assert_eq!(value["truncated"], true);
    }
}
