use thiserror::Error;

pub type SshResult<T> = Result<T, SshError>;

#[derive(Debug, Error)]
pub enum SshError {
    #[error("host not found: {0}")]
    HostNotFound(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("network operation not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("ssh command timed out")]
    Timeout,
    #[error("ssh command failed: {0}")]
    Command(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
