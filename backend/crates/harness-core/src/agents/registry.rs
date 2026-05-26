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
    Generic,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDraft {
    pub kind: AgentKind,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    agents: Vec<Agent>,
    #[serde(default)]
    counter: u32,
}

#[derive(Clone)]
pub struct AgentsRegistry {
    path: PathBuf,
    state: Arc<Mutex<RegistryFile>>,
}

impl AgentsRegistry {
    pub fn new(home: &Path) -> Result<Self, Error> {
        let dir = home.join("profiles/default/agents");
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
