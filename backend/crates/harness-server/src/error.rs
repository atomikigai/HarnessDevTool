use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use harness_session::{AgentKind, SessionError};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("binary not found for {kind}")]
    BinaryNotFound { kind: AgentKind, hint: String },
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Core(#[from] harness_core::Error),
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

impl From<harness_core::StoreError> for ApiError {
    fn from(e: harness_core::StoreError) -> Self {
        match e {
            harness_core::StoreError::NotFound(s) => ApiError::NotFound(s),
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<SessionError> for ApiError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::NotFound(s) => ApiError::SessionNotFound(s),
            SessionError::BinaryNotFound(k) => ApiError::BinaryNotFound {
                kind: k,
                hint: k.install_hint().to_string(),
            },
            SessionError::Invalid(s) => ApiError::BadRequest(s),
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<module_ssh::SshError> for ApiError {
    fn from(e: module_ssh::SshError) -> Self {
        match e {
            module_ssh::SshError::HostNotFound(id) => ApiError::NotFound(id),
            module_ssh::SshError::SessionNotFound(id) => ApiError::NotFound(id),
            module_ssh::SshError::Validation(msg) => ApiError::BadRequest(msg),
            module_ssh::SshError::NotImplemented(op) => ApiError::BadRequest(op.to_string()),
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, Json(json!({ "error": m.clone() }))),
            ApiError::SessionNotFound(m) => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("session not found: {m}") })),
            ),
            ApiError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": m.clone() })),
            ),
            ApiError::BinaryNotFound { kind, hint } => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("agent binary not found for {}", kind.as_str()),
                    "install_hint": hint,
                })),
            ),
            ApiError::BadRequest(m) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": m.clone() })))
            }
            ApiError::Core(e) => {
                let (code, msg) = match e {
                    harness_core::Error::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                    harness_core::Error::InvalidTransition { .. } => {
                        (StatusCode::CONFLICT, e.to_string())
                    }
                    harness_core::Error::Busy { .. } => (StatusCode::CONFLICT, e.to_string()),
                    harness_core::Error::Validation(_) => (StatusCode::BAD_REQUEST, e.to_string()),
                    harness_core::Error::LimitExceeded(_) => (StatusCode::CONFLICT, e.to_string()),
                    harness_core::Error::LeaseNotHeld(_) => (StatusCode::FORBIDDEN, e.to_string()),
                    _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                };
                return (code, Json(json!({ "error": msg }))).into_response();
            }
        };
        (status, body).into_response()
    }
}
