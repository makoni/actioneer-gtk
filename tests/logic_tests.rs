//! Logic-level integration tests over the crate's public domain API.
//!
//! What this file replaced is worth recording. It had seven tests and none of
//! them tested this crate: one asserted that `chrono` can subtract durations,
//! and the other six re-implemented the logic they were checking *inside the
//! test file* — a local `fn is_active`, a local status-to-icon table — and then
//! asserted against their own copies. They passed no matter what the app did,
//! because a binary-only crate exposes nothing for `tests/` to import. Phase 0
//! fixed that; this file is what the fix bought.
//!
//! Localized output is deliberately absent here: `i18n_test_guard` is
//! `#[cfg(test)] pub(crate)` and is what serialises the global locale across
//! parallel tests, so anything that goes through `tr` stays in-crate. These
//! assertions are on values that never do — durations, predicates, counts.

mod common;

use actioneer::domain::counts::{
    RepoActionsState, determine_actions_state, select_latest_runs_for_workflows,
};
use actioneer::domain::filters::{
    RunFilters, RunStatusFilterKind, classify_run_status, run_matches_filters,
};
use actioneer::domain::formatting::{
    format_elapsed, is_in_progress, job_duration_text, running_duration_string, step_duration_text,
};
use actioneer::domain::models::RepoPermissions;
use actioneer::domain::models::{Job, JobStep, WorkflowRun};
use actioneer::domain::runs::{is_run_active, is_run_failure};
use common::FIXED_INSTANT;

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

// ---------------------------------------------------------------------------
// domain::runs
// ---------------------------------------------------------------------------

#[test]
fn a_run_is_active_only_while_it_has_not_finished() {
    for status in ["queued", "in_progress", "waiting", "requested", "pending"] {
        assert!(is_run_active(&run(Some(status), None)), "{status}");
    }
    for status in ["completed", "neutral", ""] {
        assert!(!is_run_active(&run(Some(status), None)), "{status}");
    }
    assert!(!is_run_active(&run(None, None)));
}

#[test]
fn a_run_counts_as_failed_for_three_conclusions() {
    for conclusion in ["failure", "cancelled", "timed_out"] {
        assert!(
            is_run_failure(&run(Some("completed"), Some(conclusion))),
            "{conclusion}"
        );
    }
    for conclusion in ["success", "skipped", "neutral", "action_required", "stale"] {
        assert!(
            !is_run_failure(&run(Some("completed"), Some(conclusion))),
            "{conclusion}"
        );
    }
    assert!(!is_run_failure(&run(Some("in_progress"), None)));
}

#[test]
fn active_and_failed_are_never_both_true() {
    // A run that is still going has no conclusion yet, so the two predicates
    // partition the states rather than overlapping.
    for status in ["queued", "in_progress", "waiting", "requested", "pending"] {
        let r = run(Some(status), None);
        assert!(!(is_run_active(&r) && is_run_failure(&r)), "{status}");
    }
}

// ---------------------------------------------------------------------------
// domain::formatting
// ---------------------------------------------------------------------------

#[test]
fn elapsed_time_gains_an_hours_field_at_exactly_one_hour() {
    assert_eq!(format_elapsed(3599), "59:59");
    assert_eq!(format_elapsed(3600), "1:00:00");
}

#[test]
fn elapsed_time_pads_minutes_and_seconds_but_not_hours() {
    assert_eq!(format_elapsed(0), "00:00");
    assert_eq!(format_elapsed(5), "00:05");
    assert_eq!(format_elapsed(3661), "1:01:01");
    assert_eq!(format_elapsed(86_399), "23:59:59");
}

#[test]
fn only_in_progress_counts_as_running() {
    assert!(is_in_progress(Some("in_progress")));
    for other in ["queued", "completed", "IN_PROGRESS", ""] {
        assert!(!is_in_progress(Some(other)), "{other}");
    }
    assert!(!is_in_progress(None));
}

#[test]
fn a_running_duration_needs_a_parsable_start_time() {
    assert_eq!(running_duration_string(None), None);
    assert_eq!(
        running_duration_string(Some(&"not a timestamp".to_string())),
        None
    );
    // A fixed instant rather than `Utc::now()`, so the assertion is about the
    // shape and not about how long the test took to reach this line.
    let elapsed = running_duration_string(Some(&FIXED_INSTANT.to_string()))
        .expect("a parsable start yields a duration");
    assert!(
        elapsed.contains(':'),
        "expected mm:ss or h:mm:ss, got {elapsed:?}"
    );
}

