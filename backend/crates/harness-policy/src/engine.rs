use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

use crate::{PolicyResult, Rule};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    #[default]
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "snake_case")]
pub enum RememberScope {
    ThisCall,
    ToolOnly,
    ToolAndArgs,
}

pub struct PolicyEngine {
    path: PathBuf,
    state: RwLock<PolicyFile>,
    write_lock: Mutex<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyFile {
    #[serde(default = "default_decision")]
    pub default: Decision,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Default for PolicyFile {
    fn default() -> Self {
        Self {
            default: default_decision(),
            timeout_secs: default_timeout(),
            rules: Vec::new(),
        }
    }
}

fn default_decision() -> Decision {
    Decision::Allow
}

fn default_timeout() -> u64 {
    60
}

impl PolicyEngine {
    pub fn load(path: PathBuf) -> PolicyResult<Self> {
        let state = read_policy_file(&path)?;
        Ok(Self::from_state(path, state))
    }

    pub fn default_at(path: PathBuf) -> Self {
        Self::from_state(path, PolicyFile::default())
    }

    fn from_state(path: PathBuf, state: PolicyFile) -> Self {
        Self {
            path,
            state: RwLock::new(state),
            write_lock: Mutex::new(()),
        }
    }

    pub fn evaluate(&self, tool: &str, args: &serde_json::Value, role: Option<&str>) -> Decision {
        self.evaluate_rule(tool, args, role).unwrap_or_else(|| {
            let state = self.state.read().expect("policy state rwlock");
            capability_default(tool, role)
                .unwrap_or_else(|| fallback_decision(tool, &state.default))
        })
    }

    pub fn evaluate_rule(
        &self,
        tool: &str,
        args: &serde_json::Value,
        role: Option<&str>,
    ) -> Option<Decision> {
        let state = self.state.read().expect("policy state rwlock");
        state
            .rules
            .iter()
            .find(|rule| rule.matches(tool, args, role))
            .map(|rule| rule.decision.clone())
    }

    pub fn timeout_secs(&self) -> u64 {
        self.state.read().expect("policy state rwlock").timeout_secs
    }

    pub fn append_rule(&self, rule: Rule) -> PolicyResult<()> {
        let _guard = self.write_lock.lock().expect("policy write mutex");
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let mut doc = if text.trim().is_empty() {
            DocumentMut::new()
        } else {
            text.parse::<DocumentMut>()?
        };

        if !doc.as_table().contains_key("rules") {
            doc["rules"] = Item::ArrayOfTables(ArrayOfTables::new());
        }
        let rules = doc["rules"]
            .as_array_of_tables_mut()
            .expect("rules should be an array of tables");
        let mut table = Table::new();
        table["tool"] = value(rule.tool);
        if let Some(role) = rule.role {
            table["role"] = value(role);
        }
        table["decision"] = value(match rule.decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
            Decision::Ask => "ask",
        });
        if let Some(created_at) = rule.created_at {
            table["created_at"] = value(created_at);
        }
        if let Some(created_by) = rule.created_by {
            table["created_by"] = value(created_by);
        }
        if let Some(args_hash) = rule.args_hash {
            table["args_hash"] = value(args_hash);
        }
        if !rule.args_match.is_empty() {
            let mut args = Table::new();
            for (key, pattern) in rule.args_match {
                args[&key] = value(pattern);
            }
            table["args_match"] = Item::Table(args);
        }
        rules.push(table);
        fs::write(&self.path, doc.to_string())?;

        let reloaded = read_policy_file(&self.path)?;
        *self.state.write().expect("policy state rwlock") = reloaded;
        Ok(())
    }
}

pub fn capability_default(tool: &str, role: Option<&str>) -> Option<Decision> {
    match role.map(|role| role.to_ascii_lowercase()).as_deref() {
        Some("planner") if matches!(tool, "task_claim" | "task_release") => Some(Decision::Deny),
        Some("planner" | "orchestrator") => None,
        Some("worker" | "generator")
            if matches!(tool, "task_create" | "spec_write" | "spec_set_section") =>
        {
            Some(Decision::Deny)
        }
        Some("worker" | "generator") => None,
        Some("evaluator") if is_sensitive_tool(tool) => Some(Decision::Deny),
        Some("evaluator") => None,
        Some(_) | None => None,
    }
}

fn fallback_decision(tool: &str, default: &Decision) -> Decision {
    if is_sensitive_tool(tool) {
        Decision::Ask
    } else {
        default.clone()
    }
}

pub fn is_sensitive_tool(tool: &str) -> bool {
    matches!(
        tool,
        "task_create"
            | "task_propose"
            | "task_claim"
            | "task_renew"
            | "task_update"
            | "task_release"
            | "task_submit"
            | "spec_write"
            | "spec_set_section"
            | "repo_write_file"
            | "repo_git_create_branch"
            | "repo_git_commit"
            | "repo_git_push"
            | "repo_github_pr_create"
            | "knowledge_pdf_ingest"
            | "knowledge_office_ingest"
            | "skill_propose"
            | "skill_promote"
            | "skill_archive"
            | "evolve_run"
            | "curator_run"
            | "docs_build"
            | "db_query"
            | "db_backup"
            | "db_memory_write"
            | "ssh_exec"
            | "sftp_get"
            | "sftp_put"
            | "sftp_mkdir"
            | "sftp_rmdir"
            | "sftp_unlink"
            | "sftp_rename"
            | "session_spawn_child"
            | "session_send_input"
            | "session_cancel_child"
    )
}

fn read_policy_file(path: &PathBuf) -> PolicyResult<PolicyFile> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(toml_edit::de::from_str::<PolicyFile>(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PolicyFile::default()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "harness-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn evaluate_default_allows_read_only_tools_when_no_rules() {
        let engine = PolicyEngine::load(tmp_path("missing.toml")).unwrap();
        assert_eq!(
            engine.evaluate("task_list", &json!({}), None),
            Decision::Allow
        );
    }

    #[test]
    fn evaluate_default_asks_for_sensitive_tools_when_no_rules() {
        let engine = PolicyEngine::load(tmp_path("missing.toml")).unwrap();

        for tool in [
            "task_create",
            "task_propose",
            "task_update",
            "spec_write",
            "db_query",
            "db_backup",
            "db_memory_write",
            "repo_git_create_branch",
            "repo_git_commit",
            "repo_git_push",
            "repo_github_pr_create",
            "session_spawn_child",
            "session_send_input",
            "session_cancel_child",
        ] {
            assert_eq!(
                engine.evaluate(tool, &json!({}), None),
                Decision::Ask,
                "{tool}"
            );
        }
    }

    #[test]
    fn explicit_rule_can_allow_sensitive_tool() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "db_query"
decision = "allow"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(
            engine.evaluate("db_query", &json!({}), None),
            Decision::Allow
        );
    }

