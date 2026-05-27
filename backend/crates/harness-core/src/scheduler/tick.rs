use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::agents::{Agent, AgentsRegistry};
use crate::pause::PauseFlag;
use crate::tasks::{ClaimResult, ListFilters, TaskEvent, TaskStatus, TaskStore};

use super::MAX_CONCURRENT_DEFAULT;

/// Generators that recently failed an evaluator check are temporarily skipped
/// when the same `(thread, task)` re-enters the queue.
pub const COOLDOWN_AFTER_VERIFY_FAIL: Duration = Duration::from_secs(60);

/// Effective role of an agent (with the legacy-default applied).
fn agent_role(a: &Agent) -> &str {
    a.role.as_deref().unwrap_or("generator")
}

/// Cooldown key — a specific generator on a specific task in a specific thread.
type CooldownKey = (String, String, String);
/// `(thread_id, task_id) -> last_observed_status`.
type StatusSnapshot = HashMap<(String, String), TaskStatus>;

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
            let mut prev_status: StatusSnapshot = HashMap::new();
            let cooldown: Arc<Mutex<HashMap<CooldownKey, Instant>>> =
                Arc::new(Mutex::new(HashMap::new()));
            loop {
                if *stop2.lock().expect("stop mutex") {
                    break;
                }
                tokio::select! {
                    _ = tick.tick() => {
                        if let Err(e) = run_ready_pass(&store, &mut announced) {
                            tracing::warn!(?e, "scheduler ready pass failed");
                        }
                        if let Err(e) = run_assign_pass(
                            &store,
                            &agents,
                            &pause,
                            max_concurrent,
                            &mut prev_status,
                            &cooldown,
                        ) {
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

/// Pick the first idle generator that is not under cooldown for `(tid,task_id)`.
///
/// Pure helper so it can be unit-tested without touching the store. `now` is
/// injected for deterministic cooldown expiry.
pub(crate) fn pick_idle_generator<'a>(
    agents: &'a [Agent],
    busy: &HashSet<String>,
    cooldown: &HashMap<CooldownKey, Instant>,
    tid: &str,
    task_id: &str,
    now: Instant,
) -> Option<&'a Agent> {
    agents.iter().find(|a| {
        if agent_role(a) != "generator" {
            return false;
        }
        if busy.contains(&a.id) {
            return false;
        }
        let key = (tid.to_string(), task_id.to_string(), a.id.clone());
        !matches!(cooldown.get(&key), Some(until) if *until > now)
    })
}

/// Auto-claim eligible queued tasks for idle generators and route
/// `pending_verify` tasks to an evaluator. Also tracks verify-fail cooldowns.
fn run_assign_pass(
    store: &TaskStore,
    agents: &AgentsRegistry,
    pause: &PauseFlag,
    max_concurrent: usize,
    prev_status: &mut StatusSnapshot,
    cooldown: &Mutex<HashMap<CooldownKey, Instant>>,
) -> Result<(), crate::Error> {
    if pause.is_paused() {
        return Ok(());
    }

    let all_agents = agents.list();
    let total_agents = all_agents.len();

    // Opportunistically drop expired cooldown entries.
    let now_i = Instant::now();
    {
        let mut cd = cooldown.lock().expect("cooldown mutex");
        cd.retain(|_, until| *until > now_i);
    }

    let mut next_snapshot: StatusSnapshot = HashMap::new();

    for tid in store.known_threads()? {
        let tasks = store.list(&tid, ListFilters::default())?;

        // Detect verify-fail transitions (pending_verify -> in_progress) and
        // record cooldown for the generator that just got rejected. The
        // generator is the most-recent entry in `previous_assignees` (pushed
        // by `reassign` when we routed to the evaluator).
        for t in &tasks {
            next_snapshot.insert((tid.clone(), t.id.clone()), t.status);
            let prev = prev_status.get(&(tid.clone(), t.id.clone())).copied();
            if prev == Some(TaskStatus::PendingVerify) && t.status == TaskStatus::InProgress {
                if let Some(gen_id) = t.previous_assignees.last() {
                    let key = (tid.clone(), t.id.clone(), gen_id.clone());
                    let until = Instant::now() + COOLDOWN_AFTER_VERIFY_FAIL;
                    cooldown.lock().expect("cooldown mutex").insert(key, until);
                    tracing::info!(
                        target: "scheduling",
                        thread = %tid,
                        task = %t.id,
                        agent = %gen_id,
                        "scheduling.cooldown"
                    );
                }
            }
        }

        let in_progress: Vec<&_> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .collect();
        let mut busy_ids: HashSet<String> = in_progress
            .iter()
            .filter_map(|t| t.assignee.clone())
            .collect();
        // Evaluators currently working a pending_verify are also busy.
        for t in &tasks {
            if t.status == TaskStatus::PendingVerify {
                if let Some(a) = &t.assignee {
                    // Only treat as busy if the assignee is in fact an evaluator;
                    // a generator that just submitted will be overwritten by the
                    // evaluator-routing pass below.
                    if all_agents
                        .iter()
                        .find(|ag| &ag.id == a)
                        .map(|ag| agent_role(ag) == "evaluator")
                        .unwrap_or(false)
                    {
                        busy_ids.insert(a.clone());
                    }
                }
            }
        }

        let queued_unblocked: Vec<&_> = tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Queued
                    && t.blocked_by.is_empty()
                    && t.assignee.is_none()
            })
            .collect();
        let pending_verify: Vec<&_> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::PendingVerify)
            .collect();

        let idle_agents = total_agents.saturating_sub(busy_ids.len());
        tracing::debug!(
            target: "scheduling",
            thread = %tid,
            queued = queued_unblocked.len(),
            in_progress = in_progress.len(),
            pending_verify = pending_verify.len(),
            idle_agents,
            "scheduling.tick"
        );

        // --- pass A: queued -> generator ---
        let mut current = busy_ids.len();
        for t in queued_unblocked {
            if current >= max_concurrent {
                break;
            }
            let cd_snapshot = cooldown.lock().expect("cooldown mutex").clone();
            let Some(agent) = pick_idle_generator(
                &all_agents,
                &busy_ids,
                &cd_snapshot,
                &tid,
                &t.id,
                Instant::now(),
            ) else {
                continue;
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
                    busy_ids.insert(holder);
                }
                Err(e) => {
                    tracing::warn!(?e, task = %t.id, "scheduler claim failed");
                }
            }
        }

        // --- pass B: pending_verify -> evaluator (different from current
        // holder). We use `reassign` because the generator's lease is still
        // active when the task transitions to pending_verify.
        for t in pending_verify {
            // Already routed to an evaluator? Skip.
            if let Some(a) = &t.assignee {
                if all_agents
                    .iter()
                    .find(|ag| &ag.id == a)
                    .map(|ag| agent_role(ag) == "evaluator")
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            let prior = t.assignee.clone();
            let evaluator = all_agents.iter().find(|a| {
                agent_role(a) == "evaluator"
                    && !busy_ids.contains(&a.id)
                    && prior.as_deref() != Some(a.id.as_str())
            });
            let Some(evaluator) = evaluator else {
                continue;
            };
            match store.reassign(
                &tid,
                &t.id,
                &evaluator.id,
                Duration::from_secs(300),
                "reassigned to evaluator",
            ) {
                Ok(_) => {
                    tracing::info!(
                        target: "scheduling",
                        thread = %tid,
                        task = %t.id,
                        agent = %evaluator.id,
                        "scheduling.route_evaluator"
                    );
                    busy_ids.insert(evaluator.id.clone());
                }
                Err(e) => {
                    tracing::warn!(?e, task = %t.id, "scheduler evaluator reassign failed");
                }
            }
        }
    }

    *prev_status = next_snapshot;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{Agent, AgentKind};
    use chrono::Utc;

    fn mk_agent(id: &str, role: Option<&str>) -> Agent {
        Agent {
            id: id.into(),
            kind: AgentKind::Claude,
            label: id.into(),
            created_at: Utc::now(),
            role: role.map(|s| s.into()),
        }
    }

    #[test]
    fn pick_skips_planner_and_evaluator() {
        let agents = vec![
            mk_agent("agent:p", Some("planner")),
            mk_agent("agent:g", Some("generator")),
            mk_agent("agent:e", Some("evaluator")),
        ];
        let busy = HashSet::new();
        let cd = HashMap::new();
        let pick = pick_idle_generator(&agents, &busy, &cd, "thr", "T-1", Instant::now());
        assert_eq!(pick.map(|a| a.id.as_str()), Some("agent:g"));
    }

    #[test]
    fn pick_treats_missing_role_as_generator() {
        let agents = vec![mk_agent("agent:legacy", None)];
        let busy = HashSet::new();
        let cd = HashMap::new();
        let pick = pick_idle_generator(&agents, &busy, &cd, "thr", "T-1", Instant::now());
        assert_eq!(pick.map(|a| a.id.as_str()), Some("agent:legacy"));
    }

    #[test]
    fn pick_skips_generator_on_cooldown_but_uses_another() {
        let agents = vec![
            mk_agent("agent:g1", Some("generator")),
            mk_agent("agent:g2", Some("generator")),
        ];
        let busy = HashSet::new();
        let now = Instant::now();
        let mut cd = HashMap::new();
        cd.insert(
            ("thr".into(), "T-1".into(), "agent:g1".into()),
            now + Duration::from_secs(60),
        );
        let pick = pick_idle_generator(&agents, &busy, &cd, "thr", "T-1", now);
        assert_eq!(pick.map(|a| a.id.as_str()), Some("agent:g2"));

        // Cooldown only applies to (thr,T-1,g1); another task is fine.
        let pick2 = pick_idle_generator(&agents, &busy, &cd, "thr", "T-2", now);
        assert_eq!(pick2.map(|a| a.id.as_str()), Some("agent:g1"));

        // Expired cooldown is ignored.
        let later = now + Duration::from_secs(120);
        let pick3 = pick_idle_generator(&agents, &busy, &cd, "thr", "T-1", later);
        assert_eq!(pick3.map(|a| a.id.as_str()), Some("agent:g1"));
    }

    #[test]
    fn assign_pass_routes_queued_to_generator_only() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let planner = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "p".into(),
                role: Some("planner".into()),
            })
            .unwrap();
        let gen = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g".into(),
                role: Some("generator".into()),
            })
            .unwrap();

        store
            .create(
                "thr-1",
                TaskDraft {
                    title: "t".into(),
                    parent: None,
                    depends_on: vec![],
                    acceptance: vec![],
                    labels: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd).unwrap();

        let t = store.get("thr-1", "T-0001").unwrap();
        assert_eq!(t.assignee.as_deref(), Some(gen.id.as_str()));
        assert_ne!(t.assignee.as_deref(), Some(planner.id.as_str()));
    }

    #[test]
    fn verify_fail_cooldown_routes_to_different_generator() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{Artifacts, TaskDraft, TaskPatch, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let g1 = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g1".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        let g2 = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g2".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        let e1 = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "e1".into(),
                role: Some("evaluator".into()),
            })
            .unwrap();

        store
            .create(
                "thr",
                TaskDraft {
                    title: "t".into(),
                    parent: None,
                    depends_on: vec![],
                    acceptance: vec![],
                    labels: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());

        // Tick 1: assign queued -> g1 (lowest id picked first).
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd).unwrap();
        let t = store.get("thr", "T-0001").unwrap();
        let first_gen = t.assignee.clone().unwrap();
        assert!(first_gen == g1.id || first_gen == g2.id);

        // First generator transitions queued -> in_progress -> pending_verify.
        store
            .patch(
                "thr",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
                &first_gen,
            )
            .unwrap();
        store
            .patch(
                "thr",
                "T-0001",
                TaskPatch {
                    artifacts: Some(Artifacts {
                        files: vec!["out.txt".into()],
                        ..Default::default()
                    }),
                    status: Some(TaskStatus::PendingVerify),
                    ..Default::default()
                },
                &first_gen,
            )
            .unwrap();

        // Tick 2: should route pending_verify -> evaluator e1, push first_gen
        // onto previous_assignees.
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd).unwrap();
        let t = store.get("thr", "T-0001").unwrap();
        assert_eq!(t.assignee.as_deref(), Some(e1.id.as_str()));
        assert_eq!(t.previous_assignees.last().map(|s| s.as_str()), Some(first_gen.as_str()));

        // Evaluator rejects: pending_verify -> in_progress.
        store
            .patch(
                "thr",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
                &e1.id,
            )
            .unwrap();

        // Tick 3: must observe the verify-fail transition and record cooldown
        // for first_gen on this task.
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd).unwrap();
        let key = ("thr".to_string(), "T-0001".to_string(), first_gen.clone());
        assert!(cd.lock().unwrap().contains_key(&key), "cooldown not recorded");

        // Simulate the human/scheduler releasing the task and re-queueing it so
        // the assignment pass picks the OTHER generator.
        store
            .with_locked("thr", "T-0001", |task| {
                task.assignee = None;
                task.claim_lease = None;
                task.status = TaskStatus::Queued;
                Ok(())
            })
            .unwrap();

        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd).unwrap();
        let t = store.get("thr", "T-0001").unwrap();
        let second_gen = t.assignee.clone().unwrap();
        let other = if first_gen == g1.id { g2.id.clone() } else { g1.id.clone() };
        assert_eq!(second_gen, other, "cooldown should have forced the other generator");
    }

    #[test]
    fn pick_skips_busy() {
        let agents = vec![
            mk_agent("agent:g1", Some("generator")),
            mk_agent("agent:g2", Some("generator")),
        ];
        let mut busy = HashSet::new();
        busy.insert("agent:g1".into());
        let cd = HashMap::new();
        let pick = pick_idle_generator(&agents, &busy, &cd, "thr", "T-1", Instant::now());
        assert_eq!(pick.map(|a| a.id.as_str()), Some("agent:g2"));
    }
}
