//! `spec_read` / `spec_write` for `<home>/profiles/default/threads/<tid>/spec.md`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use harness_core::validate_thread_id;

const MAX_SPEC_BYTES: usize = 1_048_576;

pub fn read(home: &Path, default_thread: &str, args: &Value) -> Result<Value, String> {
    let thread_id = args
        .get("thread_id")
        .and_then(|v| v.as_str())
        .unwrap_or(default_thread);
    validate_thread_id(thread_id).map_err(|e| format!("spec_read: {e}"))?;
    let path = home
        .join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.md");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("read spec: {e}")),
    };
    let content = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(e) => return Err(format!("read spec utf8: {e}")),
    };
    let version = match spec_version(&spec_events_path(home, thread_id)) {
        Ok(version) => version,
        Err(e) => return Err(format!("read spec version: {e}")),
    };
    let etag = if bytes.is_empty() && !path.exists() {
        String::new()
    } else {
        sha256_hex(&bytes)
    };
    Ok(json!({ "content": content, "etag": etag, "version": version }))
}

pub fn write(
    home: &Path,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    validate_thread_id(thread_id).map_err(|e| format!("spec_write: {e}"))?;
    let content = str_arg(args, "content")?;
    validate_content(content)?;
    let etag = opt_str_arg(args, "etag")?;

    if let Some(base) = server_url {
        let url = format!(
            "{}/api/threads/{}/spec",
            base.trim_end_matches('/'),
            thread_id
        );
        let mut body = json!({ "content": content });
        if let Some(etag) = etag {
            body["etag"] = json!(etag);
        }
        let mut req = ureq::put(&url).timeout(Duration::from_secs(5));
        if let Some(token) = api_token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        return match req.send_json(&body) {
            Ok(resp) => resp.into_json().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                Err(format!("spec_write: server returned {code}: {body}"))
            }
            Err(e) => Err(format!("spec_write: HTTP delegation failed: {e}")),
        };
    }

    write_local(home, thread_id, content, etag)
}

pub fn set_section(
    home: &Path,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let thread_id = str_arg(args, "thread_id")?;
    validate_thread_id(thread_id).map_err(|e| format!("spec_set_section: {e}"))?;
    let section = str_arg(args, "section")?;
    validate_section(section)?;
    let content = str_arg(args, "content")?;
    validate_content(content)?;
    let version_required = args.get("spec_version_required").and_then(|v| v.as_u64());
    let by = opt_str_arg(args, "by")?;

    if let Some(base) = server_url {
        let url = format!(
            "{}/api/threads/{}/spec/sections/{}",
            base.trim_end_matches('/'),
            thread_id,
            section
        );
        let mut body = json!({ "content": content });
        if let Some(version_required) = version_required {
            body["spec_version_required"] = json!(version_required);
        }
        if let Some(by) = by {
            body["by"] = json!(by);
        }
        let mut req = ureq::put(&url).timeout(Duration::from_secs(5));
        if let Some(token) = api_token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        return match req.send_json(&body) {
            Ok(resp) => resp.into_json().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                Err(format!("spec_set_section: server returned {code}: {body}"))
            }
            Err(e) => Err(format!("spec_set_section: HTTP delegation failed: {e}")),
        };
    }

    set_section_local(home, thread_id, section, content, version_required)
}

fn write_local(
    home: &Path,
    thread_id: &str,
    content: &str,
    etag: Option<&str>,
) -> Result<Value, String> {
    let path = spec_path(home, thread_id);
    let current = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("spec_write: read current spec: {e}")),
    };
    if let Some(expected) = etag {
        let Some(bytes) = current.as_deref() else {
            return Err("spec_write: etag mismatch".to_string());
        };
        if sha256_hex(bytes) != expected {
            return Err("spec_write: etag mismatch".to_string());
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| "spec_write: invalid spec path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("spec_write: create parent: {e}"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("spec_write: temp file: {e}"))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("spec_write: write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("spec_write: flush temp file: {e}"))?;
    tmp.persist(&path)
        .map_err(|e| format!("spec_write: persist spec: {}", e.error))?;

    let bytes = content.len();
    Ok(json!({
        "ok": true,
        "etag": sha256_hex(content.as_bytes()),
        "bytes": bytes,
        "created": current.is_none(),
    }))
}

fn spec_path(home: &Path, thread_id: &str) -> PathBuf {
    home.join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.md")
}

fn spec_events_path(home: &Path, thread_id: &str) -> PathBuf {
    home.join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.events.jsonl")
}

