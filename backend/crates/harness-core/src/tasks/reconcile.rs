use std::collections::{HashMap, HashSet};

use chrono::Utc;

use super::model::{
    Artifact, ReconcileEntity, ReconcileIssue, ReconcileReport, ReconcileSessionRef,
    ReconcileSeverity, Task, TaskStatus,
};

pub fn reconcile_tasks(
    thread_id: &str,
    tasks: &[Task],
    sessions: &[ReconcileSessionRef],
) -> ReconcileReport {
    let task_ids: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let tasks_by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let session_ids: HashSet<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    let mut issues = Vec::new();
    let mut artifact_ids: HashMap<String, ReconcileEntity> = HashMap::new();
    let mut artifact_count = 0usize;

    for task in tasks {
        check_task_refs(task, &tasks_by_id, &task_ids, &mut issues);
        check_artifacts(task, &task_ids, &mut artifact_ids, &mut artifact_count, &mut issues);
    }

    for session in sessions {
        check_session_refs(session, thread_id, &task_ids, &session_ids, &mut issues);
    }

    check_active_task_sessions(tasks, sessions, &mut issues);

    ReconcileReport {
        thread_id: thread_id.to_string(),
        generated_at: Utc::now(),
        task_count: tasks.len(),
        session_count: sessions.len(),
        artifact_count,
        issues,
    }
}

fn check_task_refs(
    task: &Task,
    tasks_by_id: &HashMap<&str, &Task>,
    task_ids: &HashSet<&str>,
    issues: &mut Vec<ReconcileIssue>,
) {
    let owner_entity = task_entity(&task.id);
    if let Some(parent) = task.parent.as_deref() {
        match tasks_by_id.get(parent) {
            Some(parent_task) if !parent_task.children.iter().any(|id| id == &task.id) => {
                issues.push(issue(
                    "task_parent_not_reciprocal",
                    ReconcileSeverity::Warning,
                    owner_entity.clone(),
                    format!("task {} references parent {parent}, but parent does not list it as a child", task.id),
                    vec![task_entity(parent)],
                ));
            }
            Some(_) => {}
            None => issues.push(issue(
                "task_parent_missing",
                ReconcileSeverity::Error,
                owner_entity.clone(),
                format!("task {} references missing parent {parent}", task.id),
                vec![task_entity(parent)],
            )),
        }
    }

    for child in &task.children {
        match tasks_by_id.get(child.as_str()) {
            Some(child_task) if child_task.parent.as_deref() != Some(task.id.as_str()) => {
                issues.push(issue(
                    "task_child_not_reciprocal",
                    ReconcileSeverity::Warning,
                    owner_entity.clone(),
                    format!("task {} lists child {child}, but child does not reference it as parent", task.id),
                    vec![task_entity(child)],
                ));
            }
            Some(_) => {}
            None => issues.push(issue(
                "task_child_missing",
                ReconcileSeverity::Error,
                owner_entity.clone(),
                format!("task {} lists missing child {child}", task.id),
                vec![task_entity(child)],
            )),
        }
    }

    for blocked_by in &task.blocked_by {
        if !task_ids.contains(blocked_by.as_str()) {
            issues.push(issue(
                "task_blocker_missing",
                ReconcileSeverity::Error,
                owner_entity.clone(),
                format!("task {} is blocked by missing task {blocked_by}", task.id),
                vec![task_entity(blocked_by)],
            ));
        }
    }

    for unblocks in &task.unblocks {
        match tasks_by_id.get(unblocks.as_str()) {
            Some(target) if !target.blocked_by.iter().any(|id| id == &task.id) => {
                issues.push(issue(
                    "task_unblocks_not_reciprocal",
                    ReconcileSeverity::Warning,
                    owner_entity.clone(),
                    format!("task {} claims to unblock {unblocks}, but target is not blocked by it", task.id),
                    vec![task_entity(unblocks)],
                ));
            }
            Some(_) => {}
            None => issues.push(issue(
                "task_unblocks_missing",
                ReconcileSeverity::Error,
                owner_entity.clone(),
                format!("task {} claims to unblock missing task {unblocks}", task.id),
                vec![task_entity(unblocks)],
            )),
        }
    }
}

