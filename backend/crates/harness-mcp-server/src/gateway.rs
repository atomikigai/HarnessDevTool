use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::protocol::ToolDescriptor;

/// Maximum time allowed for a complete upstream MCP round-trip
/// (initialize handshake + method call). If the upstream does not
/// produce the expected response within this window the call fails
/// and the child process is killed and reaped.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(10);

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
    upstream_request_with_timeout(upstream, method, params, UPSTREAM_TIMEOUT)
}

/// Core implementation with a configurable timeout.
///
/// All blocking I/O (writes to stdin + reads from stdout of the child) runs
/// inside a dedicated OS thread so this function never blocks forever:
///
/// * If the upstream responds in time the result is forwarded normally.
/// * If `timeout` expires we kill the child process (which closes its stdout
///   pipe, unblocking the reader thread via EOF) and return an error.
///
/// The child is always killed and reaped before this function returns,
/// preventing both zombie processes and fd leaks.
fn upstream_request_with_timeout(
    upstream: &UpstreamMcpConfig,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let mut child = Command::new(&upstream.command)
        .args(&upstream.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn upstream {}: {e}", upstream.name))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("upstream {} stdin unavailable", upstream.name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("upstream {} stdout unavailable", upstream.name))?;

    let (tx, rx) = mpsc::channel::<Result<Value, String>>();
    let upstream_name = upstream.name.clone();
    let method_owned = method.to_string();

    // The thread owns both pipe ends and performs all blocking I/O.  When the
    // child is killed below the write end of stdout is closed by the OS, so
    // any in-progress `read_line` returns EOF and the thread exits promptly.
    std::thread::spawn(move || {
        let _ = tx.send(upstream_io(stdin, stdout, &method_owned, &params));
    });

    let outcome = rx.recv_timeout(timeout);

    // Always kill and reap the child — on the normal path it is still running
    // (MCP servers keep their process alive); on the timeout path this is
    // required to unblock the reader thread and reclaim resources.
    let _ = child.kill();
    let _ = child.wait();

    match outcome {
        Ok(result) => result,
        Err(_elapsed) => Err(format!(
            "upstream {upstream_name} timed out after {}s",
            timeout.as_secs()
        )),
    }
}

/// Blocking I/O helper that runs inside a dedicated thread.
///
/// Performs the full MCP conversation: initialize handshake, then the
/// requested method call.  Returns the `result` field of the JSON-RPC
/// response on success.
fn upstream_io(
    mut stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    method: &str,
    params: &Value,
) -> Result<Value, String> {
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
    read_response(&mut reader, 2)
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
    use super::{upstream_request_with_timeout, Gateway, UpstreamMcpConfig};
    use serde_json::json;
    use std::time::{Duration, Instant};

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

    /// A silent upstream (one that never writes anything to stdout) must time
    /// out and return an error well within the test budget — it must NOT hang
    /// forever.
    ///
    /// Mechanism: `sleep 60` is spawned as the "upstream".  It never writes to
    /// stdout, so the reader thread blocks on `read_line`.  After the 2-second
    /// test timeout fires, `upstream_request_with_timeout` kills the child
    /// (closing the write-end of its stdout pipe), which causes `read_line` to
    /// return EOF; the reader thread then exits cleanly.  `child.wait()` reaps
    /// the process, leaving no zombies.
    #[test]
    fn silent_upstream_times_out() {
        let config = UpstreamMcpConfig {
            name: "silent".into(),
            command: "sleep".into(),
            args: vec!["60".into()],
        };
        let timeout = Duration::from_secs(2);
        let start = Instant::now();

        let result = upstream_request_with_timeout(&config, "tools/list", json!({}), timeout);

        let elapsed = start.elapsed();

        assert!(
            result.is_err(),
            "expected an error from the silent upstream"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("timed out"),
            "error message should mention timeout, got: {msg}"
        );
        // The call should finish promptly after the timeout fires — allow a
        // generous 3 × margin for slow CI machines, but it must never hang.
        assert!(
            elapsed < Duration::from_secs(6),
            "call took too long ({elapsed:?}); possible hang — timeout not working"
        );
    }

    /// An upstream that emits an infinite stream of valid JSON-RPC frames but
    /// with IDs that never match must also time out (the `read_response` loop
    /// would spin forever on mismatched IDs without a global deadline).
    ///
    /// We use a shell one-liner that spins writing framed responses with
    /// id=99 — none will match the expected id=1 or id=2.
    #[test]
    fn mismatched_id_upstream_times_out() {
        // Each frame: Content-Length header + blank line + JSON body.
        // id=99 never matches the 1 or 2 we wait for, so the loop would run
        // forever without the timeout.
        let frame = r#"{"jsonrpc":"2.0","id":99,"result":{}}"#;
        let script = format!(
            "while true; do printf 'Content-Length: {}\\r\\n\\r\\n{}'; done",
            frame.len(),
            frame
        );
        let config = UpstreamMcpConfig {
            name: "wrongid".into(),
            command: "sh".into(),
            args: vec!["-c".into(), script],
        };
        let timeout = Duration::from_secs(2);
        let start = Instant::now();

        let result = upstream_request_with_timeout(&config, "tools/list", json!({}), timeout);

        let elapsed = start.elapsed();

        assert!(
            result.is_err(),
            "expected an error from the wrong-id upstream"
        );
        assert!(
            elapsed < Duration::from_secs(6),
            "call took too long ({elapsed:?}); possible hang"
        );
    }

    /// Reaping check (grafted from the Codex head-to-head implementation): after
    /// a timeout, the spawned upstream child must be killed AND reaped — no
    /// zombie / fd leak. The "upstream" is a shell script that records its PID
    /// (which survives the `exec sleep`) so the test can assert the process is
    /// actually gone, not just that an error was returned.
    #[test]
    #[cfg(unix)]
    fn timed_out_upstream_child_is_reaped() {
        let unique = format!(
            "harness-mcp-reap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let script_path = std::env::temp_dir().join(format!("{unique}.sh"));
        let pid_path = std::env::temp_dir().join(format!("{unique}.pid"));
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" > '{}'\nexec sleep 30\n",
            pid_path.display()
        );
        std::fs::write(&script_path, script).unwrap();

        let config = UpstreamMcpConfig {
            name: "reap".into(),
            command: "sh".into(),
            args: vec![script_path.display().to_string()],
        };

        let result = upstream_request_with_timeout(
            &config,
            "tools/list",
            json!({}),
            Duration::from_millis(250),
        );
        assert!(result.is_err(), "expected a timeout error");

        let pid = std::fs::read_to_string(&pid_path)
            .unwrap()
            .trim()
            .to_string();
        let alive = std::process::Command::new("sh")
            .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        assert!(!alive, "upstream child {pid} still alive (not reaped)");

        let _ = std::fs::remove_file(&script_path);
        let _ = std::fs::remove_file(&pid_path);
    }
}
