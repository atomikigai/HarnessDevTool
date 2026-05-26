//! `skills_search` — stub until F5.

use serde_json::{json, Value};

pub fn search(_args: &Value) -> Result<Value, String> {
    Ok(json!([]))
}
