use super::context::{JobContextMap, JobRefreshContext, JobRefreshContextParams};
use super::formatting::{
    format_job_status, get_job_status_class, get_job_status_icon, update_job_summary_badges,
};
use crate::api::models::{Job, JobStep, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::job_logs_window::JobLogsWindow;
use crate::ui::utils::MainContextChannelExt;
use crate::ui::utils::widget_data::{get_data_copy, set_data};
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

const JOB_LOAD_IN_FLIGHT_KEY: &str = "actioneer-job-load-in-flight";

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

        for (index, step) in job.steps.iter().enumerate() {
            steps_box.append(&create_job_step_row(step, index + 1));
        }

        job_box.append(&steps_box);
    }

    job_box
}

fn format_step_title(step: &JobStep, display_number: usize) -> String {
    match step.name.as_deref() {
        Some(name) => format!("{display_number}. {name}"),
        None => format!("#{display_number}"),
    }
}

fn create_job_step_row(step: &JobStep, display_number: usize) -> gtk::Box {
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

    let title = format_step_title(step, display_number);
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

fn try_begin_job_load(expander: &gtk::Expander) -> bool {
    if get_data_copy::<bool, _>(expander, JOB_LOAD_IN_FLIGHT_KEY).unwrap_or(false) {
        return false;
    }

    set_data(expander, JOB_LOAD_IN_FLIGHT_KEY, true);
    true
}

fn finish_job_load(expander: &gtk::Expander) {
    set_data(expander, JOB_LOAD_IN_FLIGHT_KEY, false);
}

fn cached_jobs_for_run(job_contexts: &JobContextMap, run_id: i64) -> Arc<Vec<Job>> {
    let contexts = job_contexts.borrow();
    contexts
        .get(&run_id)
        .map(|context| context.jobs())
        .unwrap_or_else(|| Arc::new(Vec::new()))
}

#[allow(clippy::too_many_arguments)]
fn store_job_refresh_context(
    job_contexts: &JobContextMap,
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    workflow_id: i64,
    run_id: i64,
    expander: gtk::Expander,
    jobs_box: gtk::Box,
    badges_box: Option<gtk::Box>,
    parent_window: gtk::Window,
    repo_model: Repo,
    run_branch: Option<String>,
    run_title: String,
    jobs: Arc<Vec<Job>>,
) {
    job_contexts.borrow_mut().insert(
        run_id,
        JobRefreshContext::from_params(JobRefreshContextParams {
            client,
            owner,
            repo,
            workflow_id,
            run_id,
            expander,
            jobs_box,
            badges_box,
            parent_window,
            repo_model,
            branch: run_branch,
            run_title,
            jobs,
        }),
    );
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

    if !try_begin_job_load(&expander) {
        return;
    }

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

    store_job_refresh_context(
        &job_contexts,
        client.clone(),
        owner.clone(),
        repo.clone(),
        workflow_id,
        run_id,
        expander.clone(),
        jobs_box.clone(),
        badges_box.clone(),
        parent_window.clone(),
        repo_model.clone(),
        run_branch.clone(),
        run_title.clone(),
        cached_jobs_for_run(&job_contexts, run_id),
    );

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Arc<Vec<Job>>, GitHubError>>(glib::Priority::default());

    let client_for_retry = client.clone();
    let owner_for_retry = owner.clone();
    let repo_for_retry = repo.clone();

    let client_for_api = client.clone();
    let owner_for_api = owner.clone();
    let repo_for_api = repo.clone();
    let expander_for_receiver = expander.clone();

    receiver.attach(None, move |result| {
        finish_job_load(&expander_for_receiver);
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

                store_job_refresh_context(
                    &job_contexts,
                    client.clone(),
                    owner.clone(),
                    repo.clone(),
                    workflow_id,
                    run_id,
                    expander.clone(),
                    jobs_box.clone(),
                    badges_box.clone(),
                    parent_window.clone(),
                    repo_model.clone(),
                    run_branch.clone(),
                    run_title.clone(),
                    jobs.clone(),
                );

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
    use crate::api::models::{JobStep, User};
    use crate::ui::test_helpers::gtk_test_guard;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

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
    fn in_flight_job_load_flag_blocks_duplicate_start() {
        let Some(_guard) = gtk_test_guard("in_flight_job_load_flag_blocks_duplicate_start") else {
            return;
        };

        let expander = gtk::Expander::new(None::<&str>);
        assert!(try_begin_job_load(&expander));
        assert!(!try_begin_job_load(&expander));
        finish_job_load(&expander);
        assert!(try_begin_job_load(&expander));
    }

    #[test]
    fn step_titles_use_contiguous_display_numbers() {
        let named = JobStep {
            name: Some("Compile".into()),
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            number: Some(16),
            started_at: None,
            completed_at: None,
        };
        assert_eq!(format_step_title(&named, 10), "10. Compile");

        let unnamed = JobStep {
            name: None,
            status: Some("queued".into()),
            conclusion: None,
            number: Some(99),
            started_at: None,
            completed_at: None,
        };
        assert_eq!(format_step_title(&unnamed, 3), "#3");
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn provisional_context_preserves_cached_jobs() {
        let Some(_guard) = gtk_test_guard("provisional_context_preserves_cached_jobs") else {
            return;
        };

        let client = Arc::new(Mutex::new(
            GitHubClient::new(None).expect("client should build"),
        ));
        let repo = Repo {
            id: 1,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            default_branch: Some("main".into()),
            is_private: false,
            permissions: None,
        };
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let cached_jobs = Arc::new(vec![Job {
            id: 10,
            run_id: 42,
            status: Some("in_progress".into()),
            conclusion: None,
            started_at: None,
            completed_at: None,
            name: Some("Build".into()),
            steps: Vec::new(),
            html_url: None,
        }]);

        store_job_refresh_context(
            &job_contexts,
            client.clone(),
            "mak".into(),
            "actioneer".into(),
            7,
            42,
            gtk::Expander::new(None::<&str>),
            gtk::Box::new(gtk::Orientation::Vertical, 0),
            None,
            gtk::Window::new(),
            repo.clone(),
            Some("main".into()),
            "CI".into(),
            cached_jobs.clone(),
        );

        let preserved = cached_jobs_for_run(&job_contexts, 42);
        assert_eq!(preserved.len(), 1);
        assert_eq!(preserved[0].id, 10);
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
