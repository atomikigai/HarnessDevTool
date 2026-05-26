use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Core(#[from] harness_core::Error),
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match &self {
            ApiError::Core(e) => match e {
                harness_core::Error::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                harness_core::Error::InvalidTransition { .. } => {
                    (StatusCode::CONFLICT, e.to_string())
                }
                harness_core::Error::Busy { .. } => (StatusCode::CONFLICT, e.to_string()),
                harness_core::Error::Validation(_) => (StatusCode::BAD_REQUEST, e.to_string()),
                harness_core::Error::LeaseNotHeld(_) => (StatusCode::FORBIDDEN, e.to_string()),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            },
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
        };
        (code, Json(json!({ "error": msg }))).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
