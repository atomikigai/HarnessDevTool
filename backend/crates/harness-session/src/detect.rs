//! Heuristic detection of the child CLI's interaction state.
//!
//! The harness owns the PTY but not the CLI's TUI — so to know whether the
//! agent is thinking, waiting for the user, or done, we tail the ANSI buffer
//! and pattern-match per-CLI. Inspired by herdr's `detect.rs` (AGPL) but
//! re-implemented from scratch; only the *concept* (regex over scrollback) is
//! reused, no code copied.
//!
//! Rules are intentionally conservative — false-positive Working is worse
//! than false-negative Working, because a stuck Blocked would silently hold
//! the orchestrator. When unsure we return `Unknown` and the orchestrator
//! falls back to its previous polling strategy.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::kind::AgentKind;

/// Interaction phase of the CLI hijo. Derived from the scrollback tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum AgentState {
    /// LLM is generating / executing a tool. Visual cues: spinners, "Cooked
    /// for Xs" markers, "esc to interrupt" footer.
    Working,
    /// Waiting for human input — approval prompt, choice, password.
    Blocked,
    /// Visible prompt with no activity and no pending choice → ready for the
    /// next message. The most ambiguous bucket; we only emit it when we see
    /// a clear quiescent prompt indicator at the very tail.
    Idle,
    /// We saw output but couldn't classify. Default at startup.
    Unknown,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Idle => "idle",
            AgentState::Unknown => "unknown",
        }
    }
}

/// How many bytes from the PTY scrollback tail the detector looks at on each
/// pass. 8 KiB covers ~30-50 lines after ANSI stripping for typical TUIs.
pub const TAIL_WINDOW_BYTES: usize = 8 * 1024;

/// Entry point: classify the most recent state of the CLI given the last
/// `TAIL_WINDOW_BYTES` of its PTY output. The input may contain ANSI escape
/// sequences; we strip them before matching for simpler regexes.
pub fn detect(kind: AgentKind, tail_bytes: &[u8]) -> AgentState {
    if tail_bytes.is_empty() {
        return AgentState::Unknown;
    }
    let text = String::from_utf8_lossy(tail_bytes);
    let stripped = strip_ansi(&text);
    let underlying = kind.underlying_cli();
    match underlying {
        AgentKind::Claude => detect_claude(&stripped),
        AgentKind::Codex => detect_codex(&stripped),
        AgentKind::Cursor => detect_cursor(&stripped),
        AgentKind::Antigravity => detect_antigravity(&stripped),
        AgentKind::Zeus => AgentState::Unknown, // unreachable via underlying_cli
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ANSI stripping
// ────────────────────────────────────────────────────────────────────────────

/// Best-effort ANSI escape removal. Handles CSI (`ESC [ ... letter`), OSC
/// (`ESC ] ... BEL` or `ESC ] ... ST`), and standalone ESC. Doesn't try to
/// be a full terminal emulator — we just want patterns to match readably.
fn strip_ansi(input: &str) -> String {
    static ANSI: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]")
            .expect("ansi regex compiles")
    });
    ANSI.replace_all(input, "").into_owned()
}

// ────────────────────────────────────────────────────────────────────────────
// Claude Code
// ────────────────────────────────────────────────────────────────────────────

