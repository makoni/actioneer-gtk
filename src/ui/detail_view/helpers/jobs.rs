use super::context::{JobContextMap, JobRefreshContext, JobRefreshContextParams};
use super::formatting::{
    format_job_status, get_job_status_class, get_job_status_icon, update_job_summary_badges,
};
use crate::api::models::{Job, JobStep, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::job_logs_window::JobLogsWindow;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

pub(super) struct LoadJobsParams {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) run_id: i64,
    pub(super) expander: gtk::Expander,
    pub(super) jobs_box: gtk::Box,
    pub(super) badges_box: Option<gtk::Box>,
    pub(super) workflow_id: i64,
    pub(super) parent_window: gtk::Window,
    pub(super) repo_model: Repo,
    pub(super) background: bool,
    pub(super) job_contexts: JobContextMap,
    pub(super) run_branch: Option<String>,
    pub(super) run_title: String,
}

#[derive(Clone)]
pub(super) struct JobRowContext {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) parent_window: gtk::Window,
    pub(super) repo: Repo,
    pub(super) branch: Option<String>,
    pub(super) run_title: String,
}

pub(super) fn create_job_row_simple(job: &Job, context: Option<JobRowContext>) -> gtk::Box {
    let job_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    job_box.set_margin_top(4);
    job_box.set_margin_bottom(4);
    job_box.set_hexpand(true);
    job_box.add_css_class("job-row");
    job_box.add_css_class("hoverless-row");

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_row.set_valign(gtk::Align::Center);
    header_row.set_hexpand(true);

    let icon = gtk::Image::from_icon_name(get_job_status_icon(job));
    let status_class = get_job_status_class(job);
    if !status_class.is_empty() {
        icon.add_css_class(status_class);
    }
    icon.set_valign(gtk::Align::Center);
    header_row.append(&icon);

    let fallback_job_name = tr("Unnamed job");
    let job_name_label = gtk::Label::new(Some(
        job.name.as_deref().unwrap_or(fallback_job_name.as_str()),
    ));
    job_name_label.set_halign(gtk::Align::Start);
    job_name_label.set_hexpand(true);
    job_name_label.set_valign(gtk::Align::Center);
    job_name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header_row.append(&job_name_label);

    let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    right_box.set_valign(gtk::Align::Center);
    right_box.set_halign(gtk::Align::End);
    right_box.set_hexpand(false);

    if let Some(ctx) = context.as_ref()
        && let Some(branch) = ctx.branch.as_deref()
    {
        let branch_label = gtk::Label::new(Some(branch));
        branch_label.add_css_class("dim-label");
        branch_label.add_css_class("caption");
        branch_label.set_valign(gtk::Align::Center);
        right_box.append(&branch_label);
    }

    let status_text = format_job_status(job);
    let status_label = gtk::Label::new(Some(&status_text));
    status_label.add_css_class("dim-label");
    status_label.add_css_class("caption");
    status_label.set_valign(gtk::Align::Center);
    right_box.append(&status_label);

    if let Some(duration) = job.duration_string() {
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.add_css_class("dim-label");
        duration_label.add_css_class("caption");
        duration_label.set_valign(gtk::Align::Center);
        right_box.append(&duration_label);
    }

    if let Some(ctx) = context {
        let logs_button = gtk::Button::from_icon_name("text-x-generic-symbolic");
        logs_button.set_tooltip_text(Some(tr("View logs").as_str()));
        logs_button.add_css_class("flat");
        logs_button.add_css_class("circular");
        logs_button.set_valign(gtk::Align::Center);
        logs_button.set_focus_on_click(false);

        let parent_window = ctx.parent_window.clone();
        let repo_model = ctx.repo.clone();
        let client = ctx.client.clone();
        let job_for_logs = job.clone();
        let run_title = ctx.run_title.clone();
        logs_button.connect_clicked(move |_| {
            let logs_window = JobLogsWindow::new(
                &parent_window,
                repo_model.clone(),
                run_title.clone(),
                job_for_logs.clone(),
                client.clone(),
            );
            logs_window.present();
        });

        right_box.append(&logs_button);
    }

    if let Some(ref url) = job.html_url {
        let open_btn = gtk::Button::from_icon_name("adw-external-link-symbolic");
        open_btn.set_tooltip_text(Some(tr("Open job in GitHub").as_str()));
        open_btn.add_css_class("flat");
        open_btn.add_css_class("circular");
        open_btn.set_valign(gtk::Align::Center);
        open_btn.set_focus_on_click(false);

        let url_clone = url.clone();
        open_btn.connect_clicked(move |_| {
            if let Err(e) = open::that(&url_clone) {
                error!("Failed to open URL: {}", e);
            }
        });

        right_box.append(&open_btn);
    }

    header_row.append(&right_box);
    job_box.append(&header_row);

    if !job.steps.is_empty() {
        let steps_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        steps_box.set_margin_start(24);
        steps_box.set_margin_end(8);
        steps_box.set_margin_bottom(4);
        steps_box.set_hexpand(true);

        for step in &job.steps {
            steps_box.append(&create_job_step_row(step));
        }

        job_box.append(&steps_box);
    }

    job_box
}

