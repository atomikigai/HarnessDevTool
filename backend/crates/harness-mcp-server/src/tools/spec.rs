//! `spec_read` — read `<home>/profiles/default/threads/<tid>/spec.md`.

use std::path::Path;

use serde_json::{json, Value};

pub fn read(home: &Path, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = args
        .get("thread_id")
        .and_then(|v| v.as_str())
        .unwrap_or(default_thread);
    let path = home
        .join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.md");
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read spec: {e}")),
    };
    Ok(json!({ "content": content }))
}
