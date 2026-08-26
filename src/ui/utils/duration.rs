//! Wall-clock durations for items that are still running, shared by the job,
//! step and run rows so every view counts up the same way.

use crate::api::models::{Job, JobStep, format_elapsed};
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};

/// Elapsed `mm:ss` (or `h:mm:ss`) for a job, step or run that is still running.
pub(crate) fn running_duration_string(started_at: Option<&String>) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at?).ok()?;
    let seconds = chrono::Utc::now()
        .signed_duration_since(started.with_timezone(&chrono::Utc))
        .num_seconds()
        .max(0);

    Some(format_elapsed(seconds))
}

/// One tick of a label whose text is recomputed from the wall clock. Split out
/// from the timer so it can be tested without waiting on the clock.
fn tick_text(label: &gtk::Label, compute: impl Fn() -> Option<String>) -> glib::ControlFlow {
    // Rows are rebuilt wholesale on every refresh. A ticker that kept running
    // against a detached label would leak one source per refresh, and each
    // would still be holding its label alive.
    if label.root().is_none() {
        return glib::ControlFlow::Break;
    }

    match compute() {
        Some(text) if text != label.text() => {
            label.set_text(&text);
            glib::ControlFlow::Continue
        }
        Some(_) => glib::ControlFlow::Continue,
        None => glib::ControlFlow::Break,
    }
}

/// Recomputes `label`'s text once a second until it leaves the widget tree, so
/// a running item shows time passing instead of freezing at whatever the last
/// poll happened to report.
pub(crate) fn start_live_text(label: &gtk::Label, compute: impl Fn() -> Option<String> + 'static) {
    let label_weak = label.downgrade();
    glib::timeout_add_seconds_local(1, move || match label_weak.upgrade() {
        Some(label) => tick_text(&label, &compute),
        None => glib::ControlFlow::Break,
    });
}

/// Counts up once a second, so a running job or step shows time passing instead
/// of freezing at whatever the last poll happened to report.
pub(crate) fn start_live_duration(label: &gtk::Label, started_at: String) {
    start_live_text(label, move || running_duration_string(Some(&started_at)))
}

/// `Some(started_at)` when a label should keep counting: the item is executing
/// and GitHub has not yet reported a final duration for it.
fn live_start(
    final_duration: Option<String>,
    status: Option<&str>,
    started_at: Option<&String>,
) -> Option<String> {
    if final_duration.is_some() || !is_in_progress(status) {
        return None;
    }
    started_at.cloned()
}

/// `started_at` is also set while a job/step is still queued, so the wall-clock
/// fallback is only correct once it is actually executing — otherwise a queued
/// item would show a ticking timer, and one that never completed would show a
/// value that keeps growing on every refresh.
fn is_in_progress(status: Option<&str>) -> bool {
    matches!(status, Some("in_progress"))
}

/// Text for a job row's duration label, plus `started_at` when the label should
/// keep counting: the job is executing and GitHub has not yet reported a final
/// duration for it.
pub(crate) fn job_duration_label(job: &Job) -> (Option<String>, Option<String>) {
    let final_duration = job.duration_string();
    let text = final_duration.clone().or_else(|| {
        is_in_progress(job.status.as_deref())
            .then(|| running_duration_string(job.started_at.as_ref()))
            .flatten()
    });
    let live = live_start(
        final_duration,
        job.status.as_deref(),
        job.started_at.as_ref(),
    );
    (text, live)
}

