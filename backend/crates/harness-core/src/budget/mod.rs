//! Per-thread budget tracking. Schema v1.
//!
//! Storage: `<home>/profiles/default/budgets/<thread_id>.toml`. Each file is
//! self-contained (no global aggregate) so concurrent thread writes don't
//! contend on a single lock. Atomic temp+rename on every config persist.
//! Budget attribution observations are append-only JSONL ledgers stored next
//! to the TOML config as `<thread_id>.ledger.jsonl`.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::Error;

pub mod pricing;
pub mod reporter;

pub use pricing::{model_price, ModelPrice};
pub use reporter::{
    ClaudeTranscriptReporter, CodexStubReporter, CostReporter, SessionCost, StubReporter, Usage,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Budget {
    pub limit_usd: f64,
    pub spent_usd: f64,
    /// Percentage (0-100) at which to emit `budget.warning`. Default 80.
    pub soft_pct: u8,
    /// Percentage (0-100) at which to auto-pause the thread. Default 100.
    pub hard_pct: u8,
    /// Optional scheduler override for this thread. `None` means use the
    /// scheduler's global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_workers: Option<usize>,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            limit_usd: 0.0,
            spent_usd: 0.0,
            soft_pct: 80,
            hard_pct: 100,
            max_concurrent_workers: None,
        }
    }
}

impl Budget {
    pub fn pct_spent(&self) -> u8 {
        if self.limit_usd <= 0.0 {
            return 0;
        }
        ((self.spent_usd / self.limit_usd) * 100.0).clamp(0.0, 255.0) as u8
    }

    pub fn over_soft(&self) -> bool {
        self.limit_usd > 0.0 && self.pct_spent() >= self.soft_pct
    }

