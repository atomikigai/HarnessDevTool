//! Azure CLI MCP tools. These wrap the host/container `az` binary without a shell.

use std::process::Command;

use serde_json::{json, Value};

const MAX_OUTPUT_BYTES: usize = 50 * 1024;
const MUTATING_TOKENS: &[&str] = &[
    "create",
    "update",
    "delete",
    "remove",
    "set",
    "unset",
    "start",
    "stop",
    "restart",
    "scale",
    "deploy",
    "deployment",
    "assign",
    "login",
    "logout",
    "get-credentials",
];

pub fn status() -> Result<Value, String> {
    match run_az(&["version", "--output", "json"]) {
        Ok(outcome) => Ok(json!({
            "installed": true,
            "command": outcome.command,
            "ok": outcome.ok,
            "code": outcome.code,
            "version": parse_json_or_text(&outcome.stdout),
            "stderr": outcome.stderr,
            "truncated": outcome.truncated,
        })),
        Err(err) if err.contains("not found") => Ok(json!({
            "installed": false,
            "ok": false,
            "error": err,
            "install_hint": "Install Azure CLI (`az`) in the host/container and authenticate with `az login` or workload identity before using azure tools."
        })),
        Err(err) => Err(err),
    }
}

pub fn account(args: &Value) -> Result<Value, String> {
    let list = args.get("list").and_then(Value::as_bool).unwrap_or(false);
    let argv: &[&str] = if list {
        &["account", "list", "--output", "json"]
    } else {
        &["account", "show", "--output", "json"]
    };
    let outcome = run_az(argv)?;
    Ok(json!({
        "command": outcome.command,
        "ok": outcome.ok,
        "code": outcome.code,
        "account": parse_json_or_text(&outcome.stdout),
        "stderr": outcome.stderr,
        "truncated": outcome.truncated,
    }))
}

pub fn cli(args: &Value) -> Result<Value, String> {
    let argv = args
        .get("args")
        .and_then(Value::as_array)
        .ok_or_else(|| "args array is required".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "all args entries must be strings".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_argv(&argv)?;

    let allow_mutating = args
        .get("allow_mutating")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_mutating_command(&argv) && !allow_mutating {
        return Err(
            "Azure command looks mutating; pass allow_mutating=true only after the user has approved the exact command."
                .to_string(),
        );
    }

    let borrowed = argv.iter().map(String::as_str).collect::<Vec<_>>();
    let outcome = run_az(&borrowed)?;
    Ok(json!({
        "command": outcome.command,
        "ok": outcome.ok,
        "code": outcome.code,
        "stdout": parse_json_or_text(&outcome.stdout),
        "stderr": outcome.stderr,
        "truncated": outcome.truncated,
        "mutating": is_mutating_command(&argv),
    }))
}

fn validate_argv(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("args must not be empty".to_string());
    }
    if argv[0] == "az" {
        return Err(
            "args must not include the `az` binary; pass only arguments after it".to_string(),
        );
    }
    if argv[0].starts_with('-') {
        return Err("first Azure CLI argument must be a command group, not an option".to_string());
    }
    if argv.iter().any(|arg| arg.contains('\0')) {
        return Err("args must not contain NUL bytes".to_string());
    }
    Ok(())
}

fn is_mutating_command(argv: &[String]) -> bool {
    argv.iter().any(|arg| {
        let normalized = arg.trim().to_ascii_lowercase();
        MUTATING_TOKENS
            .iter()
            .any(|token| normalized == *token || normalized.ends_with(&format!(":{token}")))
    })
}

fn run_az(args: &[&str]) -> Result<CommandOutcome, String> {
    let output = Command::new("az")
        .args(args)
        .env("AZURE_CORE_NO_COLOR", "true")
        .env("AZURE_CORE_ONLY_SHOW_ERRORS", "true")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Azure CLI binary `az` not found on PATH".to_string()
            } else {
                format!("run az: {e}")
            }
        })?;
    let (stdout, stdout_truncated) = decode_limited(output.stdout);
    let (stderr, stderr_truncated) = decode_limited(output.stderr);
    Ok(CommandOutcome {
        command: std::iter::once("az".to_string())
            .chain(args.iter().map(|arg| (*arg).to_string()))
            .collect(),
        ok: output.status.success(),
        code: output.status.code(),
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn decode_limited(mut bytes: Vec<u8>) -> (String, bool) {
    let truncated = bytes.len() > MAX_OUTPUT_BYTES;
    if truncated {
        bytes.truncate(MAX_OUTPUT_BYTES);
    }
    (
        String::from_utf8_lossy(&bytes).trim().to_string(),
        truncated,
    )
}

fn parse_json_or_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

struct CommandOutcome {
    command: Vec<String>,
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
    truncated: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rejects_binary_and_empty_args() {
        assert!(validate_argv(&[]).is_err());
        assert!(validate_argv(&["az".to_string(), "account".to_string()]).is_err());
        assert!(validate_argv(&["account".to_string(), "show".to_string()]).is_ok());
    }

    #[test]
    fn detects_mutating_azure_commands() {
        assert!(!is_mutating_command(&[
            "group".to_string(),
            "list".to_string()
        ]));
        assert!(is_mutating_command(&[
            "group".to_string(),
            "delete".to_string()
        ]));
        assert!(is_mutating_command(&[
            "aks".to_string(),
            "get-credentials".to_string()
        ]));
    }

    #[test]
    fn cli_requires_explicit_mutating_approval() {
        let err = cli(&json!({ "args": ["group", "delete", "--name", "rg"] })).unwrap_err();
        assert!(err.contains("allow_mutating=true"));
    }
}
