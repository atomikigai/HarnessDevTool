use std::sync::Arc;

use axum::http::{header, HeaderName, HeaderValue, Method, Uri};
use axum::{middleware, Router};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::routes;
use crate::state::AppState;

pub const PROTOCOL_VERSION: &str = "1.0";
pub const PROTOCOL_VERSION_HEADER: HeaderName = HeaderName::from_static("x-protocol-version");

pub fn build_router(state: Arc<AppState>, cfg: &Config) -> Router {
    let configured_cors_origin = cfg.cors_origin.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            cors_origin_allowed(origin, &configured_cors_origin)
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::AUTHORIZATION,
            PROTOCOL_VERSION_HEADER,
        ])
        .expose_headers([PROTOCOL_VERSION_HEADER]);

    let protocol_header_layer = SetResponseHeaderLayer::overriding(
        PROTOCOL_VERSION_HEADER,
        HeaderValue::from_static(PROTOCOL_VERSION),
    );

    let middleware = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(protocol_header_layer)
        .layer(cors)
        .layer(CompressionLayer::new());

    let protected = Router::new()
        .merge(routes::knowledge::router())
        .merge(routes::threads::router())
        .merge(routes::sessions::router())
        .merge(routes::events::router())
        .merge(routes::tasks::router())
        .merge(routes::spec::router())
        .merge(routes::agents::router())
        .merge(routes::approvals::router())
        .merge(routes::control::router())
        .merge(routes::budget::router())
        .merge(routes::db::router())
        .merge(routes::profiles::router())
        .merge(routes::transcript::router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_api_token,
        ));

    Router::new()
        .merge(routes::health::router())
        .merge(protected)
        .layer(middleware)
        .with_state(state)
}

fn cors_origin_allowed(origin: &HeaderValue, configured: &str) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    origin == configured || is_loopback_origin(origin)
}

fn is_loopback_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let scheme_allowed = matches!(uri.scheme_str(), Some("http" | "https"));
    let host_allowed = matches!(
        uri.host(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    scheme_allowed && host_allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_allows_configured_origin() {
        let origin = HeaderValue::from_static("https://example.test");

        assert!(cors_origin_allowed(&origin, "https://example.test"));
    }

    #[test]
    fn cors_allows_loopback_origins_on_any_port() {
        for raw in [
            "http://localhost:43178",
            "http://127.0.0.1:45678",
            "http://[::1]:50999",
        ] {
            let origin = HeaderValue::from_str(raw).unwrap();
            assert!(cors_origin_allowed(&origin, "https://example.test"));
        }
    }

    #[test]
    fn cors_rejects_non_loopback_origins() {
        let origin = HeaderValue::from_static("http://192.168.1.50:43178");

        assert!(!cors_origin_allowed(&origin, "https://example.test"));
    }
}
