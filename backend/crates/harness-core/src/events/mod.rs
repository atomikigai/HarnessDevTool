use serde::{Deserialize, Serialize};

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
    pub seq: u64,
    /// Unix timestamp in milliseconds.
    pub at: i64,
    /// Event type discriminator (free-form in F0).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Optional structured items.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
}
