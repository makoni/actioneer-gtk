//! Tests for [`super`].
//!
//! Split out of `formatting.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

// Moved here with the functions they cover: these assert on localized
// output, so they live where `tr` lives. They stay in-crate because
// `i18n_test_guard` is `#[cfg(test)] pub(crate)` and is what serialises the
// global locale across parallel tests.
#[test]
fn test_relative_time_from_iso_localizes_old_dates() {
    let _guard = i18n_test_guard();
    init(None);
    let _ = apply_language_preference(LanguagePreference::En);

    let formatted = relative_time_from_iso("2024-01-08T13:45:00Z");

    assert!(!formatted.is_empty());
    assert_ne!(formatted, "2024-01-08");
}

#[test]
fn test_fallback_localized_numeric_date_uses_english_order() {
    let _guard = i18n_test_guard();
    init(None);
    let _ = apply_language_preference(LanguagePreference::En);
    let date_time = chrono::DateTime::parse_from_rfc3339("2024-01-08T13:45:00Z")
        .unwrap()
        .with_timezone(&chrono::Local);

    assert_eq!(fallback_localized_numeric_date(date_time), "01/08/2024");
}

#[test]
fn test_fallback_localized_numeric_date_uses_russian_order() {
    let _guard = i18n_test_guard();
    init(None);
    let _ = apply_language_preference(LanguagePreference::Ru);
    let date_time = chrono::DateTime::parse_from_rfc3339("2024-01-08T13:45:00Z")
        .unwrap()
        .with_timezone(&chrono::Local);

    assert_eq!(fallback_localized_numeric_date(date_time), "08.01.2024");
    let _ = apply_language_preference(LanguagePreference::En);
}

use super::*;
use crate::kernel::i18n::{apply_language_preference, i18n_test_guard, init};
use crate::services::api::models::{Job, WorkflowRun};
use crate::services::preferences::LanguagePreference;
use crate::ui::test_helpers::run_gtk_test;

#[test]
fn status_presentation_covers_every_failure_conclusion() {
    // The dot is the only status signal on a row, so none of these may fall
    // through to the neutral "unknown" presentation.
    for conclusion in ["failure", "timed_out", "startup_failure"] {
        let (icon, class) = status_presentation(Some("completed"), Some(conclusion));
        assert_eq!(class, "error", "{conclusion} should tint as an error");
        assert_eq!(icon, "dialog-error-symbolic");
    }

    for conclusion in ["cancelled", "action_required", "stale"] {
        let (icon, class) = status_presentation(Some("completed"), Some(conclusion));
        assert_eq!(class, "warning", "{conclusion} should tint as a warning");
        assert_ne!(icon, "dialog-question-symbolic");
    }

    for conclusion in ["neutral", "skipped"] {
        let (_, class) = status_presentation(Some("completed"), Some(conclusion));
        assert_eq!(class, "idle");
    }
}

#[test]
fn status_presentation_maps_pending_states() {
    assert_eq!(
        status_presentation(Some("in_progress"), None),
        ("media-playback-start-symbolic", "accent")
    );
    for status in ["queued", "waiting", "pending", "requested"] {
        assert_eq!(
            status_presentation(Some(status), None),
            ("alarm-symbolic", "warning"),
            "{status} should read as pending"
        );
    }
}

fn run_stub() -> WorkflowRun {
    WorkflowRun {
        id: 1,
        run_number: None,
        workflow_id: None,
        name: None,
        display_title: None,
        head_branch: None,
        head_commit: None,
        status: None,
        conclusion: None,
        run_started_at: None,
        event: None,
        created_at: None,
        updated_at: None,
        html_url: None,
        actor: None,
        triggering_actor: None,
    }
}

#[test]
fn format_run_title_uses_display_title_and_number() {
    let mut run = run_stub();
    run.run_number = Some(42);
    run.display_title = Some("Nightly".into());

    assert_eq!(format_run_title(&run), "Nightly #42");
}

#[test]
fn get_job_status_icon_handles_unknowns() {
    let job = Job {
        id: 1,
        run_id: 1,
        status: Some("unknown".into()),
        conclusion: None,
        started_at: None,
        completed_at: None,
        name: None,
        steps: Vec::new(),
        html_url: None,
    };

    assert_eq!(get_job_status_icon(&job), "dialog-question-symbolic");
}

#[test]
fn workflow_status_text_is_localized() {
    let _guard = i18n_test_guard();
    init(None);

    let mut run = run_stub();
    run.status = Some("completed".into());
    run.conclusion = Some("failure".into());

    let _ = apply_language_preference(LanguagePreference::De);
    let (text, css_class) = workflow_status_text(&run);
    assert_eq!(text, "Fehlgeschlagen");
    assert_eq!(css_class, "error");

    let _ = apply_language_preference(LanguagePreference::En);
}

