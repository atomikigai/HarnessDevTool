//! Parser for Claude Code's per-session JSONL transcript at
//! `~/.claude/projects/<cwd-slug>/<sid>.jsonl`.
//!
//! Each input line becomes 0..N normalised events. Assistant turns flatten
//! their `content[]` (text + thinking + tool_use blocks) into separate
//! events. User turns whose content is an array of `tool_result` items
//! flatten each result into its own event.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::event::{TranscriptEvent, TranscriptKind, TranscriptSource};

/// Compute the on-disk slug Claude uses for a working directory.
/// `/home/x/y` → `-home-x-y` (replace `/` with `-` everywhere; the leading
/// slash produces a leading dash). Verified against real session paths.
pub fn cwd_slug(cwd: &Path) -> String {
    let s = cwd.to_string_lossy();
    s.replace('/', "-")
}

/// Resolve the JSONL path Claude writes for a session.
pub fn transcript_path(claude_home: &Path, cwd: &Path, sid: &str) -> PathBuf {
    claude_home
        .join("projects")
        .join(cwd_slug(cwd))
        .join(format!("{sid}.jsonl"))
}

/// Convert one raw Claude JSONL line into 0..N normalised events.
/// `seq` is set by the store at ingest time, so we leave it as 0 here.
pub fn parse_line(line: &str, session_id: &str) -> Vec<TranscriptEvent> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, "transcript line not JSON, skipping");
            return Vec::new();
        }
    };
    let kind = v
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let ts = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    match kind.as_str() {
        // Internal noise — drop entirely.
        "attachment" | "file-history-snapshot" | "last-prompt" => Vec::new(),

        // Pure plumbing — Meta. Frontend hides by default.
        "mode" | "permission-mode" | "ai-title" => {
            vec![event_meta(session_id, &ts, &v)]
        }

        // User-facing system notes (init, compact, etc.) — render as pills.
        "system" => vec![event_system_note(session_id, &ts, &v)],

        "user" => parse_user(session_id, &ts, &v),
        "assistant" => parse_assistant(session_id, &ts, &v),

        _ => vec![TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts,
            source: TranscriptSource::Claude,
            kind: TranscriptKind::Unknown,
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
            raw: Some(v),
        }],
    }
}

fn event_system_note(session_id: &str, ts: &str, raw: &Value) -> TranscriptEvent {
    let subtype = raw
        .get("subtype")
        .and_then(|s| s.as_str())
        .map(String::from);
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: ts.to_string(),
        source: TranscriptSource::Claude,
        kind: TranscriptKind::SystemNote,
        role: None,
        content: None,
        tool_name: None,
        tool_args: None,
        tool_use_id: None,
        tool_result: None,
        is_error: None,
        model: None,
        usage: None,
        subtype,
        raw: Some(raw.clone()),
    }
}

