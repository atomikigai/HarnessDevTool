use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub version: String,
    pub uptime_s: u64,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        version: state.version.to_string(),
        uptime_s: state.start_time.elapsed().as_secs(),
    })
}
