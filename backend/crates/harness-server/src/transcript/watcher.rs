//! Background task that tails a Claude-style JSONL transcript and feeds
//! every parsed event into the `TranscriptStore` + a shared broadcast bus.
//! Each session gets at most one watcher; the handle can be aborted on
//! session kill via [`WatcherHandle::stop`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::event::TranscriptEvent;
use super::store::TranscriptStore;

/// Channel name the watcher broadcasts events on. We piggyback on a
/// `broadcast::Sender<TranscriptEvent>` per session rather than overloading
/// `SessionEvent`, so the existing PTY catch-up + tail logic stays clean.
pub type TranscriptBus = broadcast::Sender<TranscriptEvent>;
pub type TranscriptParser = fn(&str, &str) -> Vec<TranscriptEvent>;

/// Cancellation handle for an in-flight watcher task.
pub struct WatcherHandle {
    join: JoinHandle<()>,
}

impl WatcherHandle {
    pub fn stop(self) {
        self.join.abort();
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.join.abort();
    }
}

/// Spawn the watcher. `source_path` is the JSONL Claude (or future CLI) is
/// writing to; `store` is our own per-session normalised log; `bus`
/// broadcasts every ingested event to live SSE subscribers.
pub fn spawn_transcript_watcher(
    session_id: String,
    source_path: PathBuf,
    store: Arc<TranscriptStore>,
    bus: TranscriptBus,
    parser: TranscriptParser,
) -> WatcherHandle {
    let join = tokio::spawn(async move {
        if let Err(e) = watch_loop(&session_id, &source_path, store, bus, parser).await {
            tracing::warn!(
                session = %session_id,
                source = %source_path.display(),
                error = %e,
                "transcript watcher exited"
            );
        }
    });
    WatcherHandle { join }
}

/// Polling-based tail. We re-stat every 500ms; when the file grows beyond
/// the last offset, we read the new lines, parse them, and ingest. The
/// source file may not exist yet at spawn time (Claude creates it on first
/// turn) — we tolerate that with patient retries until kill.
async fn watch_loop(
    session_id: &str,
    source_path: &Path,
    store: Arc<TranscriptStore>,
    bus: TranscriptBus,
    parser: TranscriptParser,
) -> std::io::Result<()> {
    // Wait for the file to appear. Claude doesn't always write the
    // transcript line until the FIRST turn completes, so we may sit here
    // for a while on a fresh session.
    let mut tried = 0u32;
    while !source_path.exists() {
        tokio::time::sleep(Duration::from_millis(500)).await;
        tried += 1;
        if tried % 60 == 0 {
            tracing::debug!(
                source = %source_path.display(),
                "transcript file not yet present"
            );
        }
    }

    let mut offset: u64 = 0;
    let mut leftover = String::new();
    loop {
        let len = match tokio::fs::metadata(source_path).await {
            Ok(m) => m.len(),
            Err(e) => {
                // Transient — wait and try again.
                tracing::debug!(error = %e, "stat transcript failed");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        if len < offset {
            // File rotated / truncated — restart from the new beginning.
            tracing::warn!(
                source = %source_path.display(),
                "transcript shrank ({offset} → {len}); resetting"
            );
            offset = 0;
            leftover.clear();
        }
        if len > offset {
            match read_new_lines(source_path, offset, &mut leftover).await {
                Ok((new_offset, lines)) => {
                    offset = new_offset;
                    for line in lines {
                        let parsed = parser(&line, session_id);
                        for ev in parsed {
                            match store.ingest(ev).await {
                                Ok(persisted) => {
                                    // Subscribers may have dropped; ignore the
                                    // resulting send error so the watcher
                                    // keeps the on-disk log up to date.
                                    let _ = bus.send(persisted);
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "transcript ingest failed");
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "transcript read failed");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Read `[offset..eof)` from `path`, splitting on `\n`. Any trailing
/// fragment without a newline is preserved in `leftover` so it joins with
/// the next chunk. Returns the new offset + the completed lines.
async fn read_new_lines(
    path: &Path,
    offset: u64,
    leftover: &mut String,
) -> std::io::Result<(u64, Vec<String>)> {
    let mut file = File::open(path).await?;
    file.seek(SeekFrom::Start(offset)).await?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut new_offset = offset;
    let mut completed = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await?;
        if n == 0 {
            break;
        }
        new_offset += n as u64;
        let chunk = String::from_utf8_lossy(&buf).into_owned();
        if chunk.ends_with('\n') {
            let line = format!("{leftover}{}", chunk.trim_end_matches('\n'));
            leftover.clear();
            if !line.is_empty() {
                completed.push(line);
            }
        } else {
            // Final partial line — defer to next read.
            leftover.push_str(&chunk);
        }
    }
    Ok((new_offset, completed))
}

#[cfg(test)]
mod tests {
    use std::future;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn dropping_watcher_handle_aborts_task() {
        let join = tokio::spawn(async {
            future::pending::<()>().await;
        });
        let abort_handle = join.abort_handle();
        let handle = WatcherHandle { join };

        drop(handle);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(abort_handle.is_finished(), "watcher task should be aborted");
    }
}
