//! `tools/call` handlers for the `task_*` family.

use std::time::Duration;

use serde_json::{json, Value};

use std::str::FromStr;

use harness_core::{Artifacts, ClaimResult, ListFilters, TaskPatch, TaskStatus, TaskStore};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn map_err(e: harness_core::Error) -> String {
    e.to_string()
}

pub fn list(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = opt_str(args, "thread_id").unwrap_or(default_thread);
    let status = match opt_str(args, "status") {
        Some(s) => Some(TaskStatus::from_str(s).map_err(|e| format!("bad status: {e}"))?),
        None => None,
    };
    let filters = ListFilters {
        status,
        label: opt_str(args, "label").map(String::from),
        assignee: opt_str(args, "assignee").map(String::from),
    };
    let tasks = store.list(thread_id, filters).map_err(map_err)?;
    Ok(json!(tasks))
}

pub fn get(store: &TaskStore, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let t = store.get(thread_id, task_id).map_err(map_err)?;
    Ok(json!(t))
}

pub fn claim(store: &TaskStore, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let agent_id = str_arg(args, "agent_id")?;
    let ttl_s = args
        .get("ttl_s")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);
    match store
        .claim(thread_id, task_id, agent_id, Duration::from_secs(ttl_s))
        .map_err(map_err)?
    {
        ClaimResult::Granted(lease) => Ok(json!({ "ok": true, "lease": lease })),
        ClaimResult::Busy { holder, until } => Ok(json!({
            "ok": false,
            "busy_holder": holder,
            "busy_until": until,
        })),
    }
}

pub fn renew(store: &TaskStore, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let agent_id = str_arg(args, "agent_id")?;
    let lease = store
        .renew(thread_id, task_id, agent_id)
        .map_err(map_err)?;
    Ok(json!({ "lease": lease }))
}

pub fn update(store: &TaskStore, agent_id: &str, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let patch_v = args
        .get("patch")
        .ok_or_else(|| "missing arg: patch".to_string())?;
    let patch: TaskPatch = serde_json::from_value(patch_v.clone())
        .map_err(|e| format!("invalid patch: {e}"))?;
    let t = store
        .patch(thread_id, task_id, patch, agent_id)
        .map_err(map_err)?;
    Ok(json!(t))
}

pub fn release(store: &TaskStore, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let agent_id = str_arg(args, "agent_id")?;
    store
        .release(thread_id, task_id, agent_id)
        .map_err(map_err)?;
    Ok(json!({ "ok": true }))
}

pub fn submit(store: &TaskStore, agent_id: &str, args: &Value) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    let task_id = str_arg(args, "task_id")?;
    let artifacts_v = args
        .get("artifacts")
        .ok_or_else(|| "missing arg: artifacts".to_string())?;
    let artifacts: Artifacts = serde_json::from_value(artifacts_v.clone())
        .map_err(|e| format!("invalid artifacts: {e}"))?;
    let t = store
        .submit(thread_id, task_id, artifacts, agent_id)
        .map_err(map_err)?;
    Ok(json!(t))
}
