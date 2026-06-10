//! Background task that tails a Claude-style JSONL transcript and feeds
//! every parsed event into the `TranscriptStore` + a shared broadcast bus.
//! Each session gets at most one watcher; the handle can be aborted on
//! session kill via [`WatcherHandle::stop`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::event::TranscriptEvent;
use super::store::TranscriptStore;
use super::{claude, codex};

/// Channel name the watcher broadcasts events on. We piggyback on a
/// `broadcast::Sender<TranscriptEvent>` per session rather than overloading
/// `SessionEvent`, so the existing PTY catch-up + tail logic stays clean.
pub type TranscriptBus = broadcast::Sender<TranscriptEvent>;

#[derive(Debug, Clone, Copy)]
pub enum TranscriptParser {
    Claude,
    Codex,
}

/// Cancellation handle for an in-flight watcher task.
pub struct WatcherHandle {
    join: JoinHandle<()>,
}

impl WatcherHandle {
    pub fn stop(self) {
        self.join.abort();
    }

    /// Inert handle for tests that need to register a `TranscriptSlot`
    /// without running a real tail task.
    #[cfg(test)]
    pub(crate) fn noop() -> Self {
        Self {
            join: tokio::spawn(async {}),
        }
    }
}

/// Persisted tail position of the *source* JSONL (the file the CLI writes).
/// Lives next to our normalised store as
/// `<session dir>/watcher-checkpoint.json`. A watcher re-registered after a
/// server restart resumes from this offset instead of re-ingesting (and
/// duplicating) the whole source history into the append-only store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherCheckpoint {
    pub source_path: PathBuf,
    pub offset: u64,
}

fn checkpoint_path(dir: &Path) -> PathBuf {
    dir.join("watcher-checkpoint.json")
}

