use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

pub const CALLER_ID_HEADER: &str = "x-harness-caller-id";
pub const CALLER_ROLE_HEADER: &str = "x-harness-caller-role";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerIdentity {
    pub id: String,
    pub role: String,
}

impl CallerIdentity {
    pub fn human() -> Self {
        Self {
            id: "human".to_string(),
            role: "human".to_string(),
        }
    }
}

pub async fn require_api_token(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let bearer_valid = state
        .api_token
        .as_deref()
        .is_some_and(|token| has_valid_bearer(req.headers(), token));
    if is_mutating(req.method()) && state.api_token.is_some() && !bearer_valid {
        Err(StatusCode::UNAUTHORIZED)
    } else {
        let caller = caller_identity(req.headers(), state.api_token.is_none() || bearer_valid);
        req.extensions_mut().insert(caller);
        Ok(next.run(req).await)
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

fn caller_identity(headers: &HeaderMap, trust_internal_headers: bool) -> CallerIdentity {
    if !trust_internal_headers {
        return CallerIdentity::human();
    }

    let Some(role) = header_str(headers, CALLER_ROLE_HEADER) else {
        return CallerIdentity::human();
    };
    if !valid_identity_part(role) {
        return CallerIdentity::human();
    }

    let id = header_str(headers, CALLER_ID_HEADER)
        .filter(|id| valid_identity_part(id))
        .unwrap_or("unknown");

    CallerIdentity {
        id: id.to_string(),
        role: role.to_string(),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn valid_identity_part(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
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

    #[test]
    fn caller_identity_defaults_to_human_without_internal_headers() {
        let headers = HeaderMap::new();

        assert_eq!(caller_identity(&headers, true), CallerIdentity::human());
    }

    #[test]
    fn caller_identity_uses_trusted_internal_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CALLER_ID_HEADER, "agent:codex-1".parse().unwrap());
        headers.insert(CALLER_ROLE_HEADER, "generator".parse().unwrap());

        assert_eq!(
            caller_identity(&headers, true),
            CallerIdentity {
                id: "agent:codex-1".to_string(),
                role: "generator".to_string(),
            }
        );
    }

    #[test]
    fn caller_identity_ignores_untrusted_internal_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CALLER_ID_HEADER, "agent:codex-1".parse().unwrap());
        headers.insert(CALLER_ROLE_HEADER, "planner".parse().unwrap());

        assert_eq!(caller_identity(&headers, false), CallerIdentity::human());
    }

    #[test]
    fn caller_identity_rejects_invalid_role_header() {
        let mut headers = HeaderMap::new();
        headers.insert(CALLER_ROLE_HEADER, "../planner".parse().unwrap());

        assert_eq!(caller_identity(&headers, true), CallerIdentity::human());
    }
}
