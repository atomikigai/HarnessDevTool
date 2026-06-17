use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use harness_core::Event;
use harness_session::{AgentState, SessionMeta};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::routes::sessions::load_session_meta;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ContextGovernorStatus {
    pub session_id: String,
    pub thread_id: String,
    pub task_id: Option<String>,
    pub role: Option<String>,
    pub latest_event_type: Option<String>,
    pub latest_event_at: Option<i64>,
    pub checkpoint_requested_at: Option<i64>,
    pub checkpoint_saved_at: Option<i64>,
    pub clear_pending_at: Option<i64>,
    pub clear_deferred_at: Option<i64>,
    pub clear_recommended_at: Option<i64>,
    pub cleared_at: Option<i64>,
    pub pressure: Option<f64>,
    pub context_tokens: Option<u64>,
    pub max_context_tokens: Option<u64>,
    pub model: Option<String>,
    pub checkpoint_preview: Option<String>,
    pub checkpoint_structured: Option<Value>,
    pub indexed_events: usize,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ContextActionResponse {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContextSearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ContextSearchHit {
    pub thread_id: String,
    pub session_id: String,
    pub event_type: String,
    pub at: i64,
    pub pressure: Option<f64>,
    pub model: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ContextSearchResponse {
    pub query: String,
    pub hits: Vec<ContextSearchHit>,
}

pub async fn get_context_status(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<ContextGovernorStatus>, ApiError> {
    let meta = load_session_meta(&state, &sid).await?;
    ensure_context_indexed(&state, &meta.thread_id)?;
    let events =
        crate::context_index::context_events_for_session(&state.harness_home, &state.profile, &sid)
            .map_err(|e| ApiError::internal_context("read indexed context events", e))?;
    let indexed_events = events.len();
    Ok(Json(context_status_from_events(
        &meta,
        &events,
        indexed_events,
    )))
}

pub async fn search_context_status(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    Query(query): Query<ContextSearchQuery>,
) -> Result<Json<ContextSearchResponse>, ApiError> {
    let meta = load_session_meta(&state, &sid).await?;
    ensure_context_indexed(&state, &meta.thread_id)?;
    let hits = crate::context_index::search_context_events(
        &state.harness_home,
        &state.profile,
        &sid,
        &query.q,
        query.limit.unwrap_or(10),
    )
    .map_err(|e| ApiError::internal_context("search context events", e))?
    .into_iter()
    .map(|hit| ContextSearchHit {
        thread_id: hit.thread_id,
        session_id: hit.session_id,
        event_type: hit.event_type,
        at: hit.at,
        pressure: hit.pressure,
        model: hit.model,
        snippet: hit.snippet,
    })
    .collect();
    Ok(Json(ContextSearchResponse {
        query: query.q,
        hits,
    }))
}

fn ensure_context_indexed(state: &AppState, thread_id: &str) -> Result<usize, ApiError> {
    match crate::context_index::last_indexed_seq(&state.harness_home, &state.profile, thread_id) {
        Ok(Some(_)) => Ok(0),
        Ok(None) => {
            let events = state.store.read_events(thread_id)?;
            crate::context_index::index_context_events(&state.harness_home, &state.profile, &events)
                .map_err(|e| ApiError::internal_context("index context events", e))
        }
        Err(e) => Err(ApiError::internal_context("read context index offset", e)),
    }
}

pub async fn request_context_checkpoint(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<ContextActionResponse>, ApiError> {
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    let meta = session.meta().await;
    let prompt = "\n\n[harness context governor]\n\
        Manual checkpoint requested. Reply with a compact checkpoint headed exactly \
        `CONTEXT CHECKPOINT`, using labels: goal, completed, current_focus, \
        next_action, files_touched, commands_run, risks, blockers.\n";
    session
        .write_input(format!("{prompt}\r").as_bytes())
        .await?;
    let target = context_target_from_meta(&meta);
    crate::context_governor::append_context_event(
        &state.store,
        &target,
        "session.context.manual_checkpoint_requested",
        json!({
            "session_id": meta.id,
            "thread_id": meta.thread_id,
            "task_id": meta.task_id,
            "role": meta.role,
        }),
        "Manual context checkpoint requested.",
    );
    Ok(Json(ContextActionResponse {
        status: "requested".into(),
        reason: None,
    }))
}

pub async fn clear_context_manual(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<ContextActionResponse>, ApiError> {
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    let meta = session.meta().await;
    let target = context_target_from_meta(&meta);
    if meta.status != harness_session::SessionStatus::Running
        || meta.detected_state != Some(AgentState::Idle)
    {
        crate::context_governor::append_context_event(
            &state.store,
            &target,
            "session.context.manual_clear_deferred",
            json!({
                "session_id": meta.id,
                "thread_id": meta.thread_id,
                "task_id": meta.task_id,
                "role": meta.role,
                "detected_state": meta.detected_state,
                "reason_code": "session_not_idle",
            }),
            "Manual context clear deferred because the session was not idle.",
        );
        return Ok(Json(ContextActionResponse {
            status: "deferred".into(),
            reason: Some("session_not_idle".into()),
        }));
    }

    session.write_input(b"/clear\r").await?;
    crate::context_governor::append_context_event(
        &state.store,
        &target,
        "session.context.manual_cleared",
        json!({
            "session_id": meta.id,
            "thread_id": meta.thread_id,
            "task_id": meta.task_id,
            "role": meta.role,
            "clear_command": "/clear",
        }),
        "Manually cleared live context.",
    );
    Ok(Json(ContextActionResponse {
        status: "cleared".into(),
        reason: None,
    }))
}

fn context_target_from_meta(meta: &SessionMeta) -> crate::context_governor::ContextGovernorTarget {
    crate::context_governor::ContextGovernorTarget {
        session_id: meta.id.clone(),
        thread_id: meta.thread_id.clone(),
        task_id: meta.task_id.clone(),
        role: meta.role.clone(),
    }
}

fn context_status_from_events(
    meta: &SessionMeta,
    events: &[Event],
    indexed_events: usize,
) -> ContextGovernorStatus {
    let mut status = ContextGovernorStatus {
        session_id: meta.id.clone(),
        thread_id: meta.thread_id.clone(),
        task_id: meta.task_id.clone(),
        role: meta.role.clone(),
        latest_event_type: None,
        latest_event_at: None,
        checkpoint_requested_at: None,
        checkpoint_saved_at: None,
        clear_pending_at: None,
        clear_deferred_at: None,
        clear_recommended_at: None,
        cleared_at: None,
        pressure: None,
        context_tokens: None,
        max_context_tokens: None,
        model: None,
        checkpoint_preview: None,
        checkpoint_structured: None,
        indexed_events,
    };
    for event in events
        .iter()
        .filter(|event| event.event_type.starts_with("session.context."))
        .filter(|event| {
            event
                .payload
                .as_ref()
                .and_then(|payload| payload.get("session_id"))
                .and_then(Value::as_str)
                == Some(meta.id.as_str())
        })
    {
        status.latest_event_type = Some(event.event_type.clone());
        status.latest_event_at = Some(event.at);
        match event.event_type.as_str() {
            "session.context.checkpoint_requested"
            | "session.context.manual_checkpoint_requested" => {
                status.checkpoint_requested_at = Some(event.at);
            }
            "session.context.checkpoint_saved" => {
                status.checkpoint_saved_at = Some(event.at);
            }
            "session.context.clear_pending" => {
                status.clear_pending_at = Some(event.at);
            }
            "session.context.clear_deferred" | "session.context.manual_clear_deferred" => {
                status.clear_deferred_at = Some(event.at);
            }
            "session.context.clear_recommended" => {
                status.clear_recommended_at = Some(event.at);
            }
            "session.context.cleared" | "session.context.manual_cleared" => {
                status.cleared_at = Some(event.at);
            }
            _ => {}
        }
        if let Some(payload) = event.payload.as_ref() {
            status.pressure = payload
                .get("pressure")
                .and_then(Value::as_f64)
                .or(status.pressure);
            status.context_tokens = payload
                .get("context_tokens")
                .and_then(Value::as_u64)
                .or(status.context_tokens);
            status.max_context_tokens = payload
                .get("max_context_tokens")
                .and_then(Value::as_u64)
                .or(status.max_context_tokens);
            status.model = payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| status.model.clone());
            if let Some(checkpoint) = payload.get("checkpoint").and_then(Value::as_str) {
                status.checkpoint_preview = Some(compact_preview(checkpoint, 260));
            }
            if let Some(structured) = payload.get("checkpoint_structured") {
                status.checkpoint_structured = Some(structured.clone());
            }
        }
    }
    status
}

fn compact_preview(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    compact.chars().take(max_chars).collect::<String>() + "..."
}
