//! Parser for Codex CLI JSONL output.
//!
//! Codex emits newline-delimited JSON when launched in JSON mode. The stable
//! surface we care about for ChatView is smaller than the raw protocol:
//! assistant messages, tool starts/results, and thread/system lifecycle.

use serde_json::Value;

use super::event::{TranscriptEvent, TranscriptKind, TranscriptSource};

pub fn parse_line(line: &str, session_id: &str) -> Vec<TranscriptEvent> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or_default();
    match ty {
        "thread.started" => vec![system_note(session_id, "thread.started", &v)],
        "item.started" => parse_item(session_id, &v, false),
        "item.completed" => parse_item(session_id, &v, true),
        "error" => vec![TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: timestamp(&v),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::SystemNote,
            role: None,
            content: v
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .or_else(|| Some("Codex reported an error".to_string())),
            tool_name: None,
            tool_args: None,
            tool_use_id: None,
            tool_result: None,
            is_error: Some(true),
            model: None,
            usage: None,
            subtype: Some("error".into()),
            raw: Some(v),
        }],
        _ => Vec::new(),
    }
}

fn parse_item(session_id: &str, raw: &Value, completed: bool) -> Vec<TranscriptEvent> {
    let item = raw.get("item").unwrap_or(raw);
    let item_type = item
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or_default();
    match item_type {
        "agent_message" if completed => item
            .get("text")
            .and_then(|t| t.as_str())
            .filter(|t| !t.trim().is_empty())
            .map(|text| {
                vec![TranscriptEvent {
                    seq: 0,
                    session_id: session_id.to_string(),
                    ts: timestamp(raw),
                    source: TranscriptSource::Codex,
                    kind: TranscriptKind::Message,
                    role: Some("assistant".into()),
                    content: Some(text.to_string()),
                    tool_name: None,
                    tool_args: None,
                    tool_use_id: None,
                    tool_result: None,
                    is_error: None,
                    model: model(raw, item),
                    usage: raw.get("usage").cloned(),
                    subtype: None,
                    raw: None,
                }]
            })
            .unwrap_or_default(),
        "user_message" if completed => item
            .get("text")
            .and_then(|t| t.as_str())
            .filter(|t| !t.trim().is_empty())
            .map(|text| {
                vec![TranscriptEvent {
                    seq: 0,
                    session_id: session_id.to_string(),
                    ts: timestamp(raw),
                    source: TranscriptSource::Codex,
                    kind: TranscriptKind::Message,
                    role: Some("user".into()),
                    content: Some(text.to_string()),
                    tool_name: None,
                    tool_args: None,
                    tool_use_id: None,
                    tool_result: None,
                    is_error: None,
                    model: None,
                    usage: None,
                    subtype: None,
                    raw: None,
                }]
            })
            .unwrap_or_default(),
        "command_execution" => vec![command_event(session_id, raw, item, completed)],
        "mcp_tool_call" => vec![mcp_tool_event(session_id, raw, item, completed)],
        _ => Vec::new(),
    }
}

fn command_event(session_id: &str, raw: &Value, item: &Value, completed: bool) -> TranscriptEvent {
    let tool_id = item_id(item);
    if completed {
        TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: timestamp(raw),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::ToolResult,
            role: None,
            content: None,
            tool_name: Some("Bash".into()),
            tool_args: None,
            tool_use_id: tool_id,
            tool_result: item
                .get("aggregated_output")
                .cloned()
                .or_else(|| item.get("output").cloned()),
            is_error: item
                .get("exit_code")
                .and_then(|c| c.as_i64())
                .map(|c| c != 0),
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    } else {
        TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: timestamp(raw),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::ToolCall,
            role: None,
            content: None,
            tool_name: Some("Bash".into()),
            tool_args: item
                .get("command")
                .and_then(|command| command.as_str())
                .map(|command| serde_json::json!({ "command": command })),
            tool_use_id: tool_id,
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    }
}

