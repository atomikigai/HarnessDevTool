use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use futures::stream::{Stream, StreamExt};
use harness_session::output::OUTPUT_READ_CHUNK_BYTES;
use harness_session::SessionEvent;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/events", get(events))
        .route("/api/events/pty", get(pty_events))
}

const PTY_CATCHUP_CHUNK: usize = 16 * 1024;
const PTY_FRAME_OUTPUT: u8 = 1;
const PTY_FRAME_EXIT: u8 = 2;
const PTY_FRAME_LAGGED: u8 = 3;

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
                let s = BroadcastStream::new(rx).filter_map(move |res| {
                    let state = state.clone();
                    async move {
                        match res {
                            Ok(payload) => Some(Ok(SseEvent::default().data(payload))),
                            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                                state.record_sse_lagged();
                                Some(Ok(lagged_event("tick", None, None, skipped)))
                            }
                        }
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

/// Binary PTY stream for native desktop clients.
///
/// Frame format:
///   byte 0: frame kind (1 output, 2 exit JSON, 3 lagged JSON)
///   bytes 1..5: big-endian u32 payload length
///   bytes 5..: payload
async fn pty_events(State(state): State<Arc<AppState>>, Query(q): Query<EventsQuery>) -> Response {
    let Some(sid) = q.session else {
        return (StatusCode::BAD_REQUEST, "missing session query parameter").into_response();
    };

    let stream = pty_binary_stream(state, sid);
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Stream for `?thread=<tid>` (no session): forwards task events for the
/// thread as named SSE events.
fn thread_stream(
    state: Arc<AppState>,
    tid: String,
) -> impl Stream<Item = Result<SseEvent, Infallible>> + Send {
    let rx = state.tasks.subscribe(&tid);
    BroadcastStream::new(rx).filter_map(move |res| {
        let tid = tid.clone();
        let state = state.clone();
        async move {
            let ev = match res {
                Ok(ev) => ev,
                Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                    state.record_sse_lagged();
                    return Some(Ok(lagged_event("thread", Some(&tid), None, skipped)));
                }
            };
            let kind = task_event_sse_name(&ev);
            let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
            Some(Ok(SseEvent::default().event(kind).data(data)))
        }
    })
}

fn task_event_sse_name(ev: &harness_core::TaskEvent) -> &'static str {
    match ev {
        harness_core::TaskEvent::Created { .. } => "task.created",
        harness_core::TaskEvent::Changed { .. } => "task.changed",
        harness_core::TaskEvent::Updated { .. } => "task.updated",
        harness_core::TaskEvent::ReasonChanged { .. } => "task.reason.changed",
        harness_core::TaskEvent::SchedulerDecision { .. } => "task.scheduler.decision",
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
    let rx = manager.subscribe();
    let next_seq = Arc::new(AtomicU64::new(0));

    // 1) Catch-up. Subscribe to the live bus first so output appended while
    // this blocking disk read runs is buffered in `rx` and not lost.
    let manager_for_history = manager.clone();
    let sid_for_history = sid.clone();
    let next_seq_for_history = next_seq.clone();
    let state_for_history = state.clone();
    let catchup_stream = async_stream::stream! {
        let mut offset = 0;
        loop {
            let sid_for_read = sid_for_history.clone();
            let manager_for_read = manager_for_history.clone();
            let read = match tokio::task::spawn_blocking(move || {
                manager_for_read.read_output_chunk(&sid_for_read, offset, OUTPUT_READ_CHUNK_BYTES)
            })
            .await
            {
                Ok(Ok(chunk)) => chunk,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, session = %sid_for_history, "no output.log for catch-up");
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, session = %sid_for_history, "output catch-up task failed");
                    break;
                }
            };

            if read.gap {
                state_for_history.record_sse_lagged();
                yield Ok(lagged_event("session", None, Some(&sid_for_history), 1));
            }

            for chunk in read.bytes.chunks(PTY_CATCHUP_CHUNK) {
                let seq = next_seq_for_history.fetch_add(1, Ordering::SeqCst);
                let payload = json!({
                    "type": "session.output",
                    "session_id": sid_for_history,
                    "seq": seq,
                    "b64": B64.encode(chunk),
                });
                yield Ok(SseEvent::default()
                    .event("session.output")
                    .data(payload.to_string()));
            }

            offset = read.next_offset;
            if read.bytes.is_empty() || offset >= read.active_len {
                break;
            }
        }
    };

    // 2) Live tail. Wrap `seq` in an atomic so the per-item closure can mutate it.
    let sid_filter = sid.clone();
    let state_for_live = state.clone();
    let live = BroadcastStream::new(rx).filter_map(move |res| {
        let sid_filter = sid_filter.clone();
        let next_seq = next_seq.clone();
        let state = state_for_live.clone();
        async move {
            let ev = match res {
                Ok(ev) => ev,
                Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                    state.record_sse_lagged();
                    return Some(Ok(lagged_event(
                        "session",
                        None,
                        Some(&sid_filter),
                        skipped,
                    )));
                }
            };
            if ev.session_id() != sid_filter {
                return None;
            }
            let event_name = ev.event_name();
            let payload = match ev {
                SessionEvent::Output {
                    session_id, bytes, ..
                } => {
                    let new_seq = next_seq.fetch_add(1, Ordering::SeqCst);
                    json!({
                        "type": "session.output",
                        "session_id": session_id,
                        "seq": new_seq,
                        "b64": B64.encode(bytes),
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

fn pty_binary_stream(
    state: Arc<AppState>,
    sid: String,
) -> impl Stream<Item = Result<Bytes, Infallible>> + Send {
    let manager = state.manager.clone();
    let rx = manager.subscribe();

    let manager_for_history = manager.clone();
    let sid_for_history = sid.clone();
    let state_for_history = state.clone();
    let catchup_stream = async_stream::stream! {
        let mut offset = 0;
        loop {
            let sid_for_read = sid_for_history.clone();
            let manager_for_read = manager_for_history.clone();
            let read = match tokio::task::spawn_blocking(move || {
                manager_for_read.read_output_chunk(&sid_for_read, offset, OUTPUT_READ_CHUNK_BYTES)
            })
            .await
            {
                Ok(Ok(chunk)) => chunk,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, session = %sid_for_history, "no output.log for binary catch-up");
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, session = %sid_for_history, "binary output catch-up task failed");
                    break;
                }
            };

            if read.gap {
                state_for_history.record_sse_lagged();
                let payload = json!({
                    "type": "lagged",
                    "stream": "session",
                    "session_id": sid_for_history,
                    "skipped": 1,
                    "resync": "reconnect",
                });
                yield Ok(pty_json_frame(PTY_FRAME_LAGGED, payload));
            }

            for chunk in read.bytes.chunks(PTY_CATCHUP_CHUNK) {
                yield Ok(pty_frame(PTY_FRAME_OUTPUT, chunk));
            }

            offset = read.next_offset;
            if read.bytes.is_empty() || offset >= read.active_len {
                break;
            }
        }
    };

    let sid_filter = sid.clone();
    let state_for_live = state.clone();
    let live = BroadcastStream::new(rx).filter_map(move |res| {
        let sid_filter = sid_filter.clone();
        let state = state_for_live.clone();
        async move {
            let ev = match res {
                Ok(ev) => ev,
                Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                    state.record_sse_lagged();
                    let payload = json!({
                        "type": "lagged",
                        "stream": "session",
                        "session_id": sid_filter,
                        "skipped": skipped,
                        "resync": "reconnect",
                    });
                    return Some(Ok(pty_json_frame(PTY_FRAME_LAGGED, payload)));
                }
            };
            if ev.session_id() != sid_filter {
                return None;
            }
            match ev {
                SessionEvent::Output { bytes, .. } => Some(Ok(pty_frame(PTY_FRAME_OUTPUT, &bytes))),
                SessionEvent::Exit {
                    session_id,
                    code,
                    signal,
                } => {
                    let payload = json!({
                        "type": "session.exit",
                        "session_id": session_id,
                        "code": code,
                        "signal": signal,
                    });
                    Some(Ok(pty_json_frame(PTY_FRAME_EXIT, payload)))
                }
                _ => None,
            }
        }
    });

    catchup_stream.chain(live)
}

fn pty_frame(kind: u8, payload: &[u8]) -> Bytes {
    let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(kind);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    Bytes::from(frame)
}

fn pty_json_frame(kind: u8, payload: serde_json::Value) -> Bytes {
    pty_frame(kind, payload.to_string().as_bytes())
}

fn lagged_event(
    stream: &'static str,
    thread_id: Option<&str>,
    session_id: Option<&str>,
    skipped: u64,
) -> SseEvent {
    let mut payload = json!({
        "type": "lagged",
        "stream": stream,
        "skipped": skipped,
        "resync": "reconnect",
    });
    if let Some(thread_id) = thread_id {
        payload["thread_id"] = json!(thread_id);
    }
    if let Some(session_id) = session_id {
        payload["session_id"] = json!(session_id);
    }
    SseEvent::default()
        .event("lagged")
        .data(payload.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_core::{SchedulerDecisionKind, SchedulerExplanation, TaskEvent, TaskStatus};

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
                TaskEvent::ReasonChanged {
                    task_id: "T-0001".into(),
                    reason_kind: "blocked_reason".into(),
                    value: "Waiting on dependency".into(),
                    by: "human".into(),
                    at: Utc::now(),
                },
                "task.reason.changed",
            ),
            (
                TaskEvent::SchedulerDecision {
                    explanation: SchedulerExplanation {
                        task_id: "T-0001".into(),
                        decision: SchedulerDecisionKind::AssignmentSkipped,
                        reason: "No idle generator is available".into(),
                        agent_id: None,
                        previous_holder: None,
                        blocked_by: vec![],
                        cooldown_seconds: None,
                        max_concurrent: None,
                        queue_depth: Some(1),
                        at: Utc::now(),
                    },
                },
                "task.scheduler.decision",
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
                    version: 1,
                    section: Some("requirements".into()),
                    section_version: Some(1),
                    bytes: 3,
                    at: Utc::now(),
                },
                "spec.changed",
            ),
            (
                TaskEvent::ArtifactAdded {
                    thread_id: "thr-1".into(),
                    artifact_id: "spec-v1".into(),
                    task_id: "".into(),
                    path: "spec.md".into(),
                    kind: "spec".into(),
                    produced_by: "legacy_put".into(),
                    summary: "Thread spec created".into(),
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
