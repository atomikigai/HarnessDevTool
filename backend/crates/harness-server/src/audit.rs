use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use harness_policy::Decision;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

const MAX_BRIDGE_AUDIT_BYTES: u64 = 10 * 1024 * 1024;
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct BridgeAuditRecord {
    pub schema_version: u32,
    pub at: i64,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_role: Option<String>,
    pub tool: String,
    pub resource: Option<String>,
    pub decision: Decision,
    pub reason: String,
    pub input_hash: String,
    pub result_hash: String,
}

impl BridgeAuditRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn capability_decision(
        tool: &str,
        args: &serde_json::Value,
        thread_id: Option<&str>,
        session_id: Option<&str>,
        actor_id: Option<&str>,
        actor_role: Option<&str>,
        decision: Decision,
        reason: impl Into<String>,
    ) -> Self {
        let input = json!({
            "tool": tool,
            "args": args,
            "thread_id": thread_id,
            "session_id": session_id,
            "actor_id": actor_id,
            "actor_role": actor_role,
        });
        let result = json!({ "decision": decision });
        Self {
            schema_version: 1,
            at: Utc::now().timestamp_millis(),
            thread_id: thread_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            actor_id: actor_id.map(str::to_string),
            actor_role: actor_role.map(str::to_string),
            tool: tool.to_string(),
            resource: resource_from_args(args),
            decision,
            reason: reason.into(),
            input_hash: hash_json(&input),
            result_hash: hash_json(&result),
        }
    }
}

pub fn append_bridge_audit(harness_home: &Path, record: &BridgeAuditRecord) -> std::io::Result<()> {
    append_bridge_audit_with_limit(harness_home, record, MAX_BRIDGE_AUDIT_BYTES)
}

fn append_bridge_audit_with_limit(
    harness_home: &Path,
    record: &BridgeAuditRecord,
    max_bytes: u64,
) -> std::io::Result<()> {
    let dir = harness_home.join(".runtime").join("audit");
    std::fs::create_dir_all(&dir)?;
    set_private_dir_permissions(&dir)?;
    let path = dir.join("bridge.jsonl");
    rotate_bridge_audit_if_needed(&path, max_bytes)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(&mut file, record)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn rotate_bridge_audit_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<()> {
    if max_bytes == 0 {
        return Ok(());
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < max_bytes {
        return Ok(());
    }

    let rotated_path = next_rotated_path(path);
    compress_file(path, &rotated_path)?;
    std::fs::remove_file(path)?;
    Ok(())
}

fn next_rotated_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bridge.jsonl");
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    let mut candidate = dir.join(format!("{base}.{stamp}.zst"));
    let mut n = 1u32;
    while candidate.exists() {
        candidate = dir.join(format!("{base}.{stamp}.{n}.zst"));
        n += 1;
    }
    candidate
}

fn compress_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut input = std::fs::File::open(src)?;
    let output = create_private_file(dst)?;
    let mut encoder = zstd::Encoder::new(output, ZSTD_LEVEL)?;
    std::io::copy(&mut input, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    set_private_file_permissions(path)?;
    Ok(file)
}

fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub(crate) fn hash_json(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    format!("sha256:{out}")
}

fn resource_from_args(args: &serde_json::Value) -> Option<String> {
    let obj = args.as_object()?;
    for key in [
        "path",
        "remote_path",
        "local_path",
        "task_id",
        "connection",
        "database",
        "host",
        "section",
    ] {
        if let Some(value) = obj.get(key).and_then(|value| value.as_str()) {
            return Some(format!("{key}:{value}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bridge_audit_appends_jsonl_with_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let record = BridgeAuditRecord::capability_decision(
            "repo_write_file",
            &json!({ "path": "src/lib.rs", "content": "x" }),
            Some("thread-1"),
            Some("session-1"),
            Some("agent-1"),
            Some("generator"),
            Decision::Allow,
            "policy allow",
        );

        append_bridge_audit(dir.path(), &record).unwrap();
        let text = std::fs::read_to_string(dir.path().join(".runtime/audit/bridge.jsonl")).unwrap();
        let line: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(line["tool"], "repo_write_file");
        assert_eq!(line["resource"], "path:src/lib.rs");
        assert_eq!(line["decision"], "allow");
        assert!(line["input_hash"].as_str().unwrap().starts_with("sha256:"));
        assert!(line["result_hash"].as_str().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn bridge_audit_rotates_to_zstd_when_active_log_is_large() {
        let dir = tempfile::tempdir().unwrap();
        let audit_dir = dir.path().join(".runtime/audit");
        std::fs::create_dir_all(&audit_dir).unwrap();
        let active = audit_dir.join("bridge.jsonl");
        std::fs::write(&active, b"{\"old\":true}\n").unwrap();
        let record = BridgeAuditRecord::capability_decision(
            "task_list",
            &json!({}),
            Some("thread-1"),
            Some("session-1"),
            Some("agent-1"),
            Some("planner"),
            Decision::Allow,
            "policy allow",
        );

        append_bridge_audit_with_limit(dir.path(), &record, 1).unwrap();

        let active_text = std::fs::read_to_string(&active).unwrap();
        assert_eq!(active_text.lines().count(), 1);
        assert!(active_text.contains("\"tool\":\"task_list\""));

        let rotated = std::fs::read_dir(&audit_dir)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("zst"))
            .expect("rotated zstd audit file");
        let bytes = std::fs::read(rotated).unwrap();
        let decoded = zstd::decode_all(bytes.as_slice()).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "{\"old\":true}\n");
    }
}
