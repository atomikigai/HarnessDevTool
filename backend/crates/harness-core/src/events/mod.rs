use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Item is a single semantic unit inside an event (text, tool call, etc.).
/// F0: just a placeholder to lock the binding shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum Item {
    Text { text: String },
}

/// Append-only event written to a thread's `events.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Event {
    /// Monotonic sequence number within the thread (0-based).
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub seq: u64,
    /// Unix timestamp in milliseconds.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub at: i64,
    /// Event type discriminator (free-form in F0).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Optional structured items.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TimelineEntity {
    pub kind: String,
    pub id: String,
}

impl TimelineEntity {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TimelineItem {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub seq: u64,
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub at: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<TimelineEntity>,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TimelineReport {
    pub thread_id: String,
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub generated_at: i64,
    pub event_count: usize,
    pub items: Vec<TimelineItem>,
}

impl TimelineItem {
    pub fn from_event(event: Event) -> Self {
        let entity = event_entity(&event);
        let summary = event_summary(&event, entity.as_ref());
        Self {
            seq: event.seq,
            at: event.at,
            event_type: event.event_type,
            actor: event.actor,
            summary,
            entity,
            payload: event.payload,
        }
    }
}

fn event_entity(event: &Event) -> Option<TimelineEntity> {
    let payload = event.payload.as_ref()?;
    for (kind, key) in [
        ("task", "task_id"),
        ("artifact", "artifact_id"),
        ("session", "session_id"),
        ("thread", "thread_id"),
    ] {
        if let Some(value) = payload.get(key).and_then(|v| v.as_str()) {
            if !value.trim().is_empty() {
                return Some(TimelineEntity::new(kind, value));
            }
        }
    }
    None
}

fn event_summary(event: &Event, entity: Option<&TimelineEntity>) -> String {
    let subject = entity
        .map(|e| format!("{} {}", e.kind, e.id))
        .unwrap_or_else(|| "thread".to_string());
    match event.event_type.as_str() {
        "task.created" => format!("Created {subject}"),
        "task.changed" => format!("Changed {subject}"),
        "task.updated" => {
            let fields = payload_array(&event.payload, "fields");
            if fields.is_empty() {
                format!("Updated {subject}")
            } else {
                format!("Updated {subject}: {}", fields.join(", "))
            }
        }
        "task.reason.changed" => {
            let reason = payload_str(&event.payload, "reason_kind").unwrap_or("reason");
            format!("Changed {subject} {reason}")
        }
        "task.scheduler.decision" => {
            let reason = event
                .payload
                .as_ref()
                .and_then(|payload| payload.get("explanation"))
                .and_then(|explanation| explanation.get("reason"))
                .and_then(|reason| reason.as_str())
                .unwrap_or("Scheduler decision recorded");
            format!("Scheduler: {reason}")
        }
        "task.ready" => format!("{subject} is ready"),
        "task.lease-expired" => format!("Lease expired for {subject}"),
        "artifact.added" => format!("Added {subject}"),
        "spec.changed" => {
            let section = payload_str(&event.payload, "section").unwrap_or("spec");
            format!("Changed spec section {section}")
        }
        "thread.readiness.checked" => {
            let status = payload_str(&event.payload, "status").unwrap_or("unknown");
            format!("Readiness checked: {status}")
        }
        "thread.autonomy.changed" => "Autonomy profile changed".to_string(),
        "handoff.created" => format!("Created handoff for {subject}"),
        "capability.decided" => "Capability decision recorded".to_string(),
        other => format!("Recorded {other}"),
    }
}

fn payload_str<'a>(payload: &'a Option<Value>, key: &str) -> Option<&'a str> {
    payload.as_ref()?.get(key)?.as_str()
}

fn payload_array(payload: &Option<Value>, key: &str) -> Vec<String> {
    payload
        .as_ref()
        .and_then(|payload| payload.get(key))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_item_summarizes_task_update_fields() {
        let item = TimelineItem::from_event(Event {
            seq: 7,
            at: 123,
            event_type: "task.updated".into(),
            items: vec![],
            thread_id: Some("thr-1".into()),
            actor: Some("scheduler".into()),
            payload: Some(serde_json::json!({
                "type": "task.updated",
                "task_id": "T-0001",
                "fields": ["status", "assignee"]
            })),
        });
        assert_eq!(item.summary, "Updated task T-0001: status, assignee");
        assert_eq!(item.entity.unwrap(), TimelineEntity::new("task", "T-0001"));
    }

    #[test]
    fn timeline_item_keeps_legacy_event_visible() {
        let item = TimelineItem::from_event(Event {
            seq: 0,
            at: 123,
            event_type: "legacy.event".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        });
        assert_eq!(item.summary, "Recorded legacy.event");
        assert!(item.entity.is_none());
    }
}
