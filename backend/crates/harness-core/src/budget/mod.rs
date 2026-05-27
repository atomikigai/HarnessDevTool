//! Per-thread budget tracking. Schema v1.
//!
//! Storage: `<home>/profiles/default/budgets/<thread_id>.toml`. Each file is
//! self-contained (no global aggregate) so concurrent thread writes don't
//! contend on a single lock. Atomic temp+rename on every persist.

use std::collections::HashMap;
use std::fs;
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
pub use reporter::{ClaudeTranscriptReporter, CodexStubReporter, CostReporter, SessionCost, Usage};

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
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            limit_usd: 0.0,
            spent_usd: 0.0,
            soft_pct: 80,
            hard_pct: 100,
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
}

impl BudgetStore {
    pub fn load(home: &Path) -> Result<Self, Error> {
        let dir = home.join("profiles/default/budgets");
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
        Ok(Self {
            dir,
            state: Arc::new(RwLock::new(map)),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_are_safe() {
        let b = Budget::default();
        assert_eq!(b.soft_pct, 80);
        assert_eq!(b.hard_pct, 100);
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
        };
        assert!(b.over_soft());
        assert!(!b.over_hard());

        let b2 = Budget {
            limit_usd: 10.0,
            spent_usd: 10.5,
            soft_pct: 80,
            hard_pct: 100,
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
    fn unknown_thread_returns_default() {
        let dir = tempdir().unwrap();
        let s = BudgetStore::load(dir.path()).unwrap();
        let got = s.get("nope");
        assert_eq!(got, Budget::default());
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