fn create_job_step_row(step: &JobStep) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_hexpand(true);
    row.add_css_class("caption");

    let icon = gtk::Image::from_icon_name(step_status_icon(step));
    let status_class = step_status_class(step);
    if !status_class.is_empty() {
        icon.add_css_class(status_class);
    }
    icon.set_valign(gtk::Align::Center);
    row.append(&icon);

    let title = match (step.number, step.name.as_deref()) {
        (Some(number), Some(name)) => format!("{number}. {name}"),
        (_, Some(name)) => name.to_string(),
        (Some(number), None) => format!("#{number}"),
        (None, None) => tr("Unknown"),
    };
    let name_label = gtk::Label::new(Some(&title));
    name_label.add_css_class("dim-label");
    name_label.set_halign(gtk::Align::Start);
    name_label.set_hexpand(true);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&name_label);

    let status_label = gtk::Label::new(Some(&step.friendly_status()));
    status_label.add_css_class("dim-label");
    status_label.add_css_class("caption");
    status_label.set_valign(gtk::Align::Center);
    row.append(&status_label);

    if let Some(duration) = step.duration_string() {
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.add_css_class("dim-label");
        duration_label.add_css_class("caption");
        duration_label.set_valign(gtk::Align::Center);
        row.append(&duration_label);
    }

    row
}

fn step_status_icon(step: &JobStep) -> &'static str {
    if let Some(conclusion) = step.conclusion.as_deref() {
        match conclusion {
            "success" => "emblem-default-symbolic",
            "failure" => "dialog-error-symbolic",
            "cancelled" => "process-stop-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else if let Some(status) = step.status.as_deref() {
        match status {
            "queued" | "waiting" => "alarm-symbolic",
            "in_progress" => "media-playback-start-symbolic",
            _ => "dialog-question-symbolic",
        }
    } else {
        "dialog-question-symbolic"
    }
}

fn step_status_class(step: &JobStep) -> &'static str {
    if let Some(conclusion) = step.conclusion.as_deref() {
        return match conclusion {
            "success" => "success",
            "failure" => "error",
            "cancelled" => "warning",
            _ => "",
        };
    }
    if let Some("in_progress") = step.status.as_deref() {
        return "accent";
    }
    ""
}

