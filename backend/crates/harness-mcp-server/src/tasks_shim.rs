//! Local shim mirroring the LOCKED `harness_core::tasks::TaskStore` API.
//!
//! Why this exists:
//! The shard `phase-2-tasks-core` (parallel agent) is responsible for the real
//! `harness_core::tasks` module. At the time this crate was written that
//! module did not yet exist on this branch. To keep `harness-mcp-server`
//! self-contained and compilable, we ship a filesystem-backed minimal
//! implementation here with the exact public surface fixed by the brief:
//!
//! ```ignore
//! TaskStore::new(home) -> Result<Self>
//! TaskStore::list(thread_id, filters)
//! TaskStore::get(thread_id, task_id)
//! TaskStore::claim(thread_id, task_id, agent_id, ttl)
//! TaskStore::renew(thread_id, task_id, agent_id)
//! TaskStore::patch(thread_id, task_id, patch, by)
//! TaskStore::release(thread_id, task_id, agent_id)
//! TaskStore::submit(thread_id, task_id, artifacts, by)
//! ```
//!
//! When the real module lands, swap `use crate::tasks_shim as tasks;` for
//! `use harness_core::tasks;` in `dispatcher.rs` and the rest is identical.
//! Format on disk mirrors what we expect the real store to use:
//!
//!   <home>/profiles/default/threads/<tid>/tasks/<task_id>.json
//!   <home>/profiles/default/threads/<tid>/tasks/<task_id>.lease.json

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListFilters {
    pub status: Option<String>,
    pub label: Option<String>,
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub holder: String,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClaimResult {
    Granted(Lease),
    Busy {
        holder: String,
        until: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskPatch {
    pub status: Option<String>,
    pub label: Option<String>,
    pub assignee: Option<String>,
    pub title: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Artifacts {
    pub files: Vec<String>,
    pub turns: Option<u64>,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    pub status: String,
    pub label: Option<String>,
    pub assignee: Option<String>,
    pub notes: Option<String>,
    pub artifacts: Option<Artifacts>,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: Option<String>,
}

#[derive(Debug)]
pub struct TaskStore {
    home: PathBuf,
    write_lock: Mutex<()>,
}

impl TaskStore {
    pub fn new(home: &Path) -> Result<Self, TaskError> {
        fs::create_dir_all(home)?;
        Ok(Self {
            home: home.to_path_buf(),
            write_lock: Mutex::new(()),
        })
    }

    fn tasks_dir(&self, thread_id: &str) -> PathBuf {
        self.home
            .join("profiles")
            .join("default")
            .join("threads")
            .join(thread_id)
            .join("tasks")
    }

    fn task_path(&self, thread_id: &str, task_id: &str) -> PathBuf {
        self.tasks_dir(thread_id).join(format!("{task_id}.json"))
    }

    fn lease_path(&self, thread_id: &str, task_id: &str) -> PathBuf {
        self.tasks_dir(thread_id)
            .join(format!("{task_id}.lease.json"))
    }

    pub fn list(&self, thread_id: &str, filters: ListFilters) -> Result<Vec<Task>, TaskError> {
        let dir = self.tasks_dir(thread_id);
        let mut out = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !name.ends_with(".json") || name.ends_with(".lease.json") {
                continue;
            }
            let bytes = fs::read(&path)?;
            let task: Task = match serde_json::from_slice(&bytes) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "skip unreadable task");
                    continue;
                }
            };
            if filters
                .status
                .as_deref()
                .map(|s| s != task.status)
                .unwrap_or(false)
            {
                continue;
            }
            if filters
                .label
                .as_deref()
                .map(|l| task.label.as_deref() != Some(l))
                .unwrap_or(false)
            {
                continue;
            }
            if filters
                .assignee
                .as_deref()
                .map(|a| task.assignee.as_deref() != Some(a))
                .unwrap_or(false)
            {
                continue;
            }
            out.push(task);
        }
        out.sort_by_key(|t| t.created_at);
        Ok(out)
    }

    pub fn get(&self, thread_id: &str, task_id: &str) -> Result<Task, TaskError> {
        let p = self.task_path(thread_id, task_id);
        if !p.exists() {
            return Err(TaskError::NotFound(format!("task:{task_id}")));
        }
        Ok(serde_json::from_slice(&fs::read(&p)?)?)
    }

    pub fn claim(
        &self,
        thread_id: &str,
        task_id: &str,
        agent_id: &str,
        ttl: Duration,
    ) -> Result<ClaimResult, TaskError> {
        let _g = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        // Ensure task exists.
        let _ = self.get(thread_id, task_id)?;
        let lease_p = self.lease_path(thread_id, task_id);
        let now = Utc::now();
        if lease_p.exists() {
            let cur: Lease = serde_json::from_slice(&fs::read(&lease_p)?)?;
            if cur.until > now && cur.holder != agent_id {
                return Ok(ClaimResult::Busy {
                    holder: cur.holder,
                    until: cur.until,
                });
            }
        }
        let until = now + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::seconds(60));
        let lease = Lease {
            holder: agent_id.to_string(),
            until,
        };
        fs::create_dir_all(lease_p.parent().unwrap())?;
        fs::write(&lease_p, serde_json::to_vec(&lease)?)?;
        Ok(ClaimResult::Granted(lease))
    }

    pub fn renew(
        &self,
        thread_id: &str,
        task_id: &str,
        agent_id: &str,
    ) -> Result<Lease, TaskError> {
        let _g = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let lease_p = self.lease_path(thread_id, task_id);
        if !lease_p.exists() {
            return Err(TaskError::Invalid("no lease to renew".into()));
        }
        let mut cur: Lease = serde_json::from_slice(&fs::read(&lease_p)?)?;
        if cur.holder != agent_id {
            return Err(TaskError::Invalid("not the lease holder".into()));
        }
        cur.until = Utc::now() + chrono::Duration::seconds(60);
        fs::write(&lease_p, serde_json::to_vec(&cur)?)?;
        Ok(cur)
    }

    pub fn patch(
        &self,
        thread_id: &str,
        task_id: &str,
        patch: TaskPatch,
        by: &str,
    ) -> Result<Task, TaskError> {
        let _g = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut t = self.get(thread_id, task_id)?;
        if let Some(s) = patch.status {
            t.status = s;
        }
        if let Some(l) = patch.label {
            t.label = Some(l);
        }
        if let Some(a) = patch.assignee {
            t.assignee = Some(a);
        }
        if let Some(title) = patch.title {
            t.title = title;
        }
        if let Some(n) = patch.notes {
            t.notes = Some(n);
        }
        t.updated_at = Utc::now().timestamp_millis();
        t.updated_by = Some(by.to_string());
        let p = self.task_path(thread_id, task_id);
        fs::write(&p, serde_json::to_vec_pretty(&t)?)?;
        Ok(t)
    }

    pub fn release(
        &self,
        thread_id: &str,
        task_id: &str,
        agent_id: &str,
    ) -> Result<(), TaskError> {
        let _g = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let lease_p = self.lease_path(thread_id, task_id);
        if !lease_p.exists() {
            return Ok(());
        }
        let cur: Lease = serde_json::from_slice(&fs::read(&lease_p)?)?;
        if cur.holder != agent_id {
            return Err(TaskError::Invalid("not the lease holder".into()));
        }
        fs::remove_file(&lease_p)?;
        Ok(())
    }

    pub fn submit(
        &self,
        thread_id: &str,
        task_id: &str,
        artifacts: Artifacts,
        by: &str,
    ) -> Result<Task, TaskError> {
        let _g = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut t = self.get(thread_id, task_id)?;
        t.artifacts = Some(artifacts);
        t.status = "submitted".into();
        t.updated_at = Utc::now().timestamp_millis();
        t.updated_by = Some(by.to_string());
        let p = self.task_path(thread_id, task_id);
        fs::write(&p, serde_json::to_vec_pretty(&t)?)?;
        Ok(t)
    }

    /// Test/seed helper — not part of the LOCKED API but useful for unit tests.
    /// The real `harness_core::tasks` will likely provide `create` instead.
    #[doc(hidden)]
    pub fn _seed(&self, task: Task) -> Result<(), TaskError> {
        let dir = self.tasks_dir(&task.thread_id);
        fs::create_dir_all(&dir)?;
        let p = self.task_path(&task.thread_id, &task.id);
        fs::write(&p, serde_json::to_vec_pretty(&task)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "harness-mcp-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn seed(store: &TaskStore, thread: &str, id: &str) {
        store
            ._seed(Task {
                id: id.into(),
                thread_id: thread.into(),
                title: "t".into(),
                status: "open".into(),
                label: None,
                assignee: None,
                notes: None,
                artifacts: None,
                created_at: 1,
                updated_at: 1,
                updated_by: None,
            })
            .unwrap();
    }

    #[test]
    fn list_and_get() {
        let home = tmp();
        let s = TaskStore::new(&home).unwrap();
        seed(&s, "t1", "task-a");
        let all = s.list("t1", ListFilters::default()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(s.get("t1", "task-a").unwrap().id, "task-a");
    }

    #[test]
    fn claim_then_busy_then_release() {
        let home = tmp();
        let s = TaskStore::new(&home).unwrap();
        seed(&s, "t1", "task-a");
        match s
            .claim("t1", "task-a", "agent:1", Duration::from_secs(60))
            .unwrap()
        {
            ClaimResult::Granted(l) => assert_eq!(l.holder, "agent:1"),
            _ => panic!("expected granted"),
        }
        match s
            .claim("t1", "task-a", "agent:2", Duration::from_secs(60))
            .unwrap()
        {
            ClaimResult::Busy { holder, .. } => assert_eq!(holder, "agent:1"),
            _ => panic!("expected busy"),
        }
        s.release("t1", "task-a", "agent:1").unwrap();
        match s
            .claim("t1", "task-a", "agent:2", Duration::from_secs(60))
            .unwrap()
        {
            ClaimResult::Granted(l) => assert_eq!(l.holder, "agent:2"),
            _ => panic!("expected granted after release"),
        }
    }

    #[test]
    fn patch_and_submit() {
        let home = tmp();
        let s = TaskStore::new(&home).unwrap();
        seed(&s, "t1", "task-a");
        let patched = s
            .patch(
                "t1",
                "task-a",
                TaskPatch {
                    status: Some("in_progress".into()),
                    ..Default::default()
                },
                "agent:1",
            )
            .unwrap();
        assert_eq!(patched.status, "in_progress");
        let submitted = s
            .submit(
                "t1",
                "task-a",
                Artifacts {
                    files: vec!["a.rs".into()],
                    turns: Some(3),
                    diff: None,
                },
                "agent:1",
            )
            .unwrap();
        assert_eq!(submitted.status, "submitted");
        assert_eq!(submitted.artifacts.unwrap().files, vec!["a.rs"]);
    }
}
