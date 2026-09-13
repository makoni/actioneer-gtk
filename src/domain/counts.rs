//! Repository status: how a repository's permissions and runs roll up.
//!
//! Pure: no GTK, no I/O. The sidebar renders what these decide.

use super::models::{Repo, RepoPermissions, WorkflowRun};
use std::collections::{HashMap, HashSet};

/// Repository actions state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoActionsState {
    Enabled,
    Disabled,
    Unknown,
}

/// Workflow status counts for a repository
#[derive(Clone, Debug, Default)]
pub struct WorkflowStatusCounts {
    pub active: usize,
    pub failed: usize,
}

pub fn determine_actions_state(permissions: Option<&RepoPermissions>) -> RepoActionsState {
    match permissions {
        Some(perms) if perms.push || perms.admin => RepoActionsState::Enabled,
        Some(_) => RepoActionsState::Disabled,
        None => RepoActionsState::Unknown,
    }
}

pub fn selected_repo_for_status_refresh(
    repos: &[Repo],
    selected_repo_id: Option<i64>,
) -> Option<Repo> {
    let selected_repo_id = selected_repo_id?;
    repos
        .iter()
        .find(|repo| repo.id == selected_repo_id)
        .cloned()
}

pub fn select_latest_runs_for_workflows(
    workflow_ids: &[i64],
    repo_runs: &[WorkflowRun],
    repo_runs_truncated: bool,
) -> (HashMap<i64, WorkflowRun>, Vec<i64>) {
    let selected_ids: HashSet<i64> = workflow_ids.iter().copied().collect();
    let mut latest_runs = HashMap::new();

    for run in repo_runs {
        let Some(workflow_id) = run.workflow_id else {
            continue;
        };

        if !selected_ids.contains(&workflow_id) || latest_runs.contains_key(&workflow_id) {
            continue;
        }

        latest_runs.insert(workflow_id, run.clone());

        if latest_runs.len() == selected_ids.len() {
            break;
        }
    }

    let missing = if repo_runs_truncated {
        workflow_ids
            .iter()
            .copied()
            .filter(|workflow_id| !latest_runs.contains_key(workflow_id))
            .collect()
    } else {
        Vec::new()
    };

    (latest_runs, missing)
}

#[cfg(test)]
mod tests {
    #[test]
    fn determine_actions_state_prefers_push_or_admin() {
        let perms = RepoPermissions {
            admin: false,
            push: true,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&perms)),
            RepoActionsState::Enabled
        );

        let admin_perms = RepoPermissions {
            admin: true,
            push: false,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&admin_perms)),
            RepoActionsState::Enabled
        );
    }

    #[test]
    fn determine_actions_state_disables_pull_only() {
        let perms = RepoPermissions {
            admin: false,
            push: false,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&perms)),
            RepoActionsState::Disabled
        );
    }

    #[test]
    fn determine_actions_state_unknown_without_permissions() {
        assert_eq!(determine_actions_state(None), RepoActionsState::Unknown);
    }

    fn run(id: i64, workflow_id: Option<i64>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            workflow_id,
            name: Some(format!("Run {id}")),
            display_title: Some(format!("Run {id}")),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            actor: None,
            head_commit: None,
            triggering_actor: None,
            html_url: None,
        }
    }

    #[test]
    fn select_latest_runs_keeps_first_repo_run_per_workflow() {
        let (latest, missing) = select_latest_runs_for_workflows(
            &[11, 22],
            &[
                run(200, Some(22)),
                run(199, Some(22)),
                run(150, None),
                run(100, Some(11)),
            ],
            false,
        );

        assert_eq!(latest.get(&22).map(|run| run.id), Some(200));
        assert_eq!(latest.get(&11).map(|run| run.id), Some(100));
        assert!(
            missing.is_empty(),
            "non-truncated responses should not trigger fallback"
        );
    }

    #[test]
    fn select_latest_runs_requests_fallback_only_for_truncated_missing_workflows() {
        let (_, missing_without_truncation) =
            select_latest_runs_for_workflows(&[11, 22, 33], &[run(300, Some(11))], false);
        assert!(
            missing_without_truncation.is_empty(),
            "missing workflows should be treated as having no runs when the repo-wide page is complete"
        );

        let (_, missing_with_truncation) =
            select_latest_runs_for_workflows(&[11, 22, 33], &[run(300, Some(11))], true);
        assert_eq!(missing_with_truncation, vec![22, 33]);
    }

    use super::*;

    #[test]
    fn status_counts_start_at_zero() {
        let counts = WorkflowStatusCounts::default();
        assert_eq!((counts.active, counts.failed), (0, 0));
    }

    #[test]
    fn status_counts_carry_what_they_are_given() {
        let counts = WorkflowStatusCounts {
            active: 3,
            failed: 2,
        };
        assert_eq!((counts.active, counts.failed), (3, 2));
    }

    #[test]
    fn actions_state_variants_are_distinct() {
        assert_ne!(RepoActionsState::Enabled, RepoActionsState::Disabled);
        assert_ne!(RepoActionsState::Enabled, RepoActionsState::Unknown);
        assert_ne!(RepoActionsState::Disabled, RepoActionsState::Unknown);
        assert_eq!(RepoActionsState::Enabled, RepoActionsState::Enabled);
    }
}
