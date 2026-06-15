use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
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
const PERSISTENT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamMcpConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub persistent: bool,
    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Gateway {
    upstreams: Vec<UpstreamMcpConfig>,
    persistent: Arc<Mutex<HashMap<String, PersistentHandle>>>,
}

#[derive(Debug, Clone)]
struct PersistentHandle {
    tx: mpsc::Sender<PersistentRequest>,
}

#[derive(Debug)]
struct PersistentRequest {
    method: String,
    params: Value,
    reply: mpsc::Sender<Result<Value, String>>,
}

impl Gateway {
    pub fn new(upstreams: Vec<UpstreamMcpConfig>) -> Self {
        Self {
            upstreams,
            persistent: Arc::new(Mutex::new(HashMap::new())),
        }
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

    pub fn list_descriptors_for(&self, names: &[&str]) -> Vec<ToolDescriptor> {
        let mut out = Vec::new();
        for upstream in &self.upstreams {
            if !names.iter().any(|name| *name == upstream.name) {
                continue;
            }
            match self.request(upstream, "tools/list", json!({})) {
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
        self.request(
            upstream,
            "tools/call",
            json!({
                "name": inner_name,
                "arguments": args,
            }),
        )
    }

    fn request(
        &self,
        upstream: &UpstreamMcpConfig,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        if upstream.persistent {
            self.persistent_request(upstream, method, params)
        } else {
            upstream_request(upstream, method, params)
        }
    }

    fn persistent_request(
        &self,
        upstream: &UpstreamMcpConfig,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            let handle = self.persistent_handle(upstream);
            let (reply_tx, reply_rx) = mpsc::channel();
            let request = PersistentRequest {
                method: method.to_string(),
                params: params.clone(),
                reply: reply_tx,
            };
            if handle.tx.send(request).is_err() {
                self.forget_persistent(&upstream.name);
                if attempts < 2 {
                    continue;
                }
                return Err(format!(
                    "persistent upstream {} is unavailable",
                    upstream.name
                ));
            }
            match reply_rx.recv_timeout(UPSTREAM_TIMEOUT) {
                Ok(result) => return result,
                Err(_) => {
                    self.forget_persistent(&upstream.name);
                    return Err(format!(
                        "persistent upstream {} timed out after {}s",
                        upstream.name,
                        UPSTREAM_TIMEOUT.as_secs()
                    ));
                }
            }
        }
    }

    fn persistent_handle(&self, upstream: &UpstreamMcpConfig) -> PersistentHandle {
        let mut handles = self.persistent.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = handles.get(&upstream.name) {
            return handle.clone();
        }
        let handle = spawn_persistent_worker(upstream.clone());
        handles.insert(upstream.name.clone(), handle.clone());
        handle
    }

    fn forget_persistent(&self, name: &str) {
        self.persistent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
    }
}

impl UpstreamMcpConfig {
    fn persistent_idle_timeout(&self) -> Duration {
        self.idle_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(PERSISTENT_IDLE_TIMEOUT)
    }
}

fn spawn_persistent_worker(upstream: UpstreamMcpConfig) -> PersistentHandle {
    let (tx, rx) = mpsc::channel::<PersistentRequest>();
    std::thread::spawn(move || {
        persistent_worker(upstream, rx);
    });
    PersistentHandle { tx }
}

fn persistent_worker(upstream: UpstreamMcpConfig, rx: mpsc::Receiver<PersistentRequest>) {
    let idle_timeout = upstream.persistent_idle_timeout();
    let mut process = match PersistentProcess::start(&upstream, UPSTREAM_TIMEOUT) {
        Ok(process) => process,
        Err(e) => {
            while let Ok(request) = rx.recv() {
                let _ = request.reply.send(Err(e.clone()));
            }
            return;
        }
    };

    loop {
        let request = match rx.recv_timeout(idle_timeout) {
            Ok(request) => request,
            Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                process.shutdown();
                return;
            }
        };
        let result = process.call(&request.method, &request.params, UPSTREAM_TIMEOUT);
        let fatal = result.as_ref().is_err_and(|e| {
            e.contains("timed out")
                || e.contains("closed")
                || e.contains("read upstream")
                || e.contains("write MCP frame")
        });
        let _ = request.reply.send(result);
        if fatal {
            process.shutdown();
            return;
        }
    }
}

struct PersistentProcess {
    name: String,
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: i64,
}

impl PersistentProcess {
    fn start(upstream: &UpstreamMcpConfig, timeout: Duration) -> Result<Self, String> {
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
        let child = Arc::new(Mutex::new(child));
        let mut process = Self {
            name: upstream.name.clone(),
            child,
            stdin,
            reader: BufReader::new(stdout),
            next_id: 2,
        };
        process.initialize(timeout)?;
        Ok(process)
    }