fn check_artifacts(
    task: &Task,
    task_ids: &HashSet<&str>,
    artifact_ids: &mut HashMap<String, ReconcileEntity>,
    artifact_count: &mut usize,
    issues: &mut Vec<ReconcileIssue>,
) {
    if task.artifacts.metadata.is_empty()
        && (!task.artifacts.files.is_empty()
            || !task.artifacts.turns.is_empty()
            || task.artifacts.diff.is_some())
    {
        issues.push(issue(
            "artifact_legacy_without_metadata",
            ReconcileSeverity::Info,
            task_entity(&task.id),
            format!("task {} has legacy artifacts without materialized metadata", task.id),
            vec![],
        ));
    }

    for artifact in &task.artifacts.metadata {
        *artifact_count += 1;
        let entity = artifact_entity(artifact);
        if artifact.artifact_id.trim().is_empty() {
            issues.push(issue(
                "artifact_id_empty",
                ReconcileSeverity::Error,
                entity.clone(),
                format!("task {} has artifact metadata with an empty artifact_id", task.id),
                vec![task_entity(&task.id)],
            ));
        } else if let Some(first) = artifact_ids.insert(artifact.artifact_id.clone(), entity.clone()) {
            issues.push(issue(
                "artifact_id_duplicate",
                ReconcileSeverity::Error,
                entity.clone(),
                format!("artifact_id {} appears more than once", artifact.artifact_id),
                vec![first],
            ));
        }

        if artifact.path.trim().is_empty() {
            issues.push(issue(
                "artifact_path_empty",
                ReconcileSeverity::Error,
                entity.clone(),
                format!("artifact {} has an empty path", artifact.artifact_id),
                vec![task_entity(&task.id)],
            ));
        }

        if artifact.task_id != task.id {
            let severity = if task_ids.contains(artifact.task_id.as_str()) {
                ReconcileSeverity::Warning
            } else {
                ReconcileSeverity::Error
            };
            issues.push(issue(
                "artifact_task_mismatch",
                severity,
                entity,
                format!(
                    "artifact {} is stored on task {} but references task {}",
                    artifact.artifact_id, task.id, artifact.task_id
                ),
                vec![task_entity(&task.id), task_entity(&artifact.task_id)],
            ));
        }
    }
}

fn check_session_refs(
    session: &ReconcileSessionRef,
    thread_id: &str,
    task_ids: &HashSet<&str>,
    session_ids: &HashSet<&str>,
    issues: &mut Vec<ReconcileIssue>,
) {
    let entity = session_entity(&session.session_id);
    if session.thread_id != thread_id {
        issues.push(issue(
            "session_thread_mismatch",
            ReconcileSeverity::Error,
            entity.clone(),
            format!(
                "session {} belongs to thread {}, not {}",
                session.session_id, session.thread_id, thread_id
            ),
            vec![ReconcileEntity::new("thread", &session.thread_id)],
        ));
    }

    if let Some(task_id) = session.task_id.as_deref() {
        if !task_ids.contains(task_id) {
            issues.push(issue(
                "session_task_missing",
                ReconcileSeverity::Error,
                entity.clone(),
                format!("session {} references missing task {task_id}", session.session_id),
                vec![task_entity(task_id)],
            ));
        }
    }

    for (kind, value) in [
        ("session_parent_missing", session.parent_session_id.as_deref()),
        ("session_owner_missing", session.owner_session_id.as_deref()),
        ("session_root_missing", session.root_session_id.as_deref()),
    ] {
        if let Some(id) = value {
            if !session_ids.contains(id) {
                issues.push(issue(
                    kind,
                    ReconcileSeverity::Warning,
                    entity.clone(),
                    format!("session {} references missing session {id}", session.session_id),
                    vec![session_entity(id)],
                ));
            }
        }
    }

    if session.parent_session_id.is_none()
        && session
            .root_session_id
            .as_deref()
            .is_some_and(|root| root != session.session_id)
    {
        issues.push(issue(
            "session_root_not_self",
            ReconcileSeverity::Warning,
            entity,
            format!("root session {} has a different root_session_id", session.session_id),
            session.root_session_id.as_deref().map(session_entity).into_iter().collect(),
        ));
    }
}

fn check_active_task_sessions(
    tasks: &[Task],
    sessions: &[ReconcileSessionRef],
    issues: &mut Vec<ReconcileIssue>,
) {
    let running_task_ids: HashSet<&str> = sessions
        .iter()
        .filter(|s| s.status == "running")
        .filter_map(|s| s.task_id.as_deref())
        .collect();

    for task in tasks {
        if matches!(task.status, TaskStatus::InProgress | TaskStatus::PendingVerify)
            && !running_task_ids.contains(task.id.as_str())
        {
            issues.push(issue(
                "active_task_without_running_session",
                ReconcileSeverity::Warning,
                task_entity(&task.id),
                format!("active task {} has no running session scoped to it", task.id),
                vec![],
            ));
        }
    }
}

