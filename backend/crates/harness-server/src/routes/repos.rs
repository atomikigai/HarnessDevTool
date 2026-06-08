use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use harness_core::{CurrentRepoReport, RepoContinuity, RepoRecord, RepoThreadRecord};
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
    let mut report = state
        .repos
        .current_report(&cwd)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    enrich_continuity(&state, &mut report);
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

fn enrich_continuity(state: &AppState, report: &mut CurrentRepoReport) {
    let Some(repo) = report.repo.as_ref() else {
        return;
    };
    let recommended_thread_id = repo.last_thread_id.clone().or_else(|| {
        report
            .threads
            .first()
            .map(|thread| thread.thread_id.clone())
    });
    let mut last_goal = repo.summary.clone();
    let mut blockers = Vec::new();

    if let Some(thread_id) = recommended_thread_id.as_deref() {
        if let Ok(thread) = state.store.get_thread(thread_id) {
            last_goal = thread.title.or(last_goal);
        }
        if let Ok(Some(readiness)) = state.store.read_readiness_report(thread_id) {
            blockers.extend(
                readiness
                    .blocking
                    .into_iter()
                    .map(|issue| issue.message)
                    .take(5),
            );
        }
    }

    report.continuity = Some(RepoContinuity {
        recommended_thread_id,
        last_thread_id: repo.last_thread_id.clone(),
        last_session_id: repo.last_session_id.clone(),
        last_goal,
        blockers,
        recent_threads: report.threads.iter().take(5).cloned().collect(),
    });
}
