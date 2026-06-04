use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use serde::de::DeserializeOwned;
use thiserror::Error;
use uuid::Uuid;

use crate::events::{Event, TimelineItem, TimelineReport};
use crate::threads::{AutonomyProfile, ExecutionMode, Handoff, ReadinessReport, Thread};
use crate::{validate_profile_id, validate_task_id, validate_thread_id};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
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
        validate_profile_id(profile).map_err(StoreError::Validation)?;
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

    fn thread_dir(&self, thread_id: &str) -> Result<PathBuf, StoreError> {
        validate_thread_id(thread_id).map_err(StoreError::Validation)?;
        Ok(self.threads_dir.join(thread_id))
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
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn set_execution_mode(&self, id: &str, mode: ExecutionMode) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        let mut thread: Thread = serde_json::from_slice(&bytes)?;
        thread.execution_mode = Some(mode);
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;
        Ok(thread)
    }

    pub fn set_autonomy_profile(
        &self,
        id: &str,
        profile: AutonomyProfile,
    ) -> Result<Thread, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let meta_path = self.thread_dir(id)?.join("meta.json");
        if !meta_path.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let bytes = std::fs::read(&meta_path)?;
        let mut thread: Thread = serde_json::from_slice(&bytes)?;
        thread.autonomy_profile = Some(profile);
        let mut meta = File::create(&meta_path)?;
        meta.write_all(serde_json::to_vec_pretty(&thread)?.as_slice())?;
        meta.sync_all()?;
        Ok(thread)
    }

    pub fn write_readiness_report(
        &self,
        thread_id: &str,
        report: &ReadinessReport,
    ) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let path = dir.join("readiness.json");
        let mut f = File::create(&path)?;
        f.write_all(serde_json::to_vec_pretty(report)?.as_slice())?;
        f.sync_all()?;
        Ok(())
    }

    pub fn read_readiness_report(
        &self,
        thread_id: &str,
    ) -> Result<Option<ReadinessReport>, StoreError> {
        let path = self.thread_dir(thread_id)?.join("readiness.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn append_handoff(&self, thread_id: &str, handoff: &Handoff) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        validate_task_id(&handoff.task_id).map_err(StoreError::Validation)?;
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let handoffs_dir = dir.join("handoffs");
        std::fs::create_dir_all(&handoffs_dir)?;
        let path = handoffs_dir.join(format!("{}.jsonl", handoff.task_id));
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(handoff)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        Ok(())
    }

    pub fn read_handoffs(
        &self,
        thread_id: &str,
        task_id: &str,
    ) -> Result<Vec<Handoff>, StoreError> {
        validate_task_id(task_id).map_err(StoreError::Validation)?;
        let path = self
            .thread_dir(thread_id)?
            .join("handoffs")
            .join(format!("{task_id}.jsonl"));
        if !path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&path)?;
        read_jsonl_lossy(BufReader::new(f), &path)
    }

    /// Append a single event to a thread's `events.jsonl`. Returns the seq written.
    pub fn append_event(&self, thread_id: &str, event: &Event) -> Result<u64, StoreError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let dir = self.thread_dir(thread_id)?;
        if !dir.exists() {
            return Err(StoreError::NotFound(thread_id.to_string()));
        }
        let path = dir.join("events.jsonl");
        let seq = count_jsonl_records(&path)?;
        let mut event = event.clone();
        event.seq = seq;
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(&event)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        Ok(seq)
    }

    pub fn read_events(&self, thread_id: &str) -> Result<Vec<Event>, StoreError> {
        let path = self.thread_dir(thread_id)?.join("events.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&path)?;
        read_jsonl_lossy(BufReader::new(f), &path)
    }

    pub fn read_timeline(&self, thread_id: &str) -> Result<TimelineReport, StoreError> {
        self.get_thread(thread_id)?;
        let mut events = self.read_events(thread_id)?;
        events.sort_by_key(|event| event.seq);
        let items: Vec<TimelineItem> = events.into_iter().map(TimelineItem::from_event).collect();
        Ok(TimelineReport {
            thread_id: thread_id.to_string(),
            generated_at: Utc::now().timestamp_millis(),
            event_count: items.len(),
            items,
        })
    }
}

fn count_jsonl_records(path: &Path) -> Result<u64, StoreError> {
    if !path.exists() {
        return Ok(0);
    }
    let f = File::open(path)?;
    let mut count = 0;
    for line in BufReader::new(f).lines() {
        if !line?.trim().is_empty() {
            count += 1;
        }
    }
    Ok(count)
}