/// Same as `job_duration_label` for a job step.
pub(crate) fn step_duration_label(step: &JobStep) -> (Option<String>, Option<String>) {
    let final_duration = step.duration_string();
    let text = final_duration.clone().or_else(|| {
        is_in_progress(step.status.as_deref())
            .then(|| running_duration_string(step.started_at.as_ref()))
            .flatten()
    });
    let live = live_start(
        final_duration,
        step.status.as_deref(),
        step.started_at.as_ref(),
    );
    (text, live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    fn only_a_running_item_without_a_final_duration_ticks() {
        let started = "2026-01-24T10:00:00Z".to_string();

        // Finished: GitHub reported the real duration, so nothing should count.
        assert_eq!(
            live_start(Some("18s".into()), Some("completed"), Some(&started)),
            None
        );
        // Queued items carry `started_at` (queue time) but are not executing.
        assert_eq!(live_start(None, Some("queued"), Some(&started)), None);
        // Running but GitHub has not told us when — nothing to count from.
        assert_eq!(live_start(None, Some("in_progress"), None), None);

        assert_eq!(
            live_start(None, Some("in_progress"), Some(&started)),
            Some(started)
        );
    }

    #[test]
    fn job_duration_label_falls_back_to_wall_clock_while_running() {
        let job = |status: &str, completed_at: Option<&str>| Job {
            id: 1,
            run_id: 1,
            status: Some(status.to_string()),
            conclusion: None,
            started_at: Some("2026-01-24T10:00:00Z".to_string()),
            completed_at: completed_at.map(str::to_string),
            name: None,
            steps: Vec::new(),
            html_url: None,
        };

        // Finished: GitHub reported the final duration, no wall-clock fallback.
        assert_eq!(
            job_duration_label(&job("completed", Some("2026-01-24T10:00:18Z"))),
            (Some("00:18".into()), None)
        );
        // Running without a final duration: wall-clock text and a live start.
        let (text, live) = job_duration_label(&job("in_progress", None));
        assert!(text.is_some());
        assert_eq!(live.as_deref(), Some("2026-01-24T10:00:00Z"));
        // Queued: `started_at` is the queue time, not an execution start.
        assert_eq!(job_duration_label(&job("queued", None)), (None, None));
    }

    #[test]
    fn step_duration_label_falls_back_to_wall_clock_while_running() {
        let step = |status: Option<&str>, started_at: Option<&str>| JobStep {
            name: Some("Build".into()),
            status: status.map(str::to_string),
            conclusion: None,
            number: Some(7),
            started_at: started_at.map(str::to_string),
            completed_at: None,
        };

        // A queued step carries `started_at` (queue time) but must not tick.
        assert_eq!(
            step_duration_label(&step(Some("queued"), Some("2020-01-01T00:00:00Z"))),
            (None, None)
        );
        // A step abandoned without `completed_at` must not grow forever either.
        assert_eq!(
            step_duration_label(&step(Some("completed"), Some("2020-01-01T00:00:00Z"))),
            (None, None)
        );
        // An executing step shows elapsed wall-clock time and keeps counting.
        let (text, live) =
            step_duration_label(&step(Some("in_progress"), Some("2020-01-01T00:00:00Z")));
        assert!(text.is_some());
        assert_eq!(live.as_deref(), Some("2020-01-01T00:00:00Z"));
    }

    #[test]
    fn running_duration_formats_mm_ss() {
        let started = (chrono::Utc::now() - chrono::Duration::seconds(72)).to_rfc3339();
        assert_eq!(
            running_duration_string(Some(&started)),
            Some("01:12".into())
        );

        let long = (chrono::Utc::now() - chrono::Duration::seconds(3661)).to_rfc3339();
        assert_eq!(running_duration_string(Some(&long)), Some("1:01:01".into()));

        assert_eq!(running_duration_string(None), None);
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn live_duration_ticks_while_attached_and_stops_once_detached() {
        run_gtk_test(
            "live_duration_ticks_while_attached_and_stops_once_detached",
            || {
                let label = gtk::Label::new(Some("--"));
                let window = gtk::Window::new();
                window.set_child(Some(&label));

                let started = (chrono::Utc::now() - chrono::Duration::seconds(65)).to_rfc3339();
                assert_eq!(
                    tick_text(&label, || running_duration_string(Some(&started))),
                    glib::ControlFlow::Continue
                );
                assert!(
                    ["01:05", "01:06"].contains(&label.text().as_str()),
                    "expected a live minute:second count, got {:?}",
                    label.text()
                );

                // A step that never reported a start cannot be counted: stop rather
                // than tick forever against nothing.
                assert_eq!(
                    tick_text(&label, || running_duration_string(Some(
                        &"not a timestamp".to_string()
                    ))),
                    glib::ControlFlow::Break
                );

                // Rows are rebuilt on every refresh; the old label's ticker has to
                // die with it or each refresh leaves another source running.
                window.set_child(None::<&gtk::Widget>);
                assert_eq!(
                    tick_text(&label, || running_duration_string(Some(&started))),
                    glib::ControlFlow::Break
                );

                window.destroy();
            },
        );
    }
}
