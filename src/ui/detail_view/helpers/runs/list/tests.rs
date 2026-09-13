//! Tests for [`super`].
//!
//! Split out of `list.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::test_run_list_model;
use super::{STATE_CONTENT, STATE_ERROR, STATE_IDLE, format_runs_counts, state_requires_load};
use crate::kernel::i18n::{i18n_test_guard, tr};
use crate::services::api::models::WorkflowRun;
use crate::ui::detail_view::RunFilters;
use crate::ui::test_helpers::run_gtk_test;
use gtk4::prelude::ListModelExt;
use std::collections::HashSet;

#[test]
fn counts_report_overall_total_when_no_filter_narrows_it() {
    let _guard = i18n_test_guard();
    let text = format_runs_counts(10, 10, 10);
    assert_eq!(
        text,
        tr("Showing {visible} of {overall}")
            .replace("{visible}", "10")
            .replace("{overall}", "10")
    );
}

#[test]
fn counts_report_filtered_total_when_filters_hide_runs() {
    let _guard = i18n_test_guard();
    // 5 of 12 runs match the active status chips, and all 5 fit the cap:
    // saying "of 12" here would imply the cap hid the other seven.
    let text = format_runs_counts(5, 5, 12);
    assert_eq!(
        text,
        tr("Showing {visible} of {filtered} matching filters")
            .replace("{visible}", "5")
            .replace("{filtered}", "5")
    );
}

#[test]
fn counts_report_cap_and_filters_together() {
    let _guard = i18n_test_guard();
    let text = format_runs_counts(3, 4, 10);
    assert_eq!(
        text,
        tr("Showing {visible} of {filtered} matching filters")
            .replace("{visible}", "3")
            .replace("{filtered}", "4")
    );
}

#[test]
fn requires_load_for_idle_or_error() {
    assert!(state_requires_load(Some(STATE_IDLE.into())));
    assert!(state_requires_load(Some(STATE_ERROR.into())));
}

#[test]
fn does_not_require_load_for_content() {
    assert!(!state_requires_load(Some(STATE_CONTENT.into())));
}

#[test]
fn treats_missing_state_as_needing_load() {
    assert!(state_requires_load(None));
}

#[test]
#[ignore = "requires GTK display"]
fn reapply_filters_updates_from_last_runs() {
    run_gtk_test("reapply_filters_updates_from_last_runs", || {
        // This test exercises the filter reapplication path without hitting the network.
        let model = test_run_list_model();

        let runs = vec![
            WorkflowRun {
                id: 1,
                run_number: Some(1),
                workflow_id: None,
                name: Some("Run 1".into()),
                display_title: Some("Run 1".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                actor: None,

                head_commit: None,

                triggering_actor: None,

                html_url: None,
            },
            WorkflowRun {
                id: 2,
                run_number: Some(2),
                workflow_id: None,
                name: Some("Run 2".into()),
                display_title: Some("Run 2".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("failure".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                actor: None,

                head_commit: None,

                triggering_actor: None,

                html_url: None,
            },
        ];

        model.set_runs(std::sync::Arc::new(runs));

        let mut expanded = HashSet::new();
        expanded.insert(2);

        let filters = RunFilters {
            include_success: false,
            include_failed: true,
            include_running: false,
        };

        let updated = model.reapply_filters(&filters, &expanded);
        assert!(updated, "reapply should run when data was loaded");
        assert_eq!(
            model.list_store.n_items(),
            1,
            "only failed run should remain"
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn prepend_run_deduplicates_cached_runs() {
    run_gtk_test("prepend_run_deduplicates_cached_runs", || {
        let model = test_run_list_model();
        let existing = WorkflowRun {
            id: 2,
            run_number: Some(2),
            workflow_id: None,
            name: Some("Existing".into()),
            display_title: Some("Existing".into()),
            head_branch: Some("main".into()),
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            actor: None,

            head_commit: None,

            triggering_actor: None,

            html_url: None,
        };
        model.set_runs(std::sync::Arc::new(vec![existing.clone()]));

        let replacement = WorkflowRun {
            id: 2,
            status: Some("queued".into()),
            conclusion: None,
            ..existing
        };

        model.prepend_run(replacement, &RunFilters::default());

        let cached = model.last_runs.borrow().clone();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].status.as_deref(), Some("queued"));
    });
}
