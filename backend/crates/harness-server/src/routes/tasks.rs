use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use harness_core::{
    AcceptanceCheck, Event, Handoff, Item, ListFilters, Task, TaskBrief, TaskDraft, TaskPatch,
    TaskStatus,
};
use serde::Deserialize;
use serde_json::json;
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
        .route(
            "/api/threads/:tid/tasks/:task_id/handoffs",
            get(list_handoffs).post(create_handoff),
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
    pub brief: Option<TaskBrief>,
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
        brief: body.brief,
        acceptance,
        labels: body.labels,
        created_by: body.created_by,
    };
    let task = s.tasks.create(&tid, draft)?;
    tracing::info!(
        thread_id = %tid,
        task_id = %task.id,
        title = %task.title,
        created_by = %task.created_by,
        "task created via REST (will emit task.created on broadcast)"
    );
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

#[derive(Debug, Deserialize)]
pub struct HandoffBody {
    pub from: String,
    pub to_role: String,
    pub status: String,
    pub goal: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(default)]
    pub commands_run: Vec<String>,
    #[serde(default)]
    pub verification_passed: Vec<String>,
    #[serde(default)]
    pub verification_not_run: Vec<String>,
    #[serde(default)]
    pub blocked_on: Vec<String>,
    pub next_agent_action: String,
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

async fn list_handoffs(
    State(s): State<Arc<AppState>>,
    Path((tid, task_id)): Path<(String, String)>,
) -> ApiResult<Json<Vec<Handoff>>> {
    // Validate task exists so callers get the same 404 semantics as get_one.
    let _ = s.tasks.get(&tid, &task_id)?;
    Ok(Json(s.store.read_handoffs(&tid, &task_id)?))
}

async fn create_handoff(
    State(s): State<Arc<AppState>>,
    Path((tid, task_id)): Path<(String, String)>,
    Json(body): Json<HandoffBody>,
) -> ApiResult<(StatusCode, Json<Handoff>)> {
    let _ = s.tasks.get(&tid, &task_id)?;
    if body.from.trim().is_empty()
        || body.to_role.trim().is_empty()
        || body.status.trim().is_empty()
        || body.goal.trim().is_empty()
        || body.next_agent_action.trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "from, to_role, status, goal and next_agent_action are required".to_string(),
        ));
    }
    let handoff = Handoff {
        at: Utc::now().timestamp_millis(),
        from: body.from,
        to_role: body.to_role,
        task_id: task_id.clone(),
        status: body.status,
        goal: body.goal,
        assumptions: body.assumptions,
        files_changed: body.files_changed,
        commands_run: body.commands_run,
        verification_passed: body.verification_passed,
        verification_not_run: body.verification_not_run,
        blocked_on: body.blocked_on,
        next_agent_action: body.next_agent_action,
    };
    s.store.append_handoff(&tid, &handoff)?;

    let seq = s.store.read_events(&tid)?.len() as u64;
    let event = Event {
        seq,
        at: Utc::now().timestamp_millis(),
        event_type: "handoff.created".to_string(),
        items: vec![Item::Text {
            text: serde_json::to_string(&json!({
                "task_id": task_id,
                "from": handoff.from,
                "to_role": handoff.to_role,
                "status": handoff.status,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }],
    };
    s.store.append_event(&tid, &event)?;

    Ok((StatusCode::CREATED, Json(handoff)))
}
