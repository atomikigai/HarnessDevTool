use std::sync::{Arc, Mutex};

use chrono::Utc;
use harness_core::{Event, Handoff, Item, Store};
use harness_session::{AgentState, Manager, SessionEvent};
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::transcript::event::{TranscriptKind, TranscriptSource};
use crate::transcript::TranscriptEvent;

const CHECKPOINT_THRESHOLD: f64 = 0.35;
const CLEAR_THRESHOLD: f64 = 0.40;
const MIN_CHECKPOINT_CHARS: usize = 120;

#[derive(Debug, Clone)]
pub struct ContextGovernorTarget {
    pub session_id: String,
    pub thread_id: String,
    pub task_id: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Default)]
struct GovernorState {
    checkpoint_requested: bool,
    checkpoint_saved: bool,
    clear_pending: bool,
    clear_in_progress: bool,
    cleared: bool,
    latest_pressure: Option<ContextPressure>,
}

#[derive(Debug, Clone)]
struct ContextPressure {
    model: String,
    context_tokens: u64,
    max_context_tokens: u64,
    pressure: f64,
    source_seq: u64,
}

pub fn spawn_context_governor(
    target: ContextGovernorTarget,
    store: Arc<Store>,
    manager: Arc<Manager>,
    mut rx: broadcast::Receiver<TranscriptEvent>,
) {
    let transcript_target = target.clone();
    let transcript_store = store.clone();
    let transcript_manager = manager.clone();
    tokio::spawn(async move {
        let state = Arc::new(Mutex::new(GovernorState::default()));
        let state_for_idle = state.clone();
        let target_for_idle = transcript_target.clone();
        let store_for_idle = transcript_store.clone();
        let manager_for_idle = transcript_manager.clone();
        tokio::spawn(async move {
            let mut state_rx = manager_for_idle.subscribe();
            loop {
                match state_rx.recv().await {
                    Ok(SessionEvent::StateChanged {
                        session_id, next, ..
                    }) if session_id == target_for_idle.session_id && next == AgentState::Idle => {
                        clear_and_resume(
                            &manager_for_idle,
                            &store_for_idle,
                            &target_for_idle,
                            &state_for_idle,
                        )
                        .await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        loop {
            match rx.recv().await {
                Ok(event) => {
                    handle_event(
                        &transcript_target,
                        &transcript_store,
                        &transcript_manager,
                        &state,
                        event,
                    )
                    .await;
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    append_context_event(
                        &transcript_store,
                        &transcript_target,
                        "session.context.governor_lagged",
                        json!({ "skipped": skipped }),
                        "Context governor lagged; checkpoint decisions may be delayed.",
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

async fn handle_event(
    target: &ContextGovernorTarget,
    store: &Arc<Store>,
    manager: &Arc<Manager>,
    state: &Arc<Mutex<GovernorState>>,
    event: TranscriptEvent,
) {
    if let Some(pressure) = event
        .usage
        .as_ref()
        .and_then(|usage| context_pressure(event.model.as_deref(), usage, event.seq))
    {
        let actions = {
            let mut s = state.lock().expect("context governor mutex poisoned");
            s.latest_pressure = Some(pressure.clone());
            let should_request =
                pressure.pressure >= CHECKPOINT_THRESHOLD && !s.checkpoint_requested;
            let should_mark_clear = pressure.pressure >= CLEAR_THRESHOLD && !s.cleared;
            if should_request {
                s.checkpoint_requested = true;
            }
            if should_mark_clear {
                s.clear_pending = true;
            }
            (should_request, should_mark_clear, s.checkpoint_saved)
        };

        if actions.0 {
            append_context_event(
                store,
                target,
                "session.context.checkpoint_requested",
                pressure_payload(&pressure, target),
                "Context pressure crossed 35%; requested compact checkpoint.",
            );
            request_checkpoint(manager, target, &pressure).await;
        }

        if actions.1 {
            append_context_event(
                store,
                target,
                "session.context.clear_pending",
                pressure_payload(&pressure, target),
                "Context pressure crossed 40%; waiting for checkpoint before clearing.",
            );
            if actions.2 {
                clear_and_resume(manager, store, target, state).await;
            }
        }
    }

    if is_checkpoint_candidate(&event) {
        let checkpoint = event.content.unwrap_or_default();
        let should_clear = {
            let mut s = state.lock().expect("context governor mutex poisoned");
            if s.checkpoint_requested && !s.checkpoint_saved {
                s.checkpoint_saved = true;
                true
            } else {
                false
            }
        };
        if should_clear {
            append_checkpoint_saved(store, target, &checkpoint, event.seq);
            if let Some(task_id) = target.task_id.as_deref() {
                append_checkpoint_handoff(store, target, task_id, &checkpoint);
            }
            clear_and_resume(manager, store, target, state).await;
        }
    }
}

fn context_pressure(
    model: Option<&str>,
    usage: &Value,
    source_seq: u64,
) -> Option<ContextPressure> {
    let model = model.unwrap_or("unknown").to_string();
    let max_context_tokens = usage
        .get("model_context_window")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .or_else(|| model_context_window(&model))?;
    let context_tokens = if usage
        .get("model_context_window")
        .and_then(Value::as_u64)
        .is_some()
    {
        usage_u64(usage, "input_tokens")
    } else {
        usage_context_tokens(usage)
    };
    if context_tokens == 0 {
        return None;
    }
    Some(ContextPressure {
        model,
        context_tokens,
        max_context_tokens,
        pressure: context_tokens as f64 / max_context_tokens as f64,
        source_seq,
    })
}

fn usage_context_tokens(usage: &Value) -> u64 {
    let input = usage_u64(usage, "input_tokens");
    let cache_read = usage_u64(usage, "cache_read_input_tokens");
    let cache_creation = usage
        .get("cache_creation")
        .map(|v| {
            usage_u64(v, "ephemeral_5m_input_tokens") + usage_u64(v, "ephemeral_1h_input_tokens")
        })
        .unwrap_or(0);
    input + cache_read + cache_creation
}

fn usage_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn model_context_window(model: &str) -> Option<u64> {
    let model = model.to_ascii_lowercase();
    if model.contains("claude") {
        return Some(200_000);
    }
    if model.contains("gpt-5") {
        return Some(400_000);
    }
    None
}

async fn request_checkpoint(
    manager: &Arc<Manager>,
    target: &ContextGovernorTarget,
    pressure: &ContextPressure,
) {
    let Some(session) = manager.get(&target.session_id) else {
        return;
    };
    let prompt = format!(
        "\n\n[harness context governor]\n\
         Context pressure is at {:.0}% ({} / {} tokens). Before continuing, \
         reply with a compact checkpoint headed exactly `CONTEXT CHECKPOINT`.\n\n\
         Use these labels so the harness can persist a structured checkpoint:\n\
         goal:\n\
         completed:\n\
         current_focus:\n\
         next_action:\n\
         files_touched:\n\
         commands_run:\n\
         risks:\n\
         blockers:\n\n\
         Keep it concise; the harness may clear live context after saving this checkpoint.\n",
        pressure.pressure * 100.0,
        pressure.context_tokens,
        pressure.max_context_tokens
    );
    if let Err(e) = session.write_input(format!("{prompt}\r").as_bytes()).await {
        tracing::warn!(
            session_id = %target.session_id,
            error = %e,
            "context governor could not request checkpoint"
        );
    }
}

async fn clear_and_resume(
    manager: &Arc<Manager>,
    store: &Arc<Store>,
    target: &ContextGovernorTarget,
    state: &Arc<Mutex<GovernorState>>,
) {
    if !auto_clear_allowed(target) {
        append_context_event(
            store,
            target,
            "session.context.clear_recommended",
            json!({
                "session_id": target.session_id,
                "thread_id": target.thread_id,
                "task_id": target.task_id,
                "role": target.role,
                "reason_code": "role_policy",
            }),
            "Context clear recommended, but automatic clear is disabled for this role.",
        );
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.clear_pending = false;
        return;
    }
    let pressure = {
        let mut s = state.lock().expect("context governor mutex poisoned");
        if s.cleared || s.clear_in_progress || !s.clear_pending || !s.checkpoint_saved {
            return;
        }
        s.clear_in_progress = true;
        s.latest_pressure.clone()
    };
    let Some(session) = manager.get(&target.session_id) else {
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.clear_in_progress = false;
        return;
    };

    if !wait_until_idle(&session).await {
        let mut payload = json!({
            "session_id": target.session_id,
            "thread_id": target.thread_id,
            "task_id": target.task_id,
            "role": target.role,
            "reason_code": "session_not_idle",
        });
        if let Some(pressure) = pressure.as_ref() {
            merge_pressure_payload(&mut payload, pressure);
        }
        append_context_event(
            store,
            target,
            "session.context.clear_deferred",
            payload,
            "Deferred context clear because the session was not idle.",
        );
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.clear_in_progress = false;
        return;
    }

    if let Err(e) = session.write_input(b"/clear\r").await {
        tracing::warn!(
            session_id = %target.session_id,
            error = %e,
            "context governor could not clear session"
        );
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.clear_in_progress = false;
        return;
    }
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    let resume = "\n[harness context governor]\n\
        Continue from the checkpoint you just produced. Do not reload the full \
        prior transcript; use task/spec/handoff state and only read files or \
        logs needed for the next action.\n";
    if let Err(e) = session.write_input(format!("{resume}\r").as_bytes()).await {
        tracing::warn!(
            session_id = %target.session_id,
            error = %e,
            "context governor could not resume session after clear"
        );
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.clear_in_progress = false;
        return;
    }
    {
        let mut s = state.lock().expect("context governor mutex poisoned");
        s.cleared = true;
        s.clear_in_progress = false;
    }

    let mut payload = json!({
        "session_id": target.session_id,
        "thread_id": target.thread_id,
        "task_id": target.task_id,
        "role": target.role,
        "clear_command": "/clear",
    });
    if let Some(pressure) = pressure {
        merge_pressure_payload(&mut payload, &pressure);
    }
    append_context_event(
        store,
        target,
        "session.context.cleared",
        payload,
        "Cleared live context after saving checkpoint.",
    );
}

fn auto_clear_allowed(target: &ContextGovernorTarget) -> bool {
    let role = target.role.as_deref().unwrap_or("").to_ascii_lowercase();
    !(role.contains("orchestrator") || role.contains("planner"))
}

async fn wait_until_idle(session: &std::sync::Arc<harness_session::AgentSession>) -> bool {
    for _ in 0..60 {
        let meta = session.meta().await;
        if meta.status != harness_session::SessionStatus::Running {
            return false;
        }
        if meta.detected_state == Some(AgentState::Idle) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    false
}

fn is_checkpoint_candidate(event: &TranscriptEvent) -> bool {
    if event.source != TranscriptSource::Claude
        || event.kind != TranscriptKind::Message
        || event.role.as_deref() != Some("assistant")
    {
        return false;
    }
    let Some(content) = event.content.as_deref() else {
        return false;
    };
    let lower = content.to_ascii_lowercase();
    content.chars().count() >= MIN_CHECKPOINT_CHARS && lower.contains("context checkpoint")
}

fn append_checkpoint_saved(
    store: &Arc<Store>,
    target: &ContextGovernorTarget,
    checkpoint: &str,
    transcript_seq: u64,
) {
    let structured = parse_checkpoint_sections(checkpoint);
    append_context_event(
        store,
        target,
        "session.context.checkpoint_saved",
        json!({
            "session_id": target.session_id,
            "thread_id": target.thread_id,
            "task_id": target.task_id,
            "role": target.role,
            "transcript_seq": transcript_seq,
            "checkpoint": checkpoint,
            "checkpoint_structured": structured,
        }),
        "Saved compact context checkpoint.",
    );
}

fn parse_checkpoint_sections(checkpoint: &str) -> Value {
    let mut out = serde_json::Map::new();
    let mut current: Option<String> = None;
    let mut buf: Vec<String> = Vec::new();
    for raw in checkpoint.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("context checkpoint") {
            continue;
        }
        if let Some((key, rest)) = trimmed.split_once(':') {
            let normalized = key.trim().to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                "goal"
                    | "completed"
                    | "current_focus"
                    | "next_action"
                    | "files_touched"
                    | "commands_run"
                    | "risks"
                    | "blockers"
            ) {
                if let Some(prev) = current.replace(normalized) {
                    out.insert(prev, Value::String(buf.join("\n").trim().to_string()));
                    buf.clear();
                }
                let rest = rest.trim();
                if !rest.is_empty() {
                    buf.push(rest.to_string());
                }
                continue;
            }
        }
        if current.is_some() && !trimmed.is_empty() {
            buf.push(trimmed.to_string());
        }
    }
    if let Some(prev) = current {
        out.insert(prev, Value::String(buf.join("\n").trim().to_string()));
    }
    Value::Object(out)
}

fn append_checkpoint_handoff(
    store: &Arc<Store>,
    target: &ContextGovernorTarget,
    task_id: &str,
    checkpoint: &str,
) {
    let handoff = Handoff {
        at: Utc::now().timestamp_millis(),
        from: format!("agent:{}", target.session_id),
        to_role: target.role.clone().unwrap_or_else(|| "orchestrator".into()),
        task_id: task_id.to_string(),
        status: "context_checkpoint".into(),
        goal: checkpoint.lines().take(8).collect::<Vec<_>>().join("\n"),
        assumptions: Vec::new(),
        files_changed: Vec::new(),
        commands_run: Vec::new(),
        verification_passed: Vec::new(),
        verification_not_run: Vec::new(),
        blocked_on: Vec::new(),
        next_agent_action: "Continue from compact context checkpoint after /clear.".into(),
    };
    if let Err(e) = store.append_handoff(&target.thread_id, &handoff) {
        tracing::warn!(
            thread_id = %target.thread_id,
            task_id,
            error = %e,
            "context governor could not append checkpoint handoff"
        );
    }
}

pub(crate) fn append_context_event(
    store: &Arc<Store>,
    target: &ContextGovernorTarget,
    event_type: &str,
    payload: Value,
    summary: &str,
) {
    let event = Event {
        seq: 0,
        at: Utc::now().timestamp_millis(),
        event_type: event_type.to_string(),
        items: vec![Item::Text {
            text: summary.to_string(),
        }],
        thread_id: Some(target.thread_id.clone()),
        actor: Some("context-governor".into()),
        payload: Some(payload),
    };
    if let Err(e) = store.append_event(&target.thread_id, &event) {
        tracing::warn!(
            thread_id = %target.thread_id,
            session_id = %target.session_id,
            event_type,
            error = %e,
            "context governor could not append event"
        );
    }
}

fn pressure_payload(pressure: &ContextPressure, target: &ContextGovernorTarget) -> Value {
    let mut payload = json!({
        "session_id": target.session_id,
        "thread_id": target.thread_id,
        "task_id": target.task_id,
        "role": target.role,
    });
    merge_pressure_payload(&mut payload, pressure);
    payload
}

fn merge_pressure_payload(payload: &mut Value, pressure: &ContextPressure) {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("model".into(), Value::String(pressure.model.clone()));
        obj.insert(
            "context_tokens".into(),
            Value::Number(pressure.context_tokens.into()),
        );
        obj.insert(
            "max_context_tokens".into(),
            Value::Number(pressure.max_context_tokens.into()),
        );
        obj.insert("pressure".into(), json!(pressure.pressure));
        obj.insert(
            "transcript_seq".into(),
            Value::Number(pressure.source_seq.into()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::event::TranscriptSource;

    #[test]
    fn context_pressure_counts_cached_input_tokens() {
        let usage = json!({
            "input_tokens": 10,
            "cache_read_input_tokens": 20,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 30,
                "ephemeral_1h_input_tokens": 40
            }
        });

        let pressure = context_pressure(Some("claude-sonnet-4-5"), &usage, 7).unwrap();

        assert_eq!(pressure.context_tokens, 100);
        assert_eq!(pressure.max_context_tokens, 200_000);
        assert_eq!(pressure.source_seq, 7);
    }

    #[test]
    fn context_pressure_uses_native_context_window_without_double_counting_cache() {
        let usage = json!({
            "input_tokens": 120_000,
            "cache_read_input_tokens": 90_000,
            "model_context_window": 258_400
        });

        let pressure = context_pressure(None, &usage, 9).unwrap();

        assert_eq!(pressure.context_tokens, 120_000);
        assert_eq!(pressure.max_context_tokens, 258_400);
        assert!(pressure.pressure > 0.46);
    }

    #[test]
    fn unknown_model_has_no_pressure_decision() {
        let usage = json!({ "input_tokens": 10 });

        assert!(context_pressure(Some("unknown-model"), &usage, 1).is_none());
    }

    #[test]
    fn checkpoint_candidate_requires_assistant_marker() {
        let event = TranscriptEvent {
            seq: 1,
            session_id: "s".into(),
            ts: "now".into(),
            source: TranscriptSource::Claude,
            kind: TranscriptKind::Message,
            role: Some("assistant".into()),
            content: Some(format!(
                "CONTEXT CHECKPOINT\n{}",
                "This is a compact checkpoint with enough detail to resume. ".repeat(4)
            )),
            tool_name: None,
            tool_args: None,
            tool_use_id: None,
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        };

        assert!(is_checkpoint_candidate(&event));
    }
}
