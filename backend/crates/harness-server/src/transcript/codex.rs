//! Parser for Codex CLI session JSONL under
//! `~/.codex/sessions/YYYY/MM/DD/rollout-...-<codex-session-id>.jsonl`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::event::{TranscriptEvent, TranscriptKind, TranscriptSource};

const CODEX_PATH_LOOKBACK_MS: i64 = 5 * 60 * 1000;

pub fn parse_line(line: &str, session_id: &str) -> Vec<TranscriptEvent> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, "codex transcript line not JSON, skipping");
            return Vec::new();
        }
    };
    let ts = v
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let top_type = v.get("type").and_then(Value::as_str).unwrap_or("");
    match top_type {
        "turn_context" => vec![event_meta(session_id, &ts, &v)],
        "event_msg" => parse_event_msg(session_id, &ts, &v),
        "response_item" => parse_response_item(session_id, &ts, &v),
        "session_meta" => vec![event_meta(session_id, &ts, &v)],
        _ => Vec::new(),
    }
}

fn parse_event_msg(session_id: &str, ts: &str, raw: &Value) -> Vec<TranscriptEvent> {
    let payload = raw.get("payload").unwrap_or(&Value::Null);
    match payload.get("type").and_then(Value::as_str).unwrap_or("") {
        "user_message" => payload
            .get("message")
            .and_then(Value::as_str)
            .map(|message| {
                vec![event_message(
                    session_id,
                    ts,
                    "user",
                    message.to_string(),
                    None,
                    None,
                )]
            })
            .unwrap_or_default(),
        "agent_message" => payload
            .get("message")
            .and_then(Value::as_str)
            .map(|message| {
                vec![event_message(
                    session_id,
                    ts,
                    "assistant",
                    message.to_string(),
                    payload
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    None,
                )]
            })
            .unwrap_or_default(),
        "token_count" => vec![event_token_count(session_id, ts, raw, payload)],
        // Codex emits the visible assistant answer as `agent_message`. The
        // later `task_complete.last_agent_message` repeats that answer as a
        // lifecycle summary; exposing both produces duplicated chat content.
        "task_complete" => vec![event_meta(session_id, ts, raw)],
        _ => vec![event_meta(session_id, ts, raw)],
    }
}

fn parse_response_item(session_id: &str, ts: &str, raw: &Value) -> Vec<TranscriptEvent> {
    let payload = raw.get("payload").unwrap_or(&Value::Null);
    match payload.get("type").and_then(Value::as_str).unwrap_or("") {
        "reasoning" => Vec::new(),
        "function_call" => vec![TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: ts.to_string(),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::ToolCall,
            role: None,
            content: None,
            tool_name: payload
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool_args: payload.get("arguments").cloned(),
            tool_use_id: payload
                .get("call_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: Some(raw.clone()),
        }],
        _ => vec![event_meta(session_id, ts, raw)],
    }
}

fn event_message(
    session_id: &str,
    ts: &str,
    role: &str,
    content: String,
    model: Option<String>,
    usage: Option<Value>,
) -> TranscriptEvent {
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: ts.to_string(),
        source: TranscriptSource::Codex,
        kind: TranscriptKind::Message,
        role: Some(role.to_string()),
        content: Some(content),
        tool_name: None,
        tool_args: None,
        tool_use_id: None,
        tool_result: None,
        is_error: None,
        model,
        usage,
        subtype: None,
        raw: None,
    }
}

fn event_meta(session_id: &str, ts: &str, raw: &Value) -> TranscriptEvent {
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: ts.to_string(),
        source: TranscriptSource::Codex,
        kind: TranscriptKind::Meta,
        role: None,
        content: None,
        tool_name: None,
        tool_args: None,
        tool_use_id: None,
        tool_result: None,
        is_error: None,
        model: None,
        usage: None,
        subtype: None,
        raw: Some(raw.clone()),
    }
}