fn detect_claude(stripped: &str) -> AgentState {
    // BLOCKED first — false-positive Working on an approval prompt would
    // make the orchestrator think it can poll for output later when in fact
    // the worker is sitting waiting forever.
    static BLOCKED: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            // Claude's "Do you want to ... ? \n 1. Yes \n 2. No" prompt.
            Regex::new(r"(?i)do you want to .*\n").unwrap(),
            // Generic continue / proceed confirmation.
            Regex::new(r"(?i)\bproceed\?").unwrap(),
            // Slash-command picker / radio choice with "❯ 1." cursor.
            Regex::new(r"❯\s*\d\.\s+").unwrap(),
        ]
    });
    if BLOCKED.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Blocked;
    }

    // WORKING — claude flips through animated verbs in its thinking footer
    // (`✻ Cooked for Xs`, "Crunched", "Cogitated", "Sautéed", ...) and shows
    // `esc to interrupt`.
    static WORKING: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            Regex::new(r"✻\s+\w+\s+for\s+\d").unwrap(),
            Regex::new(r"(?i)esc to interrupt").unwrap(),
            // Animated braille spinner used by claude code.
            Regex::new(r"[\u{2800}-\u{28FF}]\s+(?:Cooking|Crunching|Cogitating|Working)").unwrap(),
        ]
    });
    if WORKING.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Working;
    }

    // IDLE — the bottom-of-screen empty prompt ❯ on its own line.
    static IDLE: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            Regex::new(r"\n❯\s*$").unwrap(),
            Regex::new(r"(?i)bypass permissions on").unwrap(),
        ]
    });
    if IDLE.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Idle;
    }

    AgentState::Unknown
}

// ────────────────────────────────────────────────────────────────────────────
// Codex (OpenAI Codex CLI)
// ────────────────────────────────────────────────────────────────────────────

fn detect_codex(stripped: &str) -> AgentState {
    static BLOCKED: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            // Approval prompts.
            Regex::new(r"(?i)allow\s+codex\s+to").unwrap(),
            Regex::new(r"(?i)press\s+enter\s+to\s+(continue|approve)").unwrap(),
            Regex::new(r"(?i)\[y/n\]").unwrap(),
        ]
    });
    if BLOCKED.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Blocked;
    }

    static WORKING: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            // Same animated verbs as Claude — Codex's TUI uses the same idiom.
            Regex::new(r"(?i)(thinking|working|running|generating)\.\.\.").unwrap(),
            Regex::new(r"[\u{2800}-\u{28FF}]\s+\w+").unwrap(), // braille spinner + label
        ]
    });
    if WORKING.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Working;
    }

    // IDLE — Codex's input placeholder. We anchor near the end so a stale
    // mention in scrollback doesn't trigger.
    static IDLE: Lazy<Vec<Regex>> = Lazy::new(|| {
        vec![
            Regex::new(r"Message or command…\s*$").unwrap(),
            Regex::new(r"(?i)gpt-[\d.]+\s+(?:medium|low|high)?").unwrap(),
        ]
    });
    if IDLE.iter().any(|r| r.is_match(stripped)) {
        return AgentState::Idle;
    }

    AgentState::Unknown
}

// ────────────────────────────────────────────────────────────────────────────
// Cursor / Antigravity — stubs; flesh out when integrating these CLIs.
// ────────────────────────────────────────────────────────────────────────────

fn detect_cursor(_stripped: &str) -> AgentState {
    AgentState::Unknown
}

fn detect_antigravity(_stripped: &str) -> AgentState {
    AgentState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_drops_csi() {
        let inp = "\x1b[31mhello\x1b[0m world";
        assert_eq!(strip_ansi(inp), "hello world");
    }

    #[test]
    fn claude_idle_prompt_at_end() {
        let s = "some history above\n\n❯ ";
        assert_eq!(detect(AgentKind::Claude, s.as_bytes()), AgentState::Idle);
    }

    #[test]
    fn claude_cooking_footer_means_working() {
        let s = "doing stuff\n✻ Cogitated for 3m 1s\n";
        assert_eq!(detect(AgentKind::Claude, s.as_bytes()), AgentState::Working);
    }

    #[test]
    fn codex_message_placeholder_means_idle() {
        let s = "stuff\n  Message or command…";
        assert_eq!(detect(AgentKind::Codex, s.as_bytes()), AgentState::Idle);
    }

    #[test]
    fn unknown_when_empty() {
        assert_eq!(detect(AgentKind::Claude, b""), AgentState::Unknown);
    }

    #[test]
    fn zeus_routes_to_underlying_claude() {
        let s = "✻ Cooked for 14s\n";
        assert_eq!(detect(AgentKind::Zeus, s.as_bytes()), AgentState::Working);
    }
}
