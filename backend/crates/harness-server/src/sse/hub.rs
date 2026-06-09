use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::state::AppState;

pub struct TickerHandle {
    join: JoinHandle<()>,
}

impl TickerHandle {
    pub fn stop(self) {
        self.join.abort();
    }
}

impl Drop for TickerHandle {
    fn drop(&mut self) {
        self.join.abort();
    }
}

/// Spawn the background task that emits a tick every 5 seconds onto the
/// broadcast channel held in `AppState`. Senders without subscribers simply
/// discard the message, so this is always safe to run.
pub fn spawn_ticker(state: Arc<AppState>) -> TickerHandle {
    spawn_ticker_tx(state.tick_tx.clone(), Duration::from_secs(5))
}

fn spawn_ticker_tx(tx: broadcast::Sender<String>, period: Duration) -> TickerHandle {
    let join = tokio::spawn(async move {
        let mut ticker = interval(period);
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
            let _ = tx.send(payload);
        }
    });
    TickerHandle { join }
}

#[cfg(test)]
mod tests {
    use std::future;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn dropping_ticker_handle_aborts_task() {
        let join = tokio::spawn(async {
            future::pending::<()>().await;
        });
        let abort_handle = join.abort_handle();
        let handle = TickerHandle { join };

        drop(handle);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(abort_handle.is_finished(), "ticker task should be aborted");
    }
}
