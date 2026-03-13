use crate::api::models::{Job, JobSummary, WorkflowRun};
use crate::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk};

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

pub(crate) fn format_run_subtitle(run: &WorkflowRun) -> String {
    let mut parts = Vec::new();

    let status_text = run.friendly_status();
    if !status_text.is_empty() {
        parts.push(status_text);
    }

    if run.conclusion.is_some() {
        let conclusion_text = run.friendly_conclusion();
        if !conclusion_text.is_empty()
            && !parts
                .iter()
                .any(|part| part.eq_ignore_ascii_case(&conclusion_text))
        {
            parts.push(conclusion_text);
        }
    }

    if let Some(branch) = &run.head_branch {
        parts.push(branch.clone());
    }

    let time_str = run.relative_time_string();
    if !time_str.is_empty() {
        parts.push(time_str);
    }

    parts.join(" • ")
}

pub(crate) fn format_job_status(job: &Job) -> String {
    job.friendly_status()
}

pub(crate) fn get_run_status_icon(run: &WorkflowRun) -> &'static str {
    if let Some(conclusion) = run.conclusion.as_deref() {
        match conclusion {
            "success" => "emblem-default-symbolic",
            "failure" => "dialog-error-symbolic",
            "cancelled" => "process-stop-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else if let Some(status) = run.status.as_deref() {
        match status {
            "queued" | "waiting" => "alarm-symbolic",
            "in_progress" => "media-playback-start-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else {
        "dialog-question-symbolic"
    }
}

pub(crate) fn get_run_status_class(run: &WorkflowRun) -> &'static str {
    if let Some(conclusion) = run.conclusion.as_deref() {
        return match conclusion {
            "success" => "success",
            "failure" => "error",
            "cancelled" => "warning",
            _ => "dim-label",
        };
    }
    if let Some("in_progress") = run.status.as_deref() {
        return "accent";
    }
    "dim-label"
}

pub(crate) fn get_job_status_icon(job: &Job) -> &'static str {
    if let Some(conclusion) = job.conclusion.as_deref() {
        match conclusion {
            "success" => "emblem-default-symbolic",
            "failure" => "dialog-error-symbolic",
            "cancelled" => "process-stop-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else if let Some(status) = job.status.as_deref() {
        match status {
            "queued" | "waiting" => "alarm-symbolic",
            "in_progress" => "media-playback-start-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else {
        "dialog-question-symbolic"
    }
}

pub(crate) fn get_job_status_class(job: &Job) -> &'static str {
    if let Some(conclusion) = job.conclusion.as_deref() {
        return match conclusion {
            "success" => "success",
            "failure" => "error",
            "cancelled" => "warning",
            _ => "",
        };
    }
    if let Some("in_progress") = job.status.as_deref() {
        return "accent";
    }
    ""
}

pub(crate) fn update_job_summary_badges(badges_box: &gtk::Box, jobs: &[Job]) {
    update_job_summary_badges_from_summary(badges_box, &JobSummary::from_jobs(jobs));
}

pub(crate) fn update_job_summary_badges_from_summary(badges_box: &gtk::Box, summary: &JobSummary) {
    loop {
        let child_opt = badges_box.first_child();
        let Some(child) = child_opt else {
            break;
        };
        badges_box.remove(&child);
    }

    if summary.is_empty() {
        return;
    }

    if summary.completed > 0 {
        let badge = create_job_badge(
            "emblem-default-symbolic",
            &summary.completed.to_string(),
            "success",
        );
        badges_box.append(&badge);
    }

    if summary.running > 0 {
        let badge = create_job_badge(
            "media-playback-start-symbolic",
            &summary.running.to_string(),
            "accent",
        );
        badges_box.append(&badge);
    }

    if summary.queued > 0 {
        let badge = create_job_badge("alarm-symbolic", &summary.queued.to_string(), "warning");
        badges_box.append(&badge);
    }
}

