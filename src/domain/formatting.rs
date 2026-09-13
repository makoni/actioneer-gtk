//! Pure formatting: durations and elapsed time.
//!
//! No GTK and no localization — every function here returns the same string in
//! every language. Anything that needs `tr` belongs in
//! `ui/detail_view/helpers/formatting.rs` instead.

use super::models::{Job, JobStep};

/// Elapsed seconds as `mm:ss`, or `h:mm:ss` past the hour.
///
/// No language-specific units: a run shows the same shape while it is live
/// (counting from the start) and after it finishes (from the final duration).
pub fn format_elapsed(seconds: i64) -> String {
    if seconds < 3600 {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    } else {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    }
}

/// Elapsed `mm:ss` (or `h:mm:ss`) for a job, step or run that is still running.
pub fn running_duration_string(started_at: Option<&String>) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at?).ok()?;
    let seconds = chrono::Utc::now()
        .signed_duration_since(started.with_timezone(&chrono::Utc))
        .num_seconds()
        .max(0);

    Some(format_elapsed(seconds))
}

pub fn is_in_progress(status: Option<&str>) -> bool {
    matches!(status, Some("in_progress"))
}

/// The duration text for a job row: the final duration once GitHub reports one,
/// otherwise a wall-clock count while the job is executing.
pub fn job_duration_text(job: &Job) -> Option<String> {
    job.duration_string().or_else(|| {
        is_in_progress(job.status.as_deref())
            .then(|| running_duration_string(job.started_at.as_ref()))
            .flatten()
    })
}

/// Same as [`job_duration_text`] for a job step.
pub fn step_duration_text(step: &JobStep) -> Option<String> {
    step.duration_string().or_else(|| {
        is_in_progress(step.status.as_deref())
            .then(|| running_duration_string(step.started_at.as_ref()))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_under_an_hour_is_minutes_and_seconds() {
        assert_eq!(format_elapsed(0), "00:00");
        assert_eq!(format_elapsed(9), "00:09");
        assert_eq!(format_elapsed(75), "01:15");
        assert_eq!(format_elapsed(3599), "59:59");
    }

    #[test]
    fn elapsed_past_an_hour_gains_an_hours_field() {
        assert_eq!(format_elapsed(3600), "1:00:00");
        assert_eq!(format_elapsed(3661), "1:01:01");
        assert_eq!(format_elapsed(86_399), "23:59:59");
    }

    #[test]
    fn only_an_in_progress_status_counts_as_running() {
        assert!(is_in_progress(Some("in_progress")));
        for other in ["queued", "completed", "waiting", "IN_PROGRESS", ""] {
            assert!(!is_in_progress(Some(other)), "{other} is not in progress");
        }
        assert!(!is_in_progress(None));
    }

    #[test]
    fn a_running_duration_needs_a_parsable_start() {
        assert_eq!(running_duration_string(None), None);
        assert_eq!(
            running_duration_string(Some(&"not a date".to_string())),
            None
        );
        assert!(running_duration_string(Some(&"2024-01-08T13:45:00Z".to_string())).is_some());
    }
}
