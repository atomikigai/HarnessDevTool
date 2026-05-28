use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use harness_core::Thread;
use harness_session::SessionMeta;
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

/// Thread enriched with the live sessions attached to it. The frontend uses
/// this shape to render the Sessions column without a second round-trip.
#[derive(Debug, Serialize)]
pub struct ThreadWithSessions {
    #[serde(flatten)]
    pub thread: Thread,
    pub sessions: Vec<SessionMeta>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/threads", get(list_threads).post(create_thread))
}

async fn list_threads(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ThreadWithSessions>>, ApiError> {
    let threads = state.store.list_threads()?;
    // Group live session handles by thread_id.
    let mut by_thread: HashMap<String, Vec<SessionMeta>> = HashMap::new();
    for s in state.manager.all() {
        let meta = s.meta().await;
        by_thread
            .entry(meta.thread_id.clone())
            .or_default()
            .push(meta);
    }
    let enriched = threads
        .into_iter()
        .map(|t| ThreadWithSessions {
            sessions: by_thread.remove(&t.id).unwrap_or_default(),
            thread: t,
        })
        .collect();
    Ok(Json(enriched))
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