    fn initialize(&mut self, timeout: Duration) -> Result<(), String> {
        let child = self.child.clone();
        let name = self.name.clone();
        with_child_timeout(child, &name, timeout, || {
            write_framed(
                &mut self.stdin,
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
            let _ = read_response(&mut self.reader, 1)?;
            write_framed(
                &mut self.stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }),
            )
        })
    }

    fn call(&mut self, method: &str, params: &Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let child = self.child.clone();
        let name = self.name.clone();
        with_child_timeout(child, &name, timeout, || {
            write_framed(
                &mut self.stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params,
                }),
            )?;
            read_response(&mut self.reader, id)
        })
    }

    fn shutdown(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn with_child_timeout<T>(
    child: Arc<Mutex<Child>>,
    upstream_name: &str,
    timeout: Duration,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let name = upstream_name.to_string();
    std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
            let _ = child.kill();
        }
    });
    let result = operation();
    let _ = done_tx.send(());
    result.map_err(|e| {
        if e.contains("closed") || e.contains("read upstream") {
            format!("persistent upstream {name} timed out or closed: {e}")
        } else {
            e
        }
    })
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
            persistent: false,
            idle_timeout_ms: None,
        }]);

        let (upstream, inner) = gateway.prefixed_tool("crawl4ai__crawl").unwrap();
        assert_eq!(upstream.name, "crawl4ai");
        assert_eq!(inner, "crawl");
        assert!(gateway.prefixed_tool("other__crawl").is_none());
        assert!(gateway.prefixed_tool("crawl4ai__").is_none());
    }

    #[test]
    fn persistent_upstream_reuses_initialized_process() {
        let unique = format!(
            "harness-mcp-persistent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let script_path = std::env::temp_dir().join(format!("{unique}.sh"));
        let script = r#"#!/bin/sh
init='{"jsonrpc":"2.0","id":1,"result":{}}'
first='{"jsonrpc":"2.0","id":2,"result":{"marker":"first"}}'
second='{"jsonrpc":"2.0","id":3,"result":{"marker":"second"}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#init}" "$init"
printf 'Content-Length: %s\r\n\r\n%s' "${#first}" "$first"
printf 'Content-Length: %s\r\n\r\n%s' "${#second}" "$second"
sleep 5
"#;
        std::fs::write(&script_path, script).unwrap();

        let gateway = Gateway::new(vec![UpstreamMcpConfig {
            name: "mock".into(),
            command: "sh".into(),
            args: vec![script_path.display().to_string()],
            persistent: true,
            idle_timeout_ms: Some(50),
        }]);

        let first = gateway.call("mock__first", json!({})).unwrap();
        let second = gateway.call("mock__second", json!({})).unwrap();

        assert_eq!(first["marker"], "first");
        assert_eq!(second["marker"], "second");

        let _ = std::fs::remove_file(&script_path);
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
            persistent: false,
            idle_timeout_ms: None,
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
            persistent: false,
            idle_timeout_ms: None,
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
            persistent: false,
            idle_timeout_ms: None,
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
