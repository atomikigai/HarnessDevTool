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
}

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
        };
        (status, body).into_response()
    }
}
