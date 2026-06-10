//! Skill and evolution tools.

use std::path::Path;

use chrono::Utc;
use harness_core::{EvolutionObservation, SkillStore, SkillUsage};
use serde_json::{json, Value};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn u64_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| v.as_u64())
}

fn store(home: &Path, profile: &str) -> Result<SkillStore, String> {
    SkillStore::new(home, profile).map_err(|e| e.to_string())
}

pub fn search(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let query = str_arg(args, "query")?;
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let hits = store(home, profile)?
        .search(query, top_k)
        .map_err(|e| e.to_string())?;
    Ok(json!(hits))
}

pub fn propose(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let title = str_arg(args, "title")?;
    let body = str_arg(args, "body")?;
    let reason = str_arg(args, "reason")?;
    let tags = string_array(args, "tags");
    let proposal = store(home, profile)?
        .propose(title, body, tags, reason)
        .map_err(|e| e.to_string())?;
    Ok(json!(proposal))
}

pub fn promote(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id")?;
    let reason = str_arg(args, "reason")?;
    let record = store(home, profile)?
        .promote(id, reason)
        .map_err(|e| e.to_string())?;
    Ok(json!(record))
}

pub fn archive(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id")?;
    let reason = str_arg(args, "reason")?;
    let record = store(home, profile)?
        .archive(id, reason)
        .map_err(|e| e.to_string())?;
    Ok(json!(record))
}

pub fn record_usage(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let usage = SkillUsage {
        skill_id: str_arg(args, "skill_id")?.to_string(),
        outcome: str_arg(args, "outcome")?.to_string(),
        session_id: opt_str(args, "session_id"),
        task_id: opt_str(args, "task_id"),
        loaded: bool_arg(args, "loaded", true),
        used: bool_arg(args, "used", true),
        duration_ms: u64_arg(args, "duration_ms"),
        recorded_at: Utc::now(),
    };
    store(home, profile)?
        .record_usage(usage)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn observe(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let observation = EvolutionObservation {
        kind: str_arg(args, "kind")?.to_string(),
        summary: str_arg(args, "summary")?.to_string(),
        thread_id: opt_str(args, "thread_id"),
        session_id: opt_str(args, "session_id"),
        task_id: opt_str(args, "task_id"),
        signals: string_array(args, "signals"),
        evidence: string_array(args, "evidence"),
        recorded_at: Utc::now(),
    };
    store(home, profile)?
        .observe(observation)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}

pub fn evolve_run(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    let report = store(home, profile)?
        .evolve_run(limit)
        .map_err(|e| e.to_string())?;
    Ok(json!(report))
}

pub fn curator_run(home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let dry_run = bool_arg(args, "dry_run", true);
    let report = store(home, profile)?
        .curator_run(dry_run)
        .map_err(|e| e.to_string())?;
    Ok(json!(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_search_returns_proposed_hits() {
        let dir = tempfile::tempdir().unwrap();
        propose(
            dir.path(),
            "default",
            &json!({
                "title": "Rust audit workflow",
                "body": "# Rust audit workflow\n\nRun cargo audit and document accepted advisories.",
                "tags": ["rust", "security"],
                "reason": "seed"
            }),
        )
        .unwrap();
        let hits = search(dir.path(), "default", &json!({"query": "cargo audit"})).unwrap();
        assert_eq!(hits.as_array().unwrap().len(), 1);
    }
}
