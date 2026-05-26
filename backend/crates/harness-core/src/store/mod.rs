use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::events::Event;
use crate::threads::Thread;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
}

/// Filesystem-backed store rooted at `<home>/profiles/<profile>`.
///
/// Layout:
/// ```text
/// <home>/profiles/default/threads/<uuid>/meta.json
/// <home>/profiles/default/threads/<uuid>/events.jsonl
/// ```
#[derive(Debug)]
pub struct Store {
    threads_dir: PathBuf,
    write_lock: Mutex<()>,
}

impl Store {
    pub fn new(home: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::with_profile(home, "default")
    }

    pub fn with_profile(home: impl AsRef<Path>, profile: &str) -> Result<Self, StoreError> {
        let threads_dir = home.as_ref().join("profiles").join(profile).join("threads");
        std::fs::create_dir_all(&threads_dir)?;
        Ok(Self {
            threads_dir,
            write_lock: Mutex::new(()),
        })
    }

    pub fn threads_dir(&self) -> &Path {
        &self.threads_dir
    }

    pub fn create_thread(&self, title: Option<String>) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();
        let thread = Thread::new(id.clone(), title, created_at);

        let dir = self.threads_dir.join(&id);
        std::fs::create_dir_all(&dir)?;

        let meta_path = dir.join("meta.json");
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;

        // touch events.jsonl
        let events_path = dir.join("events.jsonl");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_path)?;

        Ok(thread)
    }

    pub fn list_threads(&self) -> Result<Vec<Thread>, StoreError> {
        let mut out = Vec::new();
        let read = match std::fs::read_dir(&self.threads_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in read {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            let bytes = std::fs::read(&meta_path)?;
            match serde_json::from_slice::<Thread>(&bytes) {
                Ok(t) => out.push(t),
                Err(e) => {
                    tracing::warn!(error = %e, path = %meta_path.display(), "skipping unreadable thread meta");
                }
            }
        }
        out.sort_by_key(|t| t.created_at);
        Ok(out)
    }

    pub fn get_thread(&self, id: &str) -> Result<Thread, StoreError> {
        let meta_path = self.threads_dir.join(id).join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Append a single event to a thread's `events.jsonl`. Returns the seq written.
    pub fn append_event(&self, thread_id: &str, event: &Event) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.threads_dir.join(thread_id);
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let path = dir.join("events.jsonl");
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(event)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        Ok(())
    }

    pub fn read_events(&self, thread_id: &str) -> Result<Vec<Event>, StoreError> {
        let path = self.threads_dir.join(thread_id).join("events.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&path)?;
        let reader = BufReader::new(f);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&line)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home() -> tempdir_like::TempDir {
        tempdir_like::TempDir::new("harness-core-test")
    }

    #[test]
    fn create_and_list_threads() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        assert!(store.list_threads().unwrap().is_empty());
        let t = store.create_thread(Some("hello".into())).unwrap();
        let listed = store.list_threads().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, t.id);
        assert_eq!(listed[0].title.as_deref(), Some("hello"));
    }

    #[test]
    fn append_and_read_events() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let ev = Event {
            seq: 0,
            at: 123,
            event_type: "tick".into(),
            items: vec![],
        };
        store.append_event(&t.id, &ev).unwrap();
        let read = store.read_events(&t.id).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].event_type, "tick");
    }
}

// Tiny ad-hoc tempdir helper to avoid an extra dev-dep.
#[cfg(test)]
mod tempdir_like {
    use std::path::{Path, PathBuf};

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let pid = std::process::id();
            let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}"));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
