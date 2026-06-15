//! Session attachment MCP tools.

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use serde_json::{json, Value};

const DEFAULT_MAX_BYTES: u64 = 512 * 1024;
const HARD_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn list(harness_home: &Path, session_id: Option<&str>) -> Result<Value, String> {
    let dir = attachment_dir(harness_home, session_id)?;
    if !dir.exists() {
        return Ok(json!({ "attachments": [] }));
    }

    let mut attachments = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("attach_list: {e}"))? {
        let entry = entry.map_err(|e| format!("attach_list: {e}"))?;
        let file_type = entry.file_type().map_err(|e| format!("attach_list: {e}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_safe_segment(&name) {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| format!("attach_list: metadata for {name}: {e}"))?;
        attachments.push(json!({
            "name": name,
            "size": meta.len(),
            "mime": attachment_content_type(&entry.path()),
        }));
    }
    attachments.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Ok(json!({ "attachments": attachments }))
}

pub fn read(harness_home: &Path, session_id: Option<&str>, args: &Value) -> Result<Value, String> {
    let dir = attachment_dir(harness_home, session_id)?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "attach_read requires string arg: name".to_string())?;
    if !is_safe_segment(name) {
        return Err(format!("attach_read: invalid attachment name `{name}`"));
    }

    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_BYTES)
        .clamp(1, HARD_MAX_BYTES);
    let path = dir.join(name);
    let canonical_dir = dir
        .canonicalize()
        .map_err(|_| "attach_read: no attachments for this session".to_string())?;
    let canonical_path = path
        .canonicalize()
        .map_err(|_| format!("attach_read: attachment not found `{name}`"))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(format!(
            "attach_read: attachment `{name}` escapes attachment directory"
        ));
    }

    let meta = std::fs::metadata(&canonical_path)
        .map_err(|e| format!("attach_read: metadata for `{name}`: {e}"))?;
    if !meta.is_file() {
        return Err(format!("attach_read: `{name}` is not a file"));
    }
    let bytes =
        std::fs::read(&canonical_path).map_err(|e| format!("attach_read: read `{name}`: {e}"))?;
    let truncated = bytes.len() as u64 > max_bytes;
    let kept_len = max_bytes.min(bytes.len() as u64) as usize;
    let kept = &bytes[..kept_len];
    let mime = attachment_content_type(&canonical_path);

    if let Ok(text) = std::str::from_utf8(kept) {
        Ok(json!({
            "name": name,
            "mime": mime,
            "size": meta.len(),
            "encoding": "utf-8",
            "truncated": truncated,
            "content": text,
        }))
    } else {
        Ok(json!({
            "name": name,
            "mime": mime,
            "size": meta.len(),
            "encoding": "base64",
            "truncated": truncated,
            "content_base64": BASE64_STANDARD.encode(kept),
        }))
    }
}

fn attachment_dir(harness_home: &Path, session_id: Option<&str>) -> Result<PathBuf, String> {
    let sid = session_id.ok_or_else(|| {
        "attachment tools require an MCP session id; restart the agent session".to_string()
    })?;
    if !is_safe_segment(sid) {
        return Err("attachment tools: invalid session id".to_string());
    }
    Ok(harness_home.join(".runtime/attach").join(sid))
}

fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains('/')
        && !segment.contains('\\')
        && !segment.contains('\0')
}

fn attachment_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" | "md" | "log" => "text/plain",
        "json" | "excalidraw" => "application/json",
        "csv" => "text/csv",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_reads_text_attachment() {
        let dir = tempfile::tempdir().unwrap();
        let sid = "session-1";
        let attach_dir = dir.path().join(".runtime/attach").join(sid);
        std::fs::create_dir_all(&attach_dir).unwrap();
        std::fs::write(attach_dir.join("notes.txt"), "hello attachment").unwrap();

        let listed = list(dir.path(), Some(sid)).unwrap();
        assert_eq!(listed["attachments"][0]["name"], "notes.txt");

        let read = read(dir.path(), Some(sid), &json!({ "name": "notes.txt" })).unwrap();
        assert_eq!(read["encoding"], "utf-8");
        assert_eq!(read["content"], "hello attachment");
        assert_eq!(read["truncated"], false);
    }

    #[test]
    fn read_rejects_traversal_name() {
        let dir = tempfile::tempdir().unwrap();
        let err = read(
            dir.path(),
            Some("session-1"),
            &json!({ "name": "../secret.txt" }),
        )
        .expect_err("traversal must fail");
        assert!(err.contains("invalid attachment name"));
    }
}
