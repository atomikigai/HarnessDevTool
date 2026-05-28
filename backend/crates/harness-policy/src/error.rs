pub type PolicyResult<T> = Result<T, PolicyError>;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    TomlDe(#[from] toml_edit::de::Error),
    #[error("toml edit: {0}")]
    TomlEdit(#[from] toml_edit::TomlError),
}
