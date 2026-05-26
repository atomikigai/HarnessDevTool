use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use harness_session::{AgentKind, SessionError, SessionMeta};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub kind: AgentKind,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ResizeRequest {
    pub cols: u16,
    pub rows: u16,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads/:tid/sessions", post(create_session))
        .route("/api/sessions/:sid", get(get_session))
        .route("/api/sessions/:sid/input", post(post_input))
        .route("/api/sessions/:sid/resize", post(post_resize))
        .route("/api/sessions/:sid", delete(kill_session))
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    Path(tid): Path<String>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), ApiError> {
    // 1) Thread must exist.
    state.store.get_thread(&tid)?;

    // 2) Binary must be detected.
    let binary = state
        .binaries
        .get(&req.kind)
        .cloned()
        .ok_or(ApiError::from(SessionError::BinaryNotFound(req.kind)))?;

    // 3) Resolve cwd.
    let cwd = match req.cwd {
        Some(c) => PathBuf::from(c),
        None => dirs::home_dir()
            .ok_or_else(|| ApiError::Internal("cannot resolve $HOME for default cwd".into()))?,
    };
    if !cwd.exists() {
        return Err(ApiError::BadRequest(format!(
            "cwd does not exist: {}",
            cwd.display()
        )));
    }

    let session = state.manager.spawn(req.kind, binary, tid, cwd)?;
    let meta = session.meta().await;
    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            session_id: meta.id,
        }),
    ))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    if let Some(s) = state.manager.get(&sid) {
        return Ok(Json(s.meta().await));
    }
    // Fall back to on-disk meta (session exited and may have been forgotten).
    let path = state.manager.sessions_root().join(&sid).join("meta.json");
    if !path.exists() {
        return Err(ApiError::SessionNotFound(sid));
    }
    let bytes = std::fs::read(&path).map_err(|e| ApiError::Internal(e.to_string()))?;
    let meta: SessionMeta =
        serde_json::from_slice(&bytes).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(meta))
}

async fn post_input(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    if body.len() > MAX_INPUT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "input exceeds {MAX_INPUT_BYTES} bytes",
        )));
    }
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.write_input(&body).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_resize(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    Json(req): Json<ResizeRequest>,
) -> Result<StatusCode, ApiError> {
    if req.cols == 0 || req.rows == 0 {
        return Err(ApiError::BadRequest("cols/rows must be > 0".into()));
    }
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.resize(req.cols, req.rows).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn kill_session(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.kill().await?;
    Ok(StatusCode::NO_CONTENT)
}
