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
    self, db as db_tools, knowledge as knowledge_tools, session as session_tools, skills, spec,
    tasks, wrap_error, wrap_text,
};
use harness_core::TaskStore;
use module_db::Manager as DbManager;

pub struct Dispatcher {
    store: TaskStore,
    db: DbManager,
    harness_home: PathBuf,
    profile: String,
    thread_id: String,
    agent_id: String,
    /// Stable session id owning this MCP instance. Used to attribute
    /// `session.spawn_child` calls to the right parent session in the tree.
    /// `None` for legacy callers that pre-date the `--session-id` flag.
    session_id: Option<String>,
    /// Base URL of the harness-server (e.g. `http://127.0.0.1:8787`). When
    /// `Some`, `task_create` delegates to the REST endpoint so the in-process
    /// broadcast bus emits `task.created` and the SSE stream pushes the new
    /// task into the right panel without the user having to refresh.
    server_url: Option<String>,
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
        )
    }

    pub fn new_with_server(
        harness_home: PathBuf,
        thread_id: String,
        agent_id: String,
        session_id: Option<String>,
        profile: String,
        server_url: Option<String>,
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
            session_id,
            server_url,
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
                &args,
            ),
            "task_list" => tasks::list(&self.store, &self.thread_id, &args),
            "task_get" => tasks::get(&self.store, &self.thread_id, &args),
            "task_claim" => tasks::claim(&self.store, &self.thread_id, &args),
            "task_renew" => tasks::renew(&self.store, &self.thread_id, &args),
            "task_update" => tasks::update(&self.store, &self.thread_id, &self.agent_id, &args),
            "task_release" => tasks::release(&self.store, &self.thread_id, &args),
            "task_submit" => tasks::submit(&self.store, &self.thread_id, &self.agent_id, &args),
            "spec_read" => spec::read(&self.harness_home, &self.thread_id, &args),
            "spec_write" => spec::write(&self.harness_home, self.server_url.as_deref(), &args),
            "knowledge_pdftotext_check" => Ok(knowledge_tools::pdftotext_check()),
            "knowledge_pdf_ingest" => {
                knowledge_tools::pdf_ingest(&self.harness_home, &self.profile, &args)
            }
            "skills_search" => skills::search(&args),
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
                &args,
            ),
            "session_list_children" => {
                session_tools::list_children(self.session_id.as_deref(), self.server_url.as_deref())
            }
            "session_read_child_summary" => session_tools::read_child_summary(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                &args,
            ),
            "session_send_input" => session_tools::send_input(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                &args,
            ),
            "session_cancel_child" => session_tools::cancel_child(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
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
        let server_url = self.server_url.as_deref()?;
        let payload = json!({
            "tool": tool_name,
            "args": tool_args,
            "thread_id": self.thread_id,
            "agent_id": self.agent_id,
        });
        let url = format!("{}/api/approvals/check", server_url.trim_end_matches('/'));
        match ureq::post(&url)
            .timeout(Duration::from_secs(120))
            .send_json(payload)
        {
            Ok(resp) => match resp.into_json::<Value>() {
                Ok(value) => match value.get("decision").and_then(|v| v.as_str()) {
                    Some("allow") => None,
                    Some("deny") => Some(format!("tool call denied by policy: {tool_name}")),
                    Some(other) => {
                        warn!(decision = %other, "approval check returned unknown decision, continuing");
                        None
                    }
                    None => {
                        warn!("approval check response missing decision, continuing");
                        None
                    }
                },
                Err(e) => {
                    warn!(error = %e, "approval check response parse failed, continuing");
                    None
                }
            },
            Err(e) => {
                warn!(error = %e, "approval check failed, continuing");
                None
            }
        }
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
        assert_eq!(created["acceptance"]["checks"][0]["id"], "BRIEF");
        let brief_text = created["acceptance"]["checks"][0]["text"].as_str().unwrap();
        assert!(brief_text.contains("Objetivo:\nPermitir handoff claro al worker."));
        assert!(
            brief_text.contains("Tarea:\n1. Crear task con brief\n2. Recuperar task con task_get")
        );
        assert!(
            brief_text.contains("Resultado esperado:\nEl worker puede leer el contrato completo.")
        );

        let task_id = created["id"].as_str().unwrap();
        let get_line = format!(
            r#"{{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{{"name":"task_get","arguments":{{"task_id":"{task_id}"}}}}}}"#
        );
        let resp = d.handle(parse_request(&get_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let fetched: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(fetched["id"], task_id);
        assert_eq!(
            fetched["acceptance"]["checks"][0]["text"],
            created["acceptance"]["checks"][0]["text"]
        );
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
        assert_eq!(created["acceptance"]["checks"][0]["id"], "BRIEF");
        assert_eq!(
            created["acceptance"]["checks"][0]["text"],
            "Plain text brief from an older caller."
        );
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
}
