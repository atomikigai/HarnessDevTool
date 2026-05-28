use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::agents::{Agent, AgentsRegistry};
use crate::budget::{
    ActiveSessionsSource, AgentCost, BudgetStore, BudgetWarning, BudgetWarningSink, CostReporter,
};
use crate::pause::PauseFlag;
use crate::tasks::{ClaimResult, ListFilters, TaskEvent, TaskStatus, TaskStore};

use super::spawner::{NoopSpawner, SessionSpawner, SpawnRequest, SpawnResult};
use super::MAX_CONCURRENT_DEFAULT;

/// Threshold bands used by [`run_budget_pass`]. We only re-emit
/// `budget.warning` when the spend crosses a *higher* band than the last one
/// we reported for the thread, so SSE consumers don't see warning spam on
/// every 2-second tick.
const WARNING_BANDS: &[u8] = &[75, 90, 100];

fn band_for(pct: u8) -> u8 {
    let mut chosen = 0u8;
    for &b in WARNING_BANDS {
        if pct >= b {
            chosen = b;
        }
    }
    chosen
}

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

/// Optional plumbing for the budget pass. When `Some`, the scheduler tick
/// loop polls each active session's `CostReporter`, aggregates per-thread
/// spend into [`BudgetStore`], and emits warnings / triggers the pause flag
/// when thresholds are crossed.
///
/// Reporters are keyed by `AgentKind::as_str()` so this crate stays free of a
/// dependency on `harness-session`.
pub struct BudgetWiring {
    pub store: BudgetStore,
    pub reporters: HashMap<String, Arc<dyn CostReporter>>,
    pub sessions: Arc<dyn ActiveSessionsSource>,
    pub sink: Arc<dyn BudgetWarningSink>,
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
        Self::spawn_with_budget(store, agents, pause, max_concurrent, None)
    }

    /// Same as [`spawn`](Self::spawn) but also runs [`run_budget_pass`] on
    /// every tick when `budget` is `Some`.
    pub fn spawn_with_budget(
        store: TaskStore,
        agents: Arc<AgentsRegistry>,
        pause: Arc<PauseFlag>,
        max_concurrent: Option<usize>,
        budget: Option<BudgetWiring>,
    ) -> Self {
        Self::spawn_full(
            store,
            agents,
            pause,
            max_concurrent,
            budget,
            Arc::new(NoopSpawner) as Arc<dyn SessionSpawner>,
        )
    }

    /// Full constructor taking a [`SessionSpawner`]. When the scheduler claims
    /// or reassigns a task it asks the spawner to materialize the agent as a
    /// live PTY session (no-op if one already exists for the `(thread,agent)`
    /// pair). The default [`spawn_with_budget`] wires a [`NoopSpawner`] which
    /// is fine for non-binary contexts (tests, library use).
    pub fn spawn_full(
        store: TaskStore,
        agents: Arc<AgentsRegistry>,
        pause: Arc<PauseFlag>,
        max_concurrent: Option<usize>,
        budget: Option<BudgetWiring>,
        spawner: Arc<dyn SessionSpawner>,
    ) -> Self {
        let stop = Arc::new(Mutex::new(false));
        let stop2 = stop.clone();
        let max_concurrent = max_concurrent.unwrap_or(MAX_CONCURRENT_DEFAULT);
        let pause_for_budget = pause.clone();
        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            let mut lease_tick = tokio::time::interval(Duration::from_secs(30));
            // Track tasks we have already announced as ready to avoid spam.
            let mut announced: HashSet<(String, String)> = HashSet::new();
            let mut prev_status: StatusSnapshot = HashMap::new();
            let cooldown: Arc<Mutex<HashMap<CooldownKey, Instant>>> =
                Arc::new(Mutex::new(HashMap::new()));
            // Per-thread "last warning band" so we only emit on threshold
            // *crossings* (75/90/100), not every tick.
            let mut last_warned_band: HashMap<String, u8> = HashMap::new();
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
                            spawner.as_ref(),
                        ) {
                            tracing::warn!(?e, "scheduler assign pass failed");
                        }
                        if let Some(b) = &budget {
                            if let Err(e) = run_budget_pass(
                                b,
                                &pause_for_budget,
                                &mut last_warned_band,
                            ) {
                                tracing::warn!(?e, "scheduler budget pass failed");
                            }
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

/// Ask the [`SessionSpawner`] to materialize `agent` as a live PTY session
/// attached to `tid`. The spawner is expected to de-dupe — re-issuing the
/// request for an agent that already has a live session is cheap and turns
/// into [`SpawnResult::AlreadyRunning`]. We log structured results so the
/// "claimed but no PTY" failure mode is obvious in the scheduler logs.
fn request_spawn(spawner: &dyn SessionSpawner, tid: &str, agent: &Agent) {
    let req = SpawnRequest {
        agent_id: agent.id.clone(),
        role: agent_role(agent).to_string(),
        kind: agent.kind.as_str().to_string(),
        thread_id: tid.to_string(),
        cwd: None,
    };
    match spawner.spawn(req) {
        SpawnResult::Launched { session_id } => {
            tracing::info!(
                target: "scheduling",
                thread = %tid,
                agent = %agent.id,
                role = %agent_role(agent),
                session = %session_id,
                "scheduling.spawn_launched"
            );
        }
        SpawnResult::AlreadyRunning { session_id } => {
            tracing::debug!(
                target: "scheduling",
                thread = %tid,
                agent = %agent.id,
                session = %session_id,
                "scheduling.spawn_already_running"
            );
        }
        SpawnResult::Failed(why) => {
            // CRITICAL — without this log line the "claimed but no PTY" bug
            // is invisible. Keep at warn so it shows up in default logs.
            tracing::warn!(
                target: "scheduling",
                thread = %tid,
                agent = %agent.id,
                role = %agent_role(agent),
                why = %why,
                "scheduling.spawn_failed"
            );
        }
    }
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
    spawner: &dyn SessionSpawner,
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
                t.status == TaskStatus::Queued && t.blocked_by.is_empty() && t.assignee.is_none()
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
                    request_spawn(spawner, &tid, agent);
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
                    request_spawn(spawner, &tid, evaluator);
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

/// One pass of the budget reconciler: for every active session, poll the
/// kind-specific [`CostReporter`], aggregate per-thread spend, persist via
/// [`BudgetStore::set_spent`], and emit warnings / trip the pause flag when a
/// new threshold band is crossed.
///
/// `last_warned_band` is keyed by `thread_id` and remembers the highest band
/// (`75 / 90 / 100`) already reported so we don't emit on every tick.
pub fn run_budget_pass(
    wiring: &BudgetWiring,
    pause: &PauseFlag,
    last_warned_band: &mut HashMap<String, u8>,
) -> Result<(), crate::Error> {
    let sessions = wiring.sessions.snapshot();
    // Per-thread aggregation: sum of per-session cost_usd reported this tick.
    let mut per_thread: HashMap<String, f64> = HashMap::new();
    let mut per_agent: HashMap<(String, String), AgentCost> = HashMap::new();
    for s in &sessions {
        let Some(reporter) = wiring.reporters.get(&s.kind) else {
            continue;
        };
        match reporter.poll(&s.session_id, &s.cwd) {
            Ok(cost) => {
                *per_thread.entry(s.thread_id.clone()).or_insert(0.0) += cost.cost_usd;
                let agent_id = s.agent_id.clone().unwrap_or_else(|| "unknown".into());
                let role = s.role.clone().unwrap_or_else(|| "generator".into());
                let entry = per_agent
                    .entry((s.thread_id.clone(), agent_id.clone()))
                    .or_insert_with(|| AgentCost {
                        agent_id,
                        role,
                        sessions: 0,
                        spent_usd: 0.0,
                    });
                entry.sessions += 1;
                entry.spent_usd += cost.cost_usd;
            }
            Err(e) => {
                tracing::warn!(
                    target: "budget",
                    error = %e,
                    session = %s.session_id,
                    thread = %s.thread_id,
                    "cost reporter poll failed"
                );
            }
        }
    }

    for (thread_id, spent_usd) in per_thread {
        let budget = match wiring.store.set_spent(&thread_id, spent_usd) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(?e, thread = %thread_id, "budget persist failed");
                continue;
            }
        };
        let mut agents = per_agent
            .iter()
            .filter(|((tid, _), _)| tid == &thread_id)
            .map(|(_, agent)| agent.clone())
            .collect::<Vec<_>>();
        agents.sort_by(|a, b| {
            b.spent_usd
                .partial_cmp(&a.spent_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });
        wiring.store.set_agents_breakdown(&thread_id, agents);

        // No limit set => nothing to warn or trip on.
        if budget.limit_usd <= 0.0 {
            continue;
        }

        let pct = budget.pct_spent();
        let band = band_for(pct);
        let last = last_warned_band.get(&thread_id).copied().unwrap_or(0);

        if band > last {
            last_warned_band.insert(thread_id.clone(), band);
            wiring.sink.emit(BudgetWarning {
                thread_id: thread_id.clone(),
                spent_usd: budget.spent_usd,
                limit_usd: budget.limit_usd,
                pct,
            });
            tracing::info!(
                target: "budget",
                thread = %thread_id,
                pct,
                "budget.warning"
            );
        }

        if budget.over_hard() && !pause.is_paused() {
            if let Err(e) = pause.set(true) {
                tracing::warn!(?e, thread = %thread_id, "budget hard cap pause failed");
            } else {
                tracing::warn!(
                    target: "budget",
                    thread = %thread_id,
                    spent = budget.spent_usd,
                    limit = budget.limit_usd,
                    "budget.hard_cap_pause"
                );
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
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &NoopSpawner).unwrap();

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
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &NoopSpawner).unwrap();
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
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &NoopSpawner).unwrap();
        let t = store.get("thr", "T-0001").unwrap();
        assert_eq!(t.assignee.as_deref(), Some(e1.id.as_str()));
        assert_eq!(
            t.previous_assignees.last().map(|s| s.as_str()),
            Some(first_gen.as_str())
        );

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
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &NoopSpawner).unwrap();
        let key = ("thr".to_string(), "T-0001".to_string(), first_gen.clone());
        assert!(
            cd.lock().unwrap().contains_key(&key),
            "cooldown not recorded"
        );

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

        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &NoopSpawner).unwrap();
        let t = store.get("thr", "T-0001").unwrap();
        let second_gen = t.assignee.clone().unwrap();
        let other = if first_gen == g1.id {
            g2.id.clone()
        } else {
            g1.id.clone()
        };
        assert_eq!(
            second_gen, other,
            "cooldown should have forced the other generator"
        );
    }

    // ----- budget pass -----

    use crate::budget::{
        ActiveSession, ActiveSessionsSource, BudgetStore, BudgetWarning, BudgetWarningSink,
        CostReporter, SessionCost,
    };
    use std::path::Path;
    use std::sync::Mutex as StdMutex;
    use tempfile::tempdir;

    struct MockReporter {
        cost: f64,
    }
    impl CostReporter for MockReporter {
        fn poll(&self, _sid: &str, _cwd: &Path) -> Result<SessionCost, crate::Error> {
            Ok(SessionCost {
                model: "claude-opus-4-7".into(),
                usage: Default::default(),
                cost_usd: self.cost,
            })
        }
    }

    struct SidCostReporter(HashMap<String, f64>);
    impl CostReporter for SidCostReporter {
        fn poll(&self, sid: &str, _cwd: &Path) -> Result<SessionCost, crate::Error> {
            Ok(SessionCost {
                model: "claude-opus-4-7".into(),
                usage: Default::default(),
                cost_usd: *self.0.get(sid).unwrap_or(&0.0),
            })
        }
    }

    struct StaticSessions(Vec<ActiveSession>);
    impl ActiveSessionsSource for StaticSessions {
        fn snapshot(&self) -> Vec<ActiveSession> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct RecordingSink(StdMutex<Vec<BudgetWarning>>);
    impl BudgetWarningSink for RecordingSink {
        fn emit(&self, w: BudgetWarning) {
            self.0.lock().unwrap().push(w);
        }
    }

    fn mk_wiring(
        dir: &Path,
        cost: f64,
        sessions: Vec<ActiveSession>,
        sink: Arc<RecordingSink>,
    ) -> BudgetWiring {
        let store = BudgetStore::load(dir).unwrap();
        let mut reporters: HashMap<String, Arc<dyn CostReporter>> = HashMap::new();
        reporters.insert("claude".into(), Arc::new(MockReporter { cost }));
        BudgetWiring {
            store,
            reporters,
            sessions: Arc::new(StaticSessions(sessions)),
            sink,
        }
    }

    #[test]
    fn budget_pass_emits_warning_on_soft_crossing_only_once() {
        let dir = tempdir().unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let sessions = vec![ActiveSession {
            thread_id: "thr-1".into(),
            session_id: "sid-1".into(),
            cwd: dir.path().to_path_buf(),
            kind: "claude".into(),
            agent_id: None,
            role: None,
        }];

        // 80% — over soft (band 75), under hard.
        let wiring = mk_wiring(dir.path(), 8.0, sessions.clone(), sink.clone());
        wiring.store.set_limit("thr-1", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        run_budget_pass(&wiring, &pause, &mut bands).unwrap();
        run_budget_pass(&wiring, &pause, &mut bands).unwrap();

        let emitted = sink.0.lock().unwrap();
        assert_eq!(
            emitted.len(),
            1,
            "warning should fire exactly once per band"
        );
        assert_eq!(emitted[0].thread_id, "thr-1");
        assert_eq!(emitted[0].pct, 80);
        assert!(!pause.is_paused(), "soft cross must not trip pause");
        assert_eq!(bands.get("thr-1").copied(), Some(75));
    }

    #[test]
    fn budget_pass_trips_pause_at_hard_cap() {
        let dir = tempdir().unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let sessions = vec![ActiveSession {
            thread_id: "thr-x".into(),
            session_id: "sid-x".into(),
            cwd: dir.path().to_path_buf(),
            kind: "claude".into(),
            agent_id: None,
            role: None,
        }];
        let wiring = mk_wiring(dir.path(), 10.0, sessions, sink.clone());
        wiring.store.set_limit("thr-x", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        run_budget_pass(&wiring, &pause, &mut bands).unwrap();

        assert!(pause.is_paused(), "hard cap must trip global pause");
        let emitted = sink.0.lock().unwrap();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].pct, 100);
    }

    #[test]
    fn budget_pass_skips_sessions_without_reporter_for_kind() {
        let dir = tempdir().unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let sessions = vec![ActiveSession {
            thread_id: "thr-c".into(),
            session_id: "sid-c".into(),
            cwd: dir.path().to_path_buf(),
            kind: "codex".into(), // not in reporters map
            agent_id: None,
            role: None,
        }];
        let wiring = mk_wiring(dir.path(), 10.0, sessions, sink.clone());
        wiring.store.set_limit("thr-c", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        run_budget_pass(&wiring, &pause, &mut bands).unwrap();
        assert!(sink.0.lock().unwrap().is_empty());
        assert!(!pause.is_paused());
    }

    #[test]
    fn budget_pass_emits_per_agent_breakdown() {
        let dir = tempdir().unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let sessions = vec![
            ActiveSession {
                thread_id: "thr-1".into(),
                session_id: "sid-1".into(),
                cwd: dir.path().to_path_buf(),
                kind: "claude".into(),
                agent_id: Some("a1".into()),
                role: Some("generator".into()),
            },
            ActiveSession {
                thread_id: "thr-1".into(),
                session_id: "sid-2".into(),
                cwd: dir.path().to_path_buf(),
                kind: "claude".into(),
                agent_id: Some("a1".into()),
                role: Some("generator".into()),
            },
            ActiveSession {
                thread_id: "thr-1".into(),
                session_id: "sid-3".into(),
                cwd: dir.path().to_path_buf(),
                kind: "claude".into(),
                agent_id: Some("a2".into()),
                role: Some("planner".into()),
            },
        ];
        let store = BudgetStore::load(dir.path()).unwrap();
        let mut reporters: HashMap<String, Arc<dyn CostReporter>> = HashMap::new();
        reporters.insert(
            "claude".into(),
            Arc::new(SidCostReporter(HashMap::from([
                ("sid-1".into(), 1.0),
                ("sid-2".into(), 2.0),
                ("sid-3".into(), 5.0),
            ]))),
        );
        let wiring = BudgetWiring {
            store,
            reporters,
            sessions: Arc::new(StaticSessions(sessions)),
            sink,
        };

        let mut bands: HashMap<String, u8> = HashMap::new();
        run_budget_pass(&wiring, &pause, &mut bands).unwrap();

        let agents = wiring.store.agents_for("thr-1");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].agent_id, "a2");
        assert_eq!(agents[0].role, "planner");
        assert_eq!(agents[0].sessions, 1);
        assert_eq!(agents[0].spent_usd, 5.0);
        assert_eq!(agents[1].agent_id, "a1");
        assert_eq!(agents[1].role, "generator");
        assert_eq!(agents[1].sessions, 2);
        assert_eq!(agents[1].spent_usd, 3.0);
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

    // ----- spawner wiring -----

    /// Records every spawn request the scheduler issues so we can assert on
    /// `(agent_id, role, thread_id)` from the test body.
    #[derive(Default)]
    struct RecordingSpawner(StdMutex<Vec<SpawnRequest>>);
    impl SessionSpawner for RecordingSpawner {
        fn spawn(&self, req: SpawnRequest) -> SpawnResult {
            let session_id = format!("sess-{}", req.agent_id);
            self.0.lock().unwrap().push(req);
            SpawnResult::Launched { session_id }
        }
    }

    /// When the scheduler claims a queued task for a generator, the spawner
    /// must be asked to materialize a PTY session for that agent. Before this
    /// wiring landed, `run_assign_pass` only set the assignee in the task
    /// store, leaving the agent as a phantom (no PTY ever started). Regression
    /// guard for that bug.
    #[test]
    fn assign_pass_asks_spawner_to_launch_claimed_agent() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let gen = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g".into(),
                role: Some("generator".into()),
            })
            .unwrap();

        store
            .create(
                "thr-spawn",
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

        let spawner = RecordingSpawner::default();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &spawner).unwrap();

        let calls = spawner.0.lock().unwrap();
        assert_eq!(calls.len(), 1, "spawner should be called exactly once");
        assert_eq!(calls[0].agent_id, gen.id);
        assert_eq!(calls[0].role, "generator");
        assert_eq!(calls[0].thread_id, "thr-spawn");
    }

    /// Routing `pending_verify` to an evaluator must also request a spawn —
    /// otherwise the evaluator has nothing to review with.
    #[test]
    fn assign_pass_spawns_evaluator_on_pending_verify_route() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{Artifacts, TaskDraft, TaskPatch, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let gen = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        let evaluator = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "e".into(),
                role: Some("evaluator".into()),
            })
            .unwrap();

        store
            .create(
                "thr-eval",
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

        let spawner = RecordingSpawner::default();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());

        // Tick 1: queued -> claim by generator (1 spawn).
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &spawner).unwrap();
        // Generator submits.
        store
            .patch(
                "thr-eval",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
                &gen.id,
            )
            .unwrap();
        store
            .patch(
                "thr-eval",
                "T-0001",
                TaskPatch {
                    artifacts: Some(Artifacts {
                        files: vec!["x.txt".into()],
                        ..Default::default()
                    }),
                    status: Some(TaskStatus::PendingVerify),
                    ..Default::default()
                },
                &gen.id,
            )
            .unwrap();
        // Tick 2: should route to evaluator AND request a spawn for them.
        run_assign_pass(&store, &agents, &pause, 3, &mut prev, &cd, &spawner).unwrap();

        let calls = spawner.0.lock().unwrap();
        let roles: Vec<&str> = calls.iter().map(|c| c.role.as_str()).collect();
        assert!(
            roles.contains(&"generator"),
            "generator spawn missing: {roles:?}"
        );
        assert!(
            roles.contains(&"evaluator"),
            "evaluator spawn missing: {roles:?}"
        );
        let eval_call = calls.iter().find(|c| c.role == "evaluator").unwrap();
        assert_eq!(eval_call.agent_id, evaluator.id);
        assert_eq!(eval_call.thread_id, "thr-eval");
    }
}
