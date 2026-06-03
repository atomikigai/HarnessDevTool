use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use harness_core::AutonomyProfile;
use harness_policy::{
    capability::infer_resource, Actor, CapabilityCheck, CapabilityDecision, Decision,
    RememberScope, Rule,
};
use serde::{Deserialize, Serialize};

use crate::approvals::ApprovalSummary;
use crate::auth::CallerIdentity;
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
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckResponse {
    decision: Decision,
}

async fn check(
    State(state): State<Arc<AppState>>,
    caller: Option<Extension<CallerIdentity>>,
    Json(body): Json<CheckBody>,
) -> Json<CheckResponse> {
    let actor = actor_for_check(caller.map(|Extension(caller)| caller), &body);
    let resource = infer_resource(&body.tool, &body.args, body.thread_id.clone());
    let capability = state.policy.authorize(CapabilityCheck {
        actor: &actor,
        tool: &body.tool,
        resource,
        args: &body.args,
    });
    if let CapabilityDecision::Deny { reason } = capability {
        tracing::warn!(
            tool = %body.tool,
            role = %actor.role,
            agent_id = %actor.agent_id,
            ?reason,
            "capability policy denied approval check"
        );
        return Json(CheckResponse {
            decision: Decision::Deny,
        });
    }

    let decision = state.policy.evaluate(&body.tool, &body.args);
    let decision = match decision {
        Decision::Allow => Decision::Allow,
        Decision::Deny => Decision::Deny,
        Decision::Ask if should_auto_allow_for_thread(&state, body.thread_id.as_deref()) => {
            Decision::Allow
        }
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

fn actor_for_check(caller: Option<CallerIdentity>, body: &CheckBody) -> Actor {
    let role = body
        .role
        .as_deref()
        .or_else(|| caller.as_ref().map(|caller| caller.role.as_str()))
        .unwrap_or("human")
        .to_string();
    let agent_id = body
        .agent_id
        .as_deref()
        .or_else(|| caller.as_ref().map(|caller| caller.id.as_str()))
        .unwrap_or("human")
        .to_string();

    Actor {
        agent_id,
        role,
        session_id: body.session_id.clone(),
    }
}

fn should_auto_allow_for_thread(state: &AppState, thread_id: Option<&str>) -> bool {
    let Some(thread_id) = thread_id else {
        return false;
    };
    match state.store.get_thread(thread_id) {
        Ok(thread) => matches!(
            thread.autonomy_profile,
            Some(AutonomyProfile::Autonomous | AutonomyProfile::Ci)
        ),
        Err(e) => {
            tracing::warn!(thread_id, error = %e, "approval autonomy lookup failed");
            false
        }
    }
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
