//! `tools/call` handlers for the `session_*` family (Zeus session tree).
//!
//! These tools let an orchestrator session ask the harness to spawn / list /
//! cancel CHILD sessions under it. The MCP server itself doesn't spawn
//! anything — it delegates to the harness-server REST surface, which is the
//! only component holding the `Manager` and the binary discovery map.
//!
//! The current session id is bound at MCP-server start via `--session-id`
//! and stored on the dispatcher; we never trust the caller to pass it.

use std::time::Duration;

use serde_json::{json, Value};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// `session_spawn_child` — ask the harness to create a child session under
/// the current session. Used by Zeus to delegate role-specific work.
pub fn spawn_child(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let parent_sid = session_id.ok_or_else(|| {
        "session.spawn_child requires the MCP server to know its parent \
         session id; spawn it with --session-id"
            .to_string()
    })?;
    let server = server_url.ok_or_else(|| "session.spawn_child needs --server-url".to_string())?;

    let kind = str_arg(args, "kind")?;
    let role = str_arg(args, "role")?;
    let initial_prompt = str_arg(args, "initial_prompt")?;
    let cwd = opt_str(args, "working_dir");
    let model = opt_str(args, "model");

    let url = format!(
        "{}/api/sessions/{}/children",
        server.trim_end_matches('/'),
        parent_sid
    );
    let body = json!({
        "kind": kind,
        "role": role,
        "initial_prompt": initial_prompt,
        "cwd": cwd,
        "model": model,
    });
    let mut req = ureq::post(&url).timeout(Duration::from_secs(10));
    if let Some(token) = api_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    req.send_json(&body)
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// `session_list_children` — direct children of the current session.
pub fn list_children(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
) -> Result<Value, String> {
    let parent_sid =
        session_id.ok_or_else(|| "session.list_children requires --session-id".to_string())?;
    let server =
        server_url.ok_or_else(|| "session.list_children needs --server-url".to_string())?;
    let url = format!(
        "{}/api/sessions/{}/children",
        server.trim_end_matches('/'),
        parent_sid
    );
    let mut req = ureq::get(&url).timeout(Duration::from_secs(5));
    if let Some(token) = api_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// `session_send_input` — write raw input bytes into a descendant session's
/// PTY. Lets the orchestrator unstick a worker that's waiting for Enter, or
/// type a follow-up message. The server validates that the target is inside
/// the caller's session tree before forwarding to the input endpoint.
pub fn send_input(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let parent_sid =
        session_id.ok_or_else(|| "session.send_input requires --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "session.send_input needs --server-url".to_string())?;
    let child_sid = str_arg(args, "child_session_id")?;
    let text = str_arg(args, "text")?;
    // The harness `/sessions/:sid/input` endpoint is binary; we POST the raw
    // bytes directly. Use a helper proxy route on the server so we can keep
    // the tree-guard centralised.
    let url = format!(
        "{}/api/sessions/{}/children/{}/input",
        server.trim_end_matches('/'),
        parent_sid,
        child_sid
    );
    let mut req = ureq::post(&url)
        .timeout(Duration::from_secs(5))
        .set("Content-Type", "application/octet-stream");
    if let Some(token) = api_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    req.send_bytes(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "bytes": text.len() }))
}

/// `session_cancel_child` — kill a descendant session of the current one.
/// The server validates that the target is actually in our tree.
pub fn cancel_child(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let parent_sid =
        session_id.ok_or_else(|| "session.cancel_child requires --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "session.cancel_child needs --server-url".to_string())?;
    let child_sid = str_arg(args, "child_session_id")?;
    let reason = opt_str(args, "reason");
    let url = format!(
        "{}/api/sessions/{}/children/{}",
        server.trim_end_matches('/'),
        parent_sid,
        child_sid
    );
    let mut req = ureq::delete(&url).timeout(Duration::from_secs(5));
    if let Some(token) = api_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    if let Some(r) = reason {
        req = req.set("X-Cancel-Reason", r);
    }
    req.call().map_err(|e| e.to_string())?;
    Ok(json!({ "cancelled": child_sid }))
}

/// `session_read_child_summary` — fetch summary/status of a child session.
/// Today this is a thin proxy over `GET /api/sessions/:sid` so the caller
/// gets meta + status; richer "handoff summary" parsing lands with F3.
pub fn read_child_summary(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let _parent_sid =
        session_id.ok_or_else(|| "session.read_child_summary requires --session-id".to_string())?;
    let server =
        server_url.ok_or_else(|| "session.read_child_summary needs --server-url".to_string())?;
    let child_sid = str_arg(args, "child_session_id")?;
    let url = format!(
        "{}/api/sessions/{}",
        server.trim_end_matches('/'),
        child_sid
    );
    let mut req = ureq::get(&url).timeout(Duration::from_secs(5));
    if let Some(token) = api_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}
