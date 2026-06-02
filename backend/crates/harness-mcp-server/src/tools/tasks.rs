//! `tools/call` handlers for the `task_*` family.

use std::time::Duration;

use serde_json::{json, Value};

use std::str::FromStr;

use harness_core::{
    validate_task_id, validate_thread_id, AcceptanceCheck, Artifacts, ClaimResult, ListFilters,
    TaskDraft, TaskPatch, TaskStatus, TaskStore,
};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn valid_thread_or_default<'a>(
    args: &'a Value,
    default_thread: &'a str,
) -> Result<&'a str, String> {
    let thread_id = opt_str(args, "thread_id").unwrap_or(default_thread);
    validate_thread_id(thread_id)?;
    Ok(thread_id)
}

fn valid_task_arg(args: &Value) -> Result<&str, String> {
    let task_id = str_arg(args, "task_id")?;
    validate_task_id(task_id)?;
    Ok(task_id)
}

fn string_array_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn map_err(e: harness_core::Error) -> String {
    e.to_string()
}

fn brief_field<'a>(brief: &'a Value, spanish: &str, english: &str) -> Option<&'a str> {
    brief
        .get(spanish)
        .or_else(|| brief.get(english))
        .and_then(|v| v.as_str())
}

fn brief_list(brief: &Value, spanish: &str, english: &str) -> Vec<String> {
    brief
        .get(spanish)
        .or_else(|| brief.get(english))
        .map(|v| match v {
            Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect(),
            Value::String(s) => vec![s.clone()],
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

fn validate_task_brief_object(brief: &Value) -> Result<(), String> {
    let objetivo = brief_field(brief, "objetivo", "objective")
        .unwrap_or("")
        .trim();
    let contexto = brief_field(brief, "contexto", "context")
        .unwrap_or("")
        .trim();
    let tareas = brief_list(brief, "tarea", "tasks")
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>();
    let reglas = brief_list(brief, "reglas", "rules")
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>();
    let resultado = brief_field(brief, "resultado_esperado", "expected_result")
        .unwrap_or("")
        .trim();

    let mut missing = Vec::new();
    if objetivo.is_empty() {
        missing.push("objetivo");
    }
    if contexto.is_empty() {
        missing.push("contexto");
    }
    if tareas.is_empty() {
        missing.push("tarea");
    }
    if reglas.is_empty() {
        missing.push("reglas");
    }
    if resultado.is_empty() {
        missing.push("resultado_esperado");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "brief incomplete; missing required field(s): {}. Retry task_create with brief using this exact shape: \
             {{ \"objetivo\": \"Que quieres lograr\", \"contexto\": \"stack, archivos, restricciones\", \
             \"tarea\": [\"paso 1\", \"paso 2\"], \"reglas\": [\"No romper\", \"Cambios mínimos\", \
             \"Seguir estilo existente\", \"Agregar test\"], \"resultado_esperado\": \"que debe funcionar\" }}",
            missing.join(", ")
        ))
    }
}

fn render_task_brief(brief: &Value) -> Result<Option<String>, String> {
    let Some(brief) = brief.as_object().map(|_| brief) else {
        return match brief.as_str() {
            Some(s) if !s.trim().is_empty() => Ok(Some(s.trim().to_string())),
            Some(_) => Ok(None),
            None => Err("brief must be an object or string".into()),
        };
    };

    validate_task_brief_object(brief)?;

    let objetivo = brief_field(brief, "objetivo", "objective")
        .unwrap_or("")
        .trim();
    let contexto = brief_field(brief, "contexto", "context")
        .unwrap_or("")
        .trim();
    let tareas = brief_list(brief, "tarea", "tasks");
    let reglas = brief_list(brief, "reglas", "rules");
    let resultado = brief_field(brief, "resultado_esperado", "expected_result")
        .unwrap_or("")
        .trim();

    if objetivo.is_empty()
        && contexto.is_empty()
        && tareas.is_empty()
        && reglas.is_empty()
        && resultado.is_empty()
    {
        return Ok(None);
    }

    let mut out = String::new();
    out.push_str("Objetivo:\n");
    out.push_str(if objetivo.is_empty() {
        "(sin objetivo)"
    } else {
        objetivo
    });
    out.push_str("\n\nContexto:\n");
    out.push_str(if contexto.is_empty() {
        "(sin contexto)"
    } else {
        contexto
    });
    out.push_str("\n\nTarea:\n");
    if tareas.is_empty() {
        out.push_str("1. (sin pasos)\n");
    } else {
        for (idx, tarea) in tareas.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", idx + 1, tarea.trim()));
        }
    }
    out.push_str("\nReglas:\n");
    if reglas.is_empty() {
        out.push_str(
            "- No romper.\n- Cambios mínimos.\n- Seguir estilo existente.\n- Agregar test.\n",
        );
    } else {
        for regla in reglas {
            out.push_str(&format!("- {}\n", regla.trim()));
        }
    }
    out.push_str("\nResultado esperado:\n");
    out.push_str(if resultado.is_empty() {
        "(sin resultado esperado)"
    } else {
        resultado
    });

    Ok(Some(out))
}

