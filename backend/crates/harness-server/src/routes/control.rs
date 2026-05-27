//! Global kill-switch routes — toggles scheduler auto-assignment on/off.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/pause-all", post(pause_all).get(get_pause))
        .route("/api/resume-all", post(resume_all))
}

async fn pause_all(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    state
        .pause
        .set(true)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resume_all(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    state
        .pause
        .set(false)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_pause(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "paused": state.pause.is_paused() }))
}
