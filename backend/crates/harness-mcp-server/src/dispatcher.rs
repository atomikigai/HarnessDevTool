//! Maps incoming JSON-RPC requests to handlers.

use std::path::PathBuf;

use serde_json::{json, Value};
use tracing::warn;

use crate::protocol::{
    error_response, error_response_with, result_response, Request, RpcError, PROTOCOL_VERSION,
    SERVER_NAME, SERVER_VERSION,
};
use crate::tasks_shim::TaskStore;
use crate::tools::{self, skills, spec, tasks, wrap_error, wrap_text};

pub struct Dispatcher {
    store: TaskStore,
    harness_home: PathBuf,
    thread_id: String,
    agent_id: String,
}

impl Dispatcher {
    pub fn new(
        harness_home: PathBuf,
        thread_id: String,
        agent_id: String,
    ) -> Result<Self, String> {
        let store = TaskStore::new(&harness_home).map_err(|e| e.to_string())?;
        Ok(Self {
            store,
            harness_home,
            thread_id,
            agent_id,
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

        let outcome: Result<Value, String> = match name.as_str() {
            "task_list" => tasks::list(&self.store, &self.thread_id, &args),
            "task_get" => tasks::get(&self.store, &args),
            "task_claim" => tasks::claim(&self.store, &args),
            "task_renew" => tasks::renew(&self.store, &args),
            "task_update" => tasks::update(&self.store, &self.agent_id, &args),
            "task_release" => tasks::release(&self.store, &args),
            "task_submit" => tasks::submit(&self.store, &self.agent_id, &args),
            "spec_read" => spec::read(&self.harness_home, &self.thread_id, &args),
            "skills_search" => skills::search(&args),
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
    use crate::tasks_shim::{Artifacts, Task};

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
        let d =
            Dispatcher::new(home.clone(), thread.to_string(), agent.to_string()).unwrap();
        (d, home)
    }

    #[test]
    fn initialize_then_list_tools() {
        let (d, _) = mk("t1", "agent:1");

        let init_line =
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
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
            "task_list",
            "task_get",
            "task_claim",
            "task_renew",
            "task_update",
            "task_release",
            "task_submit",
            "spec_read",
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
    fn task_get_then_submit_roundtrip() {
        let (d, home) = mk("t1", "agent:1");
        // Seed via the shim directly.
        let store = TaskStore::new(&home).unwrap();
        store
            ._seed(Task {
                id: "task-1".into(),
                thread_id: "t1".into(),
                title: "demo".into(),
                status: "open".into(),
                label: None,
                assignee: None,
                notes: None,
                artifacts: None,
                created_at: 0,
                updated_at: 0,
                updated_by: None,
            })
            .unwrap();

        let get_line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"task_get","arguments":{"thread_id":"t1","task_id":"task-1"}}}"#;
        let resp = d.handle(parse_request(get_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"id\":\"task-1\""));

        let submit_line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"task_submit","arguments":{"thread_id":"t1","task_id":"task-1","artifacts":{"files":["a.rs","b.rs"],"turns":4}}}}"#;
        let resp = d.handle(parse_request(submit_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"status\":\"submitted\""));
        assert!(text.contains("a.rs"));

        let _ = Artifacts::default();
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
}
