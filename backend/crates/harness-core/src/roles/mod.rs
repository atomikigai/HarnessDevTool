//! Role templates — per-profile reusable agent personas (planner / generator /
//! evaluator / …). Each role bundles a prompt template and a tool allow/deny
//! list; the sessions route looks them up by name to inject an initial prompt
//! into the spawned PTY.
//!
//! Storage: `<home>/profiles/default/roles/*.toml`. On load we materialize any
//! missing baseline templates so the user has a starting point without
//! overwriting customized roles.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::agents::AgentKind;
use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Role {
    pub name: String,
    pub cli: AgentKind,
    pub prompt_template: String,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
}

#[derive(Clone)]
pub struct RolesRegistry {
    #[allow(dead_code)]
    dir: PathBuf,
    state: Arc<Mutex<Vec<Role>>>,
}

impl RolesRegistry {
    /// Scan `<home>/profiles/default/roles/*.toml` and write any missing
    /// baseline templates. Kept for backwards compatibility with tests; prefer
    /// [`Self::load_for_profile`].
    pub fn load(home: &Path) -> Result<Self, Error> {
        Self::load_for_profile(home, "default")
    }

    /// Load roles for a specific profile (workspace).
    pub fn load_for_profile(home: &Path, profile: &str) -> Result<Self, Error> {
        let dir = home.join("profiles").join(profile).join("roles");
        fs::create_dir_all(&dir)?;

        let mut roles = read_roles(&dir)?;
        materialize_missing_baseline_roles(&dir, &mut roles)?;

        Ok(Self {
            dir,
            state: Arc::new(Mutex::new(roles)),
        })
    }

    pub fn get(&self, name: &str) -> Option<Role> {
        self.state
            .lock()
            .expect("roles mutex")
            .iter()
            .find(|r| r.name == name)
            .cloned()
    }

    pub fn list(&self) -> Vec<Role> {
        self.state.lock().expect("roles mutex").clone()
    }
}

fn read_roles(dir: &Path) -> Result<Vec<Role>, Error> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        match toml_edit::de::from_str::<Role>(&text) {
            Ok(r) => out.push(r),
            Err(e) => {
                tracing::warn!(?path, ?e, "skipping invalid role TOML");
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn materialize_missing_baseline_roles(dir: &Path, roles: &mut Vec<Role>) -> Result<(), Error> {
    for r in baseline_roles() {
        if roles.iter().any(|existing| existing.name == r.name) {
            continue;
        }

        let path = dir.join(format!("{}.toml", r.name));
        if path.exists() {
            continue;
        }

        let text = toml_edit::ser::to_string_pretty(&r).map_err(|e| Error::Toml(e.to_string()))?;
        fs::write(&path, text)?;
        roles.push(r);
    }
    roles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(())
}

fn baseline_roles() -> Vec<Role> {
    vec![
        Role {
            name: "planner".into(),
            cli: AgentKind::Claude,
            prompt_template:
                "You are the planner. Read spec.md and create tasks via task.* MCP tools.".into(),
            enabled_tools: vec!["task.*".into(), "spec.*".into()],
            disabled_tools: vec![],
        },
        Role {
            name: "generator".into(),
            cli: AgentKind::Claude,
            prompt_template:
                "You are the generator. Claim a task and submit artifacts via task.* MCP tools."
                    .into(),
            enabled_tools: vec!["task.*".into(), "spec.read".into(), "artifact.*".into()],
            disabled_tools: vec![],
        },
        Role {
            name: "evaluator".into(),
            cli: AgentKind::Claude,
            prompt_template:
                "You are the evaluator. Verify submitted artifacts against acceptance checks."
                    .into(),
            enabled_tools: vec!["task.*".into(), "spec.read".into(), "artifact.read".into()],
            disabled_tools: vec![],
        },
        Role {
            name: "frontend-visual".into(),
            cli: AgentKind::Cursor,
            prompt_template:
                "You are the frontend visual worker. Focus on Svelte views, CSS, layout, responsive behavior, polish and visual accessibility."
                    .into(),
            enabled_tools: vec!["task.*".into(), "spec.read".into(), "artifact.*".into()],
            disabled_tools: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_writes_baseline_when_empty() {
        let dir = tempdir().unwrap();
        let reg = RolesRegistry::load(dir.path()).unwrap();
        let names: Vec<String> = reg.list().into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"planner".to_string()));
        assert!(names.contains(&"generator".to_string()));
        assert!(names.contains(&"evaluator".to_string()));
        assert!(names.contains(&"frontend-visual".to_string()));

        // Files exist on disk.
        let roles_dir = dir.path().join("profiles/default/roles");
        assert!(roles_dir.join("planner.toml").exists());
        assert!(roles_dir.join("generator.toml").exists());
        assert!(roles_dir.join("evaluator.toml").exists());
        assert!(roles_dir.join("frontend-visual.toml").exists());

        // Reload reads from disk without re-creating.
        let reg2 = RolesRegistry::load(dir.path()).unwrap();
        assert_eq!(reg2.list().len(), 4);

        // get() round trips.
        let g = reg2.get("generator").unwrap();
        assert!(g.prompt_template.contains("generator"));
        assert!(reg2.get("nope").is_none());
    }

    #[test]
    fn load_adds_missing_baseline_roles_without_overwriting_existing_roles() {
        let dir = tempdir().unwrap();
        let roles_dir = dir.path().join("profiles/default/roles");
        fs::create_dir_all(&roles_dir).unwrap();
        fs::write(
            roles_dir.join("generator.toml"),
            r#"
name = "generator"
cli = "codex"
prompt_template = "custom generator"
enabled_tools = ["task.*"]
disabled_tools = []
"#,
        )
        .unwrap();

        let reg = RolesRegistry::load(dir.path()).unwrap();
        assert_eq!(reg.get("generator").unwrap().cli, AgentKind::Codex);
        assert_eq!(
            reg.get("generator").unwrap().prompt_template,
            "custom generator"
        );
        assert_eq!(reg.list().len(), 4);
        assert!(roles_dir.join("planner.toml").exists());
        assert!(roles_dir.join("evaluator.toml").exists());
        assert!(roles_dir.join("frontend-visual.toml").exists());
    }
}
