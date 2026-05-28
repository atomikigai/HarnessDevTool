use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use harness_core::{AcceptanceCheck, ListFilters, Task, TaskDraft, TaskPatch, TaskStatus};
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads/:tid/tasks", get(list).post(create))
        .route(
            "/api/threads/:tid/tasks/:task_id",
            get(get_one).patch(patch_one).delete(delete_one),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub label: Option<String>,
    pub assignee: Option<String>,
}

async fn list(
    State(s): State<Arc<AppState>>,
    Path(tid): Path<String>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    let status = match q.status {
        Some(v) => Some(
            TaskStatus::from_str(&v).map_err(|e| ApiError::BadRequest(format!("status: {e}")))?,
        ),
        None => None,
    };
    let filters = ListFilters {
        status,
        label: q.label,
        assignee: q.assignee,
    };
    Ok(Json(s.tasks.list(&tid, filters)?))
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub title: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub acceptance: Option<AcceptanceBody>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub created_by: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptanceBody {
    #[serde(default)]
    pub checks: Vec<AcceptanceCheckBody>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptanceCheckBody {
    pub text: String,
    #[serde(default)]
    pub id: Option<String>,
}

async fn create(
    State(s): State<Arc<AppState>>,
    Path(tid): Path<String>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<Task>)> {
    let acceptance = body
        .acceptance
        .map(|a| {
            a.checks
                .into_iter()
                .map(|c| AcceptanceCheck {
                    id: c.id.unwrap_or_default(),
                    text: c.text,
                    verified: false,
                    verified_by: None,
                })
                .collect()
        })
        .unwrap_or_default();
    let draft = TaskDraft {
        title: body.title,
        parent: body.parent,
        depends_on: body.depends_on,
        acceptance,
        labels: body.labels,
        created_by: body.created_by,
    };
    let task = s.tasks.create(&tid, draft)?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn get_one(
    State(s): State<Arc<AppState>>,
    Path((tid, task_id)): Path<(String, String)>,
) -> ApiResult<Json<Task>> {
    Ok(Json(s.tasks.get(&tid, &task_id)?))
}

#[derive(Debug, Deserialize)]
pub struct PatchBody {
    pub by: String,
    #[serde(flatten)]
    pub patch: TaskPatch,
}

async fn patch_one(
    State(s): State<Arc<AppState>>,
    Path((tid, task_id)): Path<(String, String)>,
    Json(body): Json<PatchBody>,
) -> ApiResult<Json<Task>> {
    Ok(Json(s.tasks.patch(&tid, &task_id, body.patch, &body.by)?))
}

#[derive(Debug, Deserialize)]
pub struct DeleteBody {
    pub why: String,
    pub by: String,
}

async fn delete_one(
    State(s): State<Arc<AppState>>,
    Path((tid, task_id)): Path<(String, String)>,
    Json(body): Json<DeleteBody>,
) -> ApiResult<StatusCode> {
    s.tasks.delete(&tid, &task_id, body.why, &body.by)?;
    Ok(StatusCode::NO_CONTENT)
}
