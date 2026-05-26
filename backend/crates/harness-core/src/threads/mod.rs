use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Thread {
    /// UUID v4 of the thread.
    pub id: String,
    /// Optional human-readable title.
    pub title: Option<String>,
    /// Unix timestamp (ms) of creation.
    pub created_at: i64,
}

impl Thread {
    pub fn new(id: String, title: Option<String>, created_at: i64) -> Self {
        Self {
            id,
            title,
            created_at,
        }
    }
}