    pub fn over_hard(&self) -> bool {
        self.limit_usd > 0.0 && self.pct_spent() >= self.hard_pct
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct AgentCost {
    pub agent_id: String,
    pub role: String,
    pub sessions: usize,
    pub spent_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SessionCostView {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub task_bucket: String,
    pub agent_id: String,
    pub role: String,
    pub kind: String,
    pub model: String,
    pub spent_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TaskCost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub task_bucket: String,
    pub sessions: usize,
    pub spent_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RoleCost {
    pub role: String,
    pub sessions: usize,
    pub spent_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct BudgetObservation {
    pub thread_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub agent_id: String,
    pub role: String,
    pub kind: String,
    pub model: String,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_session_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct BudgetBreakdown {
    pub spent_usd: f64,
    pub agents: Vec<AgentCost>,
    pub tasks: Vec<TaskCost>,
    pub roles: Vec<RoleCost>,
    pub sessions: Vec<SessionCostView>,
}

pub type BudgetLedgerView = BudgetBreakdown;

/// Snapshot used by the budget pass to map a running PTY session back to its
/// owning thread and the kind-specific cost reporter.
///
/// Kept stringly-typed (`kind` is the reporter map key) so this crate does
/// **not** need to import `harness-session::AgentKind`.
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub thread_id: String,
    pub session_id: String,
    pub cwd: PathBuf,
    pub kind: String,
    pub agent_id: Option<String>,
    pub role: Option<String>,
    pub task_id: Option<String>,
    pub owner_session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub root_session_id: Option<String>,
}

/// Pluggable provider of currently-active sessions. The server wires this to
/// `harness-session::Manager::all()`.
pub trait ActiveSessionsSource: Send + Sync {
    fn snapshot(&self) -> Vec<ActiveSession>;
}

/// SSE-payload struct emitted when a thread crosses its `soft_pct` boundary
/// for the first time (re-emitted on subsequent threshold jumps but not on
/// every tick — see [`crate::scheduler::tick::run_budget_pass`]).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct BudgetWarning {
    pub thread_id: String,
    pub spent_usd: f64,
    pub limit_usd: f64,
    pub pct: u8,
}

/// Sink for `budget.warning` events. Server impl forwards to the SSE hub.
pub trait BudgetWarningSink: Send + Sync {
    fn emit(&self, warning: BudgetWarning);
}

#[derive(Clone)]
pub struct BudgetStore {
    dir: PathBuf,
    state: Arc<RwLock<HashMap<String, Budget>>>,
    breakdowns: Arc<RwLock<HashMap<String, BudgetBreakdown>>>,
}

impl BudgetStore {
    /// Backwards-compatible loader: uses the `"default"` profile.
    pub fn load(home: &Path) -> Result<Self, Error> {
        Self::load_for_profile(home, "default")
    }

    /// Load the budget store for a specific profile (workspace).
    pub fn load_for_profile(home: &Path, profile: &str) -> Result<Self, Error> {
        let dir = home.join("profiles").join(profile).join("budgets");
        fs::create_dir_all(&dir)?;
        let mut map = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = fs::read_to_string(&path)?;
            match toml_edit::de::from_str::<Budget>(&text) {
                Ok(b) => {
                    map.insert(stem.to_string(), b);
                }
                Err(e) => {
                    tracing::warn!(?path, ?e, "skipping invalid budget TOML");
                }
            }
        }
        let store = Self {
            dir,
            state: Arc::new(RwLock::new(map)),
            breakdowns: Arc::new(RwLock::new(HashMap::new())),
        };
        store.load_ledgers()?;
        Ok(store)
    }

    pub fn get(&self, thread_id: &str) -> Budget {
        self.state
            .read()
            .expect("budget lock")
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn list(&self) -> HashMap<String, Budget> {
        self.state.read().expect("budget lock").clone()
    }

    pub fn agents_for(&self, thread_id: &str) -> Vec<AgentCost> {
        self.breakdown_for(thread_id).agents
    }

    pub fn set_agents_breakdown(&self, thread_id: &str, agents: Vec<AgentCost>) {
        let mut breakdown = self.breakdown_for(thread_id);
        breakdown.agents = agents;
        self.set_breakdown(thread_id, breakdown);
    }

    pub fn breakdown_for(&self, thread_id: &str) -> BudgetBreakdown {
        self.breakdowns
            .read()
            .expect("budget breakdown lock")
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_breakdown(&self, thread_id: &str, breakdown: BudgetBreakdown) {
        self.breakdowns
            .write()
            .expect("budget breakdown lock")
            .insert(thread_id.to_string(), breakdown);
    }

    pub fn record_observation(
        &self,
        thread_id: &str,
        observation: BudgetObservation,
    ) -> Result<BudgetBreakdown, Error> {
        if observation.thread_id != thread_id {
            return Err(Error::Validation(
                "observation thread_id must match ledger thread_id".into(),
            ));
        }
        if observation.session_id.is_empty() {
            return Err(Error::Validation("session_id is required".into()));
        }
        if observation.cost_usd < 0.0 {
            return Err(Error::Validation("cost_usd must be >= 0".into()));
        }
        if self
            .latest_observations(thread_id)?
            .get(&observation.session_id)
            .is_some_and(|latest| same_observation_value(latest, &observation))
        {
            return Ok(self.breakdown_for(thread_id));
        }
        self.record_observations(thread_id, vec![observation])
    }

    pub fn record_observations(
        &self,
        thread_id: &str,
        observations: Vec<BudgetObservation>,
    ) -> Result<BudgetBreakdown, Error> {
        if observations.is_empty() {
            return Ok(self.breakdown_for(thread_id));
        }
        let mut latest = self.latest_observations(thread_id)?;
        let mut to_append = Vec::new();
        for obs in observations {
            if obs.thread_id != thread_id {
                return Err(Error::Validation(
                    "observation thread_id must match ledger thread_id".into(),
                ));
            }
            if obs.session_id.is_empty() {
                return Err(Error::Validation("session_id is required".into()));
            }
            if obs.cost_usd < 0.0 {
                return Err(Error::Validation("cost_usd must be >= 0".into()));
            }
            if latest
                .get(&obs.session_id)
                .is_some_and(|prev| same_observation_value(prev, &obs))
            {
                continue;
            }
            latest.insert(obs.session_id.clone(), obs.clone());
            to_append.push(obs);
        }
        if to_append.is_empty() {
            return Ok(self.breakdown_for(thread_id));
        }
        let path = self.ledger_path(thread_id);
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        for obs in to_append {
            let line = serde_json::to_string(&obs)
                .map_err(|e| Error::Other(anyhow::anyhow!("serialize budget observation: {e}")))?;
            writeln!(file, "{line}")?;
        }
        file.sync_data()?;
        self.rebuild_thread_breakdown(thread_id)
    }

    fn load_ledgers(&self) -> Result<(), Error> {
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let thread_id = stem.strip_suffix(".ledger").unwrap_or(stem);
            let breakdown = self.read_breakdown(thread_id)?;
            if breakdown.spent_usd > 0.0 || !breakdown.sessions.is_empty() {
                self.set_breakdown(thread_id, breakdown.clone());
                let _ = self.set_spent(thread_id, breakdown.spent_usd);
            }
        }
        Ok(())
    }

    fn rebuild_thread_breakdown(&self, thread_id: &str) -> Result<BudgetBreakdown, Error> {
        let breakdown = self.read_breakdown(thread_id)?;
        self.set_breakdown(thread_id, breakdown.clone());
        let _ = self.set_spent(thread_id, breakdown.spent_usd)?;
        Ok(breakdown)
    }

    fn read_breakdown(&self, thread_id: &str) -> Result<BudgetBreakdown, Error> {
        Ok(aggregate_breakdown(
            self.latest_observations(thread_id)?.into_values(),
        ))
    }

    fn latest_observations(
        &self,
        thread_id: &str,
    ) -> Result<HashMap<String, BudgetObservation>, Error> {
        let path = self.ledger_path(thread_id);
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let text = fs::read_to_string(path)?;
        let mut latest: HashMap<String, BudgetObservation> = HashMap::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<BudgetObservation>(line) {
                Ok(obs) => {
                    latest.insert(obs.session_id.clone(), obs);
                }
                Err(e) => {
                    tracing::warn!(thread = %thread_id, error = %e, "skipping invalid budget ledger row")
                }
            }
        }
        Ok(latest)
    }

    fn ledger_path(&self, thread_id: &str) -> PathBuf {
        self.dir.join(format!("{thread_id}.ledger.jsonl"))
    }

    pub fn set_limit(&self, thread_id: &str, limit_usd: f64) -> Result<Budget, Error> {
        if limit_usd < 0.0 {
            return Err(Error::Validation("limit_usd must be >= 0".into()));
        }
        let snapshot = {
            let mut state = self.state.write().expect("budget lock");
            let entry = state.entry(thread_id.to_string()).or_default();
            entry.limit_usd = limit_usd;
            entry.clone()
        };
        self.persist(thread_id, &snapshot)?;
        Ok(snapshot)
    }

    pub fn set_max_concurrent_workers(
        &self,
        thread_id: &str,
        max_concurrent_workers: Option<usize>,
    ) -> Result<Budget, Error> {
        let snapshot = {
            let mut state = self.state.write().expect("budget lock");
            let entry = state.entry(thread_id.to_string()).or_default();
            entry.max_concurrent_workers = max_concurrent_workers;
            entry.clone()
        };
        self.persist(thread_id, &snapshot)?;
        Ok(snapshot)
    }

    pub fn set_spent(&self, thread_id: &str, spent_usd: f64) -> Result<Budget, Error> {
        let snapshot = {
            let mut state = self.state.write().expect("budget lock");
            let entry = state.entry(thread_id.to_string()).or_default();
            entry.spent_usd = spent_usd.max(0.0);
            entry.clone()
        };
        self.persist(thread_id, &snapshot)?;
        Ok(snapshot)
    }

    fn persist(&self, thread_id: &str, b: &Budget) -> Result<(), Error> {
        let path = self.dir.join(format!("{thread_id}.toml"));
        let text = toml_edit::ser::to_string_pretty(b).map_err(|e| Error::Toml(e.to_string()))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

pub fn observation_from_session(
    s: &ActiveSession,
    cost: &SessionCost,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> BudgetObservation {
    BudgetObservation {
        thread_id: s.thread_id.clone(),
        session_id: s.session_id.clone(),
        task_id: None,
        agent_id: s.agent_id.clone().unwrap_or_else(|| "unknown".into()),
        role: s.role.clone().unwrap_or_else(|| "generator".into()),
        kind: s.kind.clone(),
        model: cost.model.clone(),
        cost_usd: cost.cost_usd.max(0.0),
        input_tokens: cost.usage.input_tokens,
        output_tokens: cost.usage.output_tokens,
        cache_read_tokens: cost.usage.cache_read_tokens,
        cache_write_5m_tokens: cost.usage.cache_write_5m_tokens,
        cache_write_1h_tokens: cost.usage.cache_write_1h_tokens,
        observed_at,
        owner_session_id: None,
        parent_session_id: None,
        root_session_id: None,
    }
}

fn same_observation_value(a: &BudgetObservation, b: &BudgetObservation) -> bool {
    a.thread_id == b.thread_id
        && a.session_id == b.session_id
        && a.task_id == b.task_id
        && a.agent_id == b.agent_id
        && a.role == b.role
        && a.kind == b.kind
        && a.model == b.model
        && (a.cost_usd - b.cost_usd).abs() < f64::EPSILON
        && a.input_tokens == b.input_tokens
        && a.output_tokens == b.output_tokens
        && a.cache_read_tokens == b.cache_read_tokens
        && a.cache_write_5m_tokens == b.cache_write_5m_tokens
        && a.cache_write_1h_tokens == b.cache_write_1h_tokens
        && a.owner_session_id == b.owner_session_id
        && a.parent_session_id == b.parent_session_id
        && a.root_session_id == b.root_session_id
}

fn task_bucket(task_id: Option<&str>) -> String {
    task_id
        .filter(|id| !id.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn aggregate_breakdown<I>(observations: I) -> BudgetBreakdown
where
    I: IntoIterator<Item = BudgetObservation>,
{
    let mut agents: HashMap<(String, String), AgentCost> = HashMap::new();
    let mut tasks: HashMap<String, TaskCost> = HashMap::new();
    let mut roles: HashMap<String, RoleCost> = HashMap::new();
    let mut sessions = Vec::new();
    let mut spent_usd = 0.0;
    let mut agent_sessions: HashMap<(String, String), HashSet<String>> = HashMap::new();
    let mut task_sessions: HashMap<String, HashSet<String>> = HashMap::new();
    let mut role_sessions: HashMap<String, HashSet<String>> = HashMap::new();

    for obs in observations {
        let cost = obs.cost_usd.max(0.0);
        spent_usd += cost;
        let agent_key = (obs.agent_id.clone(), obs.role.clone());
        let agent = agents
            .entry(agent_key.clone())
            .or_insert_with(|| AgentCost {
                agent_id: obs.agent_id.clone(),
                role: obs.role.clone(),
                sessions: 0,
                spent_usd: 0.0,
            });
        agent.spent_usd += cost;
        agent_sessions
            .entry(agent_key)
            .or_default()
            .insert(obs.session_id.clone());

        let bucket = task_bucket(obs.task_id.as_deref());
        let task = tasks.entry(bucket.clone()).or_insert_with(|| TaskCost {
            task_id: obs.task_id.clone(),
            task_bucket: bucket.clone(),
            sessions: 0,
            spent_usd: 0.0,
        });
        task.spent_usd += cost;
        task_sessions
            .entry(bucket.clone())
            .or_default()
            .insert(obs.session_id.clone());

        let role = roles.entry(obs.role.clone()).or_insert_with(|| RoleCost {
            role: obs.role.clone(),
            sessions: 0,
            spent_usd: 0.0,
        });
        role.spent_usd += cost;
        role_sessions
            .entry(obs.role.clone())
            .or_default()
            .insert(obs.session_id.clone());

        sessions.push(SessionCostView {
            session_id: obs.session_id,
            task_bucket: bucket,
            task_id: obs.task_id,
            agent_id: obs.agent_id,
            role: obs.role,
            kind: obs.kind,
            model: obs.model,
            spent_usd: cost,
            input_tokens: obs.input_tokens,
            output_tokens: obs.output_tokens,
            cache_read_tokens: obs.cache_read_tokens,
            cache_write_5m_tokens: obs.cache_write_5m_tokens,
            cache_write_1h_tokens: obs.cache_write_1h_tokens,
            observed_at: obs.observed_at,
        });
    }

    for (key, ids) in agent_sessions {
        if let Some(agent) = agents.get_mut(&key) {
            agent.sessions = ids.len();
        }
    }
    for (key, ids) in task_sessions {
        if let Some(task) = tasks.get_mut(&key) {
            task.sessions = ids.len();
        }
    }
    for (key, ids) in role_sessions {
        if let Some(role) = roles.get_mut(&key) {
            role.sessions = ids.len();
        }
    }

    let mut agents = agents.into_values().collect::<Vec<_>>();
    agents.sort_by(|a, b| {
        b.spent_usd
            .partial_cmp(&a.spent_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.agent_id.cmp(&b.agent_id))
            .then_with(|| a.role.cmp(&b.role))
    });
    let mut tasks = tasks.into_values().collect::<Vec<_>>();
    tasks.sort_by(|a, b| {
        b.spent_usd
            .partial_cmp(&a.spent_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.task_bucket.cmp(&b.task_bucket))
    });
    let mut roles = roles.into_values().collect::<Vec<_>>();
    roles.sort_by(|a, b| {
        b.spent_usd
            .partial_cmp(&a.spent_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.role.cmp(&b.role))
    });
    sessions.sort_by(|a, b| {
        b.spent_usd
            .partial_cmp(&a.spent_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    BudgetBreakdown {
        spent_usd,
        agents,
        tasks,
        roles,
        sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    fn obs(session_id: &str, task_id: Option<&str>, cost_usd: f64) -> BudgetObservation {
        BudgetObservation {
            thread_id: "thr-1".into(),
            session_id: session_id.into(),
            task_id: task_id.map(str::to_string),
            agent_id: "agent-a".into(),
            role: "generator".into(),
            kind: "claude".into(),
            model: "claude-sonnet".into(),
            cost_usd,
            input_tokens: (cost_usd * 1000.0) as u64,
            output_tokens: 10,
            cache_read_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            observed_at: Utc.with_ymd_and_hms(2026, 6, 4, 12, 0, 0).unwrap(),
            owner_session_id: None,
            parent_session_id: None,
            root_session_id: None,
        }
    }

    fn ledger_lines(home: &Path, thread_id: &str) -> usize {
        let path = home
            .join("profiles")
            .join("default")
            .join("budgets")
            .join(format!("{thread_id}.ledger.jsonl"));
        fs::read_to_string(path).unwrap().lines().count()
    }

    #[test]
    fn defaults_are_safe() {
        let b = Budget::default();
        assert_eq!(b.soft_pct, 80);
        assert_eq!(b.hard_pct, 100);
        assert_eq!(b.max_concurrent_workers, None);
        assert!(!b.over_soft());
        assert!(!b.over_hard());
    }

    #[test]
    fn thresholds_trigger_at_pct() {
        let b = Budget {
            limit_usd: 10.0,
            spent_usd: 8.0,
            soft_pct: 80,
            hard_pct: 100,
            max_concurrent_workers: None,
        };
        assert!(b.over_soft());
        assert!(!b.over_hard());

        let b2 = Budget {
            limit_usd: 10.0,
            spent_usd: 10.5,
            soft_pct: 80,
            hard_pct: 100,
            max_concurrent_workers: None,
        };
        assert!(b2.over_hard());
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();

        let b = s.set_limit("thr-1", 5.0).unwrap();
        assert_eq!(b.limit_usd, 5.0);
        let b2 = s.set_spent("thr-1", 2.0).unwrap();
        assert_eq!(b2.limit_usd, 5.0);
        assert_eq!(b2.spent_usd, 2.0);

        let s2 = BudgetStore::load(dir.path()).unwrap();
        let got = s2.get("thr-1");
        assert_eq!(got.limit_usd, 5.0);
        assert_eq!(got.spent_usd, 2.0);
    }

    #[test]
    fn max_concurrent_workers_round_trips_through_disk() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();

        let b = s.set_max_concurrent_workers("thr-1", Some(2)).unwrap();
        assert_eq!(b.max_concurrent_workers, Some(2));

        let s2 = BudgetStore::load(dir.path()).unwrap();
        let got = s2.get("thr-1");
        assert_eq!(got.max_concurrent_workers, Some(2));

        let cleared = s2.set_max_concurrent_workers("thr-1", None).unwrap();
        assert_eq!(cleared.max_concurrent_workers, None);
    }

    #[test]
    fn unknown_thread_returns_default() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        let got = s.get("nope");
        assert_eq!(got, Budget::default());
    }

    #[test]
    fn agents_for_returns_empty_when_unset() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        assert!(s.agents_for("thr-1").is_empty());
    }

    #[test]
    fn agents_breakdown_round_trips_in_memory() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        let agents = vec![AgentCost {
            agent_id: "a1".into(),
            role: "generator".into(),
            sessions: 2,
            spent_usd: 1.25,
        }];

        s.set_agents_breakdown("thr-1", agents.clone());

        assert_eq!(s.agents_for("thr-1"), agents);
    }

    #[test]
    fn ledger_round_trips_latest_sessions_through_disk() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();

        s.record_observation("thr-1", obs("sid-1", Some("task-1"), 1.25))
            .unwrap();
        s.record_observation("thr-1", obs("sid-2", Some("task-2"), 2.5))
            .unwrap();

        let s2 = BudgetStore::load(dir.path()).unwrap();
        let breakdown = s2.breakdown_for("thr-1");
        assert_eq!(breakdown.sessions.len(), 2);
        assert!((s2.get("thr-1").spent_usd - 3.75).abs() < f64::EPSILON);
        assert_eq!(breakdown.tasks.len(), 2);
        assert_eq!(breakdown.agents[0].sessions, 2);
    }

    #[test]
    fn repeat_poll_same_session_does_not_duplicate() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        let mut first = obs("sid-1", Some("task-1"), 1.25);
        let mut repeated = first.clone();
        repeated.observed_at = Utc.with_ymd_and_hms(2026, 6, 4, 12, 1, 0).unwrap();

        s.record_observation("thr-1", first.clone()).unwrap();
        s.record_observation("thr-1", repeated).unwrap();
        assert_eq!(ledger_lines(dir.path(), "thr-1"), 1);
        assert!((s.breakdown_for("thr-1").spent_usd - 1.25).abs() < f64::EPSILON);

        first.cost_usd = 1.5;
        first.input_tokens = 1500;
        s.record_observation("thr-1", first).unwrap();
        assert_eq!(ledger_lines(dir.path(), "thr-1"), 2);
        assert!((s.breakdown_for("thr-1").spent_usd - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn two_sessions_same_task_sum() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        s.record_observations(
            "thr-1",
            vec![
                obs("sid-1", Some("task-1"), 1.25),
                obs("sid-2", Some("task-1"), 2.5),
            ],
        )
        .unwrap();

        let breakdown = s.breakdown_for("thr-1");
        assert_eq!(breakdown.tasks.len(), 1);
        assert_eq!(breakdown.tasks[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(breakdown.tasks[0].task_bucket, "task-1");
        assert_eq!(breakdown.tasks[0].sessions, 2);
        assert!((breakdown.tasks[0].spent_usd - 3.75).abs() < f64::EPSILON);
    }

    #[test]
    fn no_task_goes_unknown_thread_only() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        s.record_observation("thr-1", obs("sid-1", None, 1.25))
            .unwrap();

        let breakdown = s.breakdown_for("thr-1");
        assert_eq!(breakdown.tasks.len(), 1);
        assert_eq!(breakdown.tasks[0].task_id, None);
        assert_eq!(breakdown.tasks[0].task_bucket, "unknown");
        assert_eq!(breakdown.sessions[0].task_id, None);
        assert_eq!(breakdown.sessions[0].task_bucket, "unknown");
        assert!((s.get("thr-1").spent_usd - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn negative_limit_rejected() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        assert!(s.set_limit("thr", -1.0).is_err());
    }

    #[test]
    fn set_limit_then_spent_then_over_transitions() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        s.set_limit("t", 10.0).unwrap();

        // 50% — below both thresholds.
        let b = s.set_spent("t", 5.0).unwrap();
        assert!(!b.over_soft());
        assert!(!b.over_hard());

        // 80% — over soft, under hard.
        let b = s.set_spent("t", 8.0).unwrap();
        assert!(b.over_soft());
        assert!(!b.over_hard());

        // 100% — over both.
        let b = s.set_spent("t", 10.0).unwrap();
        assert!(b.over_soft());
        assert!(b.over_hard());
    }
}