fn event_token_count(session_id: &str, ts: &str, raw: &Value, payload: &Value) -> TranscriptEvent {
    let info = payload.get("info").unwrap_or(&Value::Null);
    let total = info.get("total_token_usage").unwrap_or(&Value::Null);
    let usage = json!({
        "input_tokens": total.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
        "cache_read_input_tokens": total.get("cached_input_tokens").and_then(Value::as_u64).unwrap_or(0),
        "output_tokens": total.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        "reasoning_output_tokens": total.get("reasoning_output_tokens").and_then(Value::as_u64).unwrap_or(0),
        "total_tokens": total.get("total_tokens").and_then(Value::as_u64).unwrap_or(0),
        "model_context_window": info.get("model_context_window").and_then(Value::as_u64),
        "codex_total_token_usage": total,
        "codex_last_token_usage": info.get("last_token_usage").cloned().unwrap_or(Value::Null),
    });
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: ts.to_string(),
        source: TranscriptSource::Codex,
        kind: TranscriptKind::SystemNote,
        role: None,
        content: None,
        tool_name: None,
        tool_args: None,
        tool_use_id: None,
        tool_result: None,
        is_error: None,
        model: None,
        usage: Some(usage),
        subtype: Some("token_count".into()),
        raw: Some(raw.clone()),
    }
}

pub fn find_latest_transcript_path(
    codex_home: &Path,
    cwd: &Path,
    started_at_ms: i64,
    marker: Option<&str>,
) -> std::io::Result<Option<PathBuf>> {
    let root = codex_home.join("sessions");
    if !root.exists() {
        return Ok(None);
    }
    let mut best: Option<(i64, PathBuf)> = None;
    let mut marked_best: Option<(i64, PathBuf)> = None;
    visit_jsonl(&root, &mut |path| {
        if file_modified_millis(path)
            .map(|mtime| mtime < started_at_ms.saturating_sub(CODEX_PATH_LOOKBACK_MS))
            .unwrap_or(false)
        {
            return;
        }
        let Ok(Some(meta)) = read_session_meta(path) else {
            return;
        };
        if meta.cwd.as_deref() != Some(cwd.to_string_lossy().as_ref()) {
            return;
        }
        if meta.timestamp_ms < started_at_ms.saturating_sub(CODEX_PATH_LOOKBACK_MS) {
            return;
        }
        if marker
            .map(|marker| file_prefix_contains(path, marker, 24).unwrap_or(false))
            .unwrap_or(false)
        {
            let replace = marked_best
                .as_ref()
                .map(|(current, _)| meta.timestamp_ms > *current)
                .unwrap_or(true);
            if replace {
                marked_best = Some((meta.timestamp_ms, path.to_path_buf()));
            }
            return;
        }
        let replace = best
            .as_ref()
            .map(|(current, _)| meta.timestamp_ms > *current)
            .unwrap_or(true);
        if replace {
            best = Some((meta.timestamp_ms, path.to_path_buf()));
        }
    })?;
    if let Some((_, path)) = marked_best {
        return Ok(Some(path));
    }
    if marker.is_some() {
        return Ok(None);
    }
    Ok(best.map(|(_, path)| path))
}

fn file_modified_millis(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

struct CodexSessionMeta {
    cwd: Option<String>,
    timestamp_ms: i64,
}

fn read_session_meta(path: &Path) -> std::io::Result<Option<CodexSessionMeta>> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let Some(line) = lines.next() else {
        return Ok(None);
    };
    let line = line?;
    let value: Value = match serde_json::from_str(&line) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Ok(None);
    }
    let payload = value.get("payload").unwrap_or(&Value::Null);
    Ok(Some(CodexSessionMeta {
        cwd: payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string),
        timestamp_ms: parse_ts_millis(
            payload
                .get("timestamp")
                .and_then(Value::as_str)
                .or_else(|| value.get("timestamp").and_then(Value::as_str))
                .unwrap_or(""),
        )
        .unwrap_or(0),
    }))
}

fn file_prefix_contains(path: &Path, marker: &str, max_lines: usize) -> std::io::Result<bool> {
    let file = File::open(path)?;
    for line in BufReader::new(file).lines().take(max_lines) {
        if line?.contains(marker) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn visit_jsonl(dir: &Path, f: &mut impl FnMut(&Path)) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            visit_jsonl(&path, f)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            f(&path);
        }
    }
    Ok(())
}