fn set_section_local(
    home: &Path,
    thread_id: &str,
    section: &str,
    content: &str,
    version_required: Option<u64>,
) -> Result<Value, String> {
    let events_path = spec_events_path(home, thread_id);
    let current_version =
        spec_version(&events_path).map_err(|e| format!("spec_set_section: {e}"))?;
    if let Some(required) = version_required {
        if required != current_version {
            return Err(format!(
                "spec_set_section: spec_version_mismatch current_version={current_version}"
            ));
        }
    }
    let path = spec_path(home, thread_id);
    let current = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("spec_set_section: read current spec: {e}")),
    };
    let next_content = set_marked_section(&current, section, content);
    let parent = path
        .parent()
        .ok_or_else(|| "spec_set_section: invalid spec path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("spec_set_section: create parent: {e}"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("spec_set_section: temp file: {e}"))?;
    tmp.write_all(next_content.as_bytes())
        .map_err(|e| format!("spec_set_section: write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("spec_set_section: flush temp file: {e}"))?;
    tmp.persist(&path)
        .map_err(|e| format!("spec_set_section: persist spec: {}", e.error))?;

    let version = append_spec_event(&events_path, section)
        .map_err(|e| format!("spec_set_section: append event: {e}"))?;
    Ok(json!({
        "ok": true,
        "etag": sha256_hex(next_content.as_bytes()),
        "version": version,
        "section": section,
        "bytes": next_content.len(),
    }))
}

fn spec_version(path: &Path) -> std::io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u64)
}

fn append_spec_event(path: &Path, section: &str) -> std::io::Result<u64> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("invalid spec events path"))?;
    std::fs::create_dir_all(parent)?;
    let version = spec_version(path)? + 1;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(
        json!({ "version": version, "section": section })
            .to_string()
            .as_bytes(),
    )?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(version)
}

fn set_marked_section(current: &str, section: &str, section_content: &str) -> String {
    let start = format!("<!-- harness:section {section} -->");
    let end = format!("<!-- /harness:section {section} -->");
    let replacement = format!("{start}\n{}\n{end}", section_content.trim_matches('\n'));
    let Some(start_idx) = current.find(&start) else {
        let mut next = current.trim_end().to_string();
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(&replacement);
        next.push('\n');
        return next;
    };
    let Some(end_rel) = current[start_idx..].find(&end) else {
        let mut next = current.trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(&replacement);
        next.push('\n');
        return next;
    };
    let end_idx = start_idx + end_rel + end.len();
    let mut next = String::new();
    next.push_str(&current[..start_idx]);
    next.push_str(&replacement);
    next.push_str(&current[end_idx..]);
    next
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str_arg<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("non-string arg: {key}")),
        None => Ok(None),
    }
}

fn validate_content(content: &str) -> Result<(), String> {
    if content.len() > MAX_SPEC_BYTES {
        return Err(format!(
            "spec_write: content exceeds {MAX_SPEC_BYTES} byte limit"
        ));
    }
    Ok(())
}

fn validate_section(section: &str) -> Result<(), String> {
    let valid = !section.is_empty()
        && section.len() <= 128
        && section
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'));
    if valid {
        Ok(())
    } else {
        Err("spec_set_section: section must be 1-128 chars of [A-Za-z0-9_.:-]".into())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_content(value: &Value) -> &str {
        value["content"].as_str().unwrap()
    }

    #[test]
    fn write_then_read_round_trips_and_checks_etag() {
        let dir = tempfile::tempdir().unwrap();
        let first = write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "t1", "content": "# Spec\n" }),
        )
        .unwrap();

        assert_eq!(first["ok"], true);
        assert_eq!(first["bytes"], 7);
        assert_eq!(first["created"], true);
        let etag = first["etag"].as_str().unwrap();

        let read_back = read(dir.path(), "default", &json!({ "thread_id": "t1" })).unwrap();
        assert_eq!(read_content(&read_back), "# Spec\n");

        let second = write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "t1", "content": "updated", "etag": etag }),
        )
        .unwrap();
        assert_eq!(second["created"], false);
        assert_eq!(second["bytes"], 7);
    }

    #[test]
    fn rejects_etag_mismatch_and_nonexistent_etag() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "t1", "content": "current" }),
        )
        .unwrap();

        let err = write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "t1", "content": "new", "etag": "stale" }),
        )
        .unwrap_err();
        assert_eq!(err, "spec_write: etag mismatch");

        let err = write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "missing", "content": "new", "etag": "anything" }),
        )
        .unwrap_err();
        assert_eq!(err, "spec_write: etag mismatch");
    }

    #[test]
    fn rejects_invalid_thread_ids() {
        let dir = tempfile::tempdir().unwrap();
        for thread_id in ["", "..", "a/b"] {
            let err = write(
                dir.path(),
                None,
                None,
                &json!({ "thread_id": thread_id, "content": "x" }),
            )
            .unwrap_err();
            assert!(
                err.contains("thread_id"),
                "expected thread_id error for {thread_id:?}, got {err}"
            );
        }
    }

    #[test]
    fn rejects_oversize_content() {
        let dir = tempfile::tempdir().unwrap();
        let err = write(
            dir.path(),
            None,
            None,
            &json!({ "thread_id": "t1", "content": "x".repeat(MAX_SPEC_BYTES + 1) }),
        )
        .unwrap_err();
        assert!(err.contains("content exceeds"));
    }
}
