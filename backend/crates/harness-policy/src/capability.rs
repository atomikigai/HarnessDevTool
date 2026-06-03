#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub agent_id: String,
    pub role: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    Thread,
    Task,
    Spec,
    Db,
    Session,
    Knowledge,
    Repo,
    Unknown,
}

pub struct CapabilityCheck<'a> {
    pub actor: &'a Actor,
    pub tool: &'a str,
    pub resource: Resource,
    pub args: &'a serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityDecision {
    Allow,
    Deny { reason: DenyReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    RoleDenied,
    UnknownRole,
}

pub fn authorize(check: CapabilityCheck<'_>) -> CapabilityDecision {
    let role = normalize_role(&check.actor.role);
    if role == "human" {
        return CapabilityDecision::Allow;
    }

    if is_unknown_role(role) {
        return if is_read_only_tool(check.tool) {
            CapabilityDecision::Allow
        } else {
            CapabilityDecision::Deny {
                reason: DenyReason::UnknownRole,
            }
        };
    }

    if role_can_call_tool(role, check.tool) {
        CapabilityDecision::Allow
    } else {
        CapabilityDecision::Deny {
            reason: DenyReason::RoleDenied,
        }
    }
}

fn normalize_role(role: &str) -> &str {
    match role {
        "zeus-orchestrator" => "orchestrator",
        other => other,
    }
}

fn is_unknown_role(role: &str) -> bool {
    !matches!(
        role,
        "human"
            | "planner"
            | "orchestrator"
            | "generator"
            | "backend"
            | "frontend"
            | "qa"
            | "evaluator"
            | "learner"
            | "curator"
            | "arbitrator"
    )
}

fn role_can_call_tool(role: &str, tool: &str) -> bool {
    if is_read_only_tool(tool) {
        return true;
    }

    match role {
        "planner" | "orchestrator" => true,
        "generator" | "backend" | "frontend" | "qa" | "evaluator" => matches!(
            tool,
            "task_propose"
                | "task_claim"
                | "task_renew"
                | "task_update"
                | "task_release"
                | "task_submit"
        ),
        "learner" => matches!(tool, "task_propose" | "knowledge_pdf_ingest"),
        "curator" => matches!(tool, "knowledge_pdf_ingest" | "db_memory_write"),
        "arbitrator" => matches!(tool, "task_update" | "task_release"),
        _ => false,
    }
}

fn is_read_only_tool(tool: &str) -> bool {
    matches!(
        tool,
        "task_list"
            | "task_get"
            | "spec_read"
            | "repo_analyze"
            | "repo_scan"
            | "repo_read_file"
            | "repo_git_status"
            | "repo_git_log"
            | "repo_git_diff"
            | "repo_codebase_memory_status"
            | "skills_search"
            | "knowledge_pdftotext_check"
            | "db_schema"
            | "db_explain"
            | "db_performance_audit"
            | "db_memory_read"
            | "session_list_children"
    )
}

pub fn infer_resource(tool: &str, args: &serde_json::Value, thread_id: Option<String>) -> Resource {
    let kind = if tool.starts_with("task_") {
        ResourceKind::Task
    } else if tool.starts_with("spec_") {
        ResourceKind::Spec
    } else if tool.starts_with("db_") {
        ResourceKind::Db
    } else if tool.starts_with("session_") {
        ResourceKind::Session
    } else if tool.starts_with("knowledge_") {
        ResourceKind::Knowledge
    } else if tool.starts_with("repo_") {
        ResourceKind::Repo
    } else {
        ResourceKind::Unknown
    };

    let id = args
        .get("task_id")
        .or_else(|| args.get("session_id"))
        .or_else(|| args.get("connection_id"))
        .and_then(|value| value.as_str())
        .map(str::to_string);

    Resource {
        kind,
        id,
        thread_id,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn actor(role: &str) -> Actor {
        Actor {
            agent_id: "agent:test".to_string(),
            role: role.to_string(),
            session_id: None,
        }
    }

    fn decision(role: &str, tool: &str) -> CapabilityDecision {
        authorize(CapabilityCheck {
            actor: &actor(role),
            tool,
            resource: infer_resource(tool, &json!({}), Some("t1".to_string())),
            args: &json!({}),
        })
    }

    #[test]
    fn human_allows_mutating_tools() {
        assert_eq!(decision("human", "task_create"), CapabilityDecision::Allow);
        assert_eq!(decision("human", "spec_write"), CapabilityDecision::Allow);
    }

    #[test]
    fn worker_denies_task_create_but_allows_task_propose() {
        assert_eq!(
            decision("backend", "task_create"),
            CapabilityDecision::Deny {
                reason: DenyReason::RoleDenied
            }
        );
        assert_eq!(
            decision("backend", "task_propose"),
            CapabilityDecision::Allow
        );
    }

    #[test]
    fn worker_denies_spec_write_but_allows_spec_read() {
        assert_eq!(
            decision("generator", "spec_write"),
            CapabilityDecision::Deny {
                reason: DenyReason::RoleDenied
            }
        );
        assert_eq!(
            decision("generator", "spec_read"),
            CapabilityDecision::Allow
        );
    }

    #[test]
    fn planner_allows_task_create() {
        assert_eq!(
            decision("planner", "task_create"),
            CapabilityDecision::Allow
        );
    }

    #[test]
    fn learner_denies_task_update() {
        assert_eq!(
            decision("learner", "task_update"),
            CapabilityDecision::Deny {
                reason: DenyReason::RoleDenied
            }
        );
    }

    #[test]
    fn unknown_role_can_read_but_not_mutate() {
        assert_eq!(decision("custom", "repo_scan"), CapabilityDecision::Allow);
        assert_eq!(
            decision("custom", "task_create"),
            CapabilityDecision::Deny {
                reason: DenyReason::UnknownRole
            }
        );
    }
}
