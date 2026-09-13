//! Rules over workflow runs.
//!
//! Pure: no GTK, no I/O, no localization. The UI asks these questions to decide
//! what to render; the answers do not depend on anything but the run itself.

use super::models::WorkflowRun;

/// Check if a workflow run is currently active
pub fn is_run_active(run: &WorkflowRun) -> bool {
    run.is_active()
}

/// Check if a workflow run has failed
pub fn is_run_failure(run: &WorkflowRun) -> bool {
    run.conclusion
        .as_deref()
        .map(|c| matches!(c, "failure" | "cancelled" | "timed_out"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: Option<&str>, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: Some(1),
            workflow_id: Some(1),
            name: None,
            display_title: None,
            head_branch: None,
            status: status.map(str::to_string),
            conclusion: conclusion.map(str::to_string),
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
    fn active_statuses_are_the_five_pending_ones() {
        for status in ["queued", "in_progress", "waiting", "requested", "pending"] {
            assert!(
                is_run_active(&run(Some(status), None)),
                "{status} should count as active"
            );
        }
    }

    #[test]
    fn finished_and_unknown_statuses_are_not_active() {
        for status in ["completed", "failure", "neutral", "", "COMPLETED"] {
            assert!(
                !is_run_active(&run(Some(status), None)),
                "{status} should not count as active"
            );
        }
        assert!(!is_run_active(&run(None, None)), "a run without a status");
    }

    #[test]
    fn active_status_matching_ignores_case() {
        assert!(is_run_active(&run(Some("IN_PROGRESS"), None)));
        assert!(is_run_active(&run(Some("Queued"), None)));
    }

    #[test]
    fn failure_conclusions_are_failure_cancelled_and_timed_out() {
        for conclusion in ["failure", "cancelled", "timed_out"] {
            assert!(
                is_run_failure(&run(Some("completed"), Some(conclusion))),
                "{conclusion} should count as a failure"
            );
        }
    }

    #[test]
    fn other_conclusions_are_not_failures() {
        for conclusion in ["success", "skipped", "neutral", "action_required", "stale"] {
            assert!(
                !is_run_failure(&run(Some("completed"), Some(conclusion))),
                "{conclusion} should not count as a failure"
            );
        }
    }

    #[test]
    fn a_run_without_a_conclusion_is_not_a_failure() {
        assert!(!is_run_failure(&run(Some("in_progress"), None)));
    }

    #[test]
    fn failure_conclusion_matching_is_case_sensitive() {
        // Deliberately pinned: the mapping does not lowercase, unlike the
        // status check above. GitHub only ever sends lowercase conclusions.
        assert!(!is_run_failure(&run(Some("completed"), Some("FAILURE"))));
    }
}
