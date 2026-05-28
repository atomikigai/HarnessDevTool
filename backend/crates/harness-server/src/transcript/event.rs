//! Normalised transcript event — CLI-agnostic shape. Each underlying CLI
//! has a per-source parser that emits these.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptSource {
    Claude,
    Codex,
    Cursor,
    Antigravity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind {
    /// Free-form user or assistant message (text body).
    Message,
    /// Assistant "extended thinking" block — visible but visually distinct.
    Thinking,
    /// Assistant invoked a tool. Pairs with a later ToolResult by `tool_use_id`.
    ToolCall,
    /// Result returned for a prior ToolCall.
    ToolResult,
    /// User-facing system notes — init / compact / context events that the
    /// reader actually wants to know happened.
    SystemNote,
    /// Pure plumbing (mode flips, permission changes, internal titles). The
    /// frontend hides these by default.
    Meta,
    /// Anything the parser couldn't normalise; frontend renders raw fallback.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEvent {
    /// Monotonic per-session sequence assigned by [`TranscriptStore`] at
    /// ingestion. Lets the frontend reconnect with `?since=<seq>` and
    /// receive only events it doesn't already have.
    pub seq: u64,
    pub session_id: String,
    /// RFC3339 timestamp from the source line.
    pub ts: String,
    pub source: TranscriptSource,
    pub kind: TranscriptKind,
    /// "user" or "assistant" for `Message` / `Thinking` kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Free-form body text for `Message` / `Thinking`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool name (`ToolCall`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool call arguments (`ToolCall`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_args: Option<Value>,
    /// Identifier linking `ToolCall` and `ToolResult`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// Tool result payload (`ToolResult`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<Value>,
    /// True when the tool returned an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Model that produced this event (assistant turns only). E.g.
    /// `"claude-opus-4-7"`. Useful as an avatar tooltip / footer caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Token usage for the assistant turn this event belongs to. Whole
    /// object (input/output/cache fields) preserved verbatim — the
    /// frontend picks what to render. Only set on the first event of an
    /// assistant turn to avoid duplication when a turn produces multiple
    /// events (thinking + text + tool_use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// User-facing summary for `SystemNote` events (e.g. `"init"`,
    /// `"compact"`). Lets the frontend render a one-line pill without
    /// digging into `raw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// Original line for fallback rendering when the parser couldn't fully
    /// normalise. Always present so the frontend has SOMETHING to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}