/// Read the persisted checkpoint for a session transcript dir, if any.
pub fn read_checkpoint(dir: &Path) -> Option<WatcherCheckpoint> {
    let raw = std::fs::read(checkpoint_path(dir)).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Best-effort atomic write (tmp + rename). The checkpoint is derived state:
/// losing it only costs a fallback to the conservative resume heuristics in
/// [`initial_offset`], never correctness of the store itself.
async fn write_checkpoint(dir: &Path, cp: &WatcherCheckpoint) {
    let Ok(raw) = serde_json::to_vec(cp) else {
        return;
    };
    let path = checkpoint_path(dir);
    let tmp = dir.join("watcher-checkpoint.json.tmp");
    let result = match tokio::fs::write(&tmp, raw).await {
        Ok(()) => tokio::fs::rename(&tmp, &path).await,
        Err(e) => Err(e),
    };
    if result.is_err() {
        tracing::debug!(path = %path.display(), "could not persist watcher checkpoint");
    }
}

/// Decide where in the source file a (re)started watcher should begin.
///
/// - Matching checkpoint → resume exactly where the previous watcher stopped.
/// - No checkpoint, empty store → fresh session, read from the beginning.
/// - No checkpoint, non-empty store (pre-checkpoint session) → start at EOF
///   so history is never ingested twice; only new events stream.
async fn initial_offset(source_path: &Path, store: &TranscriptStore) -> u64 {
    match read_checkpoint(store.dir()) {
        Some(cp) if cp.source_path == source_path => cp.offset,
        _ => {
            if store.last_seq() == 0 {
                0
            } else {
                tokio::fs::metadata(source_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0)
            }
        }
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
    parser: TranscriptParser,
    store: Arc<TranscriptStore>,
    bus: TranscriptBus,
) -> WatcherHandle {
    let join = tokio::spawn(async move {
        if let Err(e) = watch_loop(&session_id, &source_path, parser, store, bus).await {
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

/// Spawn a Codex watcher whose source JSONL is discovered after the PTY has
/// already started. Codex can create its session file after the backend route
/// returns, so the transcript slot must exist before the source path does.
pub fn spawn_codex_transcript_watcher(
    session_id: String,
    codex_home: PathBuf,
    cwd: PathBuf,
    started_at_ms: i64,
    marker: String,
    store: Arc<TranscriptStore>,
    bus: TranscriptBus,
) -> WatcherHandle {
    let join = tokio::spawn(async move {
        // A persisted checkpoint pins the already-discovered source file —
        // skip rediscovery entirely on rehydration so we never attach to a
        // different (newer) rollout than the one we ingested from.
        if let Some(cp) = read_checkpoint(store.dir()) {
            if cp.source_path.exists() {
                if let Err(e) = watch_loop(
                    &session_id,
                    &cp.source_path,
                    TranscriptParser::Codex,
                    store,
                    bus,
                )
                .await
                {
                    tracing::warn!(
                        session = %session_id,
                        source = %cp.source_path.display(),
                        error = %e,
                        "codex transcript watcher exited"
                    );
                }
                return;
            }
        }

        let mut attempts = 0u32;
        let source_path = loop {
            let marker_result =
                codex::find_latest_transcript_path(&codex_home, &cwd, started_at_ms, Some(&marker));
            match marker_result {
                Ok(Some(path)) => break path,
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!(
                        session = %session_id,
                        error = %e,
                        "codex transcript marker lookup failed"
                    );
                }
            }

            if attempts >= 10 {
                match codex::find_latest_transcript_path(&codex_home, &cwd, started_at_ms, None) {
                    Ok(Some(path)) => break path,
                    Ok(None) => {}
                    Err(e) => {
                        tracing::debug!(
                            session = %session_id,
                            error = %e,
                            "codex transcript fallback lookup failed"
                        );
                    }
                }
            }

            attempts = attempts.saturating_add(1);
            if attempts % 60 == 0 {
                tracing::debug!(
                    session = %session_id,
                    cwd = %cwd.display(),
                    "waiting for codex transcript file"
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };

        if let Err(e) = watch_loop(
            &session_id,
            &source_path,
            TranscriptParser::Codex,
            store,
            bus,
        )
        .await
        {
            tracing::warn!(
                session = %session_id,
                source = %source_path.display(),
                error = %e,
                "codex transcript watcher exited"
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
    parser: TranscriptParser,
    store: Arc<TranscriptStore>,
    bus: TranscriptBus,
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

    let mut offset: u64 = initial_offset(source_path, &store).await;
    if offset > 0 {
        tracing::info!(
            session = %session_id,
            source = %source_path.display(),
            offset,
            "transcript watcher resuming from persisted position"
        );
    }
    write_checkpoint(
        store.dir(),
        &WatcherCheckpoint {
            source_path: source_path.to_path_buf(),
            offset,
        },
    )
    .await;
    let mut checkpointed = offset;
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
        }
        if len > offset {
            match read_new_lines(source_path, offset).await {
                Ok((new_offset, lines)) => {
                    offset = new_offset;
                    for line in lines {
                        let parsed = match parser {
                            TranscriptParser::Claude => claude::parse_line(&line, session_id),
                            TranscriptParser::Codex => codex::parse_line(&line, session_id),
                        };
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
                    // Only persist offsets at line boundaries: with a partial
                    // line at EOF `offset` does not move, so skipping the
                    // rewrite also avoids checkpoint churn on every poll.
                    if offset != checkpointed {
                        write_checkpoint(
                            store.dir(),
                            &WatcherCheckpoint {
                                source_path: source_path.to_path_buf(),
                                offset,
                            },
                        )
                        .await;
                        checkpointed = offset;
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

/// Read `[offset..eof)` from `path`, splitting on `\n`. Returns the new
/// offset + the completed lines. A trailing fragment without a newline is
/// **not** consumed: the returned offset stays at the start of the partial
/// line, so the next poll — or a watcher restarted from the persisted
/// checkpoint — re-reads it whole once the CLI finishes writing it. This is
/// what makes the checkpoint crash-safe: it never points past data we have
/// not ingested.
async fn read_new_lines(path: &Path, offset: u64) -> std::io::Result<(u64, Vec<String>)> {
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
        if buf.last() != Some(&b'\n') {
            // Final partial line — leave the offset at its start and re-read
            // it on the next poll.
            break;
        }
        new_offset += n as u64;
        let chunk = String::from_utf8_lossy(&buf);
        let line = chunk.trim_end_matches('\n');
        if !line.is_empty() {
            completed.push(line.to_owned());
        }
    }
    Ok((new_offset, completed))
}

#[cfg(test)]
mod tests {
    use std::future;
    use std::io::Write;
    use std::time::Duration;

    use super::*;
    use crate::transcript::store::read_events_since;

    const LINE_ONE: &str = r#"{"type":"user","timestamp":"2026-06-10T00:00:00Z","message":{"role":"user","content":"uno"}}"#;
    const LINE_TWO: &str = r#"{"type":"user","timestamp":"2026-06-10T00:00:01Z","message":{"role":"user","content":"dos"}}"#;

    async fn wait_for_events(store_dir: &Path, expected: usize) -> Vec<TranscriptEvent> {
        let path = store_dir.join("transcript.jsonl");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let events = read_events_since(&path, 0).await.unwrap_or_default();
            if events.len() >= expected {
                return events;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {expected} events; got {}",
                events.len()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn restarted_watcher_resumes_from_checkpoint_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join("session");
        let source = dir.path().join("source.jsonl");
        std::fs::write(&source, format!("{LINE_ONE}\n")).unwrap();

        // First watcher run ingests line one and persists a checkpoint.
        let store = TranscriptStore::open(&store_dir).unwrap();
        let (bus, _rx) = broadcast::channel(16);
        let handle = spawn_transcript_watcher(
            "sid-1".to_string(),
            source.clone(),
            TranscriptParser::Claude,
            store,
            bus,
        );
        let events = wait_for_events(&store_dir, 1).await;
        assert_eq!(events[0].content.as_deref(), Some("uno"));
        handle.stop();

        let cp = read_checkpoint(&store_dir).expect("checkpoint persisted");
        assert_eq!(cp.source_path, source);
        assert_eq!(cp.offset, (LINE_ONE.len() + 1) as u64);

        // The CLI writes more while no watcher is running (server restart).
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .unwrap();
        writeln!(f, "{LINE_TWO}").unwrap();
        drop(f);

        // Second watcher (rehydration) must ingest ONLY the new line.
        let store = TranscriptStore::open(&store_dir).unwrap();
        let (bus, _rx) = broadcast::channel(16);
        let handle = spawn_transcript_watcher(
            "sid-1".to_string(),
            source.clone(),
            TranscriptParser::Claude,
            store,
            bus,
        );
        let events = wait_for_events(&store_dir, 2).await;
        handle.stop();

        assert_eq!(
            events.len(),
            2,
            "history must not be re-ingested: {events:?}"
        );
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[0].content.as_deref(), Some("uno"));
        assert_eq!(events[1].seq, 2);
        assert_eq!(events[1].content.as_deref(), Some("dos"));
    }

    /// Waits until the persisted checkpoint reaches `expected` (the watcher
    /// writes it asynchronously after ingesting).
    async fn wait_for_checkpoint(store_dir: &Path, expected: u64) -> WatcherCheckpoint {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(cp) = read_checkpoint(store_dir) {
                if cp.offset == expected {
                    return cp;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for checkpoint offset {expected}; got {:?}",
                read_checkpoint(store_dir)
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn partial_line_at_eof_survives_restart_without_loss_or_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join("session");
        let source = dir.path().join("source.jsonl");
        // One complete line + the first half of a second line (CLI mid-write).
        let (half, rest) = LINE_TWO.split_at(40);
        std::fs::write(&source, format!("{LINE_ONE}\n{half}")).unwrap();

        // First watcher run: ingests line one only; the checkpoint must stop
        // at the start of the partial line, never past it.
        let store = TranscriptStore::open(&store_dir).unwrap();
        let (bus, _rx) = broadcast::channel(16);
        let handle = spawn_transcript_watcher(
            "sid-1".to_string(),
            source.clone(),
            TranscriptParser::Claude,
            store,
            bus,
        );
        let events = wait_for_events(&store_dir, 1).await;
        assert_eq!(events[0].content.as_deref(), Some("uno"));
        let cp = wait_for_checkpoint(&store_dir, (LINE_ONE.len() + 1) as u64).await;
        assert_eq!(cp.source_path, source);
        handle.stop();

        // The CLI completes the line while the server is down (crash/restart).
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .unwrap();
        write!(f, "{rest}").unwrap();
        writeln!(f).unwrap();
        drop(f);

        // Restarted watcher resumes from the checkpoint and ingests the
        // completed line exactly once — not lost, not duplicated.
        let store = TranscriptStore::open(&store_dir).unwrap();
        let (bus, _rx) = broadcast::channel(16);
        let handle = spawn_transcript_watcher(
            "sid-1".to_string(),
            source.clone(),
            TranscriptParser::Claude,
            store,
            bus,
        );
        let events = wait_for_events(&store_dir, 2).await;
        handle.stop();

        assert_eq!(events.len(), 2, "exactly two events expected: {events:?}");
        assert_eq!(events[0].content.as_deref(), Some("uno"));
        assert_eq!(events[1].content.as_deref(), Some("dos"));
    }

    #[tokio::test]
    async fn pre_checkpoint_store_with_history_starts_at_source_eof() {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join("session");
        let source = dir.path().join("source.jsonl");
        std::fs::write(&source, format!("{LINE_ONE}\n")).unwrap();

        // Simulate a pre-checkpoint store that already holds an event.
        let store = TranscriptStore::open(&store_dir).unwrap();
        let ev = crate::transcript::claude::parse_line(LINE_ONE, "sid-1")
            .pop()
            .unwrap();
        store.ingest(ev).await.unwrap();

        // No checkpoint + non-empty store → resume at EOF, never at 0.
        assert_eq!(
            initial_offset(&source, &store).await,
            (LINE_ONE.len() + 1) as u64
        );
    }

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
