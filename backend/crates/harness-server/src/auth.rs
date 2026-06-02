use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

pub async fn require_api_token(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if !is_mutating(req.method()) {
        return Ok(next.run(req).await);
    }
    let Some(token) = state.api_token.as_deref() else {
        return Ok(next.run(req).await);
    };
    if has_valid_bearer(req.headers(), token) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    )
}

fn has_valid_bearer(headers: &HeaderMap, token: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(received) = value.strip_prefix("Bearer ") else {
        return false;
    };
    received == token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_header_must_match_token() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());

        assert!(has_valid_bearer(&headers, "secret"));
        assert!(!has_valid_bearer(&headers, "other"));
    }

    #[test]
    fn non_bearer_header_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Basic secret".parse().unwrap());

        assert!(!has_valid_bearer(&headers, "secret"));
    }
}
