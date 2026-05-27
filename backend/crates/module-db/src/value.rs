//! JSON-friendly cell value. Used both at the wire (REST + ts-rs) and as the
//! intermediate type between sqlx row decode and serde_json.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// Polymorphic cell value. Serialized as plain JSON values; non-JSON-native
/// types use string forms to preserve precision/bytes.
///
/// - `Decimal(s)` — string form, no float roundtrip.
/// - `Bytes(b64)` — base64 string.
/// - `Date`/`Time`/`DateTime` — ISO 8601 strings.
/// - `Json` — passthrough JSON value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// Wrapped tag values to disambiguate from plain strings/JSON.
    Tagged(TaggedValue),
}

/// Tagged value forms — JSON `{ "_t": "decimal", "v": "..." }` so the frontend
/// can recover semantic type when needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(tag = "_t", content = "v", rename_all = "snake_case")]
pub enum TaggedValue {
    Decimal(String),
    Bytes(String), // base64
    Date(String),
    Time(String),
    DateTime(String),
    Json(serde_json::Value),
}

impl Value {
    pub fn decimal(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Decimal(s.into()))
    }
    pub fn bytes(b: &[u8]) -> Self {
        Value::Tagged(TaggedValue::Bytes(
            base64::engine::general_purpose::STANDARD.encode(b),
        ))
    }
    pub fn datetime(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::DateTime(s.into()))
    }
    pub fn date(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Date(s.into()))
    }
    pub fn time(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Time(s.into()))
    }
    pub fn json(v: serde_json::Value) -> Self {
        Value::Tagged(TaggedValue::Json(v))
    }

    /// Convert to a `serde_json::Value` for inclusion in a `QueryResult`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl From<Value> for serde_json::Value {
    fn from(v: Value) -> Self {
        v.to_json()
    }
}
