//! Normalised transcript persistence per session. Writes one JSONL file per
//! session under `<harness_home>/profiles/<p>/sessions/<sid>/transcript.jsonl`.
//! Lets the frontend reconnect with `?since=<seq>` without depending on the
//! upstream CLI's transcript format.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::fs::File as AsyncFile;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::sync::Mutex;

use super::event::TranscriptEvent;

/// Per-session writer that assigns monotonic `seq` numbers and persists each
/// event as a JSONL line. Cheap to share via `Arc` across the watcher task
/// and the SSE replay route.
pub struct TranscriptStore {
    dir: PathBuf,
    seq: AtomicU64,
    file: Mutex<std::fs::File>,
}

impl TranscriptStore {
    /// Open (creating + appending) the transcript log for a session. Reads
    /// the existing file (if any) to recover the next `seq` so restarts of
    /// the watcher don't reset the counter.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Arc<Self>> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("transcript.jsonl");
        let last_seq = recover_last_seq(&path).unwrap_or(0);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Arc::new(Self {
            dir,
            seq: AtomicU64::new(last_seq),
            file: Mutex::new(file),
        }))
    }

    /// Assign the next `seq` and persist the event. Returns the event with
    /// `seq` populated so callers can broadcast the same value they wrote.
    pub async fn ingest(&self, mut ev: TranscriptEvent) -> std::io::Result<TranscriptEvent> {
        ev.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let line = serde_json::to_string(&ev).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("serialize: {e}"))
        })?;
        let mut f = self.file.lock().await;
        writeln!(f, "{line}")?;
        Ok(ev)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Stream every persisted event with `seq > since` from disk. Used by the
/// replay arm of the SSE endpoint.
pub async fn read_events_since(
    transcript_path: &Path,
    since: u64,
) -> std::io::Result<Vec<TranscriptEvent>> {
    if !transcript_path.exists() {
        return Ok(Vec::new());
    }
    let file = AsyncFile::open(transcript_path).await?;
    let mut reader = AsyncBufReader::new(file);
    let mut out = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).await?;
        if n == 0 {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<TranscriptEvent>(line) {
            Ok(ev) => {
                if ev.seq > since {
                    out.push(ev);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "skipping malformed transcript line");
            }
        }
    }
    Ok(out)
}

/// Scan the existing transcript file for the highest `seq` so we resume the
/// counter on watcher restart. Cheap — single pass, allocates one line at
/// a time. Tolerates malformed lines.
fn recover_last_seq(path: &Path) -> std::io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut max_seq: u64 = 0;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        // Cheap field extraction without full deserialisation; safe because
        // we control the writer.
        if let Some(idx) = line.find("\"seq\":") {
            let after = &line[idx + 6..];
            let end = after
                .find(',')
                .or_else(|| after.find('}'))
                .unwrap_or(after.len());
            if let Ok(n) = after[..end].trim().parse::<u64>() {
                if n > max_seq {
                    max_seq = n;
                }
            }
        }
    }
    Ok(max_seq)
}
