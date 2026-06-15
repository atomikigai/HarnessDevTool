//! Structured transcript stream for sessions whose underlying CLI emits a
//! JSONL transcript on disk (Claude today; Codex / Cursor / Antigravity
//! TBD). The watcher tail-reads that file, normalises every line into a
//! `TranscriptEvent`, persists it to our own profile dir, and broadcasts
//! it through the session bus.
//!
//! The frontend's Chat view consumes this; the Terminal view keeps reading
//! the raw PTY bytes. Same process, two presentations.
//!
//! Per-CLI parsers live in submodules so adding Codex/Cursor support later
//! is a matter of implementing `TranscriptParser` for that source.

pub mod claude;
pub mod codex;
pub mod event;
pub mod store;
pub mod watcher;

pub use event::TranscriptEvent;
pub use store::{
    query_transcript_events, transcript_tool_results, TranscriptQueryOptions, TranscriptStore,
    TranscriptToolResultsOptions,
};
pub use watcher::{
    spawn_codex_transcript_watcher, spawn_transcript_watcher, TranscriptParser, WatcherHandle,
};

/// Re-export of `store::read_events_since` under a more-specific name so the
/// route layer doesn't pull in the whole store module.
pub use store::read_events_since as read_events_since_helper;
