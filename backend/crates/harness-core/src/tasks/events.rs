//! Per-thread broadcast events emitted by the [`super::store::TaskStore`] and
//! the [`crate::scheduler::Scheduler`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::model::TaskStatus;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(tag = "type", rename_all = "kebab-case")]
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
    #[serde(rename = "spec.changed")]
    SpecChanged {
        thread_id: String,
        etag: String,
        bytes: u64,
        at: DateTime<Utc>,
    },
    #[serde(rename = "artifact.added")]
    ArtifactAdded {
        thread_id: String,
        path: String,
        kind: String,
        at: DateTime<Utc>,
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
            TaskEvent::SpecChanged { .. } | TaskEvent::ArtifactAdded { .. } => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_changed_round_trips() {
        let ev = TaskEvent::SpecChanged {
            thread_id: "t1".to_string(),
            etag: "abc123".to_string(),
            bytes: 42,
            at: Utc::now(),
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"spec.changed\""));
        let decoded: TaskEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ev);
        assert_eq!(decoded.task_id(), "");
    }

    #[test]
    fn artifact_added_round_trips() {
        let ev = TaskEvent::ArtifactAdded {
            thread_id: "t1".to_string(),
            path: "spec.md".to_string(),
            kind: "spec".to_string(),
            at: Utc::now(),
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"artifact.added\""));
        let decoded: TaskEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ev);
        assert_eq!(decoded.task_id(), "");
    }
}
