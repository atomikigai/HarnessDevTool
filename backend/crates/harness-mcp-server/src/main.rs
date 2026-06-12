//! Standalone MCP stdio server exposing Harness task/spec/skills tools to
//! agent CLIs (`claude --mcp-config ...`).
//!
//! Transport: MCP stdio (`Content-Length`) or legacy line-delimited JSON-RPC 2.0.
//! Logging:   `tracing` to **stderr only** — stdout is the MCP wire.
//!
//! Usage:
//!   harness-mcp-server --thread <tid> --agent-id <aid> --harness-home <path>

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{json, Value};
use tracing::{debug, error, info};

mod dispatcher;
mod gateway;
mod protocol;
mod tools;

use crate::dispatcher::Dispatcher;
use crate::protocol::{error_response, parse_request, RpcError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WireMode {
    ContentLength,
    JsonLine,
}

fn trim_line_end(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

fn parse_content_length(header: &str) -> Option<usize> {
    let (name, value) = header.split_once(':')?;
    if !name.eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse().ok()
}

fn read_wire_message<R: BufRead>(reader: &mut R) -> std::io::Result<Option<(String, WireMode)>> {
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }

        let header = trim_line_end(&line);
        if header.trim().is_empty() {
            continue;
        }

        let Some(len) = parse_content_length(header) else {
            return Ok(Some((header.to_string(), WireMode::JsonLine)));
        };

        loop {
            let mut header_line = String::new();
            if reader.read_line(&mut header_line)? == 0 {
                return Ok(None);
            }
            if trim_line_end(&header_line).is_empty() {
                break;
            }
        }

        let mut buf = vec![0_u8; len];
        reader.read_exact(&mut buf)?;
        let payload = String::from_utf8(buf).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid utf-8: {e}"),
            )
        })?;
        return Ok(Some((payload, WireMode::ContentLength)));
    }
}

fn write_wire_message<W: Write>(out: &mut W, mode: WireMode, payload: &str) -> std::io::Result<()> {
    match mode {
        WireMode::JsonLine => {
            writeln!(out, "{payload}")?;
        }
        WireMode::ContentLength => {
            write!(out, "Content-Length: {}\r\n\r\n", payload.len())?;
            out.write_all(payload.as_bytes())?;
        }
    }
    out.flush()
}

#[derive(Debug)]
struct CliArgs {
    thread_id: String,
    agent_id: String,
    /// Pre-minted session id owning this MCP server child. Lets
    /// `session.spawn_child` attribute spawns to the right parent.
    session_id: Option<String>,
    harness_home: PathBuf,
    profile: String,
    /// Optional base URL of the harness HTTP server (e.g. `http://127.0.0.1:8787`).
    /// When set, `task_create` delegates to `POST /api/threads/:tid/tasks` so the
    /// in-process broadcast bus emits `task.created` and SSE consumers see the
    /// new task. Without it, `task_create` falls back to a direct filesystem
    /// write but cannot notify the HTTP server (sessions panel will lag).
    server_url: Option<String>,
    /// Workspace root this MCP instance may inspect.
    cwd: PathBuf,
    /// Shared backend API token passed by the trusted harness-server parent.
    api_token: Option<String>,
    /// Role label for minimal task_create gating. Omitted means legacy
    /// permissive behavior.
    role: Option<String>,
    /// Trusted task id passed by harness-server when this MCP instance is
    /// scoped to a task.
    task_id: Option<String>,
    /// Trusted resource scopes granted to this MCP instance.
    scopes: Vec<String>,
    /// Optional gateway upstreams granted by the smart capability loader.
    upstream_config: Option<PathBuf>,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_args_from(args)
}

