//! Standalone MCP stdio server exposing Harness task/spec/skills tools to
//! agent CLIs (`claude --mcp-config ...`).
//!
//! Transport: line-delimited JSON-RPC 2.0 on stdin/stdout.
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
mod protocol;
mod tools;

use crate::dispatcher::Dispatcher;
use crate::protocol::{error_response, parse_request, RpcError};

#[derive(Debug)]
struct CliArgs {
    thread_id: String,
    agent_id: String,
    harness_home: PathBuf,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut thread_id: Option<String> = None;
    let mut agent_id: Option<String> = None;
    let mut harness_home: Option<PathBuf> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
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
            "--harness-home" => {
                harness_home = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: harness-mcp-server --thread <tid> --agent-id <aid> --harness-home <path>"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(CliArgs {
        thread_id: thread_id.ok_or_else(|| "missing --thread".to_string())?,
        agent_id: agent_id.ok_or_else(|| "missing --agent-id".to_string())?,
        harness_home: harness_home.ok_or_else(|| "missing --harness-home".to_string())?,
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
        "harness-mcp-server starting"
    );

    let dispatcher = match Dispatcher::new(
        args.harness_home.clone(),
        args.thread_id.clone(),
        args.agent_id.clone(),
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
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!(error = %e, "stdin read error");
                break;
            }
        };
        let trimmed = line.trim();
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
            if writeln!(out, "{s}").is_err() {
                break;
            }
            if out.flush().is_err() {
                break;
            }
        }
    }

    info!("harness-mcp-server exiting");
    let _ = RpcError::InvalidRequest; // keep error variants used
    ExitCode::SUCCESS
}
