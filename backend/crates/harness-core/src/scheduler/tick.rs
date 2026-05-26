use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::tasks::{ListFilters, TaskEvent, TaskStatus, TaskStore};

/// Background scheduler loop. Spawn once at boot; the [`JoinHandle`] keeps it
/// alive (drop to stop — uses an internal `CancellationToken` style flag).
pub struct Scheduler {
    handle: Option<JoinHandle<()>>,
    stop: Arc<Mutex<bool>>,
}

impl Scheduler {
    pub fn spawn(store: TaskStore) -> Self {
        let stop = Arc::new(Mutex::new(false));
        let stop2 = stop.clone();
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
