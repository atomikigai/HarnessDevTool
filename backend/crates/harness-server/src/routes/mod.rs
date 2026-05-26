use axum::Router;

use crate::state::AppState;

pub mod agents;
pub mod events;
pub mod tasks;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(tasks::router())
        .merge(agents::router())
        .merge(events::router())
}
