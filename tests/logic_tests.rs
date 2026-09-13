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

use actioneer::domain::formatting::{
    format_elapsed, is_in_progress, job_duration_text, running_duration_string, step_duration_text,
};
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
