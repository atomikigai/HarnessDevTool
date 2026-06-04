//! Per-thread budget routes.
//!
//! GET  /api/threads/:tid/budget  -> current spend snapshot
//! POST /api/threads/:tid/budget  -> set the limit (USD)
//!
//! The scheduler's budget pass (see `harness-core::scheduler::run_budget_pass`)
//! writes `spent_usd` on every tick; clients should rely on the periodic SSE
//! `budget.warning` event for change notifications rather than polling.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use harness_core::{AgentCost, Budget, RoleCost, SessionCostView, TaskCost};
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/threads/:tid/budget", get(get_budget).post(set_budget))
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SetBudgetRequest {
    pub limit_usd: f64,
    #[serde(default, deserialize_with = "deserialize_max_concurrent_workers")]
    pub max_concurrent_workers: Option<Option<usize>>,
}

fn deserialize_max_concurrent_workers<'de, D>(
    deserializer: D,
) -> Result<Option<Option<usize>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Option<Option<usize>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("null or a positive integer")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(None))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            usize::deserialize(deserializer).map(|value| Some(Some(value)))
        }
    }

    deserializer.deserialize_option(Visitor)
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct BudgetView {
    pub thread_id: String,
    pub spent_usd: f64,
    pub limit_usd: f64,
    pub pct: u8,
    pub soft_pct: u8,
    pub hard_pct: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_workers: Option<usize>,
    #[serde(default)]
    pub agents: Vec<AgentCost>,
    #[serde(default)]
    pub tasks: Vec<TaskCost>,
    #[serde(default)]
    pub roles: Vec<RoleCost>,
    #[serde(default)]
    pub sessions: Vec<SessionCostView>,
}

fn view(
    thread_id: &str,
    b: &Budget,
    agents: Vec<AgentCost>,
    tasks: Vec<TaskCost>,
    roles: Vec<RoleCost>,
    sessions: Vec<SessionCostView>,
) -> BudgetView {
    BudgetView {
        thread_id: thread_id.to_string(),
        spent_usd: b.spent_usd,
        limit_usd: b.limit_usd,
        pct: b.pct_spent(),
        soft_pct: b.soft_pct,
        hard_pct: b.hard_pct,
        max_concurrent_workers: b.max_concurrent_workers,
        agents,
        tasks,
        roles,
        sessions,
    }
}

async fn get_budget(
    State(s): State<Arc<AppState>>,
    Path(tid): Path<String>,
) -> ApiResult<Json<BudgetView>> {
    let b = s.budgets.get(&tid);
    let breakdown = s.budgets.breakdown_for(&tid);
    Ok(Json(view(
        &tid,
        &b,
        breakdown.agents,
        breakdown.tasks,
        breakdown.roles,
        breakdown.sessions,
    )))
}

async fn set_budget(
    State(s): State<Arc<AppState>>,
    Path(tid): Path<String>,
    Json(body): Json<SetBudgetRequest>,
) -> ApiResult<Json<BudgetView>> {
    if !body.limit_usd.is_finite() || body.limit_usd < 0.0 {
        return Err(ApiError::BadRequest(
            "limit_usd must be a finite non-negative number".into(),
        ));
    }
    if matches!(body.max_concurrent_workers, Some(Some(0))) {
        return Err(ApiError::BadRequest(
            "max_concurrent_workers must be >= 1 when set".into(),
        ));
    }
    let mut b = s.budgets.set_limit(&tid, body.limit_usd)?;
    if let Some(max_concurrent_workers) = body.max_concurrent_workers {
        if b.max_concurrent_workers != max_concurrent_workers {
            b = s
                .budgets
                .set_max_concurrent_workers(&tid, max_concurrent_workers)?;
        }
    }
    let breakdown = s.budgets.breakdown_for(&tid);
    Ok(Json(view(
        &tid,
        &b,
        breakdown.agents,
        breakdown.tasks,
        breakdown.roles,
        breakdown.sessions,
    )))
}

#[cfg(test)]
mod tests {
    use super::SetBudgetRequest;

    #[test]
    fn set_budget_request_preserves_when_max_workers_absent() {
        let req: SetBudgetRequest = serde_json::from_str(r#"{"limit_usd":5.0}"#).unwrap();
        assert_eq!(req.limit_usd, 5.0);
        assert_eq!(req.max_concurrent_workers, None);
    }

    #[test]
    fn set_budget_request_clears_when_max_workers_null() {
        let req: SetBudgetRequest =
            serde_json::from_str(r#"{"limit_usd":5.0,"max_concurrent_workers":null}"#).unwrap();
        assert_eq!(req.max_concurrent_workers, Some(None));
    }

    #[test]
    fn set_budget_request_sets_when_max_workers_number() {
        let req: SetBudgetRequest =
            serde_json::from_str(r#"{"limit_usd":5.0,"max_concurrent_workers":2}"#).unwrap();
        assert_eq!(req.max_concurrent_workers, Some(Some(2)));
    }
}