fn event_meta(session_id: &str, ts: &str, raw: &Value) -> TranscriptEvent {
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: ts.to_string(),
        source: TranscriptSource::Claude,
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

/// `user` turns have two shapes:
/// 1. `message.content` is a string → plain user message.
/// 2. `message.content` is an array of `tool_result` items → flatten each.
fn parse_user(session_id: &str, ts: &str, v: &Value) -> Vec<TranscriptEvent> {
    let content = v.get("message").and_then(|m| m.get("content"));
    match content {
        Some(Value::String(text)) => vec![TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: ts.to_string(),
            source: TranscriptSource::Claude,
            kind: TranscriptKind::Message,
            role: Some("user".into()),
            content: Some(text.clone()),
            tool_name: None,
            tool_args: None,
            tool_use_id: None,
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let kind = item.get("type").and_then(|t| t.as_str())?;
                if kind != "tool_result" {
                    return None;
                }
                Some(TranscriptEvent {
                    seq: 0,
                    session_id: session_id.to_string(),
                    ts: ts.to_string(),
                    source: TranscriptSource::Claude,
                    kind: TranscriptKind::ToolResult,
                    role: None,
                    content: None,
                    tool_name: None,
                    tool_args: None,
                    tool_use_id: item
                        .get("tool_use_id")
                        .and_then(|s| s.as_str())
                        .map(String::from),
                    tool_result: item.get("content").cloned(),
                    is_error: item.get("is_error").and_then(|b| b.as_bool()),
                    model: None,
                    usage: None,
                    subtype: None,
                    raw: None,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `assistant` turns have `message.content` as an array of blocks; each
/// block becomes its own normalised event. `model` rides every event of the
/// turn (cheap identity); `usage` is attached only to the FIRST event so
/// the frontend can show "tokens used by this turn" without double-counting.
fn parse_assistant(session_id: &str, ts: &str, v: &Value) -> Vec<TranscriptEvent> {
    let message = v.get("message");
    let model = message
        .and_then(|m| m.get("model"))
        .and_then(|m| m.as_str())
        .map(String::from);
    let usage = message.and_then(|m| m.get("usage")).cloned();
    let Some(items) = message
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(items.len());
    let mut usage_taken = false;
    for item in items {
        let Some(item_kind) = item.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        let attach_usage = if !usage_taken {
            usage_taken = true;
            usage.clone()
        } else {
            None
        };
        let event = match item_kind {
            "text" => TranscriptEvent {
                seq: 0,
                session_id: session_id.to_string(),
                ts: ts.to_string(),
                source: TranscriptSource::Claude,
                kind: TranscriptKind::Message,
                role: Some("assistant".into()),
                content: item.get("text").and_then(|t| t.as_str()).map(String::from),
                tool_name: None,
                tool_args: None,
                tool_use_id: None,
                tool_result: None,
                is_error: None,
                model: model.clone(),
                usage: attach_usage,
                subtype: None,
                raw: None,
            },
            "thinking" => TranscriptEvent {
                seq: 0,
                session_id: session_id.to_string(),
                ts: ts.to_string(),
                source: TranscriptSource::Claude,
                kind: TranscriptKind::Thinking,
                role: Some("assistant".into()),
                content: item
                    .get("thinking")
                    .and_then(|t| t.as_str())
                    .map(String::from),
                tool_name: None,
                tool_args: None,
                tool_use_id: None,
                tool_result: None,
                is_error: None,
                model: model.clone(),
                usage: attach_usage,
                subtype: None,
                raw: None,
            },
            "tool_use" => TranscriptEvent {
                seq: 0,
                session_id: session_id.to_string(),
                ts: ts.to_string(),
                source: TranscriptSource::Claude,
                kind: TranscriptKind::ToolCall,
                role: None,
                content: None,
                tool_name: item.get("name").and_then(|t| t.as_str()).map(String::from),
                tool_args: item.get("input").cloned(),
                tool_use_id: item.get("id").and_then(|t| t.as_str()).map(String::from),
                tool_result: None,
                is_error: None,
                model: model.clone(),
                usage: attach_usage,
                subtype: None,
                raw: None,
            },
            _ => TranscriptEvent {
                seq: 0,
                session_id: session_id.to_string(),
                ts: ts.to_string(),
                source: TranscriptSource::Claude,
                kind: TranscriptKind::Unknown,
                role: Some("assistant".into()),
                content: None,
                tool_name: None,
                tool_args: None,
                tool_use_id: None,
                tool_result: None,
                is_error: None,
                model: model.clone(),
                usage: attach_usage,
                subtype: None,
                raw: Some(item.clone()),
            },
        };
        out.push(event);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn slug_replaces_slashes() {
        let p = Path::new("/home/jostick/Desktop/personal/Projects/workspaces");
        assert_eq!(
            cwd_slug(p),
            "-home-jostick-Desktop-personal-Projects-workspaces"
        );
    }

    #[test]
    fn user_string_message() {
        let line = r#"{"type":"user","timestamp":"2026-05-27T16:15:40.868Z","message":{"role":"user","content":"hola"}}"#;
        let events = parse_line(line, "sid-1");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::Message);
        assert_eq!(events[0].content.as_deref(), Some("hola"));
    }

    #[test]
    fn user_tool_result_array() {
        let line = r#"{"type":"user","timestamp":"2026-05-27T16:16:00Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_X","is_error":false,"content":"ok"}]}}"#;
        let events = parse_line(line, "sid-1");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::ToolResult);
        assert_eq!(events[0].tool_use_id.as_deref(), Some("toolu_X"));
    }

    #[test]
    fn assistant_multi_content_flattens() {
        let line = r#"{"type":"assistant","timestamp":"2026-05-27T16:16:01Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"hola"},{"type":"tool_use","id":"toolu_X","name":"Bash","input":{"command":"ls"}}]}}"#;
        let events = parse_line(line, "sid-1");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, TranscriptKind::Thinking);
        assert_eq!(events[1].kind, TranscriptKind::Message);
        assert_eq!(events[2].kind, TranscriptKind::ToolCall);
        assert_eq!(events[2].tool_name.as_deref(), Some("Bash"));
    }

    #[test]
    fn noise_types_dropped() {
        let line = r#"{"type":"attachment","timestamp":"2026-05-27T16:15:40Z"}"#;
        assert!(parse_line(line, "sid").is_empty());
        let line2 = r#"{"type":"file-history-snapshot","timestamp":"2026-05-27T16:15:40Z"}"#;
        assert!(parse_line(line2, "sid").is_empty());
    }

    #[test]
    fn meta_types_kept() {
        let line = r#"{"type":"permission-mode","permissionMode":"bypassPermissions"}"#;
        let events = parse_line(line, "sid");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::Meta);
    }

    #[test]
    fn system_routes_to_system_note() {
        let line = r#"{"type":"system","subtype":"init","timestamp":"2026-05-27T16:15:40Z"}"#;
        let events = parse_line(line, "sid");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::SystemNote);
        assert_eq!(events[0].subtype.as_deref(), Some("init"));
    }

    #[test]
    fn assistant_usage_attached_to_first_event_only() {
        let line = r#"{"type":"assistant","timestamp":"2026-05-27T16:16:01Z","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":6,"output_tokens":168},"content":[{"type":"text","text":"hola"},{"type":"tool_use","id":"toolu_X","name":"Bash","input":{"command":"ls"}}]}}"#;
        let events = parse_line(line, "sid-1");
        assert_eq!(events.len(), 2);
        // model on every event
        assert_eq!(events[0].model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(events[1].model.as_deref(), Some("claude-opus-4-7"));
        // usage on first only
        assert!(events[0].usage.is_some());
        assert!(events[1].usage.is_none());
    }
}
