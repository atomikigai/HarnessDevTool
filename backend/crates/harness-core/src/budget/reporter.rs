//! Cost reporter: read the per-session transcript that the CLI persists on
//! disk, sum token usage, and convert to USD via the static price table.
//!
//! Why disk parsing: harness agents run inside an interactive PTY, so the
//! JSON cost summary from `--print` isn't available. Claude Code writes a
//! line-delimited JSONL transcript at `~/.claude/projects/{cwd-slug}/{session_id}.jsonl`
//! which contains per-assistant-message `usage` blocks plus the `model` id.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;

use super::pricing::model_price;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
}

impl Usage {
    pub fn cost_usd(&self, model: &str) -> f64 {
        let p = model_price(model);
        self.input_tokens as f64 * p.input
            + self.output_tokens as f64 * p.output
            + self.cache_read_tokens as f64 * p.cache_read
            + self.cache_write_5m_tokens as f64 * p.cache_write_5m
            + self.cache_write_1h_tokens as f64 * p.cache_write_1h
    }
}

/// Cost data for a single agent session.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SessionCost {
    pub model: String,
    pub usage: Usage,
    pub cost_usd: f64,
}

pub trait CostReporter: Send + Sync {
    fn poll(&self, session_id: &str, cwd: &Path) -> Result<SessionCost, Error>;
}

/// Reads `~/.claude/projects/{slug(cwd)}/{session_id}.jsonl`.
pub struct ClaudeTranscriptReporter {
    projects_root: PathBuf,
}

impl ClaudeTranscriptReporter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self {
            projects_root: home.join(".claude/projects"),
        }
    }

    pub fn with_root(projects_root: PathBuf) -> Self {
        Self { projects_root }
    }

    /// Replicate Claude Code's directory-naming rule: replace `/` with `-`,
    /// keep everything else verbatim. (Validated against this repo at
    /// `~/.claude/projects/-home-...-HarnessDevTool/`.)
    fn slug(cwd: &Path) -> String {
        cwd.to_string_lossy().replace('/', "-")
    }

    pub fn transcript_path(&self, session_id: &str, cwd: &Path) -> PathBuf {
        self.projects_root
            .join(Self::slug(cwd))
            .join(format!("{session_id}.jsonl"))
    }
}

impl Default for ClaudeTranscriptReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl CostReporter for ClaudeTranscriptReporter {
    fn poll(&self, session_id: &str, cwd: &Path) -> Result<SessionCost, Error> {
        let path = self.transcript_path(session_id, cwd);
        if !path.exists() {
            return Ok(SessionCost::default());
        }
        let text = fs::read_to_string(&path)?;
        let mut total = Usage::default();
        let mut model = String::new();
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<TranscriptLine>(line) else {
                continue;
            };
            let Some(msg) = entry.message else { continue };
            if entry.r#type.as_deref() != Some("assistant") {
                continue;
            }
            let Some(u) = msg.usage else { continue };
            total.input_tokens += u.input_tokens.unwrap_or(0);
            total.output_tokens += u.output_tokens.unwrap_or(0);
            total.cache_read_tokens += u.cache_read_input_tokens.unwrap_or(0);
            if let Some(cc) = u.cache_creation {
                total.cache_write_5m_tokens += cc.ephemeral_5m_input_tokens.unwrap_or(0);
                total.cache_write_1h_tokens += cc.ephemeral_1h_input_tokens.unwrap_or(0);
            } else {
                total.cache_write_5m_tokens += u.cache_creation_input_tokens.unwrap_or(0);
            }
            if model.is_empty() {
                if let Some(m) = msg.model {
                    model = m;
                }
            }
        }
        let cost = total.cost_usd(&model);
        Ok(SessionCost {
            model,
            usage: total,
            cost_usd: cost,
        })
    }
}

/// Codex transcript format is not yet known to this build. Returns zeros and
/// logs once per session_id. Replace when codex stdout/format is reverse-
/// engineered.
pub struct CodexStubReporter;

impl CostReporter for CodexStubReporter {
    fn poll(&self, session_id: &str, _cwd: &Path) -> Result<SessionCost, Error> {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            tracing::info!(
                target: "budget",
                session = %session_id,
                "codex cost reporting not implemented; reporting $0"
            );
        });
        Ok(SessionCost::default())
    }
}

#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
}

#[derive(Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<TranscriptUsage>,
}

#[derive(Deserialize)]
struct TranscriptUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: Option<u64>,
    #[serde(default)]
    ephemeral_1h_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_transcript(dir: &Path, session: &str, body: &str) -> PathBuf {
        let path = dir.join(format!("{session}.jsonl"));
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn missing_transcript_yields_zero() {
        let root = tempdir().unwrap();
        let r = ClaudeTranscriptReporter::with_root(root.path().to_path_buf());
        let got = r.poll("nope", Path::new("/tmp/x")).unwrap();
        assert_eq!(got, SessionCost::default());
    }

    #[test]
    fn aggregates_assistant_usage_only() {
        let root = tempdir().unwrap();
        let cwd = Path::new("/tmp/foo");
        let slug_dir = root.path().join(ClaudeTranscriptReporter::slug(cwd));
        fs::create_dir_all(&slug_dir).unwrap();
        let body = r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"model":"claude-opus-4-7","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5,"cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3}}}}
{"type":"assistant","message":{"model":"claude-opus-4-7","usage":{"input_tokens":1,"output_tokens":2}}}
{"type":"last-prompt","lastPrompt":"x"}
"#;
        write_transcript(&slug_dir, "sid-1", body);

        let r = ClaudeTranscriptReporter::with_root(root.path().to_path_buf());
        let got = r.poll("sid-1", cwd).unwrap();
        assert_eq!(got.usage.input_tokens, 11);
        assert_eq!(got.usage.output_tokens, 22);
        assert_eq!(got.usage.cache_read_tokens, 5);
        assert_eq!(got.usage.cache_write_5m_tokens, 2);
        assert_eq!(got.usage.cache_write_1h_tokens, 3);
        assert_eq!(got.model, "claude-opus-4-7");
        // 11*5 + 22*25 + 5*0.5 + 2*6.25 + 3*10 = 55 + 550 + 2.5 + 12.5 + 30 = 650 ($ / MTok)
        // -> 650e-6
        assert!((got.cost_usd - 650e-6).abs() < 1e-12);
    }

    #[test]
    fn skips_invalid_json_lines() {
        let root = tempdir().unwrap();
        let cwd = Path::new("/tmp/bar");
        let slug_dir = root.path().join(ClaudeTranscriptReporter::slug(cwd));
        fs::create_dir_all(&slug_dir).unwrap();
        let body = "not json\n{\"type\":\"assistant\",\"message\":{\"model\":\"claude-haiku-4-5\",\"usage\":{\"input_tokens\":100,\"output_tokens\":200}}}\n";
        write_transcript(&slug_dir, "sid-2", body);

        let r = ClaudeTranscriptReporter::with_root(root.path().to_path_buf());
        let got = r.poll("sid-2", cwd).unwrap();
        assert_eq!(got.usage.input_tokens, 100);
        assert_eq!(got.usage.output_tokens, 200);
    }

    #[test]
    fn codex_stub_returns_zero() {
        let r = CodexStubReporter;
        let got = r.poll("any", Path::new("/")).unwrap();
        assert_eq!(got, SessionCost::default());
    }
}
