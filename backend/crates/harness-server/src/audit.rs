use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use chrono::Utc;
use harness_policy::Decision;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

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
    let dir = harness_home.join(".runtime").join("audit");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("bridge.jsonl");
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

fn hash_json(value: &serde_json::Value) -> String {
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
}