fn acceptance_from_args(args: &Value) -> Result<Vec<AcceptanceCheck>, String> {
    let mut checks: Vec<AcceptanceCheck> = Vec::new();

    if let Some(brief) = args.get("brief") {
        if let Some(rendered) = render_task_brief(brief)? {
            checks.push(AcceptanceCheck {
                id: "BRIEF".into(),
                text: rendered,
                verified: false,
                verified_by: None,
            });
        }
    }

    if let Some(arr) = args
        .get("acceptance")
        .and_then(|v| v.get("checks"))
        .and_then(|v| v.as_array())
    {
        checks.extend(arr.iter().filter_map(|c| {
            let text = c.get("text").and_then(|v| v.as_str())?.to_string();
            let id = c
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(AcceptanceCheck {
                id,
                text,
                verified: false,
                verified_by: None,
            })
        }));
    }

    Ok(checks)
}

/// `task_create` — primary path: delegate to the harness-server REST endpoint
/// so the in-process `TaskStore` (the one the SSE stream subscribes to) does
/// the write. That guarantees the right panel updates without a refresh.
///
/// Fallback: when the agent was spawned without `--server-url` (legacy or
/// detached MCP run), we write directly to the filesystem via our local
/// `TaskStore` clone. This keeps `task_create` functional in isolated tests
/// but the SSE stream won't fire — see `Dispatcher::server_url`.
pub fn create(
    store: &TaskStore,
    default_thread: &str,
    agent_id: &str,
    server_url: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?.to_string();
    let title = str_arg(args, "title")?.to_string();
    let parent = opt_str(args, "parent").map(String::from);
    let depends_on = string_array_arg(args, "depends_on");
    let labels = string_array_arg(args, "labels");
    let acceptance = acceptance_from_args(args)?;

    if let Some(base) = server_url {
        // Delegate to harness-server so the in-process broadcast bus emits
        // `task.created` and SSE subscribers see the new task immediately.
        let url = format!(
            "{}/api/threads/{}/tasks",
            base.trim_end_matches('/'),
            thread_id
        );
        let body = json!({
            "title": title,
            "parent": parent,
            "depends_on": depends_on,
            "labels": labels,
            "acceptance": { "checks": acceptance.iter().map(|c| json!({
                "id": c.id,
                "text": c.text,
            })).collect::<Vec<_>>() },
            "created_by": agent_id,
        });
        match ureq::post(&url)
            .timeout(Duration::from_secs(5))
            .send_json(&body)
        {
            Ok(resp) => {
                let value: Value = resp.into_json().map_err(|e| e.to_string())?;
                return Ok(value);
            }
            Err(e) => {
                tracing::warn!(error = %e, "task_create: HTTP delegation failed, falling back to local store");
                // fall through to local-store path
            }
        }
    }

    let draft = TaskDraft {
        title,
        parent,
        depends_on,
        acceptance,
        labels,
        created_by: agent_id.to_string(),
    };
    let task = store.create(&thread_id, draft).map_err(map_err)?;
    Ok(json!(task))
}

pub fn list(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
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

pub fn get(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
    let t = store.get(thread_id, task_id).map_err(map_err)?;
    Ok(json!(t))
}

pub fn claim(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
    let agent_id = str_arg(args, "agent_id")?;
    let ttl_s = args.get("ttl_s").and_then(|v| v.as_u64()).unwrap_or(60);
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

pub fn renew(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
    let agent_id = str_arg(args, "agent_id")?;
    let lease = store.renew(thread_id, task_id, agent_id).map_err(map_err)?;
    Ok(json!({ "lease": lease }))
}

pub fn update(
    store: &TaskStore,
    default_thread: &str,
    agent_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
    let patch_v = args
        .get("patch")
        .ok_or_else(|| "missing arg: patch".to_string())?;
    let patch: TaskPatch =
        serde_json::from_value(patch_v.clone()).map_err(|e| format!("invalid patch: {e}"))?;
    let t = store
        .patch(thread_id, task_id, patch, agent_id)
        .map_err(map_err)?;
    Ok(json!(t))
}

pub fn release(store: &TaskStore, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
    let agent_id = str_arg(args, "agent_id")?;
    store
        .release(thread_id, task_id, agent_id)
        .map_err(map_err)?;
    Ok(json!({ "ok": true }))
}

pub fn submit(
    store: &TaskStore,
    default_thread: &str,
    agent_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = valid_thread_or_default(args, default_thread)?;
    let task_id = valid_task_arg(args)?;
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
