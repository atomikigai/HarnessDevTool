use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use harness_core::{Agent, AgentDraft};
use std::sync::Arc;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/agents", get(list).post(create))
}

async fn list(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<Agent>>> {
    Ok(Json(s.agents.list()))
}

async fn create(
    State(s): State<Arc<AppState>>,
    Json(body): Json<AgentDraft>,
) -> ApiResult<(StatusCode, Json<Agent>)> {
    Ok((StatusCode::CREATED, Json(s.agents.create(body)?)))
}
