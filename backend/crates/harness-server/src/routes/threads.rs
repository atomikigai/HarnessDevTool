use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use harness_core::Thread;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct CreateThreadRequest {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateThreadResponse {
    pub id: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/threads", get(list_threads).post(create_thread))
}

async fn list_threads(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Thread>>, ApiError> {
    let threads = state.store.list_threads()?;
    Ok(Json(threads))
}

async fn create_thread(
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreateThreadRequest>>,
) -> Result<(StatusCode, Json<CreateThreadResponse>), ApiError> {
    let title = body.and_then(|b| b.0.title);
    let thread = state.store.create_thread(title)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateThreadResponse { id: thread.id }),
    ))
}
