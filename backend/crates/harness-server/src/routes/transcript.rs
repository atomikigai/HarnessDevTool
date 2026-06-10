//! SSE endpoint that streams the normalised transcript of a session.
//!
//! `GET /api/sessions/:sid/transcript?since=<seq>` replays every persisted
//! event with `seq > since`, then keeps the connection open and forwards
//! every new event broadcast by the watcher. Reconnects with `since` set
//! to the last received `seq` to avoid duplicates.
//!
//! Ordering guarantees:
//! - The live bus is subscribed *before* the persisted file is replayed, so
//!   an event ingested between replay and subscribe can never be lost — at
//!   worst it is seen twice, and duplicates are dropped by `seq` here.
//! - If the watcher slot is not registered yet (session just spawned, or the
//!   server restarted and rehydration is still pending), the connection
//!   replays what is on disk immediately and then waits up to
//!   [`SLOT_WAIT_TIMEOUT`] for the slot to appear instead of dying silently.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

use crate::state::AppState;
use crate::transcript::{read_events_since_helper, TranscriptEvent};

/// How often we re-check for a missing watcher slot.
const SLOT_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// How long a connection waits for the slot before giving up and closing
/// (the frontend reconnects with `since` and gets a fresh wait window).
const SLOT_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/sessions/:sid/transcript", get(transcript_stream))
}

#[derive(Debug, Default, Deserialize)]
struct Query_ {
    #[serde(default)]
    since: u64,
}

/// Structured items produced by [`transcript_item_stream`] before SSE
/// encoding. Kept separate from `SseEvent` (which has no readable accessors)
/// so tests can assert on the payload.
#[derive(Debug)]
enum StreamItem {
    Event(TranscriptEvent),
    Lagged(u64),
}