pub(super) fn load_run_jobs(params: LoadJobsParams) {
    let LoadJobsParams {
        client,
        owner,
        repo,
        run_id,
        expander,
        jobs_box,
        badges_box,
        workflow_id,
        parent_window,
        repo_model,
        background,
        job_contexts,
        run_branch,
        run_title,
    } = params;

    if !background {
        loop {
            let child_opt = jobs_box.first_child();
            let Some(child) = child_opt else {
                break;
            };
            jobs_box.remove(&child);
        }

        let spinner = gtk::Spinner::new();
        spinner.start();
        jobs_box.append(&spinner);
    }

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Arc<Vec<Job>>, GitHubError>>(glib::Priority::default());

    let client_for_retry = client.clone();
    let owner_for_retry = owner.clone();
    let repo_for_retry = repo.clone();

    let client_for_api = client.clone();
    let owner_for_api = owner.clone();
    let repo_for_api = repo.clone();

    receiver.attach(None, move |result| {
        let should_clear = result.as_ref().is_ok() || !background;
        if should_clear {
            loop {
                let child_opt = jobs_box.first_child();
                let Some(child) = child_opt else {
                    break;
                };
                jobs_box.remove(&child);
            }
        }

        match result {
            Ok(jobs) if jobs.is_empty() => {
                if !background {
                    let no_jobs = tr("No jobs found");
                    let label = gtk::Label::new(Some(no_jobs.as_str()));
                    label.add_css_class("dim-label");
                    label.set_halign(gtk::Align::Start);
                    jobs_box.append(&label);
                }
            }
            Ok(jobs) => {
                if let Some(ref badges) = badges_box {
                    update_job_summary_badges(badges, jobs.as_ref());
                }

                let context = JobRefreshContext::from_params(JobRefreshContextParams {
                    client: client.clone(),
                    owner: owner.clone(),
                    repo: repo.clone(),
                    workflow_id,
                    run_id,
                    expander: expander.clone(),
                    jobs_box: jobs_box.clone(),
                    badges_box: badges_box.clone(),
                    parent_window: parent_window.clone(),
                    repo_model: repo_model.clone(),
                    branch: run_branch.clone(),
                    run_title: run_title.clone(),
                });
                {
                    let mut contexts = job_contexts.borrow_mut();
                    contexts.insert(run_id, context);
                }

                let total_jobs = jobs.len();
                let row_context = JobRowContext {
                    client: client.clone(),
                    parent_window: parent_window.clone(),
                    repo: repo_model.clone(),
                    branch: run_branch.clone(),
                    run_title: run_title.clone(),
                };
                for job in jobs.iter() {
                    let job_row = create_job_row_simple(job, Some(row_context.clone()));
                    jobs_box.append(&job_row);
                }

                if total_jobs > 0 {
                    let count_label = gtk::Label::new(Some(
                        tr("Showing {count} jobs")
                            .replace("{count}", total_jobs.to_string().as_str())
                            .as_str(),
                    ));
                    count_label.add_css_class("dim-label");
                    count_label.add_css_class("caption");
                    count_label.set_halign(gtk::Align::Start);
                    count_label.set_margin_top(8);
                    count_label.set_margin_bottom(4);
                    jobs_box.append(&count_label);
                }
            }
            Err(e) => {
                error!("Failed to load jobs: {}", e);

                if !background {
                    let error_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
                    error_box.set_halign(gtk::Align::Start);
                    error_box.set_margin_top(8);
                    error_box.set_margin_bottom(8);

                    let error_label = gtk::Label::new(Some(tr("Unable to load jobs").as_str()));
                    error_label.add_css_class("dim-label");
                    error_label.set_halign(gtk::Align::Start);
                    error_box.append(&error_label);

                    let detail_label = gtk::Label::new(Some(
                        tr("Error: {message}")
                            .replace("{message}", e.to_string().as_str())
                            .as_str(),
                    ));
                    detail_label.add_css_class("caption");
                    detail_label.add_css_class("dim-label");
                    detail_label.set_halign(gtk::Align::Start);
                    error_box.append(&detail_label);

                    let retry_button = gtk::Button::with_label(tr("Retry").as_str());
                    retry_button.add_css_class("suggested-action");
                    retry_button.set_halign(gtk::Align::Start);
                    retry_button.set_margin_top(8);

                    let client_retry = client_for_retry.clone();
                    let owner_retry = owner_for_retry.clone();
                    let repo_retry = repo_for_retry.clone();
                    let jobs_box_retry = jobs_box.clone();
                    let job_contexts_retry = job_contexts.clone();

                    let badges_box_retry = badges_box.clone();
                    let parent_window_retry = parent_window.clone();
                    let repo_model_retry = repo_model.clone();
                    let run_branch_retry = run_branch.clone();
                    let run_title_retry = run_title.clone();
                    let expander_retry = expander.clone();

                    retry_button.connect_clicked(move |_| {
                        loop {
                            let child_opt = jobs_box_retry.first_child();
                            let Some(child) = child_opt else {
                                break;
                            };
                            jobs_box_retry.remove(&child);
                        }
                        load_run_jobs(LoadJobsParams {
                            client: client_retry.clone(),
                            owner: owner_retry.clone(),
                            repo: repo_retry.clone(),
                            run_id,
                            expander: expander_retry.clone(),
                            jobs_box: jobs_box_retry.clone(),
                            badges_box: badges_box_retry.clone(),
                            workflow_id,
                            parent_window: parent_window_retry.clone(),
                            repo_model: repo_model_retry.clone(),
                            background: false,
                            job_contexts: job_contexts_retry.clone(),
                            run_branch: run_branch_retry.clone(),
                            run_title: run_title_retry.clone(),
                        });
                    });

                    error_box.append(&retry_button);
                    jobs_box.append(&error_box);
                }
            }
        }

        glib::ControlFlow::Break
    });

    crate::runtime_handle().spawn(async move {
        let client_guard = client_for_api.lock().clone();
        let result = client_guard
            .list_jobs(&owner_for_api, &repo_for_api, run_id)
            .await
            .map(Arc::new);
        let _ = sender.send(result);
    });
}

