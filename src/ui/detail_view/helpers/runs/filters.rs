use crate::api::models::WorkflowRun;
use crate::ui::detail_view::RunFilters;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum RunStatusFilterKind {
    Success,
    Failed,
    Running,
}

pub(super) fn run_matches_filters(run: &WorkflowRun, filters: &RunFilters) -> bool {
    match classify_run_status(run) {
        RunStatusFilterKind::Success => filters.include_success,
        RunStatusFilterKind::Failed => filters.include_failed,
        RunStatusFilterKind::Running => filters.include_running,
    }
}

pub(super) fn classify_run_status(run: &WorkflowRun) -> RunStatusFilterKind {
    if run.is_active() {
        return RunStatusFilterKind::Running;
    }

    if let Some(conclusion) = run.conclusion.as_deref() {
        if conclusion.eq_ignore_ascii_case("success") {
            RunStatusFilterKind::Success
        } else {
            RunStatusFilterKind::Failed
        }
    } else if let Some(status) = run.status.as_deref() {
        if status.eq_ignore_ascii_case("completed") {
            RunStatusFilterKind::Success
        } else {
            RunStatusFilterKind::Failed
        }
    } else {
        RunStatusFilterKind::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_run(status: Option<&str>, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: Some(1),
            name: Some("Run".into()),
            display_title: Some("Run".into()),
            head_branch: Some("main".into()),
            status: status.map(|s| s.to_string()),
            conclusion: conclusion.map(|c| c.to_string()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: None,
        }
    }

    #[test]
    fn classify_run_status_identifies_running() {
        let run = build_run(Some("in_progress"), None);
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Running);
    }

    #[test]
    fn classify_run_status_prefers_conclusion_over_status() {
        let run = build_run(Some("completed"), Some("failure"));
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Failed);
    }

    #[test]
    fn classify_run_status_defaults_to_success() {
        let run = build_run(None, None);
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Success);
    }

    #[test]
    fn run_matches_filters_respects_flags() {
        let run = build_run(Some("completed"), Some("success"));
        let filters = RunFilters {
            include_success: false,
            include_failed: true,
            include_running: true,
        };

        assert!(!run_matches_filters(&run, &filters));
    }
}
