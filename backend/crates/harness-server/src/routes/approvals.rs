use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use harness_core::AutonomyProfile;
use harness_core::{Event, Item};
use harness_policy::{Decision, RememberScope, Rule};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckResponse {
    decision: Decision,
}

async fn check(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CheckBody>,
) -> Json<CheckResponse> {
    let policy_decision = state
        .policy
        .evaluate(&body.tool, &body.args, body.role.as_deref());
    if matches!(policy_decision, Decision::Deny | Decision::Ask) {
        if let Some(thread_id) = body.thread_id.as_deref() {
            if let Err(e) = append_capability_decided_event(
                &state,
                thread_id,
                &body.tool,
                body.role.as_deref(),
                &policy_decision,
                body.agent_id.as_deref(),
            ) {
                tracing::warn!(thread_id, error = %e, "failed to append capability decision event");
            }
        }
    }
    let decision = policy_decision;
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
                    body.role,
                    &state.tick_tx,
                    timeout,
                )
                .await
        }
    };
    Json(CheckResponse { decision })
}

fn append_capability_decided_event(
    state: &AppState,
    thread_id: &str,
    tool: &str,
    role: Option<&str>,
    decision: &Decision,
    agent_id: Option<&str>,
) -> ApiResult<()> {
    let event = Event {
        seq: 0,
        at: Utc::now().timestamp_millis(),
        event_type: "capability.decided".to_string(),
        items: vec![Item::Text {
            text: serde_json::to_string(&json!({
                "tool": tool,
                "role": role,
                "decision": decision,
                "agent_id": agent_id,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }],
        thread_id: Some(thread_id.to_string()),
        actor: agent_id.map(str::to_string),
        payload: Some(json!({
            "tool": tool,
            "role": role,
            "decision": decision,
            "agent_id": agent_id,
        })),
    };
    state.store.append_event(thread_id, &event)?;
    Ok(())
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
        role: summary.role.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn state(home: std::path::PathBuf) -> Arc<AppState> {
        Arc::new(
            AppState::new(&Config {
                bind: "127.0.0.1:7777".parse().unwrap(),
                home,
                cors_origin: "http://localhost:8080".to_string(),
                profile: "default".to_string(),
                autonomy_profile: harness_core::AutonomyProfile::Assisted,
                api_token: None,
            })
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn check_applies_role_policy_and_appends_capability_event() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let thread = state
            .store
            .create_thread(Some("policy audit".to_string()))
            .unwrap();
        let app = router().with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/approvals/check")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "tool": "task_create",
                            "args": {},
                            "thread_id": &thread.id,
                            "agent_id": "agent:worker",
                            "role": "worker",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["decision"], "deny");

        let events = state.store.read_events(&thread.id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "capability.decided");
        let Item::Text { text } = &events[0].items[0];
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["tool"], "task_create");
        assert_eq!(payload["role"], "worker");
        assert_eq!(payload["decision"], "deny");
        assert_eq!(payload["agent_id"], "agent:worker");
    }

    #[test]
    fn remembered_rule_preserves_approval_role() {
        let summary = ApprovalSummary {
            id: "approval-1".to_string(),
            tool: "db_query".to_string(),
            args: json!({ "sql": "select * from users", "limit": 10 }),
            thread_id: Some("thread-1".to_string()),
            session_id: Some("session-1".to_string()),
            agent_id: Some("agent:worker".to_string()),
            role: Some("worker".to_string()),
            created_at: Utc::now(),
        };

        let rule = remembered_rule(&summary, Decision::Allow, RememberScope::ToolAndArgs);

        assert_eq!(rule.tool, "db_query");
        assert_eq!(rule.role.as_deref(), Some("worker"));
        assert_eq!(
            rule.args_match.get("sql").map(String::as_str),
            Some("select * from users")
        );
        assert!(!rule.args_match.contains_key("limit"));
        assert_eq!(rule.decision, Decision::Allow);
    }
}
