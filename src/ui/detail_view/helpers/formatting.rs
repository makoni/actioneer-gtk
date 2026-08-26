use crate::api::models::{Job, WorkflowRun};
use crate::i18n::tr;
use crate::ui::utils::duration::{running_duration_string, start_live_text};
use gtk4::prelude::*;
use gtk4::{self as gtk, pango};

pub(crate) fn format_run_title(run: &WorkflowRun) -> String {
    let workflow_run_fallback = tr("Workflow Run");
    let base = run
        .display_title
        .as_ref()
        .or(run.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(workflow_run_fallback.as_str());

    if let Some(num) = run.run_number {
        format!("{} #{}", base, num)
    } else {
        base.to_string()
    }
}

pub(crate) fn format_run_tooltip(run: &WorkflowRun) -> String {
    let mut lines = Vec::new();

    let status_text = run.friendly_status();
    if !status_text.is_empty() {
        lines.push(format!("{}: {}", tr("Status"), status_text));
    }

    if run.conclusion.is_some() {
        let conclusion_text = run.friendly_conclusion();
        if !conclusion_text.is_empty() {
            lines.push(format!("{}: {}", tr("Conclusion"), conclusion_text));
        }
    }

    if let Some(branch) = &run.head_branch {
        lines.push(format!("{}: {}", tr("Branch"), branch));
    }

    if let Some(commit) = run.commit_message_headline() {
        lines.push(format!("{}: {}", tr("Commit"), commit));
    }

    if let Some(actor) = run.actor_login() {
        lines.push(format!("{}: {}", tr("Triggered by"), actor));
    }

    if let Some(event) = &run.event {
        lines.push(format!("{}: {}", tr("Event"), event));
    }

    if let Some(duration) = run.run_duration_string() {
        lines.push(format!("{}: {}", tr("Duration"), duration));
    }

    if let Some(created) = &run.run_started_at {
        lines.push(format!("{}: {}", tr("Started"), created));
    } else if let Some(created) = &run.created_at {
        lines.push(format!("{}: {}", tr("Started"), created));
    }

    if let Some(updated) = &run.updated_at {
        lines.push(format!("{}: {}", tr("Updated"), updated));
    }

    lines.join("\n")
}

/// Meta line shown under a workflow title: `<when> · <branch> · #<run>`.
pub(crate) fn format_workflow_meta(run: &WorkflowRun) -> String {
    let mut parts = Vec::new();

    let when = run.relative_time_string();
    if !when.is_empty() {
        parts.push(when);
    }

    if let Some(branch) = &run.head_branch {
        parts.push(branch.clone());
    }

    if let Some(num) = run.run_number {
        parts.push(format!("#{num}"));
    }

    parts.join(" · ")
}

/// The meta of a run row after its branch: `<actor> · <duration> · <when>`.
///
/// `live_elapsed` is the wall-clock `mm:ss` while the run is still going; once
/// it is done, GitHub's own duration applies.
fn run_meta_rest_text(run: &WorkflowRun, live_elapsed: Option<&str>) -> String {
    let mut rest = Vec::new();
    if let Some(actor) = run.actor_login() {
        rest.push(actor);
    }
    if let Some(duration) = live_elapsed
        .map(str::to_owned)
        .or_else(|| run.run_duration_string())
    {
        rest.push(duration);
    }
    let when = run.relative_time_string();
    if !when.is_empty() {
        rest.push(when);
    }

    rest.join(" · ")
}

/// `Some(started_at)` when a run row should count up from the wall clock.
///
/// A dispatched run is active from the moment it appears in the list — first
/// `queued`, then `in_progress` — but GitHub only reports `run_started_at` once
/// it actually starts. Count from the real start, falling back to when the run
/// was created, so the timer is live immediately instead of sitting at `00:00`
/// until the first job reports in.
fn run_live_start(run: &WorkflowRun) -> Option<String> {
    if !run.is_active() {
        return None;
    }
    run.run_started_at
        .clone()
        .or_else(|| run.created_at.clone())
        .or_else(|| run.updated_at.clone())
}

/// One tick of a live run meta: the elapsed time since the start, re-joined
/// with the actor and the start time.
fn run_meta_live_text(run: &WorkflowRun, started_at: &str) -> Option<String> {
    running_duration_string(Some(&started_at.to_string()))
        .map(|elapsed| run_meta_rest_text(run, Some(&elapsed)))
}

/// Fills `container` with the run meta line:
/// `<branch>` (mono) `· <actor> · <duration> · <when>`.
pub(crate) fn populate_run_meta(container: &gtk::Box, run: &WorkflowRun) {
    loop {
        let child_opt = container.first_child();
        let Some(child) = child_opt else {
            break;
        };
        container.remove(&child);
    }

    if let Some(branch) = run.head_branch.as_ref().filter(|b| !b.is_empty()) {
        let branch_label = gtk::Label::new(Some(branch));
        branch_label.add_css_class("mono");
        branch_label.add_css_class("dim-label");
        branch_label.add_css_class("caption");
        // Long branch names (renovate/…, dependabot/…) must not set the pane's
        // minimum width: the scroller has no horizontal bar to fall back on.
        branch_label.set_ellipsize(pango::EllipsizeMode::End);
        branch_label.set_max_width_chars(28);
        container.append(&branch_label);
    }

    let live = run_live_start(run);
    let live_elapsed = live
        .as_ref()
        .and_then(|started_at| running_duration_string(Some(started_at)));
    let rest_text = run_meta_rest_text(run, live_elapsed.as_deref());

    if !rest_text.is_empty() {
        if container.first_child().is_some() {
            let separator = gtk::Label::new(Some("·"));
            separator.add_css_class("dim-label");
            separator.add_css_class("caption");
            container.append(&separator);
        }

        let rest_label = gtk::Label::new(Some(&rest_text));
        rest_label.add_css_class("dim-label");
        rest_label.add_css_class("caption");
        rest_label.set_ellipsize(pango::EllipsizeMode::End);
        container.append(&rest_label);

        if let Some(started_at) = live {
            let run_for_ticker = run.clone();
            start_live_text(&rest_label, move || {
                run_meta_live_text(&run_for_ticker, &started_at)
            });
        }
    }

    container.set_visible(container.first_child().is_some());
}

/// Maps a GitHub status/conclusion pair onto the icon and tint class used by the
/// status dots.
///
/// Every documented conclusion is handled: the dot is now the only status signal
/// on a row, so an unmapped one (e.g. `timed_out`) would silently render as the
/// neutral "unknown" dot and hide a real failure.
///
/// Neutral states use `idle`, never libadwaita's `dim-label`: that class is an
/// opacity utility (0.55) which would dim the whole dot on top of the glyph's
/// own alpha, leaving it almost invisible.
pub(crate) fn status_presentation(
    status: Option<&str>,
    conclusion: Option<&str>,
) -> (&'static str, &'static str) {
    if let Some(conclusion) = conclusion {
        return match conclusion {
            "success" => ("object-select-symbolic", "success"),
            "failure" | "timed_out" | "startup_failure" => ("dialog-error-symbolic", "error"),
            "cancelled" => ("process-stop-symbolic", "warning"),
            "action_required" | "stale" => ("dialog-warning-symbolic", "warning"),
            "neutral" | "skipped" => ("media-skip-forward-symbolic", "idle"),
            _ => ("dialog-question-symbolic", "idle"),
        };
    }

    match status {
        Some("queued" | "waiting" | "pending" | "requested") => ("alarm-symbolic", "warning"),
        Some("in_progress") => ("media-playback-start-symbolic", "accent"),
        _ => ("dialog-question-symbolic", "idle"),
    }
}

pub(crate) fn get_run_status_icon(run: &WorkflowRun) -> &'static str {
    status_presentation(run.status.as_deref(), run.conclusion.as_deref()).0
}

