use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use futures::stream::{self, Stream, StreamExt};
use harness_session::SessionEvent;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_stream::wrappers::BroadcastStream;

use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/events", get(events))
}

const PTY_CATCHUP_CHUNK: usize = 4096;

async fn events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventsQuery>,
) -> Sse<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin>> {
    let stream: Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin> =
        match (q.session, q.thread) {
            (Some(sid), _) => Box::new(Box::pin(session_stream(state, sid))),
            (None, Some(tid)) => Box::new(Box::pin(thread_stream(state, tid))),
            (None, None) => {
                // Legacy F0 behavior: forward the 5s tick channel as-is.
                let rx = state.tick_tx.subscribe();
                let s = BroadcastStream::new(rx).filter_map(|res| async move {
                    match res {
                        Ok(payload) => Some(Ok(SseEvent::default().data(payload))),
                        Err(_lag) => None,
                    }
                });
                Box::new(Box::pin(s))
            }
        };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Stream for `?thread=<tid>` (no session): forwards task events for the
/// thread as named SSE events.
fn thread_stream(
    state: Arc<AppState>,
    tid: String,
) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send {
    let rx = state.tasks.subscribe(&tid);
    BroadcastStream::new(rx).filter_map(|res| async move {
        let ev = match res {
            Ok(ev) => ev,
            Err(_lag) => return None,
        };
        let kind = task_event_sse_name(&ev);
        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
        Some(Ok(SseEvent::default().event(kind).data(data)))
    })
}

fn task_event_sse_name(ev: &harness_core::TaskEvent) -> &'static str {
    match ev {
        harness_core::TaskEvent::Created { .. } => "task.created",
        harness_core::TaskEvent::Changed { .. } => "task.changed",
        harness_core::TaskEvent::Updated { .. } => "task.updated",
        harness_core::TaskEvent::Ready { .. } => "task.ready",
        harness_core::TaskEvent::LeaseExpired { .. } => "task.lease-expired",
        harness_core::TaskEvent::SpecChanged { .. } => "spec.changed",
        harness_core::TaskEvent::ArtifactAdded { .. } => "artifact.added",
    }
}

/// Stream for `?session=<sid>`:
///   1. Catch-up: chunk-encode the persisted `output.log` into `session.output`
///      events with synthetic `seq` starting at 0.
///   2. Live tail: forward bus events for this session, overriding `seq` so the
///      sequence is contiguous across catch-up + live.
fn session_stream(
    state: Arc<AppState>,
    sid: String,
) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send {
    let manager = state.manager.clone();

    // 1) Catch-up.
    let history = match manager.read_output(&sid) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(error = %e, session = %sid, "no output.log for catch-up");
            Vec::new()
        }
    };

    let mut catchup_events: Vec<Result<SseEvent, Infallible>> = Vec::new();
    let mut seq: u64 = 0;
    for chunk in history.chunks(PTY_CATCHUP_CHUNK) {
        let payload = json!({
            "type": "session.output",
            "session_id": sid,
            "seq": seq,
            "b64": B64.encode(chunk),
        });
        catchup_events.push(Ok(SseEvent::default()
            .event("session.output")
            .data(payload.to_string())));
        seq += 1;
    }
    let catchup_stream = stream::iter(catchup_events);

    // 2) Live tail. Wrap `seq` in an atomic so the per-item closure can mutate it.
    let next_seq = Arc::new(AtomicU64::new(seq));
    let rx = manager.subscribe();
    let sid_filter = sid.clone();
    let live = BroadcastStream::new(rx).filter_map(move |res| {
        let sid_filter = sid_filter.clone();
        let next_seq = next_seq.clone();
        async move {
            let ev = match res {
                Ok(ev) => ev,
                Err(_lag) => return None,
            };
            if ev.session_id() != sid_filter {
                return None;
            }
            let event_name = ev.event_name();
            let payload = match ev {
                SessionEvent::Output {
                    session_id, b64, ..
                } => {
                    let new_seq = next_seq.fetch_add(1, Ordering::SeqCst);
                    json!({
                        "type": "session.output",
                        "session_id": session_id,
                        "seq": new_seq,
                        "b64": b64,
                    })
                    .to_string()
                }
                other => serde_json::to_string(&other).unwrap_or_else(|_| "{}".to_string()),
            };
            Some(Ok(SseEvent::default().event(event_name).data(payload)))
        }
    });

    catchup_stream.chain(live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_core::{TaskEvent, TaskStatus};

    #[test]
    fn task_event_sse_names_are_unchanged() {
        let events = vec![
            (
                TaskEvent::Created {
                    task_id: "T-0001".into(),
                    by: "human".into(),
                    at: Utc::now(),
                },
                "task.created",
            ),
            (
                TaskEvent::Changed {
                    task_id: "T-0001".into(),
                    prev_status: TaskStatus::Queued,
                    next_status: TaskStatus::InProgress,
                    by: "agent:a".into(),
                    at: Utc::now(),
                },
                "task.changed",
            ),
            (
                TaskEvent::Updated {
                    task_id: "T-0001".into(),
                    by: "human".into(),
                    at: Utc::now(),
                    fields: vec!["title".into()],
                },
                "task.updated",
            ),
            (
                TaskEvent::Ready {
                    task_id: "T-0001".into(),
                },
                "task.ready",
            ),
            (
                TaskEvent::LeaseExpired {
                    task_id: "T-0001".into(),
                    previous_holder: "agent:a".into(),
                },
                "task.lease-expired",
            ),
            (
                TaskEvent::SpecChanged {
                    thread_id: "thr-1".into(),
                    etag: "abc".into(),
                    bytes: 3,
                    at: Utc::now(),
                },
                "spec.changed",
            ),
            (
                TaskEvent::ArtifactAdded {
                    thread_id: "thr-1".into(),
                    path: "spec.md".into(),
                    kind: "spec".into(),
                    at: Utc::now(),
                },
                "artifact.added",
            ),
        ];

        for (event, expected) in events {
            assert_eq!(task_event_sse_name(&event), expected);
        }
    }
}
