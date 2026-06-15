//! `tools/call` handlers for the `session_*` family (Zeus session tree).
//!
//! These tools let an orchestrator session ask the harness to spawn / list /
//! cancel CHILD sessions under it. The MCP server itself doesn't spawn
//! anything — it delegates to the harness-server REST surface, which is the
//! only component holding the `Manager` and the binary discovery map.
//!
//! The current session id is bound at MCP-server start via `--session-id`
//! and stored on the dispatcher; we never trust the caller to pass it.

use std::path::Path;
use std::time::Duration;

use harness_core::TaskStore;
use harness_session::SessionMeta;
use serde_json::{json, Value};

const HARNESS_PROTOCOL_VERSION_HEADER: &str = "X-Protocol-Version";
const HARNESS_PROTOCOL_VERSION: &str = "1.0";

fn harness_request(req: ureq::Request, api_token: Option<&str>) -> ureq::Request {
    let req = req.set(HARNESS_PROTOCOL_VERSION_HEADER, HARNESS_PROTOCOL_VERSION);
    if let Some(token) = api_token {
        req.set("Authorization", &format!("Bearer {token}"))
    } else {
        req
    }
}

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

    let kind = opt_str(args, "kind");
    let role = str_arg(args, "role")?;
    let initial_prompt = str_arg(args, "initial_prompt")?;
    let cwd = opt_str(args, "working_dir");
    let task_id = opt_str(args, "task_id");
    let scopes = args
        .get("scopes")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let url = format!(
        "{}/api/sessions/{}/children",
        server.trim_end_matches('/'),
        parent_sid
    );
    let body = json!({
        "kind": kind,
        "role": role,
        "initial_prompt": initial_prompt,
        "task_id": task_id,
        "scopes": scopes,
        "cwd": cwd,
    });
    let req = harness_request(ureq::post(&url).timeout(Duration::from_secs(10)), api_token);
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
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
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
    let req = harness_request(
        ureq::post(&url)
            .timeout(Duration::from_secs(5))
            .set("Content-Type", "application/octet-stream"),
        api_token,
    );
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
    let mut req = harness_request(
        ureq::delete(&url).timeout(Duration::from_secs(5)),
        api_token,
    );
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
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// `session_mailbox_send` — append an auditable mailbox message for a
/// descendant session. Unlike `session_send_input`, this does not write to
/// the PTY; the worker can read/ack it through mailbox tools.
pub fn mailbox_send(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let parent_sid =
        session_id.ok_or_else(|| "session.mailbox_send requires --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "session.mailbox_send needs --server-url".to_string())?;
    let to_session_id = str_arg(args, "to_session_id")?;
    let body = str_arg(args, "body")?;
    let task_id = opt_str(args, "task_id");
    let scopes = args
        .get("scopes")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let url = format!(
        "{}/api/sessions/{}/mailbox",
        server.trim_end_matches('/'),
        parent_sid
    );
    let body = json!({
        "to_session_id": to_session_id,
        "body": body,
        "task_id": task_id,
        "scopes": scopes,
    });
    let req = harness_request(ureq::post(&url).timeout(Duration::from_secs(5)), api_token);
    req.send_json(&body)
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// `session_mailbox_list` — list messages addressed to the current session.
pub fn mailbox_list(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
) -> Result<Value, String> {
    let sid = session_id.ok_or_else(|| "session.mailbox_list requires --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "session.mailbox_list needs --server-url".to_string())?;
    let url = format!(
        "{}/api/sessions/{}/mailbox",
        server.trim_end_matches('/'),
        sid
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// `session_mailbox_ack` — append an ack event for a message addressed to the
/// current session.
pub fn mailbox_ack(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = session_id.ok_or_else(|| "session.mailbox_ack requires --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "session.mailbox_ack needs --server-url".to_string())?;
    let message_id = str_arg(args, "message_id")?;
    let url = format!(
        "{}/api/sessions/{}/mailbox/{}/ack",
        server.trim_end_matches('/'),
        sid,
        message_id
    );
    let req = harness_request(ureq::post(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn context_pack(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    current_session_id: Option<&str>,
    thread_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let session_id = opt_str(args, "session_id")
        .or(current_session_id)
        .ok_or_else(|| "session_context_pack requires session_id or --session-id".to_string())?;
    let meta = read_session_meta(harness_home, profile, session_id)?;
    let task_id = opt_str(args, "task_id")
        .map(str::to_string)
        .or_else(|| meta.task_id.clone());

    let task = task_id
        .as_deref()
        .and_then(|task_id| store.get(thread_id, task_id).ok())
        .map(|task| {
            json!({
                "id": task.id,
                "title": task.title,
                "status": task.status,
                "assignee": task.assignee,
                "labels": task.labels,
                "write_paths": task.write_paths,
                "forbidden_paths": task.forbidden_paths,
                "brief": task.brief,
                "updated_at": task.updated_at,
            })
        });
    let latest_handoff = task_id
        .as_deref()
        .and_then(|task_id| store.read_handoffs(thread_id, task_id).ok())
        .and_then(|handoffs| handoffs.into_iter().max_by_key(|handoff| handoff.at))
        .map(|handoff| {
            json!({
                "at": handoff.at,
                "from": handoff.from,
                "to_role": handoff.to_role,
                "status": handoff.status,
                "goal": handoff.goal,
                "blocked_on": handoff.blocked_on,
                "files_changed": handoff.files_changed,
                "commands_run": handoff.commands_run,
                "verification_passed": handoff.verification_passed,
                "verification_not_run": handoff.verification_not_run,
                "next_agent_action": handoff.next_agent_action,
            })
        });
    let children = read_session_metas(harness_home, profile)?
        .into_iter()
        .filter(|child| child.parent_session_id.as_deref() == Some(session_id))
        .map(|child| {
            json!({
                "session_id": child.id,
                "role": child.role,
                "task_id": child.task_id,
                "status": child.status,
                "detected_state": child.detected_state,
                "started_at": child.started_at,
            })
        })
        .collect::<Vec<_>>();
    let next_actions = compact_next_actions(latest_handoff.as_ref(), task.is_some());

    Ok(json!({
        "session": {
            "session_id": meta.id,
            "thread_id": meta.thread_id,
            "role": meta.role,
            "task_id": meta.task_id,
            "scopes": meta.scopes,
            "status": meta.status,
            "detected_state": meta.detected_state,
            "parent_session_id": meta.parent_session_id,
            "root_session_id": meta.root_session_id,
            "cwd": meta.cwd,
            "loaded_capabilities": meta.loaded_capabilities,
            "has_transcript": meta.has_transcript,
        },
        "task": task,
        "latest_handoff": latest_handoff,
        "children": children,
        "next_actions": next_actions,
    }))
}

pub fn context_status(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id")
        .or(session_id)
        .ok_or_else(|| "context_status requires session_id or --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "context_status needs --server-url".to_string())?;
    let url = format!(
        "{}/api/sessions/{}/context",
        server.trim_end_matches('/'),
        sid
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn context_search(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id")
        .or(session_id)
        .ok_or_else(|| "context_search requires session_id or --session-id".to_string())?;
    let query = str_arg(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|limit| limit.clamp(1, 50))
        .unwrap_or(10);
    let server = server_url.ok_or_else(|| "context_search needs --server-url".to_string())?;
    let url = format!(
        "{}/api/sessions/{}/context/search?q={}&limit={}",
        server.trim_end_matches('/'),
        sid,
        encode_query(query),
        limit
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn context_checkpoint_request(
    session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id").or(session_id).ok_or_else(|| {
        "context_checkpoint_request requires session_id or --session-id".to_string()
    })?;
    let server =
        server_url.ok_or_else(|| "context_checkpoint_request needs --server-url".to_string())?;
    let url = format!(
        "{}/api/sessions/{}/context/checkpoint",
        server.trim_end_matches('/'),
        sid
    );
    let req = harness_request(ureq::post(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn timeline_query(
    current_thread_id: &str,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = opt_str(args, "thread_id").unwrap_or(current_thread_id);
    let server = server_url.ok_or_else(|| "timeline_query needs --server-url".to_string())?;
    let mut params = Vec::<(String, String)>::new();
    if let Some(after) = args.get("after").and_then(Value::as_u64) {
        params.push(("after".into(), after.to_string()));
    }
    if let Some(limit) = args.get("limit").and_then(Value::as_u64) {
        params.push(("limit".into(), limit.clamp(1, 1000).to_string()));
    }
    for key in ["event_type", "actor", "task_id", "session_id"] {
        if let Some(value) = opt_str(args, key).filter(|value| !value.trim().is_empty()) {
            params.push((key.to_string(), value.to_string()));
        }
    }
    let query = opt_str(args, "q")
        .or_else(|| opt_str(args, "query"))
        .filter(|value| !value.trim().is_empty());
    if let Some(query) = query {
        params.push(("q".into(), query.to_string()));
    }

    let mut url = format!(
        "{}/api/threads/{}/timeline",
        server.trim_end_matches('/'),
        encode_query(thread_id)
    );
    if !params.is_empty() {
        let query = params
            .iter()
            .map(|(key, value)| format!("{}={}", encode_query(key), encode_query(value)))
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&query);
    }
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn transcript_query(
    current_session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id")
        .or(current_session_id)
        .ok_or_else(|| "transcript_query requires session_id or --session-id".to_string())?;
    let server = server_url.ok_or_else(|| "transcript_query needs --server-url".to_string())?;
    let mut params = Vec::<(String, String)>::new();
    push_u64_param(&mut params, args, "since");
    push_limited_param(&mut params, args, "limit", 1, 1000);
    push_string_param(&mut params, args, "kind");
    push_string_param(&mut params, args, "role");
    let url = url_with_params(
        &format!(
            "{}/api/sessions/{}/transcript/query",
            server.trim_end_matches('/'),
            encode_query(sid)
        ),
        &params,
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn transcript_search(
    current_session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id")
        .or(current_session_id)
        .ok_or_else(|| "transcript_search requires session_id or --session-id".to_string())?;
    let query = opt_str(args, "query")
        .or_else(|| opt_str(args, "q"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "transcript_search requires query".to_string())?;
    let server = server_url.ok_or_else(|| "transcript_search needs --server-url".to_string())?;
    let mut params = vec![("q".to_string(), query.to_string())];
    push_u64_param(&mut params, args, "since");
    push_limited_param(&mut params, args, "limit", 1, 200);
    push_string_param(&mut params, args, "kind");
    push_string_param(&mut params, args, "role");
    let url = url_with_params(
        &format!(
            "{}/api/sessions/{}/transcript/search",
            server.trim_end_matches('/'),
            encode_query(sid)
        ),
        &params,
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

pub fn transcript_tool_results(
    current_session_id: Option<&str>,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let sid = opt_str(args, "session_id")
        .or(current_session_id)
        .ok_or_else(|| "transcript_tool_results requires session_id or --session-id".to_string())?;
    let server =
        server_url.ok_or_else(|| "transcript_tool_results needs --server-url".to_string())?;
    let mut params = Vec::<(String, String)>::new();
    push_u64_param(&mut params, args, "since");
    push_limited_param(&mut params, args, "limit", 1, 200);
    push_string_param(&mut params, args, "tool_name");
    if let Some(errors_only) = args.get("errors_only").and_then(Value::as_bool) {
        params.push(("errors_only".into(), errors_only.to_string()));
    }
    let url = url_with_params(
        &format!(
            "{}/api/sessions/{}/transcript/tool-results",
            server.trim_end_matches('/'),
            encode_query(sid)
        ),
        &params,
    );
    let req = harness_request(ureq::get(&url).timeout(Duration::from_secs(5)), api_token);
    req.call()
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

fn read_session_meta(
    harness_home: &Path,
    profile: &str,
    session_id: &str,
) -> Result<SessionMeta, String> {
    let path = harness_home
        .join("profiles")
        .join(profile)
        .join("sessions")
        .join(session_id)
        .join("meta.json");
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_session_metas(harness_home: &Path, profile: &str) -> Result<Vec<SessionMeta>, String> {
    let dir = harness_home.join("profiles").join(profile).join("sessions");
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read_dir {}: {e}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in read.filter_map(Result::ok) {
        let path = entry.path().join("meta.json");
        if !path.exists() {
            continue;
        }
        match std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SessionMeta>(&bytes).ok())
        {
            Some(meta) => out.push(meta),
            None => tracing::warn!(path = %path.display(), "skipping unreadable session meta"),
        }
    }
    Ok(out)
}

fn compact_next_actions(latest_handoff: Option<&Value>, has_task: bool) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(action) = latest_handoff
        .and_then(|handoff| handoff.get("next_agent_action"))
        .and_then(Value::as_str)
        .filter(|action| !action.trim().is_empty())
    {
        out.push(action.to_string());
    }
    if has_task {
        out.push(
            "Use task_get for full task detail only if this compact pack is insufficient.".into(),
        );
    }
    if out.is_empty() {
        out.push("No handoff next action found; inspect mailbox or task state before broad transcript reads.".into());
    }
    out
}

fn encode_query(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn push_u64_param(params: &mut Vec<(String, String)>, args: &Value, key: &str) {
    if let Some(value) = args.get(key).and_then(Value::as_u64) {
        params.push((key.to_string(), value.to_string()));
    }
}

fn push_limited_param(
    params: &mut Vec<(String, String)>,
    args: &Value,
    key: &str,
    min: u64,
    max: u64,
) {
    if let Some(value) = args.get(key).and_then(Value::as_u64) {
        params.push((key.to_string(), value.clamp(min, max).to_string()));
    }
}

fn push_string_param(params: &mut Vec<(String, String)>, args: &Value, key: &str) {
    if let Some(value) = opt_str(args, key).filter(|value| !value.trim().is_empty()) {
        params.push((key.to_string(), value.to_string()));
    }
}

fn url_with_params(base: &str, params: &[(String, String)]) -> String {
    if params.is_empty() {
        return base.to_string();
    }
    let query = params
        .iter()
        .map(|(key, value)| format!("{}={}", encode_query(key), encode_query(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    #[test]
    fn spawn_child_without_kind_posts_protocol_header_and_null_kind() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };

        let result = spawn_child(
            Some("parent-1"),
            Some(&server_url),
            None,
            &json!({
                "role": "generator",
                "initial_prompt": "build the backend",
                "working_dir": "/tmp/work",
                "scopes": ["task:T-0001"]
            }),
        )
        .expect("spawn child response");

        assert_eq!(result["session_id"], "child-1");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with("POST /api/sessions/parent-1/children HTTP/1.1"));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
        let body = captured.split("\r\n\r\n").nth(1).expect("body");
        let body: Value = serde_json::from_str(body).expect("json body");
        assert_eq!(body["kind"], Value::Null);
        assert_eq!(body["role"], "generator");
        assert_eq!(body["initial_prompt"], "build the backend");
        assert_eq!(body["cwd"], "/tmp/work");
        assert_eq!(body["scopes"], json!(["task:T-0001"]));
    }

    #[test]
    fn context_pack_returns_session_meta_and_children() {
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("profiles/default/sessions");
        std::fs::create_dir_all(sessions.join("sid-parent")).unwrap();
        std::fs::create_dir_all(sessions.join("sid-child")).unwrap();
        std::fs::write(
            sessions.join("sid-parent/meta.json"),
            serde_json::to_vec(&json!({
                "id": "sid-parent",
                "kind": "codex",
                "thread_id": "thr",
                "cwd": ".",
                "pid": 0,
                "status": "running",
                "started_at": 1,
                "role": "orchestrator",
                "scopes": ["backend"],
                "root_session_id": "sid-parent",
                "loaded_capabilities": {
                    "mcp_servers": ["harness"],
                    "skills": [],
                    "tool_groups": ["planning"]
                },
                "has_transcript": true
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            sessions.join("sid-child/meta.json"),
            serde_json::to_vec(&json!({
                "id": "sid-child",
                "kind": "codex",
                "thread_id": "thr",
                "cwd": ".",
                "pid": 0,
                "status": "exited",
                "started_at": 2,
                "role": "generator",
                "parent_session_id": "sid-parent",
                "root_session_id": "sid-parent",
                "loaded_capabilities": {
                    "mcp_servers": [],
                    "skills": [],
                    "tool_groups": []
                },
                "has_transcript": false
            }))
            .unwrap(),
        )
        .unwrap();

        let store = TaskStore::with_profile(home.path(), "default").unwrap();
        let pack = context_pack(
            &store,
            home.path(),
            "default",
            Some("sid-parent"),
            "thr",
            &json!({}),
        )
        .unwrap();

        assert_eq!(pack["session"]["session_id"], "sid-parent");
        assert_eq!(
            pack["session"]["loaded_capabilities"]["tool_groups"][0],
            "planning"
        );
        assert!(pack["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| child["session_id"] == "sid-child"));
    }

    #[test]
    fn context_search_uses_protocol_header_and_encoded_query() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };

        let result = context_search(
            Some("sid-context"),
            Some(&server_url),
            None,
            &json!({ "query": "next action", "limit": 5 }),
        )
        .expect("context search response");

        assert_eq!(result["session_id"], "child-1");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with(
            "GET /api/sessions/sid-context/context/search?q=next+action&limit=5 HTTP/1.1"
        ));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
    }

    #[test]
    fn timeline_query_uses_protocol_header_and_encoded_filters() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };

        let result = timeline_query(
            "thr-current",
            Some(&server_url),
            None,
            &json!({
                "after": 10,
                "limit": 5,
                "event_type": "task.updated",
                "actor": "agent:codex",
                "task_id": "T-0001",
                "session_id": "sid-1",
                "query": "next action"
            }),
        )
        .expect("timeline query response");

        assert_eq!(result["session_id"], "child-1");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with(
            "GET /api/threads/thr-current/timeline?after=10&limit=5&event_type=task.updated&actor=agent%3Acodex&task_id=T-0001&session_id=sid-1&q=next+action HTTP/1.1"
        ));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
    }

    #[test]
    fn transcript_search_uses_protocol_header_and_encoded_filters() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };

        let result = transcript_search(
            Some("sid-current"),
            Some(&server_url),
            None,
            &json!({
                "query": "cargo test",
                "since": 7,
                "limit": 12,
                "kind": "tool_result",
                "role": "assistant"
            }),
        )
        .expect("transcript search response");

        assert_eq!(result["session_id"], "child-1");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with(
            "GET /api/sessions/sid-current/transcript/search?q=cargo+test&since=7&limit=12&kind=tool_result&role=assistant HTTP/1.1"
        ));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
    }

    #[test]
    fn transcript_tool_results_uses_protocol_header_and_filters() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };

        let result = transcript_tool_results(
            Some("sid-current"),
            Some(&server_url),
            None,
            &json!({
                "since": 3,
                "limit": 9,
                "tool_name": "shell.exec",
                "errors_only": true
            }),
        )
        .expect("transcript tool results response");

        assert_eq!(result["session_id"], "child-1");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with(
            "GET /api/sessions/sid-current/transcript/tool-results?since=3&limit=9&tool_name=shell.exec&errors_only=true HTTP/1.1"
        ));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
    }

    fn spawn_http_capture_server() -> Option<(String, mpsc::Receiver<String>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(e) => panic!("bind test server: {e}"),
        };
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = stream.read(&mut tmp).expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(header_end) = find_header_end(&buf) {
                    let headers = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
                    let content_len = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    let total = header_end + 4 + content_len;
                    while buf.len() < total {
                        let n = stream.read(&mut tmp).expect("read body");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    break;
                }
            }
            tx.send(String::from_utf8_lossy(&buf).to_string())
                .expect("send captured request");
            let response = concat!(
                "HTTP/1.1 201 Created\r\n",
                "Content-Type: application/json\r\n",
                "Content-Length: 24\r\n",
                "\r\n",
                "{\"session_id\":\"child-1\"}"
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        Some((format!("http://{addr}"), rx))
    }

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