fn parse_ts_millis(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn parses_user_and_agent_messages() {
        let user = r#"{"timestamp":"2026-06-09T13:08:31.854Z","type":"event_msg","payload":{"type":"user_message","message":"hola"}}"#;
        let agent = r#"{"timestamp":"2026-06-09T13:08:39.417Z","type":"event_msg","payload":{"type":"agent_message","message":"listo","phase":"final_answer"}}"#;

        let user_events = parse_line(user, "sid");
        let agent_events = parse_line(agent, "sid");

        assert_eq!(user_events[0].source, TranscriptSource::Codex);
        assert_eq!(user_events[0].role.as_deref(), Some("user"));
        assert_eq!(agent_events[0].role.as_deref(), Some("assistant"));
        assert_eq!(agent_events[0].content.as_deref(), Some("listo"));
    }

    #[test]
    fn parses_token_count_as_usage_note() {
        let line = r#"{"timestamp":"2026-06-09T13:08:39.522Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":12076,"cached_input_tokens":11648,"output_tokens":328,"reasoning_output_tokens":0,"total_tokens":12404},"last_token_usage":{"input_tokens":12076},"model_context_window":258400}}}"#;

        let events = parse_line(line, "sid");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::SystemNote);
        assert_eq!(events[0].subtype.as_deref(), Some("token_count"));
        let usage = events[0].usage.as_ref().unwrap();
        assert_eq!(usage["input_tokens"].as_u64(), Some(12076));
        assert_eq!(usage["model_context_window"].as_u64(), Some(258400));
    }

    #[test]
    fn task_complete_last_agent_message_is_lifecycle_meta() {
        let line = r#"{"timestamp":"2026-06-09T13:08:39.600Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":"**OK**"}}"#;

        let events = parse_line(line, "sid");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::Meta);
        assert_eq!(events[0].role, None);
        assert_eq!(events[0].content, None);
    }

    #[test]
    fn resolves_latest_transcript_for_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions/2026/06/09");
        std::fs::create_dir_all(&sessions).unwrap();
        write_meta(
            &sessions.join("rollout-old.jsonl"),
            "/tmp/project",
            "2026-06-09T13:00:00.000Z",
        );
        write_meta(
            &sessions.join("rollout-new.jsonl"),
            "/tmp/project",
            "2026-06-09T13:10:00.000Z",
        );
        write_meta(
            &sessions.join("rollout-other.jsonl"),
            "/tmp/other",
            "2026-06-09T13:20:00.000Z",
        );

        let started = parse_ts_millis("2026-06-09T13:01:00.000Z").unwrap();
        let path =
            find_latest_transcript_path(dir.path(), Path::new("/tmp/project"), started, None)
                .unwrap()
                .unwrap();

        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("rollout-new.jsonl")
        );
    }

    #[test]
    fn marker_wins_over_newer_same_cwd_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions/2026/06/09");
        std::fs::create_dir_all(&sessions).unwrap();
        let marked = sessions.join("rollout-marked.jsonl");
        write_meta(&marked, "/tmp/project", "2026-06-09T13:10:00.000Z");
        append_line(
            &marked,
            "developer instructions [harness session marker] harness_session_id=sid-123",
        );
        write_meta(
            &sessions.join("rollout-newer.jsonl"),
            "/tmp/project",
            "2026-06-09T13:20:00.000Z",
        );

        let started = parse_ts_millis("2026-06-09T13:01:00.000Z").unwrap();
        let path = find_latest_transcript_path(
            dir.path(),
            Path::new("/tmp/project"),
            started,
            Some("[harness session marker] harness_session_id=sid-123"),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("rollout-marked.jsonl")
        );
    }

    #[test]
    fn marker_mode_does_not_fallback_to_unmarked_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions/2026/06/09");
        std::fs::create_dir_all(&sessions).unwrap();
        write_meta(
            &sessions.join("rollout-newer.jsonl"),
            "/tmp/project",
            "2026-06-09T13:20:00.000Z",
        );

        let started = parse_ts_millis("2026-06-09T13:01:00.000Z").unwrap();
        let path = find_latest_transcript_path(
            dir.path(),
            Path::new("/tmp/project"),
            started,
            Some("[harness session marker] harness_session_id=missing"),
        )
        .unwrap();

        assert!(path.is_none());
    }

    fn write_meta(path: &Path, cwd: &str, timestamp: &str) {
        let mut file = File::create(path).unwrap();
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": timestamp,
                "type": "session_meta",
                "payload": {
                    "id": "codex-session",
                    "timestamp": timestamp,
                    "cwd": cwd
                }
            })
        )
        .unwrap();
    }

    fn append_line(path: &Path, line: &str) {
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(file, "{line}").unwrap();
    }
}