pub(crate) fn get_run_status_class(run: &WorkflowRun) -> &'static str {
    status_presentation(run.status.as_deref(), run.conclusion.as_deref()).1
}

pub(crate) fn get_job_status_icon(job: &Job) -> &'static str {
    status_presentation(job.status.as_deref(), job.conclusion.as_deref()).0
}

pub(crate) fn get_job_status_class(job: &Job) -> &'static str {
    status_presentation(job.status.as_deref(), job.conclusion.as_deref()).1
}

/// Human-readable status for a workflow's latest run, paired with the same tint
/// class the status dot uses (so the tooltip and the dot can never disagree).
pub(crate) fn workflow_status_text(latest_run: &WorkflowRun) -> (String, &'static str) {
    let class = get_run_status_class(latest_run);

    let text = if latest_run.conclusion.is_some() {
        latest_run.friendly_conclusion()
    } else {
        match latest_run.status.as_deref() {
            Some("in_progress") => tr("In Progress"),
            Some("queued" | "waiting" | "pending" | "requested") => tr("Queued"),
            _ => latest_run.friendly_status(),
        }
    };

    let text = if text.is_empty() { tr("Unknown") } else { text };

    (text, class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{Job, WorkflowRun};
    use crate::i18n::{apply_language_preference, i18n_test_guard, init};
    use crate::preferences::LanguagePreference;
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
            run.actor = Some(crate::api::models::User {
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

        let live =
            run_meta_live_text(&run, &started).expect("a running run with a start counts up");
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
}
