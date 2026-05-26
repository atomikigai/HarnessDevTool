use thiserror::Error;

use crate::kind::AgentKind;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("binary not found for {0}")]
    BinaryNotFound(AgentKind),
    #[error("pty error: {0}")]
    Pty(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}
