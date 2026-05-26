//! SSE endpoint. F2 contract: `?thread=:tid` (without `?session=`) emits task
//! events for that thread. `?session=:sid` is the existing session stream that
//! the harness-session crate hooks into (parallel agent owns that path).

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use axum::Router;
use futures::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/events", get(events))
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub thread: Option<String>,
    pub session: Option<String>,
}

async fn events(
    State(s): State<AppState>,
    Query(q): Query<EventsQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Session-scoped streams are owned by harness-session; F2 only handles the
    // thread-scoped task stream. When both are absent, we emit a heartbeat.
    let thread_id = q.thread.clone();
    let _session_id = q.session;

    let rx = thread_id.as_deref().map(|tid| s.tasks.subscribe(tid));

    let stream = async_stream::stream! {
        // Initial comment so clients can detect the open stream.
        yield Ok::<_, Infallible>(Event::default().comment("open"));

        if let Some(rx) = rx {
            let mut bs = BroadcastStream::new(rx);
            while let Some(item) = bs.next().await {
                match item {
                    Ok(ev) => {
                        let kind = match &ev {
                            harness_core::TaskEvent::Created { .. } => "task.created",
                            harness_core::TaskEvent::Changed { .. } => "task.changed",
                            harness_core::TaskEvent::Updated { .. } => "task.updated",
                            harness_core::TaskEvent::Ready { .. } => "task.ready",
                            harness_core::TaskEvent::LeaseExpired { .. } => "task.lease-expired",
                        };
                        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
                        yield Ok(Event::default().event(kind).data(data));
                    }
                    Err(_lagged) => {
                        yield Ok(Event::default().event("warn").data("lagged"));
                    }
                }
            }
        } else {
            // No thread; keep-alive only (placeholder until session stream lands).
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                yield Ok(Event::default().comment("keepalive"));
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
