//! MCP / JSON-RPC 2.0 wire types.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const SERVER_NAME: &str = "harness-mcp-server";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parsed incoming JSON-RPC request (or notification).
#[derive(Debug, Clone)]
pub struct Request {
    /// `None` for notifications.
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Copy)]
pub enum RpcError {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
}

impl RpcError {
    pub fn code(self) -> i32 {
        match self {
            RpcError::ParseError => -32700,
            RpcError::InvalidRequest => -32600,
            RpcError::MethodNotFound => -32601,
            RpcError::InvalidParams => -32602,
            RpcError::InternalError => -32603,
        }
    }
    pub fn default_message(self) -> &'static str {
        match self {
            RpcError::ParseError => "Parse error",
            RpcError::InvalidRequest => "Invalid request",
            RpcError::MethodNotFound => "Method not found",
            RpcError::InvalidParams => "Invalid params",
            RpcError::InternalError => "Internal error",
        }
    }
}

/// Parse a single JSON line into a Request.
///
/// On failure returns the id (if recoverable) and an RpcError so we can still
/// reply with a structured error.
pub fn parse_request(line: &str) -> Result<Request, (Value, RpcError)> {
    let v: Value = serde_json::from_str(line).map_err(|_| (Value::Null, RpcError::ParseError))?;

    let id = v.get("id").cloned();
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or_else(|| (id.clone().unwrap_or(Value::Null), RpcError::InvalidRequest))?
        .to_string();
    let params = v.get("params").cloned().unwrap_or(Value::Null);

    Ok(Request { id, method, params })
}

/// Build a structured error response.
pub fn error_response(id: Value, err: RpcError) -> Value {
    error_response_with(id, err, err.default_message())
}

pub fn error_response_with(id: Value, err: RpcError, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": err.code(), "message": message }
    })
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// Spec-shape for a single tool descriptor in `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}
