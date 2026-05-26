//! Per-thread broadcast events emitted by the [`super::store::TaskStore`] and
//! the [`crate::scheduler::Scheduler`].

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::model::TaskStatus;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TaskEvent {
    #[serde(rename = "task.created")]
    Created {
        task_id: String,
        by: String,
        at: DateTime<Utc>,
    },
    #[serde(rename = "task.changed")]
    Changed {
        task_id: String,
        prev_status: TaskStatus,
        next_status: TaskStatus,
        by: String,
        at: DateTime<Utc>,
    },
    #[serde(rename = "task.updated")]
    Updated {
        task_id: String,
        by: String,
        at: DateTime<Utc>,
        fields: Vec<String>,
    },
    #[serde(rename = "task.ready")]
    Ready { task_id: String },
    #[serde(rename = "task.lease-expired")]
    LeaseExpired {
        task_id: String,
        previous_holder: String,
    },
}

impl TaskEvent {
    pub fn task_id(&self) -> &str {
        match self {
            TaskEvent::Created { task_id, .. }
            | TaskEvent::Changed { task_id, .. }
            | TaskEvent::Updated { task_id, .. }
            | TaskEvent::Ready { task_id }
            | TaskEvent::LeaseExpired { task_id, .. } => task_id,
        }
    }
}