fn read_jsonl_lossy<T: DeserializeOwned>(
    reader: impl BufRead,
    path: &Path,
) -> Result<Vec<T>, StoreError> {
    let mut out = Vec::new();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(value) => out.push(value),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    line = line_no + 1,
                    error = %error,
                    "skipping corrupt jsonl record"
                );
            }
        }
    }
    Ok(out)
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
            thread_id: None,
            actor: None,
            payload: None,
        };
        let seq = store.append_event(&t.id, &ev).unwrap();
        let read = store.read_events(&t.id).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].event_type, "tick");
    }

    #[test]
    fn concurrent_appends_assign_unique_monotonic_seq() {
        let home = tmp_home();
        let store = std::sync::Arc::new(Store::new(home.path()).unwrap());
        let t = store.create_thread(None).unwrap();
        let mut handles = Vec::new();

        for i in 0..16 {
            let store = store.clone();
            let tid = t.id.clone();
            handles.push(std::thread::spawn(move || {
                let ev = Event {
                    seq: 999,
                    at: i,
                    event_type: "tick".into(),
                    items: vec![],
                    thread_id: Some(tid.clone()),
                    actor: None,
                    payload: Some(serde_json::json!({ "i": i })),
                };
                store.append_event(&tid, &ev).unwrap()
            }));
        }

        let mut returned: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        returned.sort_unstable();
        assert_eq!(returned, (0..16).collect::<Vec<_>>());

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(read.len(), 16);
        assert_eq!(
            read.iter().map(|ev| ev.seq).collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
    }

    #[test]
    fn old_event_without_envelope_fields_deserializes() {
        let json = r#"{"seq":7,"at":123,"type":"capability.decided","items":[]}"#;
        let ev: Event = serde_json::from_str(json).unwrap();
        assert_eq!(ev.seq, 7);
        assert_eq!(ev.event_type, "capability.decided");
        assert!(ev.thread_id.is_none());
        assert!(ev.actor.is_none());
        assert!(ev.payload.is_none());
    }

    #[test]
    fn new_envelope_round_trips() {
        let ev = Event {
            seq: 3,
            at: 123,
            event_type: "task.created".into(),
            items: vec![],
            thread_id: Some("thr-1".into()),
            actor: Some("human".into()),
            payload: Some(serde_json::json!({
                "type": "task.created",
                "task_id": "T-0001",
                "by": "human",
            })),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"thread_id\":\"thr-1\""));
        let decoded: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type, "task.created");
        assert_eq!(decoded.thread_id.as_deref(), Some("thr-1"));
        assert_eq!(decoded.actor.as_deref(), Some("human"));
        let payload = decoded.payload.unwrap();
        assert_eq!(payload["type"], "task.created");
        assert_eq!(payload["task_id"], "T-0001");
    }

    #[test]
    fn read_events_skips_corrupt_jsonl_records() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let ev = Event {
            seq: 0,
            at: 123,
            event_type: "tick".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        };
        let ev2 = Event {
            seq: 1,
            at: 124,
            event_type: "tock".into(),
            items: vec![],
            thread_id: None,
            actor: None,
            payload: None,
        };
        let path = store.threads_dir().join(&t.id).join("events.jsonl");
        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).unwrap();
        writeln!(f, "{{not-json").unwrap();
        writeln!(f, "{}", serde_json::to_string(&ev2).unwrap()).unwrap();

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].event_type, "tick");
        assert_eq!(read[1].event_type, "tock");
    }

    #[test]
    fn replay_orders_mixed_envelopes_by_append_seq() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();

        for event_type in [
            "task.created",
            "thread.readiness.checked",
            "capability.decided",
            "task.ready",
        ] {
            let ev = Event {
                seq: 999,
                at: 123,
                event_type: event_type.into(),
                items: vec![],
                thread_id: Some(t.id.clone()),
                actor: None,
                payload: Some(serde_json::json!({ "type": event_type })),
            };
            store.append_event(&t.id, &ev).unwrap();
        }

        let read = store.read_events(&t.id).unwrap();
        assert_eq!(
            read.iter()
                .map(|ev| (ev.seq, ev.event_type.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "task.created"),
                (1, "thread.readiness.checked"),
                (2, "capability.decided"),
                (3, "task.ready"),
            ]
        );
    }

    #[test]
    fn read_timeline_returns_ordered_summaries() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        for (event_type, payload) in [
            (
                "task.created",
                serde_json::json!({ "type": "task.created", "task_id": "T-0001" }),
            ),
            (
                "task.updated",
                serde_json::json!({
                    "type": "task.updated",
                    "task_id": "T-0001",
                    "fields": ["status"]
                }),
            ),
        ] {
            store
                .append_event(
                    &t.id,
                    &Event {
                        seq: 999,
                        at: 123,
                        event_type: event_type.into(),
                        items: vec![],
                        thread_id: Some(t.id.clone()),
                        actor: Some("test".into()),
                        payload: Some(payload),
                    },
                )
                .unwrap();
        }

        let report = store.read_timeline(&t.id).unwrap();
        assert_eq!(report.event_count, 2);
        assert_eq!(report.items[0].seq, 0);
        assert_eq!(report.items[0].summary, "Created task T-0001");
        assert_eq!(report.items[1].seq, 1);
        assert_eq!(report.items[1].summary, "Updated task T-0001: status");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();

        let err = store.read_events("../escape").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));

        let err = store.read_handoffs(&t.id, "../T-0001").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn readiness_and_execution_mode_roundtrip() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(Some("hello".into())).unwrap();
        let report = ReadinessReport::new(
            123,
            "/tmp/project",
            vec![],
            vec![],
            serde_json::json!({ "package_manager": "pnpm" }),
            ExecutionMode::Quick,
        );
        store.write_readiness_report(&t.id, &report).unwrap();
        let read = store.read_readiness_report(&t.id).unwrap().unwrap();
        assert_eq!(read.suggested_execution_mode, ExecutionMode::Quick);

        let updated = store
            .set_execution_mode(&t.id, ExecutionMode::Quick)
            .unwrap();
        assert_eq!(updated.execution_mode, Some(ExecutionMode::Quick));
        let updated = store
            .set_autonomy_profile(&t.id, AutonomyProfile::Autonomous)
            .unwrap();
        assert_eq!(updated.autonomy_profile, Some(AutonomyProfile::Autonomous));
        assert_eq!(
            store.get_thread(&t.id).unwrap().execution_mode,
            Some(ExecutionMode::Quick)
        );
    }

    #[test]
    fn handoffs_are_append_only_per_task() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let handoff = Handoff {
            at: 123,
            from: "agent:frontend-1".to_string(),
            to_role: "qa".to_string(),
            task_id: "T-0001".to_string(),
            status: "ready_for_verification".to_string(),
            goal: "Verify pagination".to_string(),
            assumptions: vec![],
            files_changed: vec!["src/orders.rs".to_string()],
            commands_run: vec!["cargo test orders".to_string()],
            verification_passed: vec!["cargo test orders".to_string()],
            verification_not_run: vec![],
            blocked_on: vec![],
            next_agent_action: "QA runs edge cases".to_string(),
        };
        store.append_handoff(&t.id, &handoff).unwrap();
        store.append_handoff(&t.id, &handoff).unwrap();
        let read = store.read_handoffs(&t.id, "T-0001").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].files_changed, vec!["src/orders.rs"]);
    }

    #[test]
    fn read_handoffs_skips_corrupt_jsonl_records() {
        let home = tmp_home();
        let store = Store::new(home.path()).unwrap();
        let t = store.create_thread(None).unwrap();
        let handoff = Handoff {
            at: 123,
            from: "agent:frontend-1".to_string(),
            to_role: "qa".to_string(),
            task_id: "T-0001".to_string(),
            status: "ready_for_verification".to_string(),
            goal: "Verify pagination".to_string(),
            assumptions: vec![],
            files_changed: vec!["src/orders.rs".to_string()],
            commands_run: vec!["cargo test orders".to_string()],
            verification_passed: vec!["cargo test orders".to_string()],
            verification_not_run: vec![],
            blocked_on: vec![],
            next_agent_action: "QA runs edge cases".to_string(),
        };
        let mut handoff2 = handoff.clone();
        handoff2.status = "accepted".to_string();
        let handoffs_dir = store.threads_dir().join(&t.id).join("handoffs");
        std::fs::create_dir_all(&handoffs_dir).unwrap();
        let path = handoffs_dir.join("T-0001.jsonl");
        let mut f = File::create(path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&handoff).unwrap()).unwrap();
        writeln!(f, "{{not-json").unwrap();
        writeln!(f, "{}", serde_json::to_string(&handoff2).unwrap()).unwrap();

        let read = store.read_handoffs(&t.id, "T-0001").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].status, "ready_for_verification");
        assert_eq!(read[1].status, "accepted");
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
