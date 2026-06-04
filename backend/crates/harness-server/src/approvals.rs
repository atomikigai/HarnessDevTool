use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use harness_policy::Decision;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[derive(Clone)]
pub struct ApprovalStore {
    pending: Arc<RwLock<HashMap<String, PendingApproval>>>,
}

struct PendingApproval {
    id: String,
    tool: String,
    args: serde_json::Value,
    thread_id: Option<String>,
    session_id: Option<String>,
    agent_id: Option<String>,
    role: Option<String>,
    tx: oneshot::Sender<Decision>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ApprovalSummary {
    pub id: String,
    pub tool: String,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub args: serde_json::Value,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub role: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ApprovalStore {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request(
        &self,
        tool: String,
        args: serde_json::Value,
        thread_id: Option<String>,
        session_id: Option<String>,
        agent_id: Option<String>,
        role: Option<String>,
        tick_tx: &broadcast::Sender<String>,
        timeout: Duration,
    ) -> Decision {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let (tx, rx) = oneshot::channel();
        let pending = PendingApproval {
            id: id.clone(),
            tool,
            args,
            thread_id,
            session_id,
            agent_id,
            role,
            tx,
            created_at,
        };
        let summary = pending.summary();

        self.pending
            .write()
            .expect("approvals rwlock")
            .insert(id.clone(), pending);
        emit(
            tick_tx,
            json!({
                "type": "approval.requested",
                "summary": summary,
            }),
        );

        tokio::select! {
            result = rx => result.unwrap_or(Decision::Deny),
            () = tokio::time::sleep(timeout) => {
                self.pending.write().expect("approvals rwlock").remove(&id);
                Decision::Deny
            }
        }
    }

    pub fn decide(
        &self,
        id: &str,
        decision: Decision,
        tick_tx: &broadcast::Sender<String>,
    ) -> Result<(), String> {
        let Some(pending) = self.pending.write().expect("approvals rwlock").remove(id) else {
            return Err(format!("approval not found: {id}"));
        };
        let _ = pending.tx.send(decision);
        emit(
            tick_tx,
            json!({
                "type": "approval.resolved",
                "id": id,
            }),
        );
        Ok(())
    }

    pub fn list_pending(&self) -> Vec<ApprovalSummary> {
        self.pending
            .read()
            .expect("approvals rwlock")
            .values()
            .map(PendingApproval::summary)
            .collect()
    }
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingApproval {
    fn summary(&self) -> ApprovalSummary {
        ApprovalSummary {
            id: self.id.clone(),
            tool: self.tool.clone(),
            args: self.args.clone(),
            thread_id: self.thread_id.clone(),
            session_id: self.session_id.clone(),
            agent_id: self.agent_id.clone(),
            role: self.role.clone(),
            created_at: self.created_at,
        }
    }
}

fn emit(tx: &broadcast::Sender<String>, value: serde_json::Value) {
    let _ = tx.send(value.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn decide_resolves_pending_and_returns_decision() {
        let store = ApprovalStore::new();
        let (tick_tx, _) = broadcast::channel(8);
        let request_store = store.clone();
        let request_tx = tick_tx.clone();
        let fut = tokio::spawn(async move {
            request_store
                .request(
                    "db_query".to_string(),
                    json!({ "sql": "select 1" }),
                    None,
                    None,
                    None,
                    None,
                    &request_tx,
                    Duration::from_secs(5),
                )
                .await
        });

        let id = loop {
            let pending = store.list_pending();
            if let Some(summary) = pending.first() {
                break summary.id.clone();
            }
            tokio::task::yield_now().await;
        };
        store.decide(&id, Decision::Allow, &tick_tx).unwrap();
        assert_eq!(fut.await.unwrap(), Decision::Allow);
        assert!(store.list_pending().is_empty());
    }

    #[tokio::test]
    async fn timeout_removes_pending_and_returns_deny() {
        let store = ApprovalStore::new();
        let (tick_tx, _) = broadcast::channel(8);
        let decision = store
            .request(
                "db_query".to_string(),
                json!({ "sql": "select 1" }),
                None,
                None,
                None,
                None,
                &tick_tx,
                Duration::from_millis(50),
            )
            .await;
        assert_eq!(decision, Decision::Deny);
        assert!(store.list_pending().is_empty());
    }
}