#[test]
fn format_workflow_meta_combines_when_branch_and_number() {
    let _guard = i18n_test_guard();
    init(None);

    let mut run = run_stub();
    run.run_number = Some(128);
    run.head_branch = Some("main".into());
    run.run_started_at = Some("2024-01-01T00:00:00Z".into());

    let meta = format_workflow_meta(&run);
    assert!(meta.contains("main"));
    assert!(meta.contains("#128"));
}

#[test]
#[ignore = "requires GTK display"]
fn populate_run_meta_renders_branch_in_mono() {
    run_gtk_test("populate_run_meta_renders_branch_in_mono", || {
        let mut run = run_stub();
        run.head_branch = Some("main".into());
        run.actor = Some(crate::services::api::models::User {
            login: "makoni".into(),
        });

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        populate_run_meta(&container, &run);

        let first = container
            .first_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("branch label");
        assert_eq!(first.text().as_str(), "main");
        assert!(first.has_css_class("mono"));
        assert!(container.is_visible());
    });
}

#[test]
fn run_meta_rest_text_prefers_the_live_elapsed() {
    let _guard = i18n_test_guard();
    init(None);

    let mut run = run_stub();
    run.status = Some("in_progress".into());
    run.run_started_at = Some("2024-01-01T00:00:00Z".into());
    run.updated_at = Some("2024-01-01T00:00:05Z".into());

    let text = run_meta_rest_text(&run, Some("01:23"));
    assert!(
        text.contains("01:23"),
        "the live elapsed must show while running"
    );
    assert!(
        !text.contains("00:05"),
        "the API duration must not leak into a live row"
    );
}

#[test]
fn run_meta_rest_text_falls_back_to_the_api_duration() {
    let _guard = i18n_test_guard();
    init(None);

    let mut run = run_stub();
    run.status = Some("completed".into());
    run.conclusion = Some("success".into());
    run.run_started_at = Some("2024-01-01T00:00:00Z".into());
    run.updated_at = Some("2024-01-01T00:00:45Z".into());

    let text = run_meta_rest_text(&run, None);
    assert!(
        text.contains("00:45"),
        "a finished run shows GitHub's duration"
    );
}

#[test]
fn a_live_run_counts_from_the_best_known_start() {
    let _guard = i18n_test_guard();
    init(None);

    let created = "2026-01-24T10:00:00Z".to_string();
    let started = "2026-01-24T10:00:05Z".to_string();
    let mut run = run_stub();

    // Finished: GitHub's duration is authoritative, nothing should count.
    run.status = Some("completed".into());
    run.run_started_at = Some(started.clone());
    assert_eq!(run_live_start(&run), None);

    // Executing: count from the real start.
    run.status = Some("in_progress".into());
    assert_eq!(run_live_start(&run), Some(started.clone()));

    // Just dispatched: still active, so the timer stays live even before
    // GitHub confirms the start.
    run.status = Some("queued".into());
    assert_eq!(run_live_start(&run), Some(started.clone()));

    // No confirmed start yet (freshly queued): fall back to when the run
    // was created so it is live from the moment it appears in the list.
    run.run_started_at = None;
    run.created_at = Some(created.clone());
    assert_eq!(run_live_start(&run), Some(created.clone()));

    // No creation time either: last resort is `updated_at`.
    run.created_at = None;
    run.updated_at = Some("2026-01-24T10:00:01Z".into());
    assert_eq!(run_live_start(&run), Some("2026-01-24T10:00:01Z".into()));

    // An active run with no timestamps at all has nothing to count from.
    run.updated_at = None;
    assert_eq!(run_live_start(&run), None);
}

#[test]
fn run_meta_live_text_counts_from_the_start() {
    let _guard = i18n_test_guard();
    init(None);

    let mut run = run_stub();
    run.status = Some("in_progress".into());
    let started = (chrono::Utc::now() - chrono::Duration::seconds(65)).to_rfc3339();
    run.run_started_at = Some(started.clone());

    let live = run_meta_live_text(&run, &started).expect("a running run with a start counts up");
    assert!(
        live.contains("01:05") || live.contains("01:06"),
        "expected the live minute:second count, got {live}"
    );

    // A start GitHub never reported cannot be counted.
    assert_eq!(run_meta_live_text(&run, "not a timestamp"), None);
}

#[test]
#[ignore = "requires GTK display"]
fn run_meta_counts_up_while_the_run_is_live() {
    run_gtk_test("run_meta_counts_up_while_the_run_is_live", || {
        let mut run = run_stub();
        run.status = Some("in_progress".into());
        run.run_started_at =
            Some((chrono::Utc::now() - chrono::Duration::seconds(65)).to_rfc3339());

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        populate_run_meta(&container, &run);

        let rest = container
            .last_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("rest label");
        let text = rest.text();
        assert!(
            text.contains("01:05") || text.contains("01:06"),
            "expected the live minute:second count, got {text}"
        );
    });
}
