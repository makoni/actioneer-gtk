use crate::api::models::WorkflowRun;
use crate::ui::detail_view::RunFilters;

const MAX_VISIBLE_RUNS: usize = 10;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum RunStatusFilterKind {
    Success,
    Failed,
    Running,
}

pub(super) struct RunDisplaySummary {
    pub filtered_total: usize,
    pub visible_runs: Vec<WorkflowRun>,
}

pub(super) fn run_matches_filters(run: &WorkflowRun, filters: &RunFilters) -> bool {
    match classify_run_status(run) {
        RunStatusFilterKind::Success => filters.include_success,
        RunStatusFilterKind::Failed => filters.include_failed,
        RunStatusFilterKind::Running => filters.include_running,
    }
}

pub(super) fn summarize_visible_runs(
    runs: &[WorkflowRun],
    filters: &RunFilters,
) -> RunDisplaySummary {
    let mut filtered_total = 0;
    let mut visible_runs = Vec::new();

    for run in runs {
        if run_matches_filters(run, filters) {
            filtered_total += 1;
            if visible_runs.len() < MAX_VISIBLE_RUNS {
                visible_runs.push(run.clone());
            }
        }
    }

    RunDisplaySummary {
        filtered_total,
        visible_runs,
    }
}

pub(crate) fn classify_run_status(run: &WorkflowRun) -> RunStatusFilterKind {
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

    fn build_run(id: i64, status: Option<&str>, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            workflow_id: None,
            name: Some("Run".into()),
            display_title: Some("Run".into()),
            head_branch: Some("main".into()),
            status: status.map(|s| s.to_string()),
            conclusion: conclusion.map(|c| c.to_string()),
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
    fn classify_run_status_identifies_running() {
        let run = build_run(1, Some("in_progress"), None);
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Running);
    }

    #[test]
    fn classify_run_status_prefers_conclusion_over_status() {
        let run = build_run(1, Some("completed"), Some("failure"));
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Failed);
    }

    #[test]
    fn classify_run_status_defaults_to_success() {
        let run = build_run(1, None, None);
        assert_eq!(classify_run_status(&run), RunStatusFilterKind::Success);
    }

    #[test]
    fn run_matches_filters_respects_flags() {
        let run = build_run(1, Some("completed"), Some("success"));
        let filters = RunFilters {
            include_success: false,
            include_failed: true,
            include_running: true,
        };

        assert!(!run_matches_filters(&run, &filters));
    }

    #[test]
    fn limits_visible_runs_to_ten() {
        let runs: Vec<_> = (0..15)
            .map(|i| build_run(i, Some("completed"), Some("success")))
            .collect();
        let summary = summarize_visible_runs(&runs, &RunFilters::default());
        assert_eq!(summary.filtered_total, 15);
        assert_eq!(summary.visible_runs.len(), 10);
    }

    #[test]
    fn respects_filters_when_collecting_runs() {
        let runs = vec![
            build_run(1, Some("completed"), Some("success")),
            build_run(2, Some("completed"), Some("failure")),
            build_run(3, Some("in_progress"), None),
        ];

        let filters = RunFilters {
            include_failed: false,
            ..RunFilters::default()
        };
        let summary = summarize_visible_runs(&runs, &filters);
        assert_eq!(summary.filtered_total, 2);
        assert_eq!(summary.visible_runs.len(), 2);
        assert_eq!(summary.visible_runs[0].id, 1);
        assert_eq!(summary.visible_runs[1].id, 3);
    }
}
