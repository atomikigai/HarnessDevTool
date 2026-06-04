use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Cursor,
    Antigravity,
    Generic,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Antigravity => "antigravity",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Agent {
    pub id: String,
    pub kind: AgentKind,
    pub label: String,
    pub created_at: DateTime<Utc>,
    /// Free-form role tag. Standard values are "planner", "generator",
    /// "evaluator", but users may define custom roles. `None` is treated as
    /// "generator" by the scheduler for back-compat with legacy registries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDraft {
    pub kind: AgentKind,
    pub label: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    agents: Vec<Agent>,
    #[serde(default)]
    counter: u32,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for RegistryFile {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            agents: vec![],
            counter: 0,
        }
    }
}

#[derive(Clone)]
pub struct AgentsRegistry {
    path: PathBuf,
    state: Arc<Mutex<RegistryFile>>,
}

impl AgentsRegistry {
    /// Backwards-compatible constructor: uses the `"default"` profile.
    pub fn new(home: &Path) -> Result<Self, Error> {
        Self::with_profile(home, "default")
    }

    /// Open the agents registry for a specific profile (workspace).
    pub fn with_profile(home: &Path, profile: &str) -> Result<Self, Error> {
        let dir = home.join("profiles").join(profile).join("agents");
        fs::create_dir_all(&dir)?;
        let path = dir.join("registry.toml");
        let state = if path.exists() {
            let text = fs::read_to_string(&path)?;
            toml_edit::de::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?
        } else {
            RegistryFile::default()
        };
        Ok(Self {
            path,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn list(&self) -> Vec<Agent> {
        self.state.lock().expect("agents mutex").agents.clone()
    }

    pub fn create(&self, draft: AgentDraft) -> Result<Agent, Error> {
        let mut st = self.state.lock().expect("agents mutex");
        st.counter += 1;
        let id = format!("agent:{}-{}", draft.kind.as_str(), st.counter);
        let agent = Agent {
            id,
            kind: draft.kind,
            label: draft.label,
            created_at: Utc::now(),
            role: draft.role,
        };
        st.agents.push(agent.clone());
        let text =
            toml_edit::ser::to_string_pretty(&*st).map_err(|e| Error::Toml(e.to_string()))?;
        let tmp = self.path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &self.path)?;
        Ok(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn role_roundtrips_via_disk() {
        let dir = tempdir().unwrap();
        let reg = AgentsRegistry::new(dir.path()).unwrap();
        let created = reg
            .create(AgentDraft {
                kind: AgentKind::Claude,
                label: "eval-1".into(),
                role: Some("evaluator".into()),
            })
            .unwrap();
        assert_eq!(created.role.as_deref(), Some("evaluator"));

        // Reload from disk picks up the role.
        let reg2 = AgentsRegistry::new(dir.path()).unwrap();
        let all = reg2.list();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, created.id);
        assert_eq!(all[0].role.as_deref(), Some("evaluator"));
    }

    #[test]
    fn legacy_registry_without_role_loads() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profiles/default/agents");
        fs::create_dir_all(&path).unwrap();
        // Pre-write a legacy registry (no schema_version, no role).
        fs::write(
            path.join("registry.toml"),
            r#"counter = 1
[[agents]]
id = "agent:claude-1"
kind = "claude"
label = "legacy"
created_at = "2025-01-01T00:00:00Z"
"#,
        )
        .unwrap();
        let reg = AgentsRegistry::new(dir.path()).unwrap();
        let all = reg.list();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role, None);
    }
}
