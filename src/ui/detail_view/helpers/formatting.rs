use crate::domain::formatting::running_duration_string;
use crate::kernel::i18n::tr;
use crate::services::api::models::{Job, WorkflowRun};
use crate::ui::utils::duration::start_live_text;
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

    let status_text = friendly_status(run.status.as_deref(), run.conclusion.as_deref());
    if !status_text.is_empty() {
        lines.push(format!("{}: {}", tr("Status"), status_text));
    }

    if run.conclusion.is_some() {
        let conclusion_text = friendly_conclusion(run.conclusion.as_deref());
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

    let when = relative_time(run);
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
    let when = relative_time(run);
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
        friendly_conclusion(latest_run.conclusion.as_deref())
    } else {
        match latest_run.status.as_deref() {
            Some("in_progress") => tr("In Progress"),
            Some("queued" | "waiting" | "pending" | "requested") => tr("Queued"),
            _ => friendly_status(
                latest_run.status.as_deref(),
                latest_run.conclusion.as_deref(),
            ),
        }
    };

    let text = if text.is_empty() { tr("Unknown") } else { text };

    (text, class)
}

// ---------------------------------------------------------------------------
// Display strings for run, job and step state.
//
// These used to be six near-identical `friendly_status`/`friendly_conclusion`
// methods on `WorkflowRun`, `Job` and `JobStep`, which put `tr` — and therefore
// the UI's language — inside the data models. The models now carry raw data
// only; this is the display-formatting home, where `tr` belongs.
//
// Unified into two functions because all three impls matched on the same
// strings. One cosmetic difference is folded in deliberately: `Job` and
// `JobStep` previously fell through on `"stale"` and rendered it verbatim,
// where `WorkflowRun` translated it. GitHub only ever reports `stale` on a run,
// so the case is unreachable for the other two.
// ---------------------------------------------------------------------------

/// A human-readable status, falling through to the conclusion once complete.
pub fn friendly_status(status: Option<&str>, conclusion: Option<&str>) -> String {
    let Some(status) = status else {
        return tr("Unknown");
    };
    match status.to_lowercase().as_str() {
        "queued" => tr("Queued"),
        "in_progress" => tr("In Progress"),
        "completed" => {
            if conclusion.is_some() {
                friendly_conclusion(conclusion)
            } else {
                tr("Completed")
            }
        }
        "waiting" => tr("Waiting"),
        "requested" => tr("Requested"),
        "pending" => tr("Pending"),
        _ => status.replace('_', " "),
    }
}

/// A human-readable conclusion, or empty when there is none yet.
pub fn friendly_conclusion(conclusion: Option<&str>) -> String {
    let Some(conclusion) = conclusion else {
        return String::new();
    };
    match conclusion.to_lowercase().as_str() {
        "success" => tr("Success"),
        "failure" => tr("Failed"),
        "cancelled" => tr("Cancelled"),
        "skipped" => tr("Skipped"),
        "timed_out" => tr("Timed Out"),
        "action_required" => tr("Action Required"),
        "neutral" => tr("Neutral"),
        "stale" => tr("Stale"),
        _ => conclusion.replace('_', " "),
    }
}

/// The run's age, as "Just now", "5m ago", "Yesterday", or a localized date.
pub fn relative_time(run: &WorkflowRun) -> String {
    run.run_started_at
        .as_ref()
        .or(run.created_at.as_ref())
        .or(run.updated_at.as_ref())
        .map(|ts| relative_time_from_iso(ts))
        .unwrap_or_default()
}

fn relative_time_from_iso(iso_string: &str) -> String {
    use chrono::{DateTime, Local, Utc};

    // Try parsing the ISO string
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_string) {
        let now = Utc::now();
        let duration = now.signed_duration_since(dt.with_timezone(&Utc));

        let seconds = duration.num_seconds();

        if seconds < 60 {
            tr("Just now")
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            tr("{count}m ago").replace("{count}", minutes.to_string().as_str())
        } else if seconds < 86400 {
            let hours = seconds / 3600;
            tr("{count}h ago").replace("{count}", hours.to_string().as_str())
        } else if seconds < 604800 {
            let days = seconds / 86400;
            if days == 1 {
                tr("Yesterday")
            } else {
                tr("{count}d ago").replace("{count}", days.to_string().as_str())
            }
        } else {
            localized_absolute_date_from_iso(iso_string)
                .unwrap_or_else(|| fallback_localized_numeric_date(dt.with_timezone(&Local)))
        }
    } else {
        String::new()
    }
}

fn localized_absolute_date_from_iso(iso_string: &str) -> Option<String> {
    let date_time = gtk4::glib::DateTime::from_iso8601(iso_string, None).ok()?;
    let local_date_time = date_time.to_local().ok()?;
    let formatted = local_date_time.format("%x").ok()?;
    let formatted = formatted.trim();

    if formatted.is_empty() {
        None
    } else {
        Some(formatted.to_string())
    }
}

fn fallback_localized_numeric_date(date_time: chrono::DateTime<chrono::Local>) -> String {
    match crate::kernel::i18n::current_effective_language().as_str() {
        "en" => date_time.format("%m/%d/%Y").to_string(),
        "zh_Hans" => date_time.format("%Y/%m/%d").to_string(),
        "pt_BR" | "fr" | "es" | "hi" | "ar" | "bn" | "ur" => {
            date_time.format("%d/%m/%Y").to_string()
        }
        "de" | "nl" | "ru" => date_time.format("%d.%m.%Y").to_string(),
        _ => date_time.format("%Y-%m-%d").to_string(),
    }
}

#[cfg(test)]
mod tests;
