use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::agents::{Agent, AgentKind, AgentsRegistry};
use crate::budget::{
    ActiveSessionsSource, AgentCost, BudgetStore, BudgetWarning, BudgetWarningSink, CostReporter,
};
use crate::pause::PauseFlag;
use crate::tasks::{
    ClaimResult, SchedulerDecisionKind, SchedulerExplanation, Task, TaskEvent, TaskPatch,
    TaskStatus, TaskStore,
};

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

fn agent_can_generate(a: &Agent) -> bool {
    matches!(
        agent_role(a),
        "generator" | "frontend" | "frontend-worker" | "frontend-visual"
    )
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
                            budget.as_ref().map(|b| &b.store),
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
                                &store,
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
    for tid in store.scheduler_threads()? {
        let tasks = store.scheduler_snapshot(&tid)?;
        // Build a status lookup.
        let status_of: std::collections::HashMap<&str, TaskStatus> =
            tasks.iter().map(|t| (t.id.as_str(), t.status)).collect();
        for t in &tasks {
            match t.status {
                TaskStatus::Queued if t.blocked_by.is_empty() => {
                    let key = (tid.clone(), t.id.clone());
                    if announced.insert(key) {
                        record_scheduler_explanation(
                            store,
                            &tid,
                            scheduler_explanation(
                                &t.id,
                                SchedulerDecisionKind::Ready,
                                "Task is queued and has no blockers",
                            ),
                        );
                        store.emit(
                            &tid,
                            TaskEvent::Ready {
                                task_id: t.id.clone(),
                            },
                        );
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
                        store.emit(
                            &tid,
                            TaskEvent::Changed {
                                task_id: t.id.clone(),
                                prev_status: TaskStatus::Blocked,
                                next_status: TaskStatus::Queued,
                                by: "scheduler".into(),
                                at: Utc::now(),
                            },
                        );
                        record_scheduler_explanation(
                            store,
                            &tid,
                            scheduler_explanation(
                                &t.id,
                                SchedulerDecisionKind::AutoUnblocked,
                                "All blocking tasks are done",
                            ),
                        );
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
fn request_spawn(spawner: &dyn SessionSpawner, tid: &str, task_id: &str, agent: &Agent) {
    let req = SpawnRequest {
        agent_id: agent.id.clone(),
        role: agent_role(agent).to_string(),
        kind: agent.kind.as_str().to_string(),
        thread_id: tid.to_string(),
        task_id: Some(task_id.to_string()),
        cwd: None,
    };
    match spawner.spawn(req) {
        SpawnResult::Launched { session_id } => {
            tracing::info!(
                target: "scheduling",
                thread = %tid,
                task = %task_id,
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
                task = %task_id,
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
                task = %task_id,
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
#[cfg(test)]
pub(crate) fn pick_idle_generator<'a>(
    agents: &'a [Agent],
    busy: &HashSet<String>,
    cooldown: &HashMap<CooldownKey, Instant>,
    tid: &str,
    task_id: &str,
    now: Instant,
) -> Option<&'a Agent> {
    pick_idle_generator_preferred(agents, busy, cooldown, tid, task_id, now, None)
}

fn pick_idle_generator_preferred<'a>(
    agents: &'a [Agent],
    busy: &HashSet<String>,
    cooldown: &HashMap<CooldownKey, Instant>,
    tid: &str,
    task_id: &str,
    now: Instant,
    preferred_agent_id: Option<&str>,
) -> Option<&'a Agent> {
    let eligible = |a: &&Agent| {
        if !agent_can_generate(a) {
            return false;
        }
        if busy.contains(&a.id) {
            return false;
        }
        let key = (tid.to_string(), task_id.to_string(), a.id.clone());
        !matches!(cooldown.get(&key), Some(until) if *until > now)
    };

    if let Some(preferred) = preferred_agent_id {
        if let Some(agent) = agents
            .iter()
            .filter(eligible)
            .find(|a| a.id.as_str() == preferred)
        {
            return Some(agent);
        }
    }

    agents.iter().filter(eligible).next()
}

fn artifact_paths(task: &Task) -> impl Iterator<Item = &str> {
    task.artifacts
        .files
        .iter()
        .map(String::as_str)
        .chain(task.artifacts.metadata.iter().map(|a| a.path.as_str()))
        .filter(|p| !p.trim().is_empty())
}

fn producing_agent_for(task: &Task) -> Option<&str> {
    task.artifacts
        .metadata
        .iter()
        .rev()
        .find_map(|artifact| {
            let produced_by = artifact.produced_by.trim();
            (!produced_by.is_empty()).then_some(produced_by)
        })
        .or_else(|| task.assignee.as_deref())
        .or_else(|| task.previous_assignees.last().map(String::as_str))
}

fn recent_file_affinity(tasks: &[Task]) -> HashMap<String, String> {
    let mut affinity = HashMap::new();
    for task in tasks {
        let Some(agent) = producing_agent_for(task) else {
            continue;
        };
        for path in artifact_paths(task) {
            affinity.insert(path.to_string(), agent.to_string());
        }
    }
    affinity
}

fn task_text(task: &Task) -> String {
    let mut text = String::new();
    text.push_str(&task.title);
    text.push('\n');
    for label in &task.labels {
        text.push_str(label);
        text.push('\n');
    }
    for spec in &task.spec_refs {
        text.push_str(&spec.section);
        text.push('\n');
    }
    for check in &task.acceptance.checks {
        text.push_str(&check.text);
        text.push('\n');
    }
    if let Some(brief) = &task.brief {
        text.push_str(&brief.objective);
        text.push('\n');
        text.push_str(&brief.context);
        text.push('\n');
        for step in &brief.tasks {
            text.push_str(step);
            text.push('\n');
        }
        for rule in &brief.rules {
            text.push_str(rule);
            text.push('\n');
        }
        text.push_str(&brief.expected_result);
    }
    text.to_ascii_lowercase()
}

fn path_matches_task_text(path: &str, text: &str) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let lowered = path.to_ascii_lowercase();
    if text.contains(&lowered) {
        return true;
    }
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.is_empty() && text.contains(&name.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn preferred_generator_for_task(task: &Task, affinity: &HashMap<String, String>) -> Option<String> {
    let text = task_text(task);
    affinity
        .iter()
        .find_map(|(path, agent)| path_matches_task_text(path, &text).then(|| agent.clone()))
}

fn is_frontend_visual_task(task: &Task) -> bool {
    let text = task_text(task);
    let visual_terms = [
        "frontend",
        "ui",
        "screen",
        "view",
        "layout",
        "css",
        "responsive",
        "shadcn",
        "polish",
        "a11y",
        "accessibility",
        "visual",
        ".svelte",
        "tailwind",
    ];
    let frontend_path = task.write_paths.iter().any(|path| {
        let path = path.to_ascii_lowercase();
        path.starts_with("frontend/")
            || path.ends_with(".svelte")
            || path.ends_with(".css")
            || path.contains("/components/")
    });
    frontend_path || visual_terms.iter().any(|term| text.contains(term))
}

fn preferred_frontend_visual_agent(task: &Task, agents: &[Agent]) -> Option<String> {
    if !is_frontend_visual_task(task) {
        return None;
    }
    agents
        .iter()
        .find(|agent| {
            agent_can_generate(agent)
                && (agent.kind == AgentKind::Cursor || agent_role(agent) == "frontend-visual")
        })
        .map(|agent| agent.id.clone())
}

fn scheduler_explanation(
    task_id: &str,
    decision: SchedulerDecisionKind,
    reason: impl Into<String>,
) -> SchedulerExplanation {
    SchedulerExplanation {
        task_id: task_id.to_string(),
        decision,
        reason: reason.into(),
        agent_id: None,
        previous_holder: None,
        blocked_by: Vec::new(),
        cooldown_seconds: None,
        max_concurrent: None,
        queue_depth: None,
        at: Utc::now(),
    }
}

fn record_scheduler_explanation(store: &TaskStore, tid: &str, explanation: SchedulerExplanation) {
    if let Err(e) = store.record_scheduler_decision(tid, explanation) {
        tracing::warn!(?e, thread = %tid, "scheduler explanation persist failed");
    }
}

fn cooldown_skip_seconds(
    agents: &[Agent],
    busy: &HashSet<String>,
    cooldown: &HashMap<CooldownKey, Instant>,
    tid: &str,
    task_id: &str,
    now: Instant,
) -> Option<u64> {
    agents
        .iter()
        .filter(|a| agent_can_generate(a) && !busy.contains(&a.id))
        .filter_map(|a| {
            let key = (tid.to_string(), task_id.to_string(), a.id.clone());
            cooldown
                .get(&key)
                .filter(|until| **until > now)
                .map(|until| until.saturating_duration_since(now).as_secs())
        })
        .min()
}

/// Auto-claim eligible queued tasks for idle generators and route
/// `pending_verify` tasks to an evaluator. Also tracks verify-fail cooldowns.
fn run_assign_pass(
    store: &TaskStore,
    agents: &AgentsRegistry,
    pause: &PauseFlag,
    max_concurrent: usize,
    budget_store: Option<&BudgetStore>,
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

    for tid in store.scheduler_threads()? {
        if pause.is_thread_paused(&tid) {
            tracing::debug!(
                target: "scheduling",
                thread = %tid,
                "scheduling.thread_paused"
            );
            continue;
        }
        let tasks = store.scheduler_snapshot(&tid)?;
        let thread_max_concurrent = budget_store
            .and_then(|store| store.get(&tid).max_concurrent_workers)
            .unwrap_or(max_concurrent)
            .max(1);
        let affinity = recent_file_affinity(&tasks);

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
                    let mut explanation = scheduler_explanation(
                        &t.id,
                        SchedulerDecisionKind::CooldownAdded,
                        "Generator was rejected by verification and is cooling down for this task",
                    );
                    explanation.agent_id = Some(gen_id.clone());
                    explanation.cooldown_seconds = Some(COOLDOWN_AFTER_VERIFY_FAIL.as_secs());
                    record_scheduler_explanation(store, &tid, explanation);
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
        let queue_depth = queued_unblocked.len();
        for t in queued_unblocked {
            if current >= thread_max_concurrent {
                let mut explanation = scheduler_explanation(
                    &t.id,
                    SchedulerDecisionKind::AssignmentSkipped,
                    "Max concurrency reached for this thread",
                );
                explanation.max_concurrent = Some(thread_max_concurrent);
                explanation.queue_depth = Some(queue_depth);
                record_scheduler_explanation(store, &tid, explanation);
                continue;
            }
            let cd_snapshot = cooldown.lock().expect("cooldown mutex").clone();
            let pick_now = Instant::now();
            let preferred = preferred_frontend_visual_agent(t, &all_agents)
                .or_else(|| preferred_generator_for_task(t, &affinity));
            let Some(agent) = pick_idle_generator_preferred(
                &all_agents,
                &busy_ids,
                &cd_snapshot,
                &tid,
                &t.id,
                pick_now,
                preferred.as_deref(),
            ) else {
                let cooldown_seconds = cooldown_skip_seconds(
                    &all_agents,
                    &busy_ids,
                    &cd_snapshot,
                    &tid,
                    &t.id,
                    pick_now,
                );
                let mut explanation = scheduler_explanation(
                    &t.id,
                    if cooldown_seconds.is_some() {
                        SchedulerDecisionKind::CooldownSkipped
                    } else {
                        SchedulerDecisionKind::AssignmentSkipped
                    },
                    if cooldown_seconds.is_some() {
                        "All idle generators are cooling down for this task"
                    } else {
                        "No idle generator is available"
                    },
                );
                explanation.cooldown_seconds = cooldown_seconds;
                explanation.queue_depth = Some(queue_depth);
                record_scheduler_explanation(store, &tid, explanation);
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
                    let mut explanation = scheduler_explanation(
                        &t.id,
                        SchedulerDecisionKind::Assigned,
                        if preferred.as_deref() == Some(agent.id.as_str()) {
                            if agent.kind == AgentKind::Cursor
                                || agent_role(agent) == "frontend-visual"
                            {
                                "Assigned to preferred Cursor frontend visual generator"
                            } else {
                                "Assigned to preferred generator by file affinity"
                            }
                        } else {
                            "Assigned to an idle generator"
                        },
                    );
                    explanation.agent_id = Some(agent.id.clone());
                    record_scheduler_explanation(store, &tid, explanation);
                    request_spawn(spawner, &tid, &t.id, agent);
                }
                Ok(ClaimResult::Busy { holder, .. }) => {
                    let mut explanation = scheduler_explanation(
                        &t.id,
                        SchedulerDecisionKind::ClaimBusy,
                        "Task was already claimed before the scheduler could assign it",
                    );
                    explanation.previous_holder = Some(holder.clone());
                    record_scheduler_explanation(store, &tid, explanation);
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
                record_scheduler_explanation(
                    store,
                    &tid,
                    scheduler_explanation(
                        &t.id,
                        SchedulerDecisionKind::EvaluatorSkipped,
                        "No idle evaluator is available",
                    ),
                );
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
                    let mut explanation = scheduler_explanation(
                        &t.id,
                        SchedulerDecisionKind::RoutedToEvaluator,
                        "Routed to an idle evaluator",
                    );
                    explanation.agent_id = Some(evaluator.id.clone());
                    explanation.previous_holder = prior;
                    record_scheduler_explanation(store, &tid, explanation);
                    request_spawn(spawner, &tid, &t.id, evaluator);
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
    store: &TaskStore,
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
        if budget.over_hard() {
            pause_in_progress_tasks_for_budget(store, &thread_id, &budget)?;
        }
    }
    Ok(())
}

fn pause_in_progress_tasks_for_budget(
    store: &TaskStore,
    thread_id: &str,
    budget: &crate::budget::Budget,
) -> Result<(), crate::Error> {
    let reason = format!(
        "budget cap reached: ${:.4} spent of ${:.4} limit ({}%)",
        budget.spent_usd,
        budget.limit_usd,
        budget.pct_spent()
    );
    let tasks = store.scheduler_snapshot(thread_id)?;
    for task in tasks
        .into_iter()
        .filter(|task| task.status == TaskStatus::InProgress)
    {
        let patch = TaskPatch {
            status: Some(TaskStatus::Paused),
            paused_reason: Some(reason.clone()),
            why_paused: Some(reason.clone()),
            ..Default::default()
        };
        if let Err(e) = store.patch(thread_id, &task.id, patch, "scheduler") {
            tracing::warn!(
                ?e,
                thread = %thread_id,
                task = %task.id,
                "budget hard cap task pause failed"
            );
        }
    }
    Ok(())
}

fn run_lease_pass(store: &TaskStore) -> Result<(), crate::Error> {
    let now = Utc::now();
    for tid in store.scheduler_threads()? {
        let tasks = store.scheduler_snapshot(&tid)?;
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
            store.emit(
                &tid,
                TaskEvent::LeaseExpired {
                    task_id: t.id.clone(),
                    previous_holder: prev_holder.clone(),
                },
            );
            let mut explanation = scheduler_explanation(
                &t.id,
                SchedulerDecisionKind::LeaseExpired,
                "Claim lease expired and the task was released",
            );
            explanation.previous_holder = Some(prev_holder);
            record_scheduler_explanation(store, &tid, explanation);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{Agent, AgentKind};
    use crate::threads::Handoff;
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

    fn append_handoff(home: &std::path::Path, thread_id: &str, task_id: &str, from: &str) {
        let store = crate::Store::new(home).unwrap();
        store
            .append_handoff(
                thread_id,
                &Handoff {
                    at: Utc::now().timestamp_millis(),
                    from: from.to_string(),
                    to_role: "evaluator".to_string(),
                    task_id: task_id.to_string(),
                    status: "ready_for_verification".to_string(),
                    goal: "Verify submitted artifacts".to_string(),
                    assumptions: vec![],
                    files_changed: vec!["src/lib.rs".to_string()],
                    commands_run: vec!["cargo test".to_string()],
                    verification_passed: vec![],
                    verification_not_run: vec![],
                    blocked_on: vec![],
                    next_agent_action: "Evaluator reviews acceptance checks".to_string(),
                },
            )
            .unwrap();
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
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let t = store.get("thr-1", "T-0001").unwrap();
        assert_eq!(t.assignee.as_deref(), Some(gen.id.as_str()));
        assert_ne!(t.assignee.as_deref(), Some(planner.id.as_str()));
        let explanation = t.scheduler_explanation.expect("scheduler explanation");
        assert_eq!(explanation.decision, SchedulerDecisionKind::Assigned);
        assert_eq!(explanation.agent_id.as_deref(), Some(gen.id.as_str()));
    }

    #[test]
    fn assign_pass_stable_thread_uses_memory_snapshot_without_task_file_reads() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{
            reset_task_file_read_count, task_file_read_count, TaskDraft, TaskStore,
        };
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
                "thr-stable",
                TaskDraft {
                    title: "already claimed".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();
        store
            .claim("thr-stable", "T-0001", &gen.id, Duration::from_secs(60))
            .unwrap();

        store.scheduler_threads().unwrap();
        reset_task_file_read_count();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        assert_eq!(task_file_read_count(), 0);
    }

    #[test]
    fn assign_pass_skips_thread_scoped_pause() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g".into(),
                role: Some("generator".into()),
            })
            .unwrap();

        store
            .create(
                "thr-paused",
                TaskDraft {
                    title: "t".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();
        pause.set_thread("thr-paused", true).unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let t = store.get("thr-paused", "T-0001").unwrap();
        assert_eq!(t.status, TaskStatus::Queued);
        assert!(t.assignee.is_none());
        assert!(t.scheduler_explanation.is_none());
    }

    #[test]
    fn assign_pass_explains_max_concurrency_skip() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g1".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "g2".into(),
                role: Some("generator".into()),
            })
            .unwrap();

        for title in ["first", "second"] {
            store
                .create(
                    "thr-max",
                    TaskDraft {
                        title: title.into(),
                        parent: None,
                        depends_on: vec![],
                        brief: None,
                        acceptance: vec![],
                        labels: vec![],
                        spec_refs: vec![],
                        write_paths: vec![],
                        forbidden_paths: vec![],
                        created_by: "human".into(),
                    },
                )
                .unwrap();
        }

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            1,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let skipped = store.get("thr-max", "T-0002").unwrap();
        let explanation = skipped
            .scheduler_explanation
            .expect("scheduler explanation");
        assert_eq!(
            explanation.decision,
            SchedulerDecisionKind::AssignmentSkipped
        );
        assert_eq!(explanation.max_concurrent, Some(1));
        assert_eq!(explanation.queue_depth, Some(2));
    }

    #[test]
    fn assign_pass_uses_thread_budget_max_concurrent_workers() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::budget::BudgetStore;
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let budget = BudgetStore::load(dir.path()).unwrap();
        budget
            .set_max_concurrent_workers("thr-budget-cap", Some(1))
            .unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        for label in ["g1", "g2"] {
            agents
                .create(AgentDraft {
                    kind: AgentKind::Claude,
                    label: label.into(),
                    role: Some("generator".into()),
                })
                .unwrap();
        }

        for title in ["first", "second"] {
            store
                .create(
                    "thr-budget-cap",
                    TaskDraft {
                        title: title.into(),
                        parent: None,
                        depends_on: vec![],
                        brief: None,
                        acceptance: vec![],
                        labels: vec![],
                        spec_refs: vec![],
                        write_paths: vec![],
                        forbidden_paths: vec![],
                        created_by: "human".into(),
                    },
                )
                .unwrap();
        }

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            Some(&budget),
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let second = store.get("thr-budget-cap", "T-0002").unwrap();
        let explanation = second.scheduler_explanation.expect("scheduler explanation");
        assert_eq!(
            explanation.decision,
            SchedulerDecisionKind::AssignmentSkipped
        );
        assert_eq!(explanation.max_concurrent, Some(1));
    }

    #[test]
    fn assign_pass_prefers_recent_generator_for_matching_file() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{Artifacts, TaskDraft, TaskStore};
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

        store
            .create(
                "thr-affinity",
                TaskDraft {
                    title: "prior work".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();
        store
            .with_locked("thr-affinity", "T-0001", |task| {
                task.status = TaskStatus::Done;
                task.assignee = Some(g2.id.clone());
                task.artifacts = Artifacts {
                    files: vec!["src/lib.rs".into()],
                    ..Default::default()
                };
                Ok(())
            })
            .unwrap();

        store
            .create(
                "thr-affinity",
                TaskDraft {
                    title: "adjust src/lib.rs behavior".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let assigned = store.get("thr-affinity", "T-0002").unwrap();
        assert_eq!(assigned.assignee.as_deref(), Some(g2.id.as_str()));
        assert_ne!(assigned.assignee.as_deref(), Some(g1.id.as_str()));
        assert_eq!(
            assigned.scheduler_explanation.unwrap().reason,
            "Assigned to preferred generator by file affinity"
        );
    }

    #[test]
    fn assign_pass_prefers_cursor_for_frontend_visual_task() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{TaskDraft, TaskStore};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let codex = agents
            .create(AgentDraft {
                kind: AgentKind::Codex,
                label: "logic".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        let cursor = agents
            .create(AgentDraft {
                kind: AgentKind::Cursor,
                label: "visual".into(),
                role: Some("frontend-visual".into()),
            })
            .unwrap();

        store
            .create(
                "thr-visual",
                TaskDraft {
                    title: "Polish responsive layout for settings screen".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec!["frontend".into(), "visual".into(), "a11y".into()],
                    spec_refs: vec![],
                    write_paths: vec!["frontend/src/routes/settings/+page.svelte".into()],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();

        let assigned = store.get("thr-visual", "T-0001").unwrap();
        assert_eq!(assigned.assignee.as_deref(), Some(cursor.id.as_str()));
        assert_ne!(assigned.assignee.as_deref(), Some(codex.id.as_str()));
        assert_eq!(
            assigned.scheduler_explanation.unwrap().reason,
            "Assigned to preferred Cursor frontend visual generator"
        );
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
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());

        // Tick 1: assign queued -> g1 (lowest id picked first).
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();
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
                    rejected_reason: Some("verify failed".into()),
                    ..Default::default()
                },
                &first_gen,
            )
            .unwrap();
        append_handoff(dir.path(), "thr", "T-0001", &first_gen);
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
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();
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
        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();
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

        run_assign_pass(
            &store,
            &agents,
            &pause,
            3,
            None,
            &mut prev,
            &cd,
            &NoopSpawner,
        )
        .unwrap();
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

    fn budget_task_store(dir: &Path) -> TaskStore {
        TaskStore::new(dir).unwrap()
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
            task_id: None,
            owner_session_id: None,
            parent_session_id: None,
            root_session_id: None,
        }];

        // 80% — over soft (band 75), under hard.
        let wiring = mk_wiring(dir.path(), 8.0, sessions.clone(), sink.clone());
        wiring.store.set_limit("thr-1", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        let tasks = budget_task_store(dir.path());
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();

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
            task_id: None,
            owner_session_id: None,
            parent_session_id: None,
            root_session_id: None,
        }];
        let wiring = mk_wiring(dir.path(), 10.0, sessions, sink.clone());
        wiring.store.set_limit("thr-x", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        let tasks = budget_task_store(dir.path());
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();

        assert!(pause.is_paused(), "hard cap must trip global pause");
        let emitted = sink.0.lock().unwrap();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].pct, 100);
    }

    #[test]
    fn budget_pass_pauses_in_progress_tasks_at_hard_cap() {
        use crate::tasks::TaskDraft;

        let dir = tempdir().unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let tasks = TaskStore::new(dir.path()).unwrap();
        tasks
            .create(
                "thr-budget",
                TaskDraft {
                    title: "expensive work".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();
        tasks
            .claim("thr-budget", "T-0001", "agent:g1", Duration::from_secs(60))
            .unwrap();
        tasks
            .patch(
                "thr-budget",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
                "agent:g1",
            )
            .unwrap();

        let sessions = vec![ActiveSession {
            thread_id: "thr-budget".into(),
            session_id: "sid-budget".into(),
            cwd: dir.path().to_path_buf(),
            kind: "claude".into(),
            agent_id: Some("agent:g1".into()),
            role: Some("generator".into()),
            task_id: Some("T-0001".into()),
            owner_session_id: None,
            parent_session_id: None,
            root_session_id: None,
        }];
        let wiring = mk_wiring(dir.path(), 10.0, sessions, sink);
        wiring.store.set_limit("thr-budget", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();

        let task = tasks.get("thr-budget", "T-0001").unwrap();
        assert_eq!(task.status, TaskStatus::Paused);
        assert!(task.notes.pause_reason().contains("budget cap reached"));
        assert_eq!(task.updated_by, "scheduler");
        assert!(pause.is_paused(), "hard cap must also trip global pause");
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
            task_id: None,
            owner_session_id: None,
            parent_session_id: None,
            root_session_id: None,
        }];
        let wiring = mk_wiring(dir.path(), 10.0, sessions, sink.clone());
        wiring.store.set_limit("thr-c", 10.0).unwrap();

        let mut bands: HashMap<String, u8> = HashMap::new();
        let tasks = budget_task_store(dir.path());
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();
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
                task_id: None,
                owner_session_id: None,
                parent_session_id: None,
                root_session_id: None,
            },
            ActiveSession {
                thread_id: "thr-1".into(),
                session_id: "sid-2".into(),
                cwd: dir.path().to_path_buf(),
                kind: "claude".into(),
                agent_id: Some("a1".into()),
                role: Some("generator".into()),
                task_id: None,
                owner_session_id: None,
                parent_session_id: None,
                root_session_id: None,
            },
            ActiveSession {
                thread_id: "thr-1".into(),
                session_id: "sid-3".into(),
                cwd: dir.path().to_path_buf(),
                kind: "claude".into(),
                agent_id: Some("a2".into()),
                role: Some("planner".into()),
                task_id: None,
                owner_session_id: None,
                parent_session_id: None,
                root_session_id: None,
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
        let tasks = budget_task_store(dir.path());
        run_budget_pass(&wiring, &pause, &tasks, &mut bands).unwrap();

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
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let spawner = RecordingSpawner::default();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        run_assign_pass(&store, &agents, &pause, 3, None, &mut prev, &cd, &spawner).unwrap();

        let calls = spawner.0.lock().unwrap();
        assert_eq!(calls.len(), 1, "spawner should be called exactly once");
        assert_eq!(calls[0].agent_id, gen.id);
        assert_eq!(calls[0].role, "generator");
        assert_eq!(calls[0].thread_id, "thr-spawn");
        assert_eq!(calls[0].task_id.as_deref(), Some("T-0001"));
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
                    brief: None,
                    acceptance: vec![],
                    labels: vec![],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "human".into(),
                },
            )
            .unwrap();

        let spawner = RecordingSpawner::default();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());

        // Tick 1: queued -> claim by generator (1 spawn).
        run_assign_pass(&store, &agents, &pause, 3, None, &mut prev, &cd, &spawner).unwrap();
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
        append_handoff(dir.path(), "thr-eval", "T-0001", &gen.id);
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
        run_assign_pass(&store, &agents, &pause, 3, None, &mut prev, &cd, &spawner).unwrap();

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
        assert_eq!(eval_call.task_id.as_deref(), Some("T-0001"));
    }

    #[test]
    fn small_planner_worker_evaluator_flow_finishes_and_unblocks_dependency() {
        use crate::agents::{AgentDraft, AgentsRegistry};
        use crate::pause::PauseFlag;
        use crate::tasks::{
            AcceptanceCheck, Artifacts, TaskDraft, TaskPatch, TaskStatus, TaskStore,
        };
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let agents = AgentsRegistry::new(dir.path()).unwrap();
        let pause = PauseFlag::load(dir.path()).unwrap();
        agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "planner".into(),
                role: Some("planner".into()),
            })
            .unwrap();
        let generator = agents
            .create(AgentDraft {
                kind: AgentKind::Codex,
                label: "generator".into(),
                role: Some("generator".into()),
            })
            .unwrap();
        let evaluator = agents
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "evaluator".into(),
                role: Some("evaluator".into()),
            })
            .unwrap();

        store
            .create(
                "thr-e2e",
                TaskDraft {
                    title: "implement endpoint".into(),
                    parent: None,
                    depends_on: vec![],
                    brief: None,
                    acceptance: vec![AcceptanceCheck {
                        id: "check-1".into(),
                        text: "endpoint responds".into(),
                        verified: false,
                        verified_by: None,
                    }],
                    labels: vec!["backend".into()],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "agent:planner".into(),
                },
            )
            .unwrap();
        store
            .create(
                "thr-e2e",
                TaskDraft {
                    title: "wire frontend".into(),
                    parent: None,
                    depends_on: vec!["T-0001".into()],
                    brief: None,
                    acceptance: vec![AcceptanceCheck {
                        id: "check-1".into(),
                        text: "frontend renders endpoint data".into(),
                        verified: false,
                        verified_by: None,
                    }],
                    labels: vec!["frontend".into()],
                    spec_refs: vec![],
                    write_paths: vec![],
                    forbidden_paths: vec![],
                    created_by: "agent:planner".into(),
                },
            )
            .unwrap();

        let blocked = store.get("thr-e2e", "T-0002").unwrap();
        assert_eq!(blocked.status, TaskStatus::Blocked);
        assert_eq!(blocked.blocked_by, vec!["T-0001"]);

        let spawner = RecordingSpawner::default();
        let mut prev = StatusSnapshot::new();
        let cd = Mutex::new(HashMap::new());
        let mut announced = HashSet::new();

        run_ready_pass(&store, &mut announced).unwrap();
        run_assign_pass(&store, &agents, &pause, 3, None, &mut prev, &cd, &spawner).unwrap();

        let first = store.get("thr-e2e", "T-0001").unwrap();
        assert_eq!(first.assignee.as_deref(), Some(generator.id.as_str()));
        assert_eq!(first.status, TaskStatus::Queued);

        store
            .patch(
                "thr-e2e",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
                &generator.id,
            )
            .unwrap();
        append_handoff(dir.path(), "thr-e2e", "T-0001", &generator.id);

        store
            .submit(
                "thr-e2e",
                "T-0001",
                Artifacts {
                    files: vec!["src/routes/api.rs".into()],
                    ..Default::default()
                },
                &generator.id,
            )
            .unwrap();
        run_assign_pass(&store, &agents, &pause, 3, None, &mut prev, &cd, &spawner).unwrap();

        let first = store.get("thr-e2e", "T-0001").unwrap();
        assert_eq!(first.status, TaskStatus::PendingVerify);
        assert_eq!(first.assignee.as_deref(), Some(evaluator.id.as_str()));

        let mut checks = first.acceptance.checks.clone();
        checks[0].verified = true;
        checks[0].verified_by = Some("human:qa".into());
        store
            .patch(
                "thr-e2e",
                "T-0001",
                TaskPatch {
                    acceptance_checks: Some(checks),
                    ..Default::default()
                },
                &evaluator.id,
            )
            .unwrap();
        store
            .patch(
                "thr-e2e",
                "T-0001",
                TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                },
                &evaluator.id,
            )
            .unwrap();

        run_ready_pass(&store, &mut announced).unwrap();
        let second = store.get("thr-e2e", "T-0002").unwrap();
        assert_eq!(second.status, TaskStatus::Queued);
        assert_eq!(
            second.scheduler_explanation.as_ref().map(|e| e.decision),
            Some(SchedulerDecisionKind::AutoUnblocked)
        );

        let calls = spawner.0.lock().unwrap();
        let spawned_roles: Vec<&str> = calls.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(spawned_roles, vec!["generator", "evaluator"]);
    }
}
