use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::protocol::ToolDescriptor;

#[derive(Debug, Clone)]
pub struct ToolGroup {
    pub id: &'static str,
    pub description: &'static str,
    pub includes: &'static [&'static str],
    pub tools: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct ToolRegistry {
    groups: HashMap<&'static str, ToolGroup>,
    descriptors: HashMap<String, ToolDescriptor>,
    tool_groups: HashMap<String, &'static str>,
}

impl ToolRegistry {
    pub fn new(descriptors: Vec<ToolDescriptor>) -> Self {
        let descriptors: HashMap<_, _> = descriptors
            .into_iter()
            .map(|descriptor| (descriptor.name.clone(), descriptor))
            .collect();
        let groups: HashMap<_, _> = groups()
            .into_iter()
            .map(|group| (group.id, group))
            .collect();
        let mut tool_groups = HashMap::new();
        for group in groups.values() {
            for tool in group.tools {
                tool_groups.insert((*tool).to_string(), group.id);
            }
        }
        Self {
            groups,
            descriptors,
            tool_groups,
        }
    }

    pub fn canonical_group<'a>(&'a self, group: &'a str) -> Option<&'a str> {
        let group = group.trim();
        match group {
            "data_loader" | "document_extract" | "project_memory" | "docs_web" => Some("knowledge"),
            "repo_write" => Some("repo"),
            "code_graph" | "repo_graph" => Some("code_graph"),
            "context" | "ledger" | "memory" | "memory_runtime" => Some("context"),
            "docs_build" => Some("docs"),
            "sessions" | "agent_builtin" | "core" => Some("core"),
            "plan" | "planner" | "planning" => Some("planning"),
            "repo" => Some("repo"),
            "knowledge" => Some("knowledge"),
            "db" => Some("db"),
            "ssh" => Some("ssh"),
            "n8n" | "workflow_automation" | "automations" => Some("n8n"),
            "skills" => Some("skills"),
            "docs" => Some("docs"),
            other if self.groups.contains_key(other) => Some(other),
            _ => None,
        }
    }

