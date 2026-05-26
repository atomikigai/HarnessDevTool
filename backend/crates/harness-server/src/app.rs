use std::sync::Arc;

use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::Router;
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
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::exact(
            HeaderValue::from_str(&cfg.cors_origin).expect("invalid HARNESS_CORS_ORIGIN value"),
        ))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION])
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

    Router::new()
        .merge(routes::health::router())
        .merge(routes::threads::router())
        .merge(routes::sessions::router())
        .merge(routes::events::router())
        .layer(middleware)
        .with_state(state)
}
