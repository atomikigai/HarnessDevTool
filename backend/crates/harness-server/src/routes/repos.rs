use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use harness_core::{CurrentRepoReport, RepoRecord, RepoThreadRecord};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CurrentRepoQuery {
    pub cwd: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/repos/current", get(current_repo))
        .route("/api/repos/:id", get(get_repo))
        .route("/api/repos/:id/threads", get(list_repo_threads))
}

async fn current_repo(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CurrentRepoQuery>,
) -> Result<Json<CurrentRepoReport>, ApiError> {
    let cwd = PathBuf::from(query.cwd);
    let report = state
        .repos
        .current_report(&cwd)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(report))
}

async fn get_repo(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RepoRecord>, ApiError> {
    let repo = state.repos.get(&id).map_err(repo_error)?;
    Ok(Json(repo))
}

async fn list_repo_threads(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<RepoThreadRecord>>, ApiError> {
    state.repos.get(&id).map_err(repo_error)?;
    let threads = state
        .repos
        .list_threads(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(threads))
}

fn repo_error(error: harness_core::RepoError) -> ApiError {
    match error {
        harness_core::RepoError::NotFound(id) => ApiError::NotFound(format!("repo {id}")),
        other => ApiError::Internal(other.to_string()),
    }
}