fn parse_args_from(args: Vec<String>) -> Result<CliArgs, String> {
    let mut thread_id: Option<String> = None;
    let mut agent_id: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut harness_home: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut server_url: Option<String> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut api_token: Option<String> = None;
    let mut role: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut scopes: Vec<String> = Vec::new();
    let mut upstream_config: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let next = |i: usize| -> Result<&String, String> {
            args.get(i + 1)
                .ok_or_else(|| format!("flag {a} requires an argument"))
        };
        match a.as_str() {
            "--thread" | "--thread-id" => {
                thread_id = Some(next(i)?.clone());
                i += 2;
            }
            "--agent-id" => {
                agent_id = Some(next(i)?.clone());
                i += 2;
            }
            "--session-id" => {
                session_id = Some(next(i)?.clone());
                i += 2;
            }
            "--harness-home" => {
                harness_home = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--profile" => {
                profile = Some(next(i)?.clone());
                i += 2;
            }
            "--server-url" => {
                server_url = Some(next(i)?.clone());
                i += 2;
            }
            "--cwd" => {
                cwd = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--api-token" => {
                api_token = Some(next(i)?.clone());
                i += 2;
            }
            "--role" => {
                role = Some(next(i)?.clone());
                i += 2;
            }
            "--task-id" => {
                task_id = Some(next(i)?.clone());
                i += 2;
            }
            "--scope" => {
                scopes.push(next(i)?.clone());
                i += 2;
            }
            "--upstream-config" => {
                upstream_config = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: harness-mcp-server --thread <tid> --agent-id <aid> [--session-id <sid>] --harness-home <path> [--profile <profile>] [--server-url <url>] [--cwd <path>] [--api-token <token>] [--role <role>] [--task-id <task>] [--scope <scope>...] [--upstream-config <path>]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(CliArgs {
        thread_id: thread_id.ok_or_else(|| "missing --thread".to_string())?,
        agent_id: agent_id.ok_or_else(|| "missing --agent-id".to_string())?,
        session_id,
        harness_home: harness_home.ok_or_else(|| "missing --harness-home".to_string())?,
        profile: profile.unwrap_or_else(|| "default".to_string()),
        server_url,
        cwd: cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        api_token,
        role,
        task_id,
        scopes,
        upstream_config,
    })
}

fn main() -> ExitCode {
    // Initialize tracing to stderr (never stdout — stdout is JSON-RPC wire).
    let filter = tracing_subscriber::EnvFilter::try_from_env("HARNESS_MCP_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            error!(error = %e, "argument error");
            return ExitCode::from(2);
        }
    };
    info!(
        thread = %args.thread_id,
        agent = %args.agent_id,
        home = %args.harness_home.display(),
        profile = %args.profile,
        "harness-mcp-server starting"
    );

    let dispatcher = match Dispatcher::new_with_server(
        args.harness_home.clone(),
        args.thread_id.clone(),
        args.agent_id.clone(),
        args.session_id.clone(),
        args.profile.clone(),
        args.server_url.clone(),
        args.cwd.clone(),
        args.api_token.clone(),
        args.role.clone(),
        args.task_id.clone(),
        args.scopes.clone(),
        args.upstream_config.clone(),
    ) {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "failed to init dispatcher");
            return ExitCode::from(1);
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut reader = BufReader::new(stdin.lock());

    'read_loop: loop {
        let (message, mode) = match read_wire_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(e) => {
                error!(error = %e, "stdin read error");
                break;
            }
        };
        let trimmed = message.trim();
        if trimmed.is_empty() {
            continue;
        }
        debug!(line = %trimmed, "recv");

        let response = match parse_request(trimmed) {
            Ok(req) => dispatcher.handle(req),
            Err((id, err)) => Some(error_response(id, err)),
        };

        if let Some(resp) = response {
            let s = match serde_json::to_string(&resp) {
                Ok(s) => s,
                Err(e) => {
                    error!(error = %e, "failed to serialize response");
                    let fb = json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": {"code": -32603, "message": "internal error"}
                    });
                    fb.to_string()
                }
            };
            debug!(line = %s, "send");
            if write_wire_message(&mut out, mode, &s).is_err() {
                break 'read_loop;
            }
        }
        for notification in dispatcher.drain_notifications() {
            let s = match serde_json::to_string(&notification) {
                Ok(s) => s,
                Err(e) => {
                    error!(error = %e, "failed to serialize notification");
                    continue;
                }
            };
            debug!(line = %s, "send notification");
            if write_wire_message(&mut out, mode, &s).is_err() {
                break 'read_loop;
            }
        }
    }

    info!("harness-mcp-server exiting");
    let _ = RpcError::InvalidRequest; // keep error variants used
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{parse_args_from, read_wire_message, write_wire_message, WireMode};
    use std::io::Cursor;

    #[test]
    fn reads_legacy_json_line_message() {
        let mut input = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
        let (payload, mode) = read_wire_message(&mut input).unwrap().unwrap();
        assert_eq!(mode, WireMode::JsonLine);
        assert_eq!(
            payload,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}"
        );
    }

    #[test]
    fn reads_content_length_framed_message() {
        let body = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}";
        let wire = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut input = Cursor::new(wire.into_bytes());
        let (payload, mode) = read_wire_message(&mut input).unwrap().unwrap();
        assert_eq!(mode, WireMode::ContentLength);
        assert_eq!(payload, body);
    }

    #[test]
    fn writes_content_length_framed_message() {
        let body = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}";
        let mut out = Vec::new();
        write_wire_message(&mut out, WireMode::ContentLength, body).unwrap();
        let wire = String::from_utf8(out).unwrap();
        assert_eq!(
            wire,
            format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
        );
    }

    #[test]
    fn server_url_is_absent_without_trusted_cli_flag() {
        let args = parse_args_from(vec![
            "--thread".into(),
            "thread-1".into(),
            "--agent-id".into(),
            "agent:codex-1".into(),
            "--harness-home".into(),
            "/tmp/harness".into(),
        ])
        .unwrap();

        assert_eq!(args.server_url, None);
    }

    #[test]
    fn server_url_comes_from_trusted_cli_flag() {
        let args = parse_args_from(vec![
            "--thread".into(),
            "thread-1".into(),
            "--agent-id".into(),
            "agent:codex-1".into(),
            "--harness-home".into(),
            "/tmp/harness".into(),
            "--server-url".into(),
            "http://127.0.0.1:7777".into(),
        ])
        .unwrap();

        assert_eq!(args.server_url.as_deref(), Some("http://127.0.0.1:7777"));
    }

    #[test]
    fn api_token_comes_from_trusted_cli_flag() {
        let args = parse_args_from(vec![
            "--thread".into(),
            "thread-1".into(),
            "--agent-id".into(),
            "agent:codex-1".into(),
            "--harness-home".into(),
            "/tmp/harness".into(),
            "--api-token".into(),
            "secret".into(),
        ])
        .unwrap();

        assert_eq!(args.api_token.as_deref(), Some("secret"));
    }

    #[test]
    fn task_scope_comes_from_trusted_cli_flags() {
        let args = parse_args_from(vec![
            "--thread".into(),
            "thread-1".into(),
            "--agent-id".into(),
            "agent:codex-1".into(),
            "--harness-home".into(),
            "/tmp/harness".into(),
            "--task-id".into(),
            "T-0001".into(),
            "--scope".into(),
            "task:T-0001".into(),
            "--scope".into(),
            "frontend".into(),
        ])
        .unwrap();

        assert_eq!(args.task_id.as_deref(), Some("T-0001"));
        assert_eq!(args.scopes, vec!["task:T-0001", "frontend"]);
    }

    #[test]
    fn upstream_config_comes_from_trusted_cli_flag() {
        let args = parse_args_from(vec![
            "--thread".into(),
            "thread-1".into(),
            "--agent-id".into(),
            "agent:codex-1".into(),
            "--harness-home".into(),
            "/tmp/harness".into(),
            "--upstream-config".into(),
            "/tmp/upstreams.json".into(),
        ])
        .unwrap();

        assert_eq!(
            args.upstream_config.as_deref(),
            Some(std::path::Path::new("/tmp/upstreams.json"))
        );
    }
}