fn mcp_tool_event(session_id: &str, raw: &Value, item: &Value, completed: bool) -> TranscriptEvent {
    let tool_id = item_id(item);
    if completed {
        TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: timestamp(raw),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::ToolResult,
            role: None,
            content: None,
            tool_name: item
                .get("tool")
                .and_then(|t| t.as_str())
                .map(str::to_string),
            tool_args: None,
            tool_use_id: tool_id,
            tool_result: item
                .get("result")
                .cloned()
                .or_else(|| item.get("error").cloned()),
            is_error: item.get("error").map(|v| !v.is_null()),
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    } else {
        TranscriptEvent {
            seq: 0,
            session_id: session_id.to_string(),
            ts: timestamp(raw),
            source: TranscriptSource::Codex,
            kind: TranscriptKind::ToolCall,
            role: None,
            content: None,
            tool_name: item
                .get("tool")
                .and_then(|t| t.as_str())
                .map(str::to_string)
                .or_else(|| Some("MCP tool".into())),
            tool_args: item.get("arguments").cloned(),
            tool_use_id: tool_id,
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    }
}

fn system_note(session_id: &str, subtype: &str, raw: &Value) -> TranscriptEvent {
    TranscriptEvent {
        seq: 0,
        session_id: session_id.to_string(),
        ts: timestamp(raw),
        source: TranscriptSource::Codex,
        kind: TranscriptKind::SystemNote,
        role: None,
        content: None,
        tool_name: None,
        tool_args: None,
        tool_use_id: raw
            .get("thread_id")
            .and_then(|id| id.as_str())
            .map(str::to_string),
        tool_result: None,
        is_error: None,
        model: model(raw, raw),
        usage: None,
        subtype: Some(subtype.into()),
        raw: Some(raw.clone()),
    }
}

fn timestamp(raw: &Value) -> String {
    raw.get("timestamp")
        .or_else(|| raw.get("ts"))
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string()
}

fn item_id(item: &Value) -> Option<String> {
    item.get("id")
        .or_else(|| item.get("call_id"))
        .and_then(|id| id.as_str())
        .map(str::to_string)
}

fn model(raw: &Value, item: &Value) -> Option<String> {
    item.get("model")
        .or_else(|| raw.get("model"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::super::event::TranscriptKind;
    use super::*;

    #[test]
    fn assistant_message_completed_maps_to_message() {
        let line =
            r#"{"type":"item.completed","item":{"id":"m1","type":"agent_message","text":"done"}}"#;
        let events = parse_line(line, "sid");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptKind::Message);
        assert_eq!(events[0].role.as_deref(), Some("assistant"));
        assert_eq!(events[0].content.as_deref(), Some("done"));
    }

    #[test]
    fn command_execution_maps_start_and_result() {
        let start = r#"{"type":"item.started","item":{"id":"cmd1","type":"command_execution","command":"ls"}}"#;
        let done = r#"{"type":"item.completed","item":{"id":"cmd1","type":"command_execution","aggregated_output":"ok","exit_code":0}}"#;
        let start_events = parse_line(start, "sid");
        let done_events = parse_line(done, "sid");
        assert_eq!(start_events[0].kind, TranscriptKind::ToolCall);
        assert_eq!(start_events[0].tool_name.as_deref(), Some("Bash"));
        assert_eq!(done_events[0].kind, TranscriptKind::ToolResult);
        assert_eq!(done_events[0].tool_use_id.as_deref(), Some("cmd1"));
    }

    #[test]
    fn mcp_tool_call_maps_arguments_and_result() {
        let start = r#"{"type":"item.started","item":{"id":"tool1","type":"mcp_tool_call","tool":"task_list","arguments":{"status":"open"}}}"#;
        let done = r#"{"type":"item.completed","item":{"id":"tool1","type":"mcp_tool_call","tool":"task_list","result":{"ok":true}}}"#;
        let start_events = parse_line(start, "sid");
        let done_events = parse_line(done, "sid");
        assert_eq!(start_events[0].kind, TranscriptKind::ToolCall);
        assert_eq!(start_events[0].tool_name.as_deref(), Some("task_list"));
        assert_eq!(done_events[0].kind, TranscriptKind::ToolResult);
    }
}