async fn transcript_stream(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    Query(q): Query<Query_>,
) -> Sse<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin>> {
    let sid_for_lagged = sid.clone();
    let stream = transcript_item_stream(state, sid, q.since).map(move |item| {
        Ok::<_, Infallible>(match item {
            StreamItem::Event(ev) => {
                let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
                SseEvent::default().event("transcript").data(payload)
            }
            StreamItem::Lagged(skipped) => lagged_event(&sid_for_lagged, skipped),
        })
    });

    let boxed: Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin> =
        Box::new(Box::pin(stream));
    Sse::new(boxed).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Replay + live tail as one ordered stream of [`StreamItem`]s.
///
/// Phases:
/// 1. If the slot is missing, replay whatever is persisted right away (the
///    client should not stare at a blank chat), then poll for the slot.
/// 2. Once the slot is available, subscribe to the bus FIRST, then re-read
///    the file with `since = last` to plug the replay→subscribe gap.
/// 3. Forward live bus events, dropping anything with `seq <= last`.
fn transcript_item_stream(
    state: Arc<AppState>,
    sid: String,
    since: u64,
) -> impl Stream<Item = StreamItem> + Send {
    async_stream::stream! {
        let mut last = since;

        // Fast path: slot already registered. Subscribing here — before any
        // file read — is what makes the stream gap-free.
        let mut parts = slot_parts(&state, &sid);

        if parts.is_none() {
            // No slot (yet). Serve the persisted events immediately, then
            // wait for the watcher slot to appear (session spawn in flight,
            // or restart rehydration pending) instead of closing silently.
            let path = fallback_transcript_path(&state, &sid);
            for ev in read_events_since_helper(&path, last).await.unwrap_or_default() {
                last = last.max(ev.seq);
                yield StreamItem::Event(ev);
            }

            let deadline = tokio::time::Instant::now() + SLOT_WAIT_TIMEOUT;
            parts = loop {
                if let Some(found) = slot_parts(&state, &sid) {
                    break Some(found);
                }
                if session_deleted(&state, &sid).await {
                    tracing::debug!(session = %sid, "transcript stream: session deleted while waiting for slot");
                    break None;
                }
                if tokio::time::Instant::now() >= deadline {
                    tracing::debug!(session = %sid, "transcript stream: no watcher slot after wait; closing");
                    break None;
                }
                tokio::time::sleep(SLOT_POLL_INTERVAL).await;
            };
        }

        let Some((path, mut rx)) = parts else {
            return;
        };

        // Catch-up replay. We are already subscribed, so events ingested
        // from this point on are buffered by the broadcast channel; anything
        // that lands both on disk and on the bus is deduped by `seq` below.
        for ev in read_events_since_helper(&path, last).await.unwrap_or_default() {
            last = last.max(ev.seq);
            yield StreamItem::Event(ev);
        }

        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if ev.seq > last {
                        last = ev.seq;
                        yield StreamItem::Event(ev);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    yield StreamItem::Lagged(skipped);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// `(jsonl path, live receiver)` when the watcher slot is registered. The
/// receiver is subscribed inside this call — callers rely on subscription
/// happening *before* they replay the file. The DashMap guard never crosses
/// an await point.
fn slot_parts(
    state: &AppState,
    sid: &str,
) -> Option<(PathBuf, broadcast::Receiver<TranscriptEvent>)> {
    state.transcripts.get(sid).map(|slot| {
        (
            slot.store.dir().join("transcript.jsonl"),
            slot.bus.subscribe(),
        )
    })
}

fn session_dir(state: &AppState, sid: &str) -> PathBuf {
    state
        .harness_home
        .join("profiles")
        .join(&state.profile)
        .join("sessions")
        .join(sid)
}

/// Persisted transcript location used when no slot is registered. Same
/// directory the `TranscriptStore` writes to.
fn fallback_transcript_path(state: &AppState, sid: &str) -> PathBuf {
    session_dir(state, sid).join("transcript.jsonl")
}

/// Mirrors `harness-session`'s tombstone marker: a deleted session will
/// never grow a watcher slot, so connections waiting on it should stop.
async fn session_deleted(state: &AppState, sid: &str) -> bool {
    tokio::fs::try_exists(session_dir(state, sid).join(".deleted"))
        .await
        .unwrap_or(false)
}

fn lagged_event(session_id: &str, skipped: u64) -> SseEvent {
    let payload = json!({
        "type": "lagged",
        "stream": "transcript",
        "session_id": session_id,
        "skipped": skipped,
        "resync": "reconnect_with_since",
    });
    SseEvent::default()
        .event("lagged")
        .data(payload.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::TranscriptSlot;
    use crate::transcript::event::{TranscriptKind, TranscriptSource};
    use crate::transcript::{TranscriptStore, WatcherHandle};

    fn state(home: std::path::PathBuf) -> Arc<AppState> {
        Arc::new(
            AppState::new(&Config {
                bind: "127.0.0.1:7777".parse().unwrap(),
                home,
                cors_origin: "http://localhost:8080".to_string(),
                profile: "default".to_string(),
                autonomy_profile: harness_core::AutonomyProfile::Assisted,
                api_token: None,
                evolution: Default::default(),
            })
            .unwrap(),
        )
    }

    fn test_event(sid: &str, content: &str) -> TranscriptEvent {
        TranscriptEvent {
            seq: 0,
            session_id: sid.to_string(),
            ts: "2026-06-10T00:00:00Z".to_string(),
            source: TranscriptSource::Claude,
            kind: TranscriptKind::Message,
            role: Some("assistant".to_string()),
            content: Some(content.to_string()),
            tool_name: None,
            tool_args: None,
            tool_use_id: None,
            tool_result: None,
            is_error: None,
            model: None,
            usage: None,
            subtype: None,
            raw: None,
        }
    }

    fn register_slot(
        state: &AppState,
        sid: &str,
    ) -> (
        Arc<TranscriptStore>,
        broadcast::Sender<TranscriptEvent>,
        broadcast::Receiver<TranscriptEvent>,
    ) {
        let store = TranscriptStore::open(session_dir(state, sid)).unwrap();
        let (bus, keepalive_rx) = broadcast::channel(64);
        state.transcripts.insert(
            sid.to_string(),
            TranscriptSlot {
                store: store.clone(),
                bus: bus.clone(),
                handle: WatcherHandle::noop(),
            },
        );
        (store, bus, keepalive_rx)
    }

    async fn next_event(stream: &mut (impl Stream<Item = StreamItem> + Unpin)) -> TranscriptEvent {
        let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("stream item before timeout")
            .expect("stream still open");
        match item {
            StreamItem::Event(ev) => ev,
            StreamItem::Lagged(n) => panic!("unexpected lagged item ({n})"),
        }
    }

    #[tokio::test]
    async fn stream_opened_before_slot_exists_receives_events_when_slot_appears() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let sid = "late-slot";

        // Connect FIRST — no slot, no transcript on disk.
        let mut stream = Box::pin(transcript_item_stream(state.clone(), sid.to_string(), 0));

        // Slot appears (and an event is ingested) only after a delay.
        let state_bg = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let (store, bus, _keep) = register_slot(&state_bg, "late-slot");
            let ev = store
                .ingest(test_event("late-slot", "hello"))
                .await
                .unwrap();
            let _ = bus.send(ev);
            // Keep the bus alive long enough for the subscriber to drain.
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let ev = next_event(&mut stream).await;
        assert_eq!(ev.seq, 1);
        assert_eq!(ev.content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn no_gap_between_replay_and_subscribe_and_no_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let sid = "gap-free";
        let (store, bus, _keep) = register_slot(&state, sid);

        // ev1 persisted before the stream exists → comes from replay.
        store.ingest(test_event(sid, "one")).await.unwrap();

        let mut stream = Box::pin(transcript_item_stream(state.clone(), sid.to_string(), 0));

        // ev2 lands after the stream is created but before it is first
        // polled — the window the old implementation lost. It is on disk
        // and (because the stream has not subscribed yet) NOT on the bus.
        let ev2 = store.ingest(test_event(sid, "two")).await.unwrap();
        let _ = bus.send(ev2.clone());

        let first = next_event(&mut stream).await;
        assert_eq!((first.seq, first.content.as_deref()), (1, Some("one")));
        let second = next_event(&mut stream).await;
        assert_eq!((second.seq, second.content.as_deref()), (2, Some("two")));

        // Re-broadcasting ev2 must be deduped (seq <= last); ev3 flows.
        let _ = bus.send(ev2);
        let ev3 = store.ingest(test_event(sid, "three")).await.unwrap();
        let _ = bus.send(ev3);

        let third = next_event(&mut stream).await;
        assert_eq!((third.seq, third.content.as_deref()), (3, Some("three")));
    }

    #[tokio::test]
    async fn deleted_session_closes_stream_after_replay() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let sid = "tombstoned";
        let sdir = session_dir(&state, sid);
        std::fs::create_dir_all(&sdir).unwrap();
        std::fs::write(sdir.join(".deleted"), b"deleted\n").unwrap();

        let mut stream = Box::pin(transcript_item_stream(state.clone(), sid.to_string(), 0));
        let end = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("stream should end quickly for a deleted session");
        assert!(end.is_none(), "expected end-of-stream, got {end:?}");
    }
}
