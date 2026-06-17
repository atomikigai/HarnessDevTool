//! Scheduler pause routes — toggles auto-assignment globally or per thread.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use harness_core::validate_thread_id;
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/pause-all", post(pause_all).get(get_pause))
        .route("/api/resume-all", post(resume_all))
        .route(
            "/api/threads/:tid/pause",
            post(pause_thread).get(get_thread_pause),
        )
        .route("/api/threads/:tid/resume", post(resume_thread))
}

async fn pause_all(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.pause.set(true).map_err(ApiError::internal)?;
    Ok(Json(json!({ "paused": state.pause.is_paused() })))
}

async fn resume_all(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.pause.set(false).map_err(ApiError::internal)?;
    Ok(Json(json!({ "paused": state.pause.is_paused() })))
}

async fn get_pause(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "paused": state.pause.is_paused() }))
}

async fn pause_thread(
    State(state): State<Arc<AppState>>,
    Path(tid): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_thread_id(&tid).map_err(ApiError::bad_request)?;
    state
        .pause
        .set_thread(&tid, true)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "thread_id": tid, "paused": true })))
}

async fn resume_thread(
    State(state): State<Arc<AppState>>,
    Path(tid): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_thread_id(&tid).map_err(ApiError::bad_request)?;
    state
        .pause
        .set_thread(&tid, false)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "thread_id": tid, "paused": false })))
}

async fn get_thread_pause(
    State(state): State<Arc<AppState>>,
    Path(tid): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_thread_id(&tid).map_err(ApiError::bad_request)?;
    Ok(Json(json!({
        "thread_id": tid,
        "paused": state.pause.is_thread_paused(&tid)
    })))
}