fn create_job_badge(icon_name: &str, count: &str, css_class: &str) -> gtk::Box {
    let badge = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    badge.add_css_class("badge");
    badge.set_valign(gtk::Align::Start);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(12);
    icon.add_css_class(css_class);
    badge.append(&icon);

    let label = gtk::Label::new(Some(count));
    label.add_css_class("caption");
    label.add_css_class(css_class);
    badge.append(&label);

    badge
}

fn workflow_status_badge_content(latest_run: &WorkflowRun) -> (String, &'static str) {
    match (
        latest_run.status.as_deref(),
        latest_run.conclusion.as_deref(),
    ) {
        (Some("completed"), Some("success")) => (tr("Success"), "success"),
        (Some("completed"), Some("failure")) => (tr("Failed"), "error"),
        (Some("completed"), Some("cancelled")) => (tr("Cancelled"), "warning"),
        (Some("in_progress"), _) => (tr("In Progress"), "accent"),
        (Some("queued"), _) | (Some("waiting"), _) => (tr("Queued"), "warning"),
        _ => (tr("Unknown"), "dim-label"),
    }
}

pub(crate) fn update_workflow_status_badge(badge: &gtk::Label, latest_run: &WorkflowRun) {
    let (text, css_class) = workflow_status_badge_content(latest_run);

    badge.set_text(&text);

    let classes = ["success", "error", "warning", "accent", "dim-label"];
    for class in classes {
        badge.remove_css_class(class);
    }

    badge.add_css_class(css_class);
    badge.set_visible(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{Job, WorkflowRun};
    use crate::i18n::{apply_language_preference, i18n_test_guard, init};
    use crate::preferences::LanguagePreference;
    use crate::ui::test_helpers::gtk_test_guard;

    fn run_stub() -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: None,
            workflow_id: None,
            name: None,
            display_title: None,
            head_branch: None,
            status: None,
            conclusion: None,
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: None,
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
    fn format_run_subtitle_includes_parts() {
        let mut run = run_stub();
        run.status = Some("completed".into());
        run.conclusion = Some("success".into());
        run.head_branch = Some("main".into());
        run.updated_at = Some("2024-01-01T00:00:00Z".into());

        let subtitle = format_run_subtitle(&run);
        assert!(subtitle.contains("Success"));
        assert!(subtitle.contains("main"));
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
    fn workflow_status_badge_content_is_localized() {
        let _guard = i18n_test_guard();
        init(None);

        let mut run = run_stub();
        run.status = Some("completed".into());
        run.conclusion = Some("failure".into());

        let _ = apply_language_preference(LanguagePreference::De);
        let (text, css_class) = workflow_status_badge_content(&run);
        assert_eq!(text, "Fehlgeschlagen");
        assert_eq!(css_class, "error");

        let _ = apply_language_preference(LanguagePreference::En);
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn update_job_summary_badges_adds_expected_children() {
        let Some(_guard) = gtk_test_guard("update_job_summary_badges_adds_expected_children")
        else {
            return;
        };

        let jobs = vec![
            Job {
                id: 1,
                run_id: 1,
                status: Some("in_progress".into()),
                conclusion: None,
                started_at: None,
                completed_at: None,
                name: None,
                steps: Vec::new(),
                html_url: None,
            },
            Job {
                id: 2,
                run_id: 1,
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                started_at: None,
                completed_at: None,
                name: None,
                steps: Vec::new(),
                html_url: None,
            },
        ];

        let badges_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        update_job_summary_badges(&badges_box, &jobs);

        let mut count = 0;
        let mut child = badges_box.first_child();
        while child.is_some() {
            count += 1;
            child = child.unwrap().next_sibling();
        }

        assert!(count >= 2);
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn update_workflow_status_badge_sets_class() {
        let Some(_guard) = gtk_test_guard("update_workflow_status_badge_sets_class") else {
            return;
        };

        let mut run = run_stub();
        run.status = Some("completed".into());
        run.conclusion = Some("failure".into());

        let badge = gtk::Label::new(None);
        update_workflow_status_badge(&badge, &run);

        assert_eq!(badge.text().as_str(), "Failed");
        assert!(badge.style_context().has_class("error"));
    }
}
