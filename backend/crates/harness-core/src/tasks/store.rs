//! Filesystem-backed task store. Source of truth = one TOML per task.
//! Concurrency is enforced by `fs2::FileExt::lock_exclusive` on the `.toml`.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use fs2::FileExt;
use tokio::sync::broadcast;

use super::events::TaskEvent;
use super::ids::next_id;
use super::index::Index;
use super::model::{
    AcceptanceBlock, Artifact, ArtifactKind, Artifacts, ClaimResult, HistoryBlock, HistoryEvent,
    Lease, ListFilters, Notes, Task, TaskDraft, TaskPatch, TaskStatus,
};
use super::state_machine::validate_transition;
use crate::Store;
use crate::{validate_profile_id, validate_task_id, validate_thread_id, Error};

const BROADCAST_CAP: usize = 256;

/// Maximum number of "active" tasks (anything not `done` / `abandoned`) that
/// can coexist in a single thread. Hitting the cap forces the agent to finish
/// or abandon work in flight before opening a new front — matches the human
/// rule that a conversation should focus on one of a few small goals at a
/// time, not pile up dozens of half-started threads.
pub const THREAD_ACTIVE_TASK_CAP: usize = 3;

/// Whether a status counts toward [`THREAD_ACTIVE_TASK_CAP`] and toward the
/// "what should we resume?" auto-pick at session spawn.
fn is_active(status: TaskStatus) -> bool {
    !matches!(
        status,
        TaskStatus::Proposed | TaskStatus::Done | TaskStatus::Abandoned
    )
}

/// Filesystem-backed [`Task`] store rooted at `$HARNESS_HOME/profiles/default`.
///
/// Cheap to clone — shared internal state is `Arc`-wrapped. Use this as the
/// canonical store everywhere (REST routes, MCP, scheduler).
#[derive(Clone)]
pub struct TaskStore {
    home: PathBuf,
    /// Active profile (workspace) id. Threaded into every on-disk path so
    /// switching profiles isolates tasks per workspace.
    profile: String,
    threads: Arc<Mutex<HashMap<String, ThreadState>>>,
    event_store: Option<Arc<Store>>,
}

struct ThreadState {
    index: Arc<Mutex<Index>>,
    bus: broadcast::Sender<TaskEvent>,
}

impl TaskStore {
    /// Create a store rooted at `$HARNESS_HOME` (the parent of `profiles/`)
    /// using the `"default"` profile. Kept for backwards compatibility with
    /// tests and isolated callers; prefer [`Self::with_profile`].
    pub fn new(home: &Path) -> Result<Self, Error> {
        Self::with_profile(home, "default")
    }

    /// Create a store scoped to a specific profile (workspace) id.
    pub fn with_profile(home: &Path, profile: &str) -> Result<Self, Error> {
        validate_profile_id(profile).map_err(Error::Validation)?;
        let threads_root = home.join("profiles").join(profile).join("threads");
        fs::create_dir_all(&threads_root)?;
        Ok(Self {
            home: home.to_path_buf(),
            profile: profile.to_string(),
            threads: Arc::new(Mutex::new(HashMap::new())),
            event_store: None,
        })
    }

    /// Attach the append-only event store used by the server to persist every
    /// emitted [`TaskEvent`] as a replayable envelope. Isolated callers such as
    /// the MCP server intentionally leave this unset to avoid double writes.
    pub fn with_event_store(mut self, event_store: Arc<Store>) -> Self {
        self.event_store = Some(event_store);
        self
    }

    fn threads_root(&self) -> PathBuf {
        self.home
            .join("profiles")
            .join(&self.profile)
            .join("threads")
    }

    fn thread_dir(&self, tid: &str) -> Result<PathBuf, Error> {
        validate_thread_id(tid).map_err(Error::Validation)?;
        Ok(self.threads_root().join(tid).join("tasks"))
    }

    fn task_path(&self, tid: &str, task_id: &str) -> Result<PathBuf, Error> {
        validate_task_id(task_id).map_err(Error::Validation)?;
        Ok(self.thread_dir(tid)?.join(format!("{task_id}.toml")))
    }

    fn ensure_thread(
        &self,
        tid: &str,
    ) -> Result<(Arc<Mutex<Index>>, broadcast::Sender<TaskEvent>), Error> {
        let mut threads = self.threads.lock().expect("threads mutex poisoned");
        if let Some(s) = threads.get(tid) {
            return Ok((s.index.clone(), s.bus.clone()));
        }
        let dir = self.thread_dir(tid)?;
        fs::create_dir_all(&dir)?;
        let index = Arc::new(Mutex::new(Index::open(&dir.join("index.db"))?));
        self.rebuild_index_inner(tid, &dir, &index)?;
        let (tx, _) = broadcast::channel(BROADCAST_CAP);
        threads.insert(
            tid.to_string(),
            ThreadState {
                index: index.clone(),
                bus: tx.clone(),
            },
        );
        Ok((index, tx))
    }

