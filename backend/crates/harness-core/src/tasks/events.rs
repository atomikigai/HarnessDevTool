//! Per-thread broadcast events emitted by the [`super::store::TaskStore`] and
//! the [`crate::scheduler::Scheduler`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::Event;

use super::model::{SchedulerExplanation, TaskStatus};

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
    #[serde(rename = "task.reason.changed")]
    ReasonChanged {
        task_id: String,
        reason_kind: String,
        value: String,
        by: String,
        at: DateTime<Utc>,
    },
    #[serde(rename = "task.scheduler.decision")]
    SchedulerDecision {
        #[serde(flatten)]
        explanation: SchedulerExplanation,
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
        version: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section_version: Option<u64>,
        bytes: u64,
        at: DateTime<Utc>,
    },
    #[serde(rename = "artifact.added")]
    ArtifactAdded {
        thread_id: String,
        #[serde(default)]
        artifact_id: String,
        #[serde(default)]
        task_id: String,
        path: String,
        kind: String,
        #[serde(default)]
        produced_by: String,
        #[serde(default)]
        summary: String,
        at: DateTime<Utc>,
    },
}

impl TaskEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            TaskEvent::Created { .. } => "task.created",
            TaskEvent::Changed { .. } => "task.changed",
            TaskEvent::Updated { .. } => "task.updated",
            TaskEvent::ReasonChanged { .. } => "task.reason.changed",
            TaskEvent::SchedulerDecision { .. } => "task.scheduler.decision",
            TaskEvent::Ready { .. } => "task.ready",
            TaskEvent::LeaseExpired { .. } => "task.lease-expired",
            TaskEvent::SpecChanged { .. } => "spec.changed",
            TaskEvent::ArtifactAdded { .. } => "artifact.added",
        }
    }

    pub fn actor(&self) -> Option<&str> {
        match self {
            TaskEvent::Created { by, .. }
            | TaskEvent::Changed { by, .. }
            | TaskEvent::Updated { by, .. }
            | TaskEvent::ReasonChanged { by, .. } => Some(by.as_str()),
            TaskEvent::SchedulerDecision { .. } => Some("scheduler"),
            TaskEvent::Ready { .. } => Some("scheduler"),
            TaskEvent::LeaseExpired {
                previous_holder, ..
            } => Some(previous_holder.as_str()),
            TaskEvent::SpecChanged { .. } => None,
            TaskEvent::ArtifactAdded { produced_by, .. } if !produced_by.is_empty() => {
                Some(produced_by.as_str())
            }
            TaskEvent::ArtifactAdded { .. } => None,
        }
    }

    pub fn to_envelope(&self, tid: &str) -> Result<Event, serde_json::Error> {
        Ok(Event {
            seq: 0,
            at: Utc::now().timestamp_millis(),
            event_type: self.event_type().to_string(),
            items: Vec::new(),
            thread_id: Some(tid.to_string()),
            actor: self.actor().map(str::to_string),
            payload: Some(serde_json::to_value(self)?),
        })
    }

    pub fn task_id(&self) -> &str {
        match self {
            TaskEvent::Created { task_id, .. }
            | TaskEvent::Changed { task_id, .. }
            | TaskEvent::Updated { task_id, .. }
            | TaskEvent::ReasonChanged { task_id, .. }
            | TaskEvent::SchedulerDecision {
                explanation: SchedulerExplanation { task_id, .. },
            }
            | TaskEvent::Ready { task_id }
            | TaskEvent::LeaseExpired { task_id, .. } => task_id,
            TaskEvent::SpecChanged { .. } => "",
            TaskEvent::ArtifactAdded { task_id, .. } => task_id,
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
            version: 1,
            section: None,
            section_version: None,
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
    fn reason_changed_round_trips() {
        let ev = TaskEvent::ReasonChanged {
            task_id: "T-0001".to_string(),
            reason_kind: "blocked_reason".to_string(),
            value: "Waiting on T-0000".to_string(),
            by: "agent:planner".to_string(),
            at: Utc::now(),
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"task.reason.changed\""));
        let decoded: TaskEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ev);
        assert_eq!(decoded.task_id(), "T-0001");
    }

    #[test]
    fn scheduler_decision_round_trips() {
        let ev = TaskEvent::SchedulerDecision {
            explanation: SchedulerExplanation {
                task_id: "T-0001".to_string(),
                decision: super::super::model::SchedulerDecisionKind::AssignmentSkipped,
                reason: "Max concurrency reached".to_string(),
                agent_id: None,
                previous_holder: None,
                blocked_by: vec![],
                cooldown_seconds: None,
                max_concurrent: Some(1),
                queue_depth: Some(2),
                at: Utc::now(),
            },
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"task.scheduler.decision\""));
        let decoded: TaskEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ev);
        assert_eq!(decoded.task_id(), "T-0001");
        assert_eq!(decoded.actor(), Some("scheduler"));
    }

    #[test]
    fn artifact_added_round_trips() {
        let ev = TaskEvent::ArtifactAdded {
            thread_id: "t1".to_string(),
            artifact_id: "A-1".to_string(),
            task_id: "T-0001".to_string(),
            path: "spec.md".to_string(),
            kind: "file".to_string(),
            produced_by: "agent:a".to_string(),
            summary: "Spec file".to_string(),
            at: Utc::now(),
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"artifact.added\""));
        let decoded: TaskEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ev);
        assert_eq!(decoded.task_id(), "T-0001");
    }

    #[test]
    fn legacy_artifact_added_still_decodes() {
        let json = r#"{
            "type":"artifact.added",
            "thread_id":"t1",
            "path":"spec.md",
            "kind":"spec",
            "at":"2026-06-04T00:00:00Z"
        }"#;
        let decoded: TaskEvent = serde_json::from_str(json).unwrap();
        match decoded {
            TaskEvent::ArtifactAdded {
                artifact_id,
                task_id,
                produced_by,
                summary,
                ..
            } => {
                assert_eq!(artifact_id, "");
                assert_eq!(task_id, "");
                assert_eq!(produced_by, "");
                assert_eq!(summary, "");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