    #[test]
    fn evaluate_first_matching_rule_wins() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "db_query"
decision = "ask"

[[rules]]
tool = "db_query"
decision = "deny"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(engine.evaluate("db_query", &json!({}), None), Decision::Ask);
    }

    #[test]
    fn evaluate_args_match_glob_prefix_suffix() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "spec_write"
decision = "deny"

[rules.args_match]
path = "docs/*md"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(
            engine.evaluate("spec_write", &json!({ "path": "docs/readme.md" }), None),
            Decision::Deny
        );
        assert_eq!(
            engine.evaluate("spec_write", &json!({ "path": "src/readme.md" }), None),
            Decision::Ask
        );
    }

    #[test]
    fn evaluate_args_match_missing_key_means_no_match() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "spec_write"
decision = "deny"

[rules.args_match]
path = "*secret*"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(
            engine.evaluate("spec_write", &json!({}), None),
            Decision::Ask
        );
    }

    #[test]
    fn append_rule_creates_file_if_missing() {
        let path = tmp_path("policy.toml");
        let engine = PolicyEngine::load(path.clone()).unwrap();
        engine
            .append_rule(Rule {
                tool: "db_query".to_string(),
                role: None,
                args_match: BTreeMap::new(),
                decision: Decision::Ask,
                created_at: None,
                created_by: None,
                args_hash: None,
            })
            .unwrap();
        assert!(path.exists());
        assert_eq!(engine.evaluate("db_query", &json!({}), None), Decision::Ask);
    }

    #[test]
    fn append_rule_preserves_existing_content() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"# keep this comment
default = "allow"

[[rules]]
tool = "task_list"
decision = "allow"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path.clone()).unwrap();
        engine
            .append_rule(Rule {
                tool: "db_query".to_string(),
                role: None,
                args_match: BTreeMap::new(),
                decision: Decision::Deny,
                created_at: Some("2026-06-04T00:00:00Z".to_string()),
                created_by: Some("human".to_string()),
                args_hash: Some("sha256:test".to_string()),
            })
            .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep this comment"));
        assert!(text.contains("tool = \"task_list\""));
        assert!(text.contains("tool = \"db_query\""));
        assert!(text.contains("created_by = \"human\""));
        assert!(text.contains("args_hash = \"sha256:test\""));
    }

    #[test]
    fn capability_default_applies_role_matrix() {
        assert_eq!(capability_default("task_create", Some("planner")), None);
        assert_eq!(
            capability_default("task_claim", Some("planner")),
            Some(Decision::Deny)
        );
        assert_eq!(
            capability_default("task_create", Some("orchestrator")),
            None
        );
        assert_eq!(
            capability_default("task_create", Some("worker")),
            Some(Decision::Deny)
        );
        assert_eq!(
            capability_default("spec_write", Some("generator")),
            Some(Decision::Deny)
        );
        assert_eq!(
            capability_default("spec_set_section", Some("generator")),
            Some(Decision::Deny)
        );
        assert_eq!(
            capability_default("db_query", Some("evaluator")),
            Some(Decision::Deny)
        );
        assert_eq!(capability_default("task_list", Some("evaluator")), None);
        assert_eq!(capability_default("task_create", None), None);
        assert_eq!(capability_default("task_create", Some("unknown")), None);
    }

    #[test]
    fn role_specific_rule_matches_case_insensitively() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "task_create"
role = "Worker"
decision = "allow"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(
            engine.evaluate("task_create", &json!({}), Some("worker")),
            Decision::Allow
        );
        assert_eq!(
            engine.evaluate("task_create", &json!({}), Some("generator")),
            Decision::Deny
        );
        assert_eq!(
            engine.evaluate("task_create", &json!({}), None),
            Decision::Ask
        );
    }

    #[test]
    fn global_rule_still_matches_any_role() {
        let path = tmp_path("policy.toml");
        fs::write(
            &path,
            r#"
[[rules]]
tool = "spec_write"
decision = "allow"
"#,
        )
        .unwrap();
        let engine = PolicyEngine::load(path).unwrap();
        assert_eq!(
            engine.evaluate("spec_write", &json!({}), Some("worker")),
            Decision::Allow
        );
    }
}
