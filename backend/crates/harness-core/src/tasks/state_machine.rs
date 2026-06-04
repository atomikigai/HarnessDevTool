//! State machine transitions per lessons-learned §D2.
#![allow(clippy::collapsible_match)]

use super::model::{Task, TaskStatus};
use crate::Error;

/// Returns `Ok(())` if the transition is allowed for the given task; otherwise
/// `Err(Error::InvalidTransition)` or `Err(Error::Validation)` when a guard
/// (notes, acceptance, artifacts) is violated.
pub fn validate_transition(task: &Task, to: TaskStatus, by: &str) -> Result<(), Error> {
    use TaskStatus::*;
    let from = task.status;

    // `* -> abandoned` only humans
    if to == Abandoned {
        if !by.starts_with("human") && by != "human" {
            return Err(Error::Validation("only humans can abandon tasks".into()));
        }
        if task.notes.why_abandoned.is_empty() {
            return Err(Error::Validation(
                "notes.why_abandoned is required to abandon".into(),
            ));
        }
        return Ok(());
    }

    let ok = matches!(
        (from, to),
        (Proposed, Queued)
            | (Proposed, Blocked)
            | (Queued, InProgress)
            | (Queued, Blocked)
            | (Queued, Paused)
            | (InProgress, PendingVerify)
            | (InProgress, Paused)
            | (InProgress, Blocked)
            | (PendingVerify, Done)
            | (PendingVerify, InProgress)
            | (Paused, InProgress)
            | (Paused, Queued)
            | (Blocked, Queued)
            | (Blocked, InProgress)
    );
    if !ok {
        return Err(Error::InvalidTransition { from, to });
    }

    match to {
        InProgress if from == Queued => {
            if task.claim_lease.is_none() {
                return Err(Error::Validation(
                    "queued→in_progress requires an active claim lease".into(),
                ));
            }
        }
        PendingVerify => {
            if task.artifacts.files.is_empty() {
                return Err(Error::Validation(
                    "in_progress→pending_verify requires artifacts.files".into(),
                ));
            }
        }
        Done => {
            if task.acceptance.checks.is_empty()
                || !task.acceptance.checks.iter().all(|c| c.verified)
            {
                return Err(Error::Validation(
                    "pending_verify→done requires all acceptance.checks verified".into(),
                ));
            }
            let assignee = task.assignee.as_deref().unwrap_or("");
            for c in &task.acceptance.checks {
                if let Some(vb) = &c.verified_by {
                    if vb == assignee {
                        return Err(Error::Validation(
                            "verified_by must differ from assignee".into(),
                        ));
                    }
                }
            }
        }
        Paused => {
            if task.notes.why_paused.is_empty() {
                return Err(Error::Validation(
                    "→paused requires notes.why_paused".into(),
                ));
            }
        }
        Blocked if task.blocked_by.is_empty() => {
            return Err(Error::Validation(
                "→blocked requires non-empty blocked_by".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::model::{AcceptanceBlock, AcceptanceCheck, Artifacts, HistoryBlock, Notes};
    use chrono::Utc;

    fn base(status: TaskStatus) -> Task {
        Task {
            schema_version: 1,
            id: "T-0001".into(),
            title: "x".into(),
            status,
            created_at: Utc::now(),
            created_by: "human".into(),
            updated_at: Utc::now(),
            updated_by: "human".into(),
            parent: None,
            children: vec![],
            blocked_by: vec![],
            unblocks: vec![],
            assignee: None,
            claim_lease: None,
            previous_assignees: vec![],
            labels: vec![],
            brief: None,
            acceptance: AcceptanceBlock::default(),
            artifacts: Artifacts::default(),
            notes: Notes::default(),
            history: HistoryBlock::default(),
        }
    }

    #[test]
    fn queued_to_in_progress_requires_lease() {
        let t = base(TaskStatus::Queued);
        assert!(validate_transition(&t, TaskStatus::InProgress, "agent:a").is_err());
    }

    #[test]
    fn pending_verify_to_done_requires_other_verifier() {
        let mut t = base(TaskStatus::PendingVerify);
        t.assignee = Some("agent:a".into());
        t.acceptance.checks = vec![AcceptanceCheck {
            id: "C1".into(),
            text: "x".into(),
            verified: true,
            verified_by: Some("agent:a".into()),
        }];
        assert!(validate_transition(&t, TaskStatus::Done, "agent:b").is_err());
        t.acceptance.checks[0].verified_by = Some("agent:b".into());
        assert!(validate_transition(&t, TaskStatus::Done, "agent:b").is_ok());
    }

    #[test]
    fn abandoned_requires_human_and_reason() {
        let t = base(TaskStatus::Queued);
        assert!(validate_transition(&t, TaskStatus::Abandoned, "agent:a").is_err());
        let mut t2 = t.clone();
        assert!(validate_transition(&t2, TaskStatus::Abandoned, "human").is_err());
        t2.notes.why_abandoned = "deprecated".into();
        assert!(validate_transition(&t2, TaskStatus::Abandoned, "human").is_ok());
    }

    #[test]
    fn invalid_jump_rejected() {
        let t = base(TaskStatus::Queued);
        assert!(matches!(
            validate_transition(&t, TaskStatus::Done, "human"),
            Err(Error::InvalidTransition { .. })
        ));
    }

    #[test]
    fn proposed_can_only_promote_to_queued() {
        let t = base(TaskStatus::Proposed);
        assert!(validate_transition(&t, TaskStatus::Queued, "agent:planner").is_ok());
        let mut blocked = t.clone();
        blocked.blocked_by = vec!["T-0000".into()];
        assert!(validate_transition(&blocked, TaskStatus::Blocked, "agent:planner").is_ok());
        assert!(matches!(
            validate_transition(&t, TaskStatus::InProgress, "agent:planner"),
            Err(Error::InvalidTransition { .. })
        ));
    }
}
