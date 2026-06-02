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
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read spec: {e}")),
    };
    Ok(json!({ "content": content }))
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