pub(crate) fn refresh_jobs_for_workflows(
    job_contexts: &JobContextMap,
    workflow_ids: &HashSet<i64>,
) {
    let contexts: Vec<JobRefreshContext> = {
        let guard = job_contexts.borrow();
        guard
            .values()
            .filter(|ctx| workflow_ids.contains(&ctx.workflow_id()))
            .cloned()
            .collect()
    };

    for context in contexts {
        load_run_jobs(LoadJobsParams {
            client: context.client(),
            owner: context.owner(),
            repo: context.repo(),
            run_id: context.run_id(),
            expander: context.expander(),
            jobs_box: context.jobs_box(),
            badges_box: context.badges_box(),
            workflow_id: context.workflow_id(),
            parent_window: context.parent_window(),
            repo_model: context.repo_model(),
            background: true,
            job_contexts: job_contexts.clone(),
            run_branch: context.branch(),
            run_title: context.run_title(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::JobStep;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn test_job_row_layout_properties() {
        let Some(_guard) = gtk_test_guard("test_job_row_layout_properties") else {
            return;
        };

        let job = Job {
            id: 1,
            run_id: 1,
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            started_at: Some("2024-01-01T00:00:00Z".to_string()),
            completed_at: Some("2024-01-01T00:05:00Z".to_string()),
            name: Some("Test Job".to_string()),
            steps: Vec::new(),
            html_url: Some("https://github.com/test".to_string()),
        };

        let job_row = create_job_row_simple(&job, None);

        assert_eq!(job_row.orientation(), gtk::Orientation::Vertical);

        let mut child = job_row.first_child();
        let mut child_count = 0;
        let mut has_job_name = false;
        let mut has_right_box = false;

        while let Some(widget) = child {
            child_count += 1;

            if let Ok(box_widget) = widget.clone().downcast::<gtk::Box>() {
                let mut nested = box_widget.first_child();
                while let Some(nested_widget) = nested {
                    if let Ok(label) = nested_widget.clone().downcast::<gtk::Label>()
                        && label.text().contains("Test Job")
                    {
                        has_job_name = true;
                        assert!(label.hexpands());
                        assert_eq!(label.halign(), gtk::Align::Start);
                    }

                    if let Ok(nested_box) = nested_widget.clone().downcast::<gtk::Box>() {
                        has_right_box = true;
                        assert_eq!(nested_box.halign(), gtk::Align::End);
                        assert_eq!(nested_box.valign(), gtk::Align::Center);
                        assert!(!nested_box.hexpands());
                    }

                    nested = nested_widget.next_sibling();
                }
            }

            child = widget.next_sibling();
        }

        assert!(has_job_name);
        assert!(has_right_box);
        assert_eq!(child_count, 1);
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn test_job_row_renders_step_labels() {
        let Some(_guard) = gtk_test_guard("test_job_row_renders_step_labels") else {
            return;
        };

        let job = Job {
            id: 1,
            run_id: 1,
            status: Some("in_progress".to_string()),
            conclusion: None,
            started_at: Some("2024-01-01T00:00:00Z".to_string()),
            completed_at: None,
            name: Some("Test Job".to_string()),
            steps: vec![
                JobStep {
                    name: Some("Checkout".to_string()),
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    number: Some(1),
                    started_at: Some("2024-01-01T00:00:00Z".to_string()),
                    completed_at: Some("2024-01-01T00:00:10Z".to_string()),
                },
                JobStep {
                    name: Some("Run tests".to_string()),
                    status: Some("in_progress".to_string()),
                    conclusion: None,
                    number: Some(2),
                    started_at: Some("2024-01-01T00:00:10Z".to_string()),
                    completed_at: None,
                },
            ],
            html_url: Some("https://github.com/test".to_string()),
        };

        let job_row = create_job_row_simple(&job, None);

        let mut found_checkout = false;
        let mut found_tests = false;
        let mut stack = vec![job_row.upcast::<gtk::Widget>()];
        while let Some(widget) = stack.pop() {
            if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                let text = label.text();
                if text.contains("Checkout") {
                    found_checkout = true;
                }
                if text.contains("Run tests") {
                    found_tests = true;
                }
            }

            let mut child = widget.first_child();
            while let Some(next) = child {
                stack.push(next.clone());
                child = next.next_sibling();
            }
        }

        assert!(found_checkout);
        assert!(found_tests);
    }
}