    fn rebuild_index_inner(
        &self,
        _tid: &str,
        dir: &Path,
        index: &Arc<Mutex<Index>>,
    ) -> Result<(), Error> {
        let idx = index.lock().expect("index mutex poisoned");
        idx.clear()?;
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            match read_task_file(&path) {
                Ok(t) => {
                    if let Err(e) = idx.upsert(&t) {
                        tracing::warn!(?path, ?e, "failed to index task");
                    }
                }
                Err(e) => {
                    tracing::warn!(?path, ?e, "skipping invalid task TOML");
                }
            }
        }
        Ok(())
    }

    /// List tasks in a thread, optionally filtered.
    pub fn list(&self, tid: &str, filters: ListFilters) -> Result<Vec<Task>, Error> {
        let (index, _) = self.ensure_thread(tid)?;
        let ids = {
            let idx = index.lock().expect("index mutex poisoned");
            idx.list_ids(&filters)?
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(read_task_file(&self.task_path(tid, &id)?)?);
        }
        Ok(out)
    }

    /// Read a single task.
    pub fn get(&self, tid: &str, task_id: &str) -> Result<Task, Error> {
        self.ensure_thread(tid)?;
        let path = self.task_path(tid, task_id)?;
        if !path.exists() {
            return Err(Error::NotFound(task_id.into()));
        }
        read_task_file(&path)
    }

    /// Count active (non-terminal) tasks in a thread. Used for the per-thread
    /// cap and for the "anything to resume?" check at session spawn.
    pub fn count_active(&self, tid: &str) -> Result<usize, Error> {
        let tasks = self.list(tid, ListFilters::default())?;
        Ok(tasks.into_iter().filter(|t| is_active(t.status)).count())
    }

    /// Most-recently-updated active task in a thread, or `None` if the thread
    /// has no active tasks. Used to auto-pick "what should this session
    /// continue" at spawn time.
    pub fn latest_active(&self, tid: &str) -> Result<Option<Task>, Error> {
        let mut tasks: Vec<Task> = self
            .list(tid, ListFilters::default())?
            .into_iter()
            .filter(|t| is_active(t.status))
            .collect();
        tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at));
        Ok(tasks.into_iter().next())
    }

    /// Create a new task. Status starts as `queued`, or `blocked` if any
    /// `depends_on` is non-empty.
    ///
    /// Rejects with [`Error::LimitExceeded`] if the thread already has
    /// [`THREAD_ACTIVE_TASK_CAP`] active tasks.
    pub fn create(&self, tid: &str, draft: TaskDraft) -> Result<Task, Error> {
        let status = if draft.depends_on.is_empty() {
            TaskStatus::Queued
        } else {
            TaskStatus::Blocked
        };
        self.create_with_status(tid, draft, status)
    }

    /// Propose a new task. Proposed tasks are visible but not claimable or
    /// scheduled until a planner promotes them to `queued`.
    pub fn propose(&self, tid: &str, draft: TaskDraft) -> Result<Task, Error> {
        self.create_with_status(tid, draft, TaskStatus::Proposed)
    }

    fn create_with_status(
        &self,
        tid: &str,
        draft: TaskDraft,
        status: TaskStatus,
    ) -> Result<Task, Error> {
        let active = self.count_active(tid)?;
        if active >= THREAD_ACTIVE_TASK_CAP {
            return Err(Error::LimitExceeded(format!(
                "thread {tid} already has {active} active tasks (cap {THREAD_ACTIVE_TASK_CAP}); \
                 complete or abandon one before creating another"
            )));
        }
        let (index, _) = self.ensure_thread(tid)?;
        let dir = self.thread_dir(tid)?;
        let id = next_id(&dir)?;
        let now = Utc::now();
        let checks = draft
            .acceptance
            .into_iter()
            .enumerate()
            .map(|(i, mut c)| {
                if c.id.is_empty() {
                    c.id = format!("C{}", i + 1);
                }
                c
            })
            .collect();
        let task = Task {
            schema_version: 1,
            id: id.clone(),
            title: draft.title,
            status,
            created_at: now,
            created_by: draft.created_by.clone(),
            updated_at: now,
            updated_by: draft.created_by.clone(),
            parent: draft.parent,
            children: vec![],
            blocked_by: draft.depends_on,
            unblocks: vec![],
            assignee: None,
            claim_lease: None,
            previous_assignees: vec![],
            labels: draft.labels,
            spec_refs: draft.spec_refs,
            brief: draft.brief,
            acceptance: AcceptanceBlock { checks },
            artifacts: Artifacts::default(),
            notes: Notes::default(),
            history: HistoryBlock {
                events: vec![HistoryEvent {
                    at: now,
                    by: draft.created_by.clone(),
                    from: "*".into(),
                    to: status.as_str().into(),
                }],
            },
        };
        let path = self.task_path(tid, &id)?;
        write_task_atomic(&path, &task)?;
        {
            let idx = index.lock().expect("index mutex poisoned");
            idx.upsert(&task)?;
        }
        self.emit(
            tid,
            TaskEvent::Created {
                task_id: id.clone(),
                by: draft.created_by,
                at: now,
            },
        );
        Ok(task)
    }

    /// Apply a sparse patch to a task. Performs state-machine validation when
    /// `patch.status` is set.
    pub fn patch(
        &self,
        tid: &str,
        task_id: &str,
        patch: TaskPatch,
        by: &str,
    ) -> Result<Task, Error> {
        let (index, _) = self.ensure_thread(tid)?;
        let path = self.task_path(tid, task_id)?;
        let (task, (prev_status, changed_fields)) =
            with_locked_task(&path, |task| apply_patch(task, &patch, by))?;

        {
            let idx = index.lock().expect("index mutex poisoned");
            idx.upsert(&task)?;
        }
        if prev_status != task.status {
            self.emit(
                tid,
                TaskEvent::Changed {
                    task_id: task.id.clone(),
                    prev_status,
                    next_status: task.status,
                    by: by.into(),
                    at: task.updated_at,
                },
            );
        }
        if !changed_fields.is_empty() {
            self.emit(
                tid,
                TaskEvent::Updated {
                    task_id: task.id.clone(),
                    by: by.into(),
                    at: task.updated_at,
                    fields: changed_fields,
                },
            );
        }
        Ok(task)
    }

    /// Attempt to acquire the lease on a task.
    pub fn claim(
        &self,
        tid: &str,
        task_id: &str,
        agent_id: &str,
        ttl: Duration,
    ) -> Result<ClaimResult, Error> {
        let (index, _) = self.ensure_thread(tid)?;
        let path = self.task_path(tid, task_id)?;
        let now = Utc::now();
        let (_task, outcome) = with_locked_task(&path, |task| {
            if task.status != TaskStatus::Queued {
                return Err(Error::Validation(format!(
                    "only queued tasks can be claimed (current status: {})",
                    task.status.as_str()
                )));
            }
            if !task.blocked_by.is_empty() {
                return Err(Error::Validation(format!(
                    "queued task {} still has unresolved dependencies",
                    task.id
                )));
            }
            if let Some(l) = &task.claim_lease {
                if l.until > now && l.holder != agent_id {
                    return Ok::<_, Error>(ClaimResult::Busy {
                        holder: l.holder.clone(),
                        until: l.until,
                    });
                }
            }
            let until = now
                + chrono::Duration::from_std(ttl)
                    .unwrap_or_else(|_| chrono::Duration::seconds(300));
            let lease = Lease {
                holder: agent_id.to_string(),
                until,
            };
            task.claim_lease = Some(lease.clone());
            task.assignee = Some(agent_id.to_string());
            task.updated_at = now;
            task.updated_by = agent_id.into();
            Ok(ClaimResult::Granted(lease))
        })?;
        {
            let idx = index.lock().expect("index mutex poisoned");
            // Re-read to upsert with the persisted version
            let t = read_task_file(&path)?;
            idx.upsert(&t)?;
        }
        Ok(outcome)
    }

    /// Forcibly hand a task off to `new_agent`, pushing the prior assignee onto
    /// `previous_assignees` and stamping a history event. Status is preserved
    /// (used by the scheduler to route `pending_verify` to an evaluator without
    /// fighting the existing lease).
    pub fn reassign(
        &self,
        tid: &str,
        task_id: &str,
        new_agent: &str,
        ttl: Duration,
        note: &str,
    ) -> Result<Lease, Error> {
        let (index, _) = self.ensure_thread(tid)?;
        let path = self.task_path(tid, task_id)?;
        let now = Utc::now();
        let (_t, lease) = with_locked_task(&path, |task| {
            if let Some(prev) = task.assignee.take() {
                if prev != new_agent {
                    task.previous_assignees.push(prev);
                }
            }
            let until = now
                + chrono::Duration::from_std(ttl)
                    .unwrap_or_else(|_| chrono::Duration::seconds(300));
            let lease = Lease {
                holder: new_agent.to_string(),
                until,
            };
            task.claim_lease = Some(lease.clone());
            task.assignee = Some(new_agent.to_string());
            task.history.events.push(HistoryEvent {
                at: now,
                by: "scheduler".into(),
                from: task.status.as_str().into(),
                to: task.status.as_str().into(),
            });
            tracing::debug!(
                target: "scheduling",
                thread = %tid,
                task = %task_id,
                agent = %new_agent,
                note = %note,
                "scheduling.reassign"
            );
            task.updated_at = now;
            task.updated_by = "scheduler".into();
            Ok::<_, Error>(lease)
        })?;
        {
            let idx = index.lock().expect("index mutex poisoned");
            let t = read_task_file(&path)?;
            idx.upsert(&t)?;
        }
        Ok(lease)
    }

    /// Refresh the lease TTL. Errors if `agent_id` is not the current holder.
    pub fn renew(&self, tid: &str, task_id: &str, agent_id: &str) -> Result<Lease, Error> {
        let path = self.task_path(tid, task_id)?;
        let now = Utc::now();
        let (_, lease) = with_locked_task(&path, |task| {
            let cur = task
                .claim_lease
                .clone()
                .ok_or_else(|| Error::LeaseNotHeld(agent_id.into()))?;
            if cur.holder != agent_id {
                return Err(Error::LeaseNotHeld(agent_id.into()));
            }
            let new_until = now + chrono::Duration::seconds(300);
            let new_lease = Lease {
                holder: agent_id.into(),
                until: new_until,
            };
            task.claim_lease = Some(new_lease.clone());
            task.updated_at = now;
            task.updated_by = agent_id.into();
            Ok::<_, Error>(new_lease)
        })?;
        Ok(lease)
    }

    /// Release the lease (graceful).
    pub fn release(&self, tid: &str, task_id: &str, agent_id: &str) -> Result<(), Error> {
        let path = self.task_path(tid, task_id)?;
        with_locked_task(&path, |task| {
            if let Some(l) = &task.claim_lease {
                if l.holder != agent_id {
                    return Err(Error::LeaseNotHeld(agent_id.into()));
                }
            }
            task.claim_lease = None;
            task.updated_at = Utc::now();
            task.updated_by = agent_id.into();
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    /// Submit artifacts and transition `in_progress → pending_verify`.
    pub fn submit(
        &self,
        tid: &str,
        task_id: &str,
        mut artifacts: Artifacts,
        by: &str,
    ) -> Result<Task, Error> {
        let now = Utc::now();
        normalize_artifacts(task_id, &mut artifacts, by, now);
        let emitted_artifacts = artifacts.metadata.clone();
        let patch = TaskPatch {
            artifacts: Some(artifacts),
            status: Some(TaskStatus::PendingVerify),
            ..Default::default()
        };
        let task = self.patch(tid, task_id, patch, by)?;
        for artifact in emitted_artifacts {
            self.emit(
                tid,
                TaskEvent::ArtifactAdded {
                    thread_id: tid.to_string(),
                    artifact_id: artifact.artifact_id,
                    task_id: artifact.task_id,
                    path: artifact.path,
                    kind: artifact.kind.as_str().to_string(),
                    produced_by: artifact.produced_by,
                    summary: artifact.summary,
                    at: artifact.created_at,
                },
            );
        }
        Ok(task)
    }

    /// Return artifact metadata for a task. The task itself remains the
    /// compatibility snapshot; append-only `artifact.added` events are emitted
    /// when metadata is created.
    pub fn list_artifacts(&self, tid: &str, task_id: &str) -> Result<Vec<Artifact>, Error> {
        let task = self.get(tid, task_id)?;
        let mut artifacts = task.artifacts;
        normalize_artifacts(&task.id, &mut artifacts, &task.updated_by, task.updated_at);
        Ok(artifacts.metadata)
    }

    /// Transition the task to `abandoned` with a reason. Humans only.
    pub fn delete(&self, tid: &str, task_id: &str, why: String, by: &str) -> Result<(), Error> {
        let patch = TaskPatch {
            why_abandoned: Some(why),
            status: Some(TaskStatus::Abandoned),
            ..Default::default()
        };
        self.patch(tid, task_id, patch, by)?;
        Ok(())
    }

    /// Subscribe to events for `tid`. Returns immediately even if nobody has
    /// touched the thread yet (creates the broadcast bus on demand).
    pub fn subscribe(&self, tid: &str) -> broadcast::Receiver<TaskEvent> {
        match self.ensure_thread(tid) {
            Ok((_, tx)) => tx.subscribe(),
            Err(e) => {
                tracing::warn!(error = ?e, thread = %tid, "task subscribe rejected invalid thread");
                let (_tx, rx) = broadcast::channel(1);
                rx
            }
        }
    }

    /// Emit a task-domain event: persist it through the optional server sink,
    /// then broadcast the original [`TaskEvent`] unchanged for SSE consumers.
    pub fn emit(&self, tid: &str, event: TaskEvent) {
        let (_, tx) = match self.ensure_thread(tid) {
            Ok(thread) => thread,
            Err(e) => {
                tracing::warn!(error = %e, tid = %tid, "failed to persist task event");
                return;
            }
        };
        if let Some(store) = &self.event_store {
            match event.to_envelope(tid) {
                Ok(envelope) => {
                    if let Err(e) = store.append_event(tid, &envelope) {
                        tracing::warn!(error = %e, tid = %tid, "failed to persist task event");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, tid = %tid, "failed to persist task event");
                }
            }
        }
        let _ = tx.send(event);
    }

    /// Internal: list known threads (those with a `tasks/` dir).
    pub fn known_threads(&self) -> Result<Vec<String>, Error> {
        let root = self.threads_root();
        let mut out = vec![];
        if !root.exists() {
            return Ok(out);
        }
        for e in fs::read_dir(&root)? {
            let e = e?;
            if e.file_type()?.is_dir() && e.path().join("tasks").exists() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
        Ok(out)
    }

    /// Internal: mutate raw for scheduler tasks like lease expiration.
    pub fn with_locked<R>(
        &self,
        tid: &str,
        task_id: &str,
        f: impl FnOnce(&mut Task) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let path = self.task_path(tid, task_id)?;
        let (_, out) = with_locked_task(&path, |t| f(t))?;
        let (index, _) = self.ensure_thread(tid)?;
        let t = read_task_file(&path)?;
        let idx = index.lock().expect("index mutex poisoned");
        idx.upsert(&t)?;
        Ok(out)
    }
}

// ---------- helpers ----------

fn apply_patch(
    task: &mut Task,
    patch: &TaskPatch,
    by: &str,
) -> Result<(TaskStatus, Vec<String>), Error> {
    let prev = task.status;
    let mut fields: Vec<String> = vec![];

    if let Some(t) = &patch.title {
        task.title = t.clone();
        fields.push("title".into());
    }
    if let Some(a) = &patch.assignee {
        task.assignee = a.clone();
        fields.push("assignee".into());
    }
    if let Some(l) = &patch.labels {
        task.labels = l.clone();
        fields.push("labels".into());
    }
    if let Some(spec_refs) = &patch.spec_refs {
        task.spec_refs = spec_refs.clone();
        fields.push("spec_refs".into());
    }
    if let Some(b) = &patch.blocked_by {
        task.blocked_by = b.clone();
        fields.push("blocked_by".into());
    }
    if let Some(c) = &patch.acceptance_checks {
        task.acceptance.checks = c.clone();
        fields.push("acceptance".into());
    }
    if let Some(a) = &patch.artifacts {
        task.artifacts = a.clone();
        fields.push("artifacts".into());
    }
    if let Some(s) = &patch.why_paused {
        task.notes.why_paused = s.clone();
        fields.push("notes.why_paused".into());
    }
    if let Some(s) = &patch.why_abandoned {
        task.notes.why_abandoned = s.clone();
        fields.push("notes.why_abandoned".into());
    }
    if let Some(fb) = &patch.feedback {
        task.notes.feedback.push(fb.clone());
        fields.push("notes.feedback".into());
    }

    if let Some(next) = patch.status {
        if next != task.status {
            // For queued→in_progress, lease must be set first (caller should
            // call `claim` before patching to in_progress). We validate here.
            validate_transition(task, next, by)?;
            let now = Utc::now();
            task.history.events.push(HistoryEvent {
                at: now,
                by: by.into(),
                from: task.status.as_str().into(),
                to: next.as_str().into(),
            });
            task.status = next;
            fields.push("status".into());
            // Reset verify state on pending_verify→in_progress (rejection).
            if prev == TaskStatus::PendingVerify && next == TaskStatus::InProgress {
                for c in &mut task.acceptance.checks {
                    c.verified = false;
                    c.verified_by = None;
                }
            }
        }
    }

    task.updated_at = Utc::now();
    task.updated_by = by.into();

    Ok((prev, fields))
}

fn normalize_artifacts(
    task_id: &str,
    artifacts: &mut Artifacts,
    by: &str,
    created_at: chrono::DateTime<Utc>,
) {
    if !artifacts.metadata.is_empty() {
        for (idx, artifact) in artifacts.metadata.iter_mut().enumerate() {
            if artifact.artifact_id.trim().is_empty() {
                artifact.artifact_id = artifact_id(task_id, created_at, idx);
            }
            artifact.task_id = task_id.to_string();
            if artifact.produced_by.trim().is_empty() {
                artifact.produced_by = by.to_string();
            }
            if artifact.summary.trim().is_empty() {
                artifact.summary = summarize_artifact(&artifact.kind, &artifact.path);
            }
        }
    }

    append_legacy_artifact_metadata(task_id, artifacts, by, created_at);
}

fn append_legacy_artifact_metadata(
    task_id: &str,
    artifacts: &mut Artifacts,
    by: &str,
    created_at: chrono::DateTime<Utc>,
) {
    for path in &artifacts.files {
        let kind = classify_file_artifact(path);
        if has_artifact_metadata(artifacts, &kind, path) {
            continue;
        }
        let idx = artifacts.metadata.len();
        artifacts.metadata.push(Artifact {
            artifact_id: artifact_id(task_id, created_at, idx),
            task_id: task_id.to_string(),
            kind: kind.clone(),
            path: path.clone(),
            produced_by: by.to_string(),
            created_at,
            summary: summarize_artifact(&kind, path),
        });
    }
    for turn in &artifacts.turns {
        let path = format!("turn:{turn}");
        if has_artifact_metadata(artifacts, &ArtifactKind::Log, &path) {
            continue;
        }
        let idx = artifacts.metadata.len();
        artifacts.metadata.push(Artifact {
            artifact_id: artifact_id(task_id, created_at, idx),
            task_id: task_id.to_string(),
            kind: ArtifactKind::Log,
            path,
            produced_by: by.to_string(),
            created_at,
            summary: format!("Referenced turn {turn}"),
        });
    }
    if artifacts.diff.is_some() {
        if has_artifact_metadata(artifacts, &ArtifactKind::Diff, "diff") {
            return;
        }
        let idx = artifacts.metadata.len();
        artifacts.metadata.push(Artifact {
            artifact_id: artifact_id(task_id, created_at, idx),
            task_id: task_id.to_string(),
            kind: ArtifactKind::Diff,
            path: "diff".to_string(),
            produced_by: by.to_string(),
            created_at,
            summary: "Submitted diff".to_string(),
        });
    }
}

fn has_artifact_metadata(artifacts: &Artifacts, kind: &ArtifactKind, path: &str) -> bool {
    artifacts
        .metadata
        .iter()
        .any(|artifact| artifact.kind == *kind && artifact.path == path)
}

fn artifact_id(task_id: &str, created_at: chrono::DateTime<Utc>, idx: usize) -> String {
    format!("{task_id}-A{}-{}", created_at.timestamp_millis(), idx + 1)
}

fn classify_file_artifact(path: &str) -> ArtifactKind {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
    {
        ArtifactKind::Screenshot
    } else if lower.ends_with(".log") || lower.contains("test-output") {
        ArtifactKind::TestOutput
    } else {
        ArtifactKind::File
    }
}

fn summarize_artifact(kind: &ArtifactKind, path: &str) -> String {
    match kind {
        ArtifactKind::File => format!("Submitted file {path}"),
        ArtifactKind::Diff => "Submitted diff".to_string(),
        ArtifactKind::TestOutput => format!("Submitted test output {path}"),
        ArtifactKind::Screenshot => format!("Submitted screenshot {path}"),
        ArtifactKind::Log => format!("Submitted log {path}"),
    }
}

/// Acquire an exclusive flock, deserialize, mutate, atomically persist.
fn with_locked_task<R>(
    path: &Path,
    f: impl FnOnce(&mut Task) -> Result<R, Error>,
) -> Result<(Task, R), Error> {
    if !path.exists() {
        return Err(Error::NotFound(
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .into(),
        ));
    }
    let lock_path = path.with_extension("toml.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file.lock_exclusive()?;

    let res: Result<(Task, R), Error> = (|| {
        let mut task = read_task_file(path)?;
        let out = f(&mut task)?;
        write_task_atomic(path, &task)?;
        Ok((task, out))
    })();

    FileExt::unlock(&lock_file)?;
    res
}

fn read_task_file(path: &Path) -> Result<Task, Error> {
    let text = fs::read_to_string(path)?;
    let task: Task = toml_edit::de::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?;
    Ok(task)
}

fn write_task_atomic(path: &Path, task: &Task) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml_edit::ser::to_string_pretty(task).map_err(|e| Error::Toml(e.to_string()))?;
    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, TaskStore) {
        let dir = tempdir().unwrap();
        let s = TaskStore::new(dir.path()).unwrap();
        (dir, s)
    }

    #[test]
    fn create_then_get_and_list() {
        let (_dir, s) = store();
        let t = s
            .create(
                "thr-1",
                TaskDraft {
                    title: "first".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec!["x".into()],
                    spec_refs: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();
        assert_eq!(t.id, "T-0001");
        assert_eq!(t.status, TaskStatus::Queued);
        let got = s.get("thr-1", "T-0001").unwrap();
        assert_eq!(got.title, "first");
        let all = s.list("thr-1", ListFilters::default()).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn task_events_persist_as_envelopes_when_sink_is_attached() {
        let dir = tempdir().unwrap();
        let event_store = Arc::new(Store::new(dir.path()).unwrap());
        let thread = event_store.create_thread(Some("tasks".into())).unwrap();
        let s = TaskStore::new(dir.path())
            .unwrap()
            .with_event_store(event_store.clone());

        let task = s.create(&thread.id, mk_draft("first")).unwrap();
        s.patch(
            &thread.id,
            &task.id,
            TaskPatch {
                title: Some("renamed".into()),
                ..Default::default()
            },
            "planner",
        )
        .unwrap();

        let events = event_store.read_events(&thread.id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[0].event_type, "task.created");
        assert_eq!(events[0].thread_id.as_deref(), Some(thread.id.as_str()));
        assert_eq!(events[0].actor.as_deref(), Some("human"));
        assert_eq!(events[0].payload.as_ref().unwrap()["type"], "task.created");
        assert_eq!(events[0].payload.as_ref().unwrap()["task_id"], task.id);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[1].event_type, "task.updated");
        assert_eq!(events[1].actor.as_deref(), Some("planner"));
        assert_eq!(events[1].payload.as_ref().unwrap()["fields"][0], "title");
    }

    #[test]
    fn propose_creates_proposed_task() {
        let (_dir, s) = store();
        let t = s.propose("thr-1", mk_draft("proposal")).unwrap();
        assert_eq!(t.status, TaskStatus::Proposed);

        let err = s
            .claim("thr-1", &t.id, "agent:a", Duration::from_secs(60))
            .expect_err("proposed tasks are not claimable");
        assert!(matches!(err, Error::Validation(_)), "got {err:?}");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let (_dir, s) = store();

        let err = s.list("../escape", ListFilters::default()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));

        let err = s.get("thr-1", "../T-0001").unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn claim_then_renew_then_release() {
        let (_dir, s) = store();
        s.create(
            "thr-1",
            TaskDraft {
                title: "x".into(),
                parent: None,
                depends_on: vec![],
                brief: None,
                acceptance: vec![],
                labels: vec![],
                spec_refs: vec![],
                created_by: "human".into(),
            },
        )
        .unwrap();
        let r = s
            .claim("thr-1", "T-0001", "agent:a", Duration::from_secs(60))
            .unwrap();
        assert!(matches!(r, ClaimResult::Granted(_)));
        // Same holder re-claim is OK.
        let r2 = s
            .claim("thr-1", "T-0001", "agent:a", Duration::from_secs(60))
            .unwrap();
        assert!(matches!(r2, ClaimResult::Granted(_)));
        // Different holder busy.
        let r3 = s
            .claim("thr-1", "T-0001", "agent:b", Duration::from_secs(60))
            .unwrap();
        assert!(matches!(r3, ClaimResult::Busy { .. }));

        let _ = s.renew("thr-1", "T-0001", "agent:a").unwrap();
        assert!(s.renew("thr-1", "T-0001", "agent:b").is_err());

        s.release("thr-1", "T-0001", "agent:a").unwrap();
    }

    #[test]
    fn queued_with_blockers_is_not_claimable() {
        let (_dir, s) = store();
        s.create(
            "thr-1",
            TaskDraft {
                title: "blocked".into(),
                parent: None,
                depends_on: vec!["T-9999".into()],
                brief: None,
                acceptance: vec![],
                labels: vec![],
                spec_refs: vec![],
                created_by: "human".into(),
            },
        )
        .unwrap();
        s.patch(
            "thr-1",
            "T-0001",
            TaskPatch {
                status: Some(TaskStatus::Queued),
                ..Default::default()
            },
            "human",
        )
        .unwrap();
        let err = s
            .claim("thr-1", "T-0001", "agent:a", Duration::from_secs(60))
            .expect_err("blocked dependencies should prevent claim");
        assert!(matches!(err, Error::Validation(_)), "got {err:?}");
    }

    #[test]
    fn submit_materializes_artifact_metadata_and_events() {
        let (_dir, s) = store();
        s.create("thr-1", mk_draft("with artifacts")).unwrap();
        s.claim("thr-1", "T-0001", "agent:a", Duration::from_secs(60))
            .unwrap();
        s.patch(
            "thr-1",
            "T-0001",
            TaskPatch {
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            },
            "agent:a",
        )
        .unwrap();
        let mut rx = s.subscribe("thr-1");

        let task = s
            .submit(
                "thr-1",
                "T-0001",
                Artifacts {
                    files: vec!["src/lib.rs".into(), "screenshots/pass.png".into()],
                    turns: vec!["turn-7".into()],
                    diff: Some("--- diff".into()),
                    metadata: vec![],
                },
                "agent:a",
            )
            .unwrap();

        assert_eq!(task.status, TaskStatus::PendingVerify);
        assert_eq!(
            task.artifacts.files,
            vec!["src/lib.rs", "screenshots/pass.png"]
        );
        assert_eq!(task.artifacts.metadata.len(), 4);
        assert_eq!(task.artifacts.metadata[0].task_id, "T-0001");
        assert_eq!(task.artifacts.metadata[0].produced_by, "agent:a");
        assert_eq!(task.artifacts.metadata[1].kind, ArtifactKind::Screenshot);
        assert_eq!(task.artifacts.metadata[2].kind, ArtifactKind::Log);
        assert_eq!(task.artifacts.metadata[3].kind, ArtifactKind::Diff);

        let listed = s.list_artifacts("thr-1", "T-0001").unwrap();
        assert_eq!(listed, task.artifacts.metadata);

        let mut artifact_events = 0;
        while let Ok(ev) = rx.try_recv() {
            if let TaskEvent::ArtifactAdded {
                task_id,
                produced_by,
                ..
            } = ev
            {
                artifact_events += 1;
                assert_eq!(task_id, "T-0001");
                assert_eq!(produced_by, "agent:a");
            }
        }
        assert_eq!(artifact_events, 4);
    }

    #[test]
    fn list_artifacts_rejects_unknown_task() {
        let (_dir, s) = store();
        let err = s
            .list_artifacts("thr-1", "T-9999")
            .expect_err("unknown task should fail");
        assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn list_artifacts_synthesizes_legacy_snapshot() {
        let (_dir, s) = store();
        s.create("thr-1", mk_draft("legacy artifacts")).unwrap();
        s.patch(
            "thr-1",
            "T-0001",
            TaskPatch {
                artifacts: Some(Artifacts {
                    files: vec!["src/main.rs".into()],
                    turns: vec!["turn-3".into()],
                    diff: Some("diff".into()),
                    metadata: vec![],
                }),
                ..Default::default()
            },
            "human",
        )
        .unwrap();

        let artifacts = s.list_artifacts("thr-1", "T-0001").unwrap();
        assert_eq!(artifacts.len(), 3);
        assert_eq!(artifacts[0].kind, ArtifactKind::File);
        assert_eq!(artifacts[1].kind, ArtifactKind::Log);
        assert_eq!(artifacts[2].kind, ArtifactKind::Diff);
        assert_eq!(artifacts[0].produced_by, "human");
    }

    #[test]
    fn submit_hybrid_artifacts_emits_metadata_and_legacy_entries() {
        let (_dir, s) = store();
        s.create("thr-1", mk_draft("hybrid artifacts")).unwrap();
        s.claim("thr-1", "T-0001", "agent:a", Duration::from_secs(60))
            .unwrap();
        s.patch(
            "thr-1",
            "T-0001",
            TaskPatch {
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            },
            "agent:a",
        )
        .unwrap();
        let mut rx = s.subscribe("thr-1");

        let task = s
            .submit(
                "thr-1",
                "T-0001",
                Artifacts {
                    files: vec!["src/lib.rs".into()],
                    turns: vec!["turn-7".into()],
                    diff: Some("--- diff".into()),
                    metadata: vec![Artifact {
                        artifact_id: "custom-a".into(),
                        task_id: "wrong-task".into(),
                        kind: ArtifactKind::TestOutput,
                        path: "test-output.txt".into(),
                        produced_by: "".into(),
                        created_at: Utc::now(),
                        summary: "".into(),
                    }],
                },
                "agent:a",
            )
            .unwrap();

        assert_eq!(task.artifacts.metadata.len(), 4);
        assert_eq!(task.artifacts.metadata[0].artifact_id, "custom-a");
        assert_eq!(task.artifacts.metadata[0].task_id, "T-0001");
        assert_eq!(task.artifacts.metadata[0].produced_by, "agent:a");

        let mut artifact_events = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, TaskEvent::ArtifactAdded { .. }) {
                artifact_events += 1;
            }
        }
        assert_eq!(artifact_events, 4);
    }

    fn mk_draft(title: &str) -> TaskDraft {
        TaskDraft {
            title: title.into(),
            parent: None,
            depends_on: vec![],
            brief: None,
            acceptance: vec![],
            labels: vec![],
            spec_refs: vec![],
            created_by: "human".into(),
        }
    }

    #[test]
    fn create_rejects_when_thread_at_active_cap() {
        let (_dir, s) = store();
        for i in 1..=THREAD_ACTIVE_TASK_CAP {
            s.create("thr-1", mk_draft(&format!("t{i}"))).unwrap();
        }
        let err = s
            .create("thr-1", mk_draft("overflow"))
            .expect_err("cap should reject");
        assert!(matches!(err, Error::LimitExceeded(_)), "got {err:?}");

        // Finishing a task frees a slot.
        let patch = TaskPatch {
            status: Some(TaskStatus::Abandoned),
            why_abandoned: Some("test".into()),
            ..Default::default()
        };
        s.patch("thr-1", "T-0001", patch, "human").unwrap();
        s.create("thr-1", mk_draft("after-free")).unwrap();
    }

    #[test]
    fn latest_active_picks_most_recently_updated() {
        let (_dir, s) = store();
        s.create("thr-1", mk_draft("a")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        s.create("thr-1", mk_draft("b")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        // Touch T-0001 so it becomes the most recent.
        s.patch(
            "thr-1",
            "T-0001",
            TaskPatch {
                labels: Some(vec!["bump".into()]),
                ..Default::default()
            },
            "human",
        )
        .unwrap();
        let pick = s.latest_active("thr-1").unwrap().unwrap();
        assert_eq!(pick.id, "T-0001");
    }

    #[test]
    fn rebuild_index_from_disk() {
        let (dir, s) = store();
        s.create(
            "thr-1",
            TaskDraft {
                title: "x".into(),
                parent: None,
                depends_on: vec![],
                brief: None,
                acceptance: vec![],
                labels: vec![],
                spec_refs: vec![],
                created_by: "human".into(),
            },
        )
        .unwrap();
        // New store on the same home reads from disk.
        let s2 = TaskStore::new(dir.path()).unwrap();
        let all = s2.list("thr-1", ListFilters::default()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "T-0001");
    }
}