fn issue(
    kind: impl Into<String>,
    severity: ReconcileSeverity,
    entity: ReconcileEntity,
    message: impl Into<String>,
    related: Vec<ReconcileEntity>,
) -> ReconcileIssue {
    ReconcileIssue {
        kind: kind.into(),
        severity,
        entity,
        message: message.into(),
        related,
    }
}

fn task_entity(id: &str) -> ReconcileEntity {
    ReconcileEntity::new("task", id)
}

fn session_entity(id: &str) -> ReconcileEntity {
    ReconcileEntity::new("session", id)
}

fn artifact_entity(artifact: &Artifact) -> ReconcileEntity {
    ReconcileEntity::new("artifact", &artifact.artifact_id)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use super::super::model::{AcceptanceBlock, Artifacts, HistoryBlock, Notes};
    use crate::tasks::TaskBrief;

    fn task(id: &str) -> Task {
        let now = Utc::now();
        Task {
            schema_version: 1,
            id: id.into(),
            title: id.into(),
            status: TaskStatus::Queued,
            created_at: now,
            created_by: "test".into(),
            updated_at: now,
            updated_by: "test".into(),
            parent: None,
            children: vec![],
            blocked_by: vec![],
            unblocks: vec![],
            assignee: None,
            claim_lease: None,
            previous_assignees: vec![],
            labels: vec![],
            spec_refs: vec![],
            brief: Some(TaskBrief::default()),
            acceptance: AcceptanceBlock::default(),
            artifacts: Artifacts::default(),
            notes: Notes::default(),
            scheduler_explanation: None,
            history: HistoryBlock::default(),
        }
    }

    fn session(session_id: &str, task_id: Option<&str>) -> ReconcileSessionRef {
        ReconcileSessionRef {
            session_id: session_id.into(),
            thread_id: "thr-1".into(),
            task_id: task_id.map(str::to_string),
            parent_session_id: None,
            owner_session_id: None,
            root_session_id: Some(session_id.into()),
            status: "running".into(),
        }
    }

    #[test]
    fn clean_state_has_no_issues() {
        let tasks = vec![task("T-0001")];
        let sessions = vec![session("s1", Some("T-0001"))];
        let report = reconcile_tasks("thr-1", &tasks, &sessions);
        assert!(report.issues.is_empty());
        assert_eq!(report.task_count, 1);
        assert_eq!(report.session_count, 1);
    }

    #[test]
    fn reports_missing_task_references() {
        let mut t = task("T-0001");
        t.blocked_by = vec!["T-9999".into()];
        let report = reconcile_tasks("thr-1", &[t], &[]);
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == "task_blocker_missing"));
    }

    #[test]
    fn reports_artifact_task_mismatch_and_duplicate_ids() {
        let mut t1 = task("T-0001");
        t1.artifacts.metadata.push(Artifact {
            artifact_id: "a1".into(),
            task_id: "T-9999".into(),
            kind: crate::tasks::ArtifactKind::File,
            path: "out.txt".into(),
            produced_by: "agent:a".into(),
            created_at: Utc::now(),
            summary: String::new(),
        });
        let mut t2 = task("T-0002");
        t2.artifacts.metadata.push(Artifact {
            artifact_id: "a1".into(),
            task_id: "T-0002".into(),
            kind: crate::tasks::ArtifactKind::Log,
            path: "turn.log".into(),
            produced_by: "agent:b".into(),
            created_at: Utc::now(),
            summary: String::new(),
        });

        let report = reconcile_tasks("thr-1", &[t1, t2], &[]);
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == "artifact_task_mismatch"));
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == "artifact_id_duplicate"));
    }

    #[test]
    fn reports_session_task_missing() {
        let report = reconcile_tasks("thr-1", &[], &[session("s1", Some("T-9999"))]);
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == "session_task_missing"));
    }

    #[test]
    fn reports_active_task_without_running_session() {
        let mut t = task("T-0001");
        t.status = TaskStatus::InProgress;
        let report = reconcile_tasks("thr-1", &[t], &[]);
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == "active_task_without_running_session"));
    }
}
