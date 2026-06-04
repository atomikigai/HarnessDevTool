//! SSE endpoint that streams the normalised transcript of a session.
//!
//! `GET /api/sessions/:sid/transcript?since=<seq>` replays every persisted
//! event with `seq > since`, then keeps the connection open and forwards
//! every new event broadcast by the watcher. Reconnects with `since` set
//! to the last received `seq` to avoid duplicates.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

use crate::state::AppState;
use crate::transcript::{read_events_since_helper, TranscriptEvent};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/sessions/:sid/transcript", get(transcript_stream))
}

#[derive(Debug, Default, Deserialize)]
struct Query_ {
    #[serde(default)]
    since: u64,
}

async fn transcript_stream(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    Query(q): Query<Query_>,
) -> Sse<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin>> {
    // Catch-up from the persisted JSONL. If the session has never produced
    // a transcript (no slot registered yet, or never started), we silently
    // send an empty replay and rely on the live tail when the slot appears.
    let transcript_path = state
        .transcripts
        .get(&sid)
        .map(|slot| slot.store.dir().join("transcript.jsonl"))
        .unwrap_or_else(|| {
            state
                .harness_home
                .join("profiles")
                .join(&state.profile)
                .join("sessions")
                .join(&sid)
                .join("transcript.jsonl")
        });
    let replay: Vec<TranscriptEvent> = read_events_since_helper(&transcript_path, q.since)
        .await
        .unwrap_or_default();

    let replay_stream = stream::iter(replay.into_iter().map(|ev| {
        let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
        Ok::<_, Infallible>(SseEvent::default().event("transcript").data(payload))
    }));

    // Live tail: subscribe to the watcher bus if a slot exists. If not,
    // we'd block — for the MVP we just return whatever was on disk and
    // close. The frontend will reconnect on session-state-change events.
    let live: Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin> =
        if let Some(slot) = state.transcripts.get(&sid) {
            let rx = slot.bus.subscribe();
            let sid_filter = sid.clone();
            let s = BroadcastStream::new(rx).filter_map(move |res| {
                let sid = sid_filter.clone();
                async move {
                    let ev = match res {
                        Ok(ev) => ev,
                        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                            return Some(Ok(lagged_event(&sid, skipped)));
                        }
                    };
                    let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
                    Some(Ok(SseEvent::default().event("transcript").data(payload)))
                }
            });
            Box::new(Box::pin(s))
        } else {
            // No live source — just close after replay.
            Box::new(Box::pin(stream::empty()))
        };

    let combined: Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send + Unpin> =
        Box::new(Box::pin(replay_stream.chain(live)));

    Sse::new(combined).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
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
