use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::agents::AgentsRegistry;
use crate::pause::PauseFlag;
use crate::tasks::{ClaimResult, ListFilters, TaskEvent, TaskStatus, TaskStore};

use super::MAX_CONCURRENT_DEFAULT;

/// Background scheduler loop. Spawn once at boot; the [`JoinHandle`] keeps it
/// alive (drop to stop — uses an internal `CancellationToken` style flag).
pub struct Scheduler {
    handle: Option<JoinHandle<()>>,
    stop: Arc<Mutex<bool>>,
}

impl Scheduler {
    /// Spawn the scheduler loop.
    ///
    /// `max_concurrent` caps in-progress tasks per thread; defaults to
    /// [`MAX_CONCURRENT_DEFAULT`] when `None`.
    pub fn spawn(
        store: TaskStore,
        agents: Arc<AgentsRegistry>,
        pause: Arc<PauseFlag>,
        max_concurrent: Option<usize>,
    ) -> Self {
        let stop = Arc::new(Mutex::new(false));
        let stop2 = stop.clone();
        let max_concurrent = max_concurrent.unwrap_or(MAX_CONCURRENT_DEFAULT);
        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            let mut lease_tick = tokio::time::interval(Duration::from_secs(30));
            // Track tasks we have already announced as ready to avoid spam.
            let mut announced: HashSet<(String, String)> = HashSet::new();
            loop {
                if *stop2.lock().expect("stop mutex") {
                    break;
                }
                tokio::select! {
                    _ = tick.tick() => {
                        if let Err(e) = run_ready_pass(&store, &mut announced) {
                            tracing::warn!(?e, "scheduler ready pass failed");
                        }
                        if let Err(e) = run_assign_pass(&store, &agents, &pause, max_concurrent) {
                            tracing::warn!(?e, "scheduler assign pass failed");
                        }
                    }
                    _ = lease_tick.tick() => {
                        if let Err(e) = run_lease_pass(&store) {
                            tracing::warn!(?e, "scheduler lease pass failed");
                        }
                    }
                }
            }
        });
        Self {
            handle: Some(handle),
            stop,
        }
    }

    pub fn stop(mut self) {
        *self.stop.lock().expect("stop mutex") = true;
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        *self.stop.lock().expect("stop mutex") = true;
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

fn run_ready_pass(
    store: &TaskStore,
    announced: &mut HashSet<(String, String)>,
) -> Result<(), crate::Error> {
    for tid in store.known_threads()? {
        let tasks = store.list(&tid, ListFilters::default())?;
        // Build a status lookup.
        let status_of: std::collections::HashMap<&str, TaskStatus> =
            tasks.iter().map(|t| (t.id.as_str(), t.status)).collect();
        let sender = store.sender(&tid)?;
        for t in &tasks {
            match t.status {
                TaskStatus::Queued if t.blocked_by.is_empty() => {
                    let key = (tid.clone(), t.id.clone());
                    if announced.insert(key) {
                        let _ = sender.send(TaskEvent::Ready {
                            task_id: t.id.clone(),
                        });
                    }
                }
                TaskStatus::Blocked => {
                    let all_done = t
                        .blocked_by
                        .iter()
                        .all(|d| status_of.get(d.as_str()).copied() == Some(TaskStatus::Done));
                    if all_done {
                        store.with_locked(&tid, &t.id, |task| {
                            let prev = task.status;
                            task.status = TaskStatus::Queued;
                            task.history.events.push(crate::tasks::HistoryEvent {
                                at: Utc::now(),
                                by: "scheduler".into(),
                                from: prev.as_str().into(),
                                to: TaskStatus::Queued.as_str().into(),
                            });
                            task.updated_at = Utc::now();
                            task.updated_by = "scheduler".into();
                            Ok(())
                        })?;
                        let _ = sender.send(TaskEvent::Changed {
                            task_id: t.id.clone(),
                            prev_status: TaskStatus::Blocked,
                            next_status: TaskStatus::Queued,
                            by: "scheduler".into(),
                            at: Utc::now(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Auto-claim eligible queued tasks for idle generators.
///
/// [F3-followup] Role-based agent typing not yet modeled — for this slice any
/// registered agent is treated as a generator. When role typing lands, filter
/// the candidate set here.
fn run_assign_pass(
    store: &TaskStore,
    agents: &AgentsRegistry,
    pause: &PauseFlag,
    max_concurrent: usize,
) -> Result<(), crate::Error> {
    if pause.is_paused() {
        return Ok(());
    }

    let all_agents = agents.list();
    let total_agents = all_agents.len();

    for tid in store.known_threads()? {
        let tasks = store.list(&tid, ListFilters::default())?;
        let in_progress: Vec<&_> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .collect();
        let mut busy_ids: HashSet<String> = in_progress
            .iter()
            .filter_map(|t| t.assignee.clone())
            .collect();

        let queued_unblocked: Vec<&_> = tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Queued
                    && t.blocked_by.is_empty()
                    && t.assignee.is_none()
            })
            .collect();

        let idle_agents = total_agents.saturating_sub(busy_ids.len());
        tracing::debug!(
            target: "scheduling",
            thread = %tid,
            queued = queued_unblocked.len(),
            in_progress = in_progress.len(),
            idle_agents,
            "scheduling.tick"
        );

        let mut current = busy_ids.len();
        for t in queued_unblocked {
            if current >= max_concurrent {
                break;
            }
            // Pick any agent not currently busy in this thread.
            let candidate = all_agents.iter().find(|a| !busy_ids.contains(&a.id));
            let Some(agent) = candidate else {
                break;
            };

            match store.claim(&tid, &t.id, &agent.id, Duration::from_secs(300)) {
                Ok(ClaimResult::Granted(_)) => {
                    tracing::info!(
                        target: "scheduling",
                        thread = %tid,
                        task = %t.id,
                        agent = %agent.id,
                        "scheduling.assign"
                    );
                    busy_ids.insert(agent.id.clone());
                    current += 1;
                }
                Ok(ClaimResult::Busy { holder, .. }) => {
                    // Someone else (likely a human via API) beat us to it; treat
                    // that holder as busy and continue.
                    busy_ids.insert(holder);
                }
                Err(e) => {
                    tracing::warn!(?e, task = %t.id, "scheduler claim failed");
                }
            }
        }
    }
    Ok(())
}

fn run_lease_pass(store: &TaskStore) -> Result<(), crate::Error> {
    let now = Utc::now();
    for tid in store.known_threads()? {
        let tasks = store.list(&tid, ListFilters::default())?;
        let sender = store.sender(&tid)?;
        for t in tasks {
            let expired = t
                .claim_lease
                .as_ref()
                .map(|l| l.until <= now)
                .unwrap_or(false);
            if !expired {
                continue;
            }
            let prev_holder = t
                .claim_lease
                .as_ref()
                .map(|l| l.holder.clone())
                .unwrap_or_default();
            store.with_locked(&tid, &t.id, |task| {
                if let Some(a) = task.assignee.take() {
                    task.previous_assignees.push(a);
                }
                task.claim_lease = None;
                task.history.events.push(crate::tasks::HistoryEvent {
                    at: now,
                    by: "scheduler".into(),
                    from: task.status.as_str().into(),
                    to: task.status.as_str().into(),
                });
                task.updated_at = now;
                task.updated_by = "scheduler".into();
                Ok(())
            })?;
            let _ = sender.send(TaskEvent::LeaseExpired {
                task_id: t.id,
                previous_holder: prev_holder,
            });
        }
    }
    Ok(())
}
