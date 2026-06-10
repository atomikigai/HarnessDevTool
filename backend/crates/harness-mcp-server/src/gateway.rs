use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::protocol::ToolDescriptor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamMcpConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Gateway {
    upstreams: Vec<UpstreamMcpConfig>,
}

impl Gateway {
    pub fn new(upstreams: Vec<UpstreamMcpConfig>) -> Self {
        Self { upstreams }
    }

    pub fn from_config_path(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read upstream MCP config {}: {e}", path.display()))?;
        let upstreams: Vec<UpstreamMcpConfig> =
            serde_json::from_str(&text).map_err(|e| format!("parse upstream MCP config: {e}"))?;
        Ok(Self::new(upstreams))
    }

    pub fn prefixed_tool(&self, tool_name: &str) -> Option<(&UpstreamMcpConfig, String)> {
        let (prefix, inner) = tool_name.split_once("__")?;
        if inner.is_empty() {
            return None;
        }
        self.upstreams
            .iter()
            .find(|upstream| upstream.name == prefix)
            .map(|upstream| (upstream, inner.to_string()))
    }

    pub fn has_upstream(&self, name: &str) -> bool {
        self.upstreams.iter().any(|upstream| upstream.name == name)
    }

    pub fn list_descriptors(&self) -> Vec<ToolDescriptor> {
        let mut out = Vec::new();
        for upstream in &self.upstreams {
            match upstream_request(upstream, "tools/list", json!({})) {
                Ok(result) => {
                    let tools = result
                        .get("tools")
                        .and_then(|tools| tools.as_array())
                        .cloned()
                        .unwrap_or_default();
                    for tool in tools {
                        match serde_json::from_value::<ToolDescriptor>(tool) {
                            Ok(mut descriptor) => {
                                descriptor.name = format!("{}__{}", upstream.name, descriptor.name);
                                descriptor.description = format!(
                                    "[{} via Harness gateway] {}",
                                    upstream.name, descriptor.description
                                );
                                out.push(descriptor);
                            }
                            Err(e) => warn!(
                                upstream = %upstream.name,
                                error = %e,
                                "upstream returned invalid tool descriptor"
                            ),
                        }
                    }
                }
                Err(e) => warn!(
                    upstream = %upstream.name,
                    error = %e,
                    "failed to list upstream MCP tools"
                ),
            }
        }
        out
    }

    pub fn call(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        let (upstream, inner_name) = self
            .prefixed_tool(tool_name)
            .ok_or_else(|| format!("unknown gateway tool: {tool_name}"))?;
        upstream_request(
            upstream,
            "tools/call",
            json!({
                "name": inner_name,
                "arguments": args,
            }),
        )
    }
}

fn upstream_request(
    upstream: &UpstreamMcpConfig,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let mut child = Command::new(&upstream.command)
        .args(&upstream.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn upstream {}: {e}", upstream.name))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("upstream {} stdin unavailable", upstream.name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("upstream {} stdout unavailable", upstream.name))?;
    let mut reader = BufReader::new(stdout);

    write_framed(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": crate::protocol::PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "harness-mcp-gateway", "version": crate::protocol::SERVER_VERSION }
            }
        }),
    )?;
    let _ = read_response(&mut reader, 1)?;

    write_framed(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )?;

    write_framed(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params
        }),
    )?;
    let result = read_response(&mut reader, 2);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn write_framed<W: Write>(out: &mut W, value: &Value) -> Result<(), String> {
    let body = serde_json::to_string(value).map_err(|e| format!("serialize MCP request: {e}"))?;
    write!(out, "Content-Length: {}\r\n\r\n", body.len())
        .map_err(|e| format!("write MCP frame header: {e}"))?;
    out.write_all(body.as_bytes())
        .map_err(|e| format!("write MCP frame body: {e}"))?;
    out.flush().map_err(|e| format!("flush MCP frame: {e}"))
}

fn read_response<R: BufRead>(reader: &mut R, wanted_id: i64) -> Result<Value, String> {
    loop {
        let payload = read_framed_or_line(reader)?;
        let value: Value =
            serde_json::from_str(&payload).map_err(|e| format!("parse upstream response: {e}"))?;
        let id = value.get("id").and_then(|id| id.as_i64());
        if id != Some(wanted_id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("upstream MCP error: {error}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| "upstream MCP response missing result".to_string());
    }
}

fn read_framed_or_line<R: BufRead>(reader: &mut R) -> Result<String, String> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("read upstream MCP header: {e}"))?;
        if n == 0 {
            return Err("upstream MCP closed stdout".to_string());
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.trim().is_empty() {
            continue;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                let len = value
                    .trim()
                    .parse::<usize>()
                    .map_err(|e| format!("invalid upstream content-length: {e}"))?;
                loop {
                    let mut blank = String::new();
                    let n = reader
                        .read_line(&mut blank)
                        .map_err(|e| format!("read upstream MCP blank line: {e}"))?;
                    if n == 0 {
                        return Err("upstream MCP closed before frame body".to_string());
                    }
                    if blank.trim_end_matches(['\r', '\n']).is_empty() {
                        break;
                    }
                }
                let mut buf = vec![0_u8; len];
                reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("read upstream MCP frame body: {e}"))?;
                return String::from_utf8(buf)
                    .map_err(|e| format!("upstream MCP frame was not utf-8: {e}"));
            }
        }
        return Ok(header.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::{Gateway, UpstreamMcpConfig};

    #[test]
    fn maps_prefixed_tool_to_upstream() {
        let gateway = Gateway::new(vec![UpstreamMcpConfig {
            name: "crawl4ai".into(),
            command: "npx".into(),
            args: vec!["mcp-remote".into()],
        }]);

        let (upstream, inner) = gateway.prefixed_tool("crawl4ai__crawl").unwrap();
        assert_eq!(upstream.name, "crawl4ai");
        assert_eq!(inner, "crawl");
        assert!(gateway.prefixed_tool("other__crawl").is_none());
        assert!(gateway.prefixed_tool("crawl4ai__").is_none());
    }
}
