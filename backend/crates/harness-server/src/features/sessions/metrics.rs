use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::transcript::event::TranscriptKind;
use crate::transcript::TranscriptEvent;

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ConversationMetrics {
    pub transcript_event_count: u64,
    pub user_message_count: u64,
    pub assistant_message_count: u64,
    pub thinking_event_count: u64,
    pub tool_result_count: u64,
    pub tool_error_count: u64,
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub conversation_duration_ms: Option<u64>,
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub max_gap_ms: Option<u64>,
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub max_gap_after_seq: Option<u64>,
    pub max_output_tokens_single_turn: u64,
    pub max_tool_args_bytes: u64,
    pub max_tool_result_bytes: u64,
    pub tool_duration_ms_by_name: BTreeMap<String, ToolDurationStats>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ToolDurationStats {
    pub count: u64,
    pub total_ms: u64,
    pub max_ms: u64,
}

pub(crate) fn tool_call_breakdown_from_events(events: &[TranscriptEvent]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for ev in events {
        if ev.kind != TranscriptKind::ToolCall {
            continue;
        }
        let name = ev
            .tool_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("(unknown)")
            .to_string();
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn conversation_metrics_from_events(events: &[TranscriptEvent]) -> ConversationMetrics {
    #[derive(Debug)]
    struct PendingToolCall {
        name: String,
        started_at: DateTime<Utc>,
    }

    let mut metrics = ConversationMetrics {
        transcript_event_count: events.len() as u64,
        ..ConversationMetrics::default()
    };
    let mut pending_tools: HashMap<String, PendingToolCall> = HashMap::new();
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut previous_ts: Option<DateTime<Utc>> = None;
    let mut previous_seq: Option<u64> = None;

    for ev in events {
        let ts = parse_event_ts(&ev.ts);
        if let Some(current_ts) = ts {
            first_ts = Some(first_ts.map_or(current_ts, |first| first.min(current_ts)));
            last_ts = Some(last_ts.map_or(current_ts, |last| last.max(current_ts)));
            if let Some(prev_ts) = previous_ts {
                let gap_ms = current_ts.signed_duration_since(prev_ts).num_milliseconds();
                if gap_ms > 0 {
                    let gap_ms = gap_ms as u64;
                    if metrics.max_gap_ms.is_none_or(|max| gap_ms > max) {
                        metrics.max_gap_ms = Some(gap_ms);
                        metrics.max_gap_after_seq = previous_seq;
                    }
                }
            }
            previous_ts = Some(current_ts);
            previous_seq = Some(ev.seq);
        }

        if let Some(usage) = ev.usage.as_ref() {
            metrics.max_output_tokens_single_turn = metrics
                .max_output_tokens_single_turn
                .max(usage_u64(usage, "output_tokens"));
        }

        match ev.kind {
            TranscriptKind::Message => match ev.role.as_deref() {
                Some("user") => metrics.user_message_count += 1,
                Some("assistant") => metrics.assistant_message_count += 1,
                _ => {}
            },
            TranscriptKind::Thinking => metrics.thinking_event_count += 1,
            TranscriptKind::ToolCall => {
                if let Some(args) = ev.tool_args.as_ref() {
                    metrics.max_tool_args_bytes =
                        metrics.max_tool_args_bytes.max(json_payload_bytes(args));
                }
                if let (Some(id), Some(started_at)) = (ev.tool_use_id.as_ref(), ts) {
                    let name = ev
                        .tool_name
                        .as_deref()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or("(unknown)")
                        .to_string();
                    pending_tools.insert(id.clone(), PendingToolCall { name, started_at });
                }
            }
            TranscriptKind::ToolResult => {
                metrics.tool_result_count += 1;
                if ev.is_error.unwrap_or(false) {
                    metrics.tool_error_count += 1;
                }
                if let Some(result) = ev.tool_result.as_ref() {
                    metrics.max_tool_result_bytes = metrics
                        .max_tool_result_bytes
                        .max(json_payload_bytes(result));
                }
                if let (Some(id), Some(finished_at)) = (ev.tool_use_id.as_ref(), ts) {
                    if let Some(pending) = pending_tools.remove(id) {
                        let duration_ms = finished_at
                            .signed_duration_since(pending.started_at)
                            .num_milliseconds();
                        if duration_ms >= 0 {
                            let stats = metrics
                                .tool_duration_ms_by_name
                                .entry(pending.name)
                                .or_default();
                            let duration_ms = duration_ms as u64;
                            stats.count += 1;
                            stats.total_ms += duration_ms;
                            stats.max_ms = stats.max_ms.max(duration_ms);
                        }
                    }
                }
            }
            TranscriptKind::SystemNote | TranscriptKind::Meta | TranscriptKind::Unknown => {}
        }
    }

    metrics.conversation_duration_ms = match (first_ts, last_ts) {
        (Some(first), Some(last)) => last
            .signed_duration_since(first)
            .num_milliseconds()
            .try_into()
            .ok(),
        _ => None,
    };
    metrics
}

fn parse_event_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|ts| ts.with_timezone(&Utc))
}

fn usage_u64(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn json_payload_bytes(value: &Value) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(0)
}
