//! Compact evidence packs for review and QA handoffs.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use harness_core::TaskStore;
use harness_session::SessionMeta;
use serde_json::{json, Value};

const MAX_CMD_BYTES: usize = 24 * 1024;
const MAX_PATHS: usize = 80;

pub fn pack(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    cwd: &Path,
    thread_id: &str,
    current_session_id: Option<&str>,
    current_task_id: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let session_id = opt_str(args, "session_id").or(current_session_id);
    let task_id = opt_str(args, "task_id").or(current_task_id);
    let requested_paths = requested_paths(cwd, args)?;

    let task = match task_id {
        Some(task_id) => store.get(thread_id, task_id).ok().map(|task| {
            let artifacts = store
                .list_artifacts(thread_id, task_id)
                .unwrap_or_default()
                .into_iter()
                .take(20)
                .map(|artifact| {
                    json!({
                        "id": artifact.artifact_id,
                        "kind": artifact.kind,
                        "path": artifact.path,
                        "summary": artifact.summary,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "id": task.id,
                "title": task.title,
                "status": task.status,
                "assignee": task.assignee,
                "labels": task.labels,
                "acceptance_count": task.acceptance.checks.len(),
                "artifact_count": artifacts.len(),
                "artifacts": artifacts,
                "scheduler_explanation": task.scheduler_explanation,
            })
        }),
        None => None,
    };

    let session = match session_id {
        Some(session_id) => read_session_meta(harness_home, profile, session_id)
            .ok()
            .map(session_summary),
        None => None,
    };

    let git = git_evidence(cwd, &requested_paths);
    let hints = next_steps(task_id, session_id, &git);

    Ok(json!({
        "thread_id": thread_id,
        "session_id": session_id,
        "task_id": task_id,
        "cwd": cwd.display().to_string(),
        "scope": {
            "requested_paths": requested_paths,
            "path_limit": MAX_PATHS,
        },
        "git": git,
        "task": task,
        "session": session,
        "evidence_gaps": [
            "Use transcript_tool_results or transcript_search for indexed command/tool evidence when session transcript is available.",
            "Screenshots/artifacts appear only when attached to the task artifact list."
        ],
        "next_steps": hints,
    }))
}

fn git_evidence(cwd: &Path, paths: &[String]) -> Value {
    if !is_git_repo(cwd) {
        return json!({
            "available": false,
            "reason": "cwd is not inside a git worktree",
        });
    }
    json!({
        "available": true,
        "status_short": run_git(cwd, ["status", "--short"], paths),
        "changed_files": run_git(cwd, ["diff", "--name-status"], paths),
        "stat": run_git(cwd, ["diff", "--stat"], paths),
        "staged_changed_files": run_git(cwd, ["diff", "--cached", "--name-status"], paths),
        "staged_stat": run_git(cwd, ["diff", "--cached", "--stat"], paths),
    })
}

fn is_git_repo(cwd: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn run_git<const N: usize>(cwd: &Path, args: [&str; N], paths: &[String]) -> Value {
    let mut command = Command::new("git");
    command.args(args).arg("--");
    for path in paths {
        command.arg(path);
    }
    match command.current_dir(cwd).output() {
        Ok(out) => json!({
            "ok": out.status.success(),
            "stdout": truncate(String::from_utf8_lossy(&out.stdout).trim()),
            "stderr": truncate(String::from_utf8_lossy(&out.stderr).trim()),
        }),
        Err(e) => json!({
            "ok": false,
            "stdout": "",
            "stderr": e.to_string(),
        }),
    }
}

fn requested_paths(cwd: &Path, args: &Value) -> Result<Vec<String>, String> {
    let Some(values) = args.get("paths").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in values.iter().take(MAX_PATHS) {
        let path = value
            .as_str()
            .ok_or_else(|| "paths must contain strings".to_string())?;
        out.push(validate_relative_path(cwd, path)?);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn validate_relative_path(cwd: &Path, raw: &str) -> Result<String, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err(format!("path must be relative to cwd: {raw}"));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("path must stay under cwd: {raw}"));
    }
    let joined = cwd.join(&path);
    if let Ok(canonical) = joined.canonicalize() {
        let root = cwd
            .canonicalize()
            .map_err(|e| format!("canonicalize cwd {}: {e}", cwd.display()))?;
        if !canonical.starts_with(root) {
            return Err(format!("path escapes cwd: {raw}"));
        }
    }
    Ok(path.to_string_lossy().to_string())
}

fn read_session_meta(
    harness_home: &Path,
    profile: &str,
    session_id: &str,
) -> Result<SessionMeta, String> {
    let path = harness_home
        .join("profiles")
        .join(profile)
        .join("sessions")
        .join(session_id)
        .join("meta.json");
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn session_summary(meta: SessionMeta) -> Value {
    json!({
        "id": meta.id,
        "kind": meta.kind,
        "status": meta.status,
        "role": meta.role,
        "task_id": meta.task_id,
        "cwd": meta.cwd,
        "started_at": meta.started_at,
        "exit_code": meta.exit_code,
        "detected_state": meta.detected_state,
        "loaded_capabilities": meta.loaded_capabilities,
        "has_transcript": meta.has_transcript,
    })
}

fn next_steps(task_id: Option<&str>, session_id: Option<&str>, git: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if task_id.is_some() {
        out.push(
            "Use task_get only if this evidence pack is missing task details needed for review."
                .into(),
        );
    }
    if session_id.is_some() {
        out.push("Use session_context_pack for latest next action or handoff before replaying transcript.".into());
    }
    let has_changes = git
        .get("status_short")
        .and_then(|v| v.get("stdout"))
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty());
    if has_changes {
        out.push(
            "Review changed_files/stat first; request file snippets only for risky paths.".into(),
        );
    }
    if out.is_empty() {
        out.push(
            "No scoped evidence found; request task/session id or paths for a narrower pack."
                .into(),
        );
    }
    out
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn truncate(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    if text.len() <= MAX_CMD_BYTES {
        return text.to_string();
    }
    let mut idx = MAX_CMD_BYTES;
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    format!("{}\n...[truncated]", &text[..idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::tasks::{TaskDraft, TaskStore};
    use tempfile::tempdir;

    #[test]
    fn evidence_pack_returns_task_and_git_summary() {
        let dir = tempdir().unwrap();
        let cwd = dir.path().join("repo");
        std::fs::create_dir(&cwd).unwrap();
        run(&cwd, "git", &["init"]).unwrap();
        std::fs::write(cwd.join("src.rs"), "fn main() {}\n").unwrap();
        run(&cwd, "git", &["add", "src.rs"]).unwrap();
        run(&cwd, "git", &["commit", "-m", "init"]).unwrap();
        std::fs::write(cwd.join("src.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();

        let store = TaskStore::new(dir.path()).unwrap();
        store.create("thr-1", draft()).unwrap();

        let result = pack(
            &store,
            dir.path(),
            "default",
            &cwd,
            "thr-1",
            None,
            Some("T-0001"),
            &json!({"paths": ["src.rs"]}),
        )
        .unwrap();

        assert_eq!(result["task"]["id"], "T-0001");
        assert_eq!(result["git"]["available"], true);
        assert!(result["git"]["changed_files"]["stdout"]
            .as_str()
            .unwrap()
            .contains("src.rs"));
    }

    #[test]
    fn evidence_pack_rejects_path_escape() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let err = pack(
            &store,
            dir.path(),
            "default",
            dir.path(),
            "thr-1",
            None,
            None,
            &json!({"paths": ["../secret"]}),
        )
        .unwrap_err();

        assert!(err.contains("stay under cwd"));
    }

    fn draft() -> TaskDraft {
        TaskDraft {
            title: "review".into(),
            parent: None,
            depends_on: vec![],
            brief: None,
            acceptance: vec![],
            labels: vec![],
            spec_refs: vec![],
            write_paths: vec![],
            forbidden_paths: vec![],
            created_by: "human".into(),
        }
    }

    fn run(cwd: &Path, bin: &str, args: &[&str]) -> Result<(), String> {
        let output = Command::new(bin)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Harness")
            .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
            .env("GIT_COMMITTER_NAME", "Harness")
            .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
            .current_dir(cwd)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}
