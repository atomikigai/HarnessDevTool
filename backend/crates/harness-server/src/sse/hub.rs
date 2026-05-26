use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tokio::time::interval;

use crate::state::AppState;

/// Spawn the background task that emits a tick every 5 seconds onto the
/// broadcast channel held in `AppState`. Senders without subscribers simply
/// discard the message, so this is always safe to run.
pub fn spawn_ticker(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        // Skip the immediate first tick so subscribers don't get an instant burst.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let payload = json!({
                "type": "tick",
                "at": Utc::now().timestamp_millis(),
            })
            .to_string();
            // Ignore send errors (no subscribers).
            let _ = state.tick_tx.send(payload);
        }
    });
}