#[test]
fn a_finished_job_shows_its_recorded_duration_not_a_live_count() {
    let job = Job {
        id: 1,
        run_id: 1,
        name: Some("build".into()),
        status: Some("completed".into()),
        conclusion: Some("success".into()),
        started_at: Some("2026-01-24T11:00:00Z".into()),
        completed_at: Some("2026-01-24T11:02:30Z".into()),
        html_url: None,
        steps: Vec::new(),
    };
    assert_eq!(job_duration_text(&job).as_deref(), Some("02:30"));
}

#[test]
fn a_job_that_never_started_has_no_duration() {
    let job = Job {
        id: 1,
        run_id: 1,
        name: Some("build".into()),
        status: Some("queued".into()),
        conclusion: None,
        started_at: None,
        completed_at: None,
        html_url: None,
        steps: Vec::new(),
    };
    assert_eq!(job_duration_text(&job), None);
}

#[test]
fn a_step_follows_the_same_duration_rule_as_a_job() {
    let step = JobStep {
        name: Some("checkout".into()),
        status: Some("completed".into()),
        conclusion: Some("success".into()),
        number: Some(1),
        started_at: Some("2026-01-24T11:00:00Z".into()),
        completed_at: Some("2026-01-24T11:00:09Z".into()),
    };
    assert_eq!(step_duration_text(&step).as_deref(), Some("00:09"));
}

// ---------------------------------------------------------------------------
// domain::filters
// ---------------------------------------------------------------------------

#[test]
fn the_default_filter_admits_every_run() {
    let filters = RunFilters::default();
    for (status, conclusion) in [
        (Some("completed"), Some("success")),
        (Some("completed"), Some("failure")),
        (Some("in_progress"), None),
        (Some("queued"), None),
    ] {
        assert!(
            run_matches_filters(&run(status, conclusion), &filters),
            "{status:?}/{conclusion:?} should pass the default filter"
        );
    }
}

#[test]
fn clearing_a_filter_drops_exactly_that_state() {
    let filters = RunFilters {
        include_failed: false,
        ..RunFilters::default()
    };

    assert!(run_matches_filters(
        &run(Some("completed"), Some("success")),
        &filters
    ));
    assert!(!run_matches_filters(
        &run(Some("completed"), Some("failure")),
        &filters
    ));
    assert!(
        run_matches_filters(&run(Some("in_progress"), None), &filters),
        "hiding failures must not hide running work"
    );
}

#[test]
fn a_run_is_classified_by_state_not_by_conclusion_alone() {
    assert_eq!(
        classify_run_status(&run(Some("in_progress"), None)),
        RunStatusFilterKind::Running
    );
    assert_eq!(
        classify_run_status(&run(Some("completed"), Some("success"))),
        RunStatusFilterKind::Success
    );
    assert_eq!(
        classify_run_status(&run(Some("completed"), Some("failure"))),
        RunStatusFilterKind::Failed
    );
}

// ---------------------------------------------------------------------------
// domain::counts
// ---------------------------------------------------------------------------

#[test]
fn write_access_decides_whether_actions_are_available() {
    let perms = |admin: bool, push: bool, pull: bool| RepoPermissions { admin, push, pull };

    assert_eq!(
        determine_actions_state(Some(&perms(false, true, true))),
        RepoActionsState::Enabled
    );
    assert_eq!(
        determine_actions_state(Some(&perms(true, false, true))),
        RepoActionsState::Enabled
    );
    assert_eq!(
        determine_actions_state(Some(&perms(false, false, true))),
        RepoActionsState::Disabled
    );
    assert_eq!(determine_actions_state(None), RepoActionsState::Unknown);
}

#[test]
fn the_latest_run_per_workflow_is_the_first_one_seen() {
    // The API returns runs newest first, so the first match wins and later ones
    // for the same workflow are ignored.
    let mut newest = run(Some("completed"), Some("success"));
    newest.id = 200;
    newest.workflow_id = Some(11);
    let mut older = run(Some("completed"), Some("failure"));
    older.id = 100;
    older.workflow_id = Some(11);

    let (latest, missing) = select_latest_runs_for_workflows(&[11], &[newest, older], false);
    assert_eq!(latest.get(&11).map(|r| r.id), Some(200));
    assert!(missing.is_empty());
}

#[test]
fn only_a_truncated_page_reports_workflows_as_missing() {
    let (_, missing) = select_latest_runs_for_workflows(&[11, 22], &[], false);
    assert!(
        missing.is_empty(),
        "a complete page means the workflows simply have no runs"
    );

    let (_, missing) = select_latest_runs_for_workflows(&[11, 22], &[], true);
    assert_eq!(
        missing.len(),
        2,
        "a truncated page means they may have runs we did not see"
    );
}
