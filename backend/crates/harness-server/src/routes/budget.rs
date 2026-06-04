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
use serde::{Deserialize, Serialize};

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
    let b = s.budgets.set_limit(&tid, body.limit_usd)?;
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
