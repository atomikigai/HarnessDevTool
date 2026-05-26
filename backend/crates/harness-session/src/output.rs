use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::errors::SessionError;

const MAX_LOG_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ROTATED: usize = 5;
const ZSTD_LEVEL: i32 = 3;

/// Append-only writer for the raw PTY byte stream of a session.
///
/// Writes to `<dir>/output.log`. When the active file exceeds [`MAX_LOG_BYTES`],
/// it is compressed with zstd and rotated to `output.log.<N>.zst`, keeping at
/// most [`MAX_ROTATED`] historical files.
#[derive(Debug)]
pub struct OutputWriter {
    dir: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    file: File,
    written: u64,
}

impl OutputWriter {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SessionError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("output.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            dir,
            inner: Mutex::new(Inner { file, written }),
        })
    }

    pub fn active_path(&self) -> PathBuf {
        self.dir.join("output.log")
    }

    /// Append a chunk of bytes. May trigger rotation when the threshold is hit.
    pub fn append(&self, bytes: &[u8]) -> Result<(), SessionError> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.file.write_all(bytes)?;
        inner.written += bytes.len() as u64;
        if inner.written >= MAX_LOG_BYTES {
            if let Err(e) = self.rotate_locked(&mut inner) {
                tracing::warn!(error = %e, "log rotation failed");
            }
        }
        Ok(())
    }

    /// Read the full active `output.log` from disk (for SSE catch-up).
    /// Does NOT include rotated history (those are intentionally not replayed —
    /// once compressed they are considered archival).
    pub fn read_active(&self) -> Result<Vec<u8>, SessionError> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.dir.join("output.log");
        // Use a fresh handle since `inner.file` is opened in append mode.
        let mut f = File::open(&path)?;
        let mut buf = Vec::with_capacity(inner.written as usize);
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Force-flush to disk.
    pub fn flush(&self) -> Result<(), SessionError> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.file.flush()?;
        Ok(())
    }

    fn rotate_locked(&self, inner: &mut Inner) -> Result<(), SessionError> {
        inner.file.flush()?;

        // Shift existing rotated files: <N>.zst -> <N+1>.zst; drop oldest above MAX_ROTATED.
        // Indexes are 1-based; 1 = newest rotated.
        for n in (1..=MAX_ROTATED).rev() {
            let from = self.dir.join(format!("output.log.{n}.zst"));
            if !from.exists() {
                continue;
            }
            if n == MAX_ROTATED {
                std::fs::remove_file(&from)?;
            } else {
                let to = self.dir.join(format!("output.log.{}.zst", n + 1));
                std::fs::rename(&from, &to)?;
            }
        }

        // Compress current output.log -> output.log.1.zst
        let src_path = self.dir.join("output.log");
        let dst_path = self.dir.join("output.log.1.zst");
        compress_file(&src_path, &dst_path)?;

        // Truncate (reuse the same file handle).
        inner.file.set_len(0)?;
        inner.file.seek(SeekFrom::Start(0))?;
        inner.written = 0;
        Ok(())
    }
}

fn compress_file(src: &Path, dst: &Path) -> Result<(), SessionError> {
    let mut input = File::open(src)?;
    let output = File::create(dst)?;
    let mut encoder = zstd::Encoder::new(output, ZSTD_LEVEL)?;
    std::io::copy(&mut input, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("harness-session-test-{name}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn append_and_read_back() {
        let dir = tmp("rw");
        let w = OutputWriter::open(&dir).unwrap();
        w.append(b"hello").unwrap();
        w.append(b" world").unwrap();
        w.flush().unwrap();
        let bytes = w.read_active().unwrap();
        assert_eq!(bytes, b"hello world");
        std::fs::remove_dir_all(&dir).ok();
    }
}
