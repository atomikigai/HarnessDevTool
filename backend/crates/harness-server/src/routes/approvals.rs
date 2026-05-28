use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use harness_policy::{Decision, RememberScope, Rule};
use serde::{Deserialize, Serialize};

use crate::approvals::ApprovalSummary;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/approvals/check", post(check))
        .route("/api/approvals", get(list))
        .route("/api/approvals/:id/decide", post(decide))
}

#[derive(Debug, Deserialize)]
struct CheckBody {
    tool: String,
    #[serde(default)]
    args: serde_json::Value,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckResponse {
    decision: Decision,
}

async fn check(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CheckBody>,
) -> Json<CheckResponse> {
    let decision = state.policy.evaluate(&body.tool, &body.args);
    let decision = match decision {
        Decision::Allow => Decision::Allow,
        Decision::Deny => Decision::Deny,
        Decision::Ask => {
            let timeout = Duration::from_secs(state.policy.timeout_secs());
            state
                .approvals
                .request(
                    body.tool,
                    body.args,
                    body.thread_id,
                    body.session_id,
                    body.agent_id,
                    &state.tick_tx,
                    timeout,
                )
                .await
        }
    };
    Json(CheckResponse { decision })
}

async fn list(State(state): State<Arc<AppState>>) -> Json<Vec<ApprovalSummary>> {
    Json(state.approvals.list_pending())
}

#[derive(Debug, Deserialize)]
struct DecideBody {
    decision: Decision,
    #[serde(default)]
    remember_scope: Option<RememberScope>,
}

async fn decide(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<DecideBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.decision == Decision::Ask {
        return Err(ApiError::BadRequest(
            "approval decision must be allow or deny".to_string(),
        ));
    }

    let pending = state
        .approvals
        .list_pending()
        .into_iter()
        .find(|summary| summary.id == id);

    state
        .approvals
        .decide(&id, body.decision.clone(), &state.tick_tx)
        .map_err(ApiError::BadRequest)?;

    if let Some(scope @ (RememberScope::ToolOnly | RememberScope::ToolAndArgs)) =
        body.remember_scope
    {
        if let Some(summary) = pending {
            let rule = remembered_rule(&summary, body.decision, scope);
            if let Err(e) = state.policy.append_rule(rule) {
                tracing::warn!(approval_id = %id, error = %e, "failed to persist remembered policy rule");
            }
        }
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

fn remembered_rule(
    summary: &ApprovalSummary,
    decision: Decision,
    remember_scope: RememberScope,
) -> Rule {
    let args_match = match remember_scope {
        RememberScope::ToolAndArgs => string_args_match(&summary.args),
        RememberScope::ToolOnly | RememberScope::ThisCall => BTreeMap::new(),
    };
    Rule {
        tool: summary.tool.clone(),
        args_match,
        decision,
    }
}

fn string_args_match(args: &serde_json::Value) -> BTreeMap<String, String> {
    let Some(obj) = args.as_object() else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter_map(|(key, value)| value.as_str().map(|s| (key.clone(), s.to_string())))
        .collect()
}
