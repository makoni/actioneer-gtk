use crate::api::models::{Job, WorkflowRun};
use crate::i18n::tr;
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
        container.append(&branch_label);
    }

    let mut rest = Vec::new();
    if let Some(actor) = run.actor_login() {
        rest.push(actor);
    }
    if let Some(duration) = run.run_duration_string() {
        rest.push(duration);
    }
    let when = run.relative_time_string();
    if !when.is_empty() {
        rest.push(when);
    }

    if !rest.is_empty() {
        if container.first_child().is_some() {
            let separator = gtk::Label::new(Some("·"));
            separator.add_css_class("dim-label");
            separator.add_css_class("caption");
            container.append(&separator);
        }

        let rest_label = gtk::Label::new(Some(&rest.join(" · ")));
        rest_label.add_css_class("dim-label");
        rest_label.add_css_class("caption");
        rest_label.set_ellipsize(pango::EllipsizeMode::End);
        container.append(&rest_label);
    }

    container.set_visible(container.first_child().is_some());
}

pub(crate) fn get_run_status_icon(run: &WorkflowRun) -> &'static str {
    if let Some(conclusion) = run.conclusion.as_deref() {
        match conclusion {
            "success" => "object-select-symbolic",
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
            "success" => "object-select-symbolic",
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

pub(crate) fn workflow_status_text(latest_run: &WorkflowRun) -> (String, &'static str) {
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
        let Some(_guard) = gtk_test_guard("populate_run_meta_renders_branch_in_mono") else {
            return;
        };

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
    }
}