    pub fn group_for_tool(&self, tool: &str) -> Option<&'static str> {
        self.tool_groups.get(tool).copied()
    }

    pub fn visible_descriptors(
        &self,
        active_groups: &HashSet<String>,
    ) -> Result<Vec<ToolDescriptor>, String> {
        let mut groups = vec!["core".to_string()];
        let mut extra: Vec<_> = active_groups.iter().cloned().collect();
        extra.sort();
        groups.extend(extra);

        let mut tool_names = Vec::new();
        let mut seen_tools = HashSet::new();
        for group in groups {
            for tool in self.resolve_group_tools(&group)? {
                if seen_tools.insert(tool.to_string()) {
                    tool_names.push(tool.to_string());
                }
            }
        }

        let mut descriptors = Vec::new();
        for name in tool_names {
            if let Some(descriptor) = self.descriptors.get(&name) {
                descriptors.push(descriptor.clone());
            }
        }
        Ok(descriptors)
    }

    pub fn search(&self, active_groups: &HashSet<String>, query: &str) -> Value {
        let query = normalize_search_text(query);
        let mut active: Vec<_> = active_groups.iter().cloned().collect();
        active.sort();

        let mut group_hits = Vec::new();
        let mut groups: Vec<_> = self.groups.values().collect();
        groups.sort_by_key(|group| group.id);
        for group in groups {
            if group.id == "core" {
                continue;
            }
            let haystack = format!(
                "{} {} {}",
                group.id,
                group.description,
                group.tools.join(" ")
            )
            .to_lowercase();
            if query.is_empty() || haystack.contains(&query) || fuzzy_words_match(&haystack, &query)
            {
                group_hits.push(json!({
                    "group": group.id,
                    "description": group.description,
                    "loaded": active_groups.contains(group.id),
                    "includes": group.includes,
                    "tools": group.tools,
                }));
            }
        }

        let mut tool_hits = Vec::new();
        let mut tools: Vec<_> = self.descriptors.values().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        for descriptor in tools {
            let Some(group) = self.group_for_tool(&descriptor.name) else {
                continue;
            };
            let haystack =
                format!("{} {} {}", descriptor.name, descriptor.description, group).to_lowercase();
            if query.is_empty() || haystack.contains(&query) || fuzzy_words_match(&haystack, &query)
            {
                tool_hits.push(json!({
                    "name": descriptor.name,
                    "group": group,
                    "loaded": group == "core" || active_groups.contains(group),
                    "description": descriptor.description,
                }));
            }
        }

        json!({
            "active_groups": active,
            "groups": group_hits,
            "tools": tool_hits,
        })
    }

    fn resolve_group_tools(&self, group: &str) -> Result<Vec<&'static str>, String> {
        let mut stack = Vec::new();
        let mut seen_groups = HashSet::new();
        let mut seen_tools = HashSet::new();
        let mut tools = Vec::new();
        self.resolve_group_tools_inner(
            group,
            &mut stack,
            &mut seen_groups,
            &mut seen_tools,
            &mut tools,
        )?;
        Ok(tools)
    }

    fn resolve_group_tools_inner(
        &self,
        group: &str,
        stack: &mut Vec<String>,
        seen_groups: &mut HashSet<String>,
        seen_tools: &mut HashSet<&'static str>,
        tools: &mut Vec<&'static str>,
    ) -> Result<(), String> {
        let group = self
            .canonical_group(group)
            .ok_or_else(|| format!("unknown tool group: {group}"))?;
        if stack.iter().any(|item| item == group) {
            stack.push(group.to_string());
            return Err(format!("tool group include cycle: {}", stack.join(" -> ")));
        }
        if !seen_groups.insert(group.to_string()) {
            return Ok(());
        }
        let definition = self
            .groups
            .get(group)
            .ok_or_else(|| format!("unknown tool group: {group}"))?;
        stack.push(group.to_string());
        for include in definition.includes {
            self.resolve_group_tools_inner(include, stack, seen_groups, seen_tools, tools)?;
        }
        stack.pop();
        for tool in definition.tools {
            if seen_tools.insert(*tool) {
                tools.push(*tool);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn from_groups_for_test(groups: Vec<ToolGroup>) -> Self {
        Self {
            groups: groups.into_iter().map(|group| (group.id, group)).collect(),
            descriptors: HashMap::new(),
            tool_groups: HashMap::new(),
        }
    }
}

fn fuzzy_words_match(haystack: &str, query: &str) -> bool {
    query
        .split_whitespace()
        .filter(|word| {
            word.len() >= 3
                && !matches!(*word, "una" | "uno" | "the" | "and" | "for" | "con" | "csv")
        })
        .all(|word| haystack.contains(word))
}

fn normalize_search_text(text: &str) -> String {
    text.to_lowercase()
        .replace("tabla", "table")
        .replace("datos", "data")
        .replace("consulta", "query")
}

fn groups() -> Vec<ToolGroup> {
    vec![
        ToolGroup {
            id: "core",
            description: "Task, spec-read, session tree, mailbox, and tool loading controls.",
            includes: &[],
            tools: &[
                "tools_search",
                "tools_load",
                "tools_unload",
                "attach_list",
                "attach_read",
                "task_create",
                "task_propose",
                "task_list",
                "task_get",
                "task_claim",
                "task_renew",
                "task_update",
                "task_release",
                "task_submit",
                "spec_read",
                "session_spawn_child",
                "session_list_children",
                "session_read_child_summary",
                "session_send_input",
                "session_cancel_child",
                "session_mailbox_send",
                "session_mailbox_list",
                "session_mailbox_ack",
            ],
        },
        ToolGroup {
            id: "repo",
            description: "Workspace inspection, file IO, git operations, and codebase memory status.",
            includes: &[],
            tools: &[
                "repo_analyze",
                "repo_scan",
                "repo_find",
                "repo_read_file",
                "repo_write_file",
                "repo_git_status",
                "repo_git_log",
                "repo_git_diff",
                "repo_git_create_branch",
                "repo_git_commit",
                "repo_git_push",
                "repo_github_pr_create",
                "repo_codebase_memory_status",
                "repo_manifest",
                "repo_symbol_search",
                "repo_related_files",
            ],
        },
        ToolGroup {
            id: "code_graph",
            description: "Optional code graph acceleration and native repo-intelligence fallbacks for symbols, related files, and impact-oriented exploration.",
            includes: &["repo"],
            tools: &["repo_code_graph_status"],
        },
        ToolGroup {
            id: "planning",
            description: "Smart-loading task intake, focused test selection, Harness contract guardrails, and compact review/QA evidence.",
            includes: &[],
            tools: &[
                "planning_pack",
                "test_selector",
                "contract_guard",
                "evidence_pack",
                "task_list_summary",
                "task_next_best",
            ],
        },
        ToolGroup {
            id: "context",
            description: "Compact operational context, handoff, and ledger rails for resuming agent work without transcript replay.",
            includes: &[],
            tools: &[
                "session_context_pack",
                "agent_ledger_list",
                "agent_ledger_get",
                "handoff_latest",
                "session_handoff_submit",
                "context_status",
                "context_search",
                "context_checkpoint_request",
                "timeline_query",
                "transcript_query",
                "transcript_search",
                "transcript_tool_results",
                "memory_search",
                "memory_read",
                "memory_continuity",
                "memory_note_propose",
            ],
        },
        ToolGroup {
            id: "knowledge",
            description: "Document extraction, persistent knowledge, and external documentation upstreams.",
            includes: &[],
            tools: &[
                "knowledge_pdf_ingest",
                "knowledge_office_ingest",
                "knowledge_data_ingest",
                "knowledge_search",
                "db_memory_read",
                "db_memory_write",
            ],
        },
        ToolGroup {
            id: "db",
            description: "Database schema inspection, querying, row operations, export, backup, and performance tools.",
            includes: &["knowledge"],
            tools: &[
                "db_query",
                "db_context_refresh",
                "db_context",
                "db_select",
                "db_validate_query",
                "db_schema",
                "db_table_info",
                "db_search_tables",
                "db_sample",
                "db_count",
                "db_distinct_values",
                "db_find_rows",
                "db_aggregate",
                "db_extract_enriched",
                "db_relation_performance",
                "db_row_insert",
                "db_row_delete",
                "db_row_duplicate",
                "db_export_table",
                "db_export_query",
                "db_generate_view_sql",
                "db_drop_table",
                "db_drop_schema",
                "db_explain",
                "db_performance_audit",
                "db_backup",
            ],
        },
        ToolGroup {
            id: "ssh",
            description: "Saved SSH hosts, remote command execution, and SFTP file operations.",
            includes: &[],
            tools: &[
                "ssh_hosts",
                "ssh_test",
                "ssh_exec",
                "ssh_context_refresh",
                "ssh_context",
                "sftp_list",
                "sftp_get",
                "sftp_put",
                "sftp_mkdir",
                "sftp_rmdir",
                "sftp_unlink",
                "sftp_rename",
            ],
        },
        ToolGroup {
            id: "n8n",
            description: "Generate, validate, save, import, activate, and smoke-test n8n workflow automations.",
            includes: &[],
            tools: &[
                "n8n_configure",
                "n8n_status",
                "n8n_local_start",
                "n8n_local_stop",
                "n8n_save_workflow",
                "n8n_list_saved_workflows",
                "n8n_read_workflow",
                "n8n_validate_workflow",
                "n8n_import_workflow",
                "n8n_list_remote_workflows",
                "n8n_activate_workflow",
                "n8n_deactivate_workflow",
                "n8n_webhook_request",
            ],
        },
        ToolGroup {
            id: "skills",
            description: "Skill search, proposal, promotion, archiving, usage telemetry, and learner batches.",
            includes: &[],
            tools: &[
                "skills_search",
                "skill_propose",
                "skill_promote",
                "skill_archive",
                "skill_record_usage",
                "evolve_observe",
                "evolve_run",
                "curator_run",
            ],
        },
        ToolGroup {
            id: "docs",
            description: "Local documentation site build/scaffold and spec write helpers.",
            includes: &["repo"],
            tools: &["docs_build", "spec_write", "spec_set_section"],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_includes_resolve_once() {
        let registry = ToolRegistry::from_groups_for_test(vec![
            ToolGroup {
                id: "core",
                description: "",
                includes: &[],
                tools: &["a"],
            },
            ToolGroup {
                id: "repo",
                description: "",
                includes: &["core"],
                tools: &["b"],
            },
            ToolGroup {
                id: "docs",
                description: "",
                includes: &["repo", "core"],
                tools: &["c"],
            },
        ]);

        assert_eq!(
            registry.resolve_group_tools("docs").unwrap(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn include_cycles_are_reported() {
        let registry = ToolRegistry::from_groups_for_test(vec![
            ToolGroup {
                id: "a",
                description: "",
                includes: &["b"],
                tools: &[],
            },
            ToolGroup {
                id: "b",
                description: "",
                includes: &["c"],
                tools: &[],
            },
            ToolGroup {
                id: "c",
                description: "",
                includes: &["a"],
                tools: &[],
            },
        ]);

        let err = registry.resolve_group_tools("a").unwrap_err();
        assert!(err.contains("a -> b -> c -> a"));
    }
}
