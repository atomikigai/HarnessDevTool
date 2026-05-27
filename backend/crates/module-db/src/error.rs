use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("unsupported engine for this operation: {0}")]
    Unsupported(String),
    #[error("table has no detectable primary key: {0}")]
    NoPrimaryKey(String),
    #[error("query not found or already finished: {0}")]
    QueryNotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(String),
    #[error("keyring: {0}")]
    Keyring(String),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("internal: {0}")]
    Internal(String),
}

pub type DbResult<T> = Result<T, DbError>;

impl From<keyring::Error> for DbError {
    fn from(e: keyring::Error) -> Self {
        DbError::Keyring(e.to_string())
    }
}
