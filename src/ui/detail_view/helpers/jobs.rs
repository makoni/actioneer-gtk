use super::context::{
    JobContextMap, JobRefreshContext, JobRefreshContextParams, RunBadgeSummaryMap,
};
use super::duration::{is_in_progress, live_start, running_duration_string, start_live_duration};
use super::formatting::{get_job_status_class, get_job_status_icon};
use super::runs::WorkflowRunListModel;
use super::status_dot::{JOB_DOT_SIZE, STEP_DOT_SIZE, build_status_dot};
use crate::api::models::{Job, JobStep, JobSummary, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::job_logs_window::JobLogsWindow;
use crate::ui::utils::MainContextChannelExt;
use crate::ui::utils::widget_data::{get_data_clone, get_data_copy, set_data};
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
    pub(super) workflow_id: i64,
    pub(super) parent_window: gtk::Window,
    pub(super) repo_model: Repo,
    pub(super) background: bool,
    pub(super) job_contexts: JobContextMap,
    pub(super) job_summaries: RunBadgeSummaryMap,
    pub(super) run_branch: Option<String>,
    pub(super) run_title: String,
}

#[derive(Clone)]
pub(super) struct JobRowContext {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) parent_window: gtk::Window,
    pub(super) repo: Repo,
    pub(super) run_title: String,
    /// Every job of the run, so the log window can offer the others in its
    /// sidebar instead of trapping the reader in the one row they clicked.
    pub(super) run_jobs: Arc<Vec<Job>>,
}

fn job_duration_label_text(job: &Job) -> Option<String> {
    job.duration_string().or_else(|| {
        is_in_progress(job.status.as_deref())
            .then(|| running_duration_string(job.started_at.as_ref()))
            .flatten()
    })
}

fn step_duration_label_text(step: &JobStep) -> Option<String> {
    step.duration_string().or_else(|| {
        is_in_progress(step.status.as_deref())
            .then(|| running_duration_string(step.started_at.as_ref()))
            .flatten()
    })
}

pub(super) fn create_job_row_simple(job: &Job, context: Option<JobRowContext>) -> gtk::Box {
    let job_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    job_box.set_hexpand(true);
    job_box.set_overflow(gtk::Overflow::Hidden);
    job_box.add_css_class("job-card");

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    header_row.set_valign(gtk::Align::Center);
    header_row.set_hexpand(true);
    header_row.set_margin_start(11);
    header_row.set_margin_end(8);
    header_row.set_margin_top(8);
    header_row.set_margin_bottom(8);

    let dot = build_status_dot(
        get_job_status_icon(job),
        get_job_status_class(job),
        JOB_DOT_SIZE,
    );
    let job_status_text = job.friendly_status();
    if !job_status_text.is_empty() {
        crate::ui::utils::describe_control(&dot, &job_status_text);
    }
    header_row.append(&dot);

    let fallback_job_name = tr("Unnamed job");
    let job_name_label = gtk::Label::new(Some(
        job.name.as_deref().unwrap_or(fallback_job_name.as_str()),
    ));
    job_name_label.set_halign(gtk::Align::Start);
    job_name_label.set_hexpand(true);
    job_name_label.set_valign(gtk::Align::Center);
    job_name_label.add_css_class("job-name");
    job_name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header_row.append(&job_name_label);

    let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    right_box.set_valign(gtk::Align::Center);
    right_box.set_halign(gtk::Align::End);
    right_box.set_hexpand(false);

    if let Some(duration) = job_duration_label_text(job) {
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.add_css_class("mono");
        duration_label.add_css_class("dim-label");
        duration_label.add_css_class("caption");
        duration_label.set_valign(gtk::Align::Center);
        if let Some(started_at) = live_start(
            job.duration_string(),
            job.status.as_deref(),
            job.started_at.as_ref(),
        ) {
            start_live_duration(&duration_label, started_at);
        }
        right_box.append(&duration_label);
    }

    if let Some(ctx) = context {
        let logs_button = gtk::Button::from_icon_name("text-x-generic-symbolic");
        crate::ui::utils::describe_control(&logs_button, tr("View logs").as_str());
        logs_button.add_css_class("row-action-btn");
        logs_button.set_valign(gtk::Align::Center);
        logs_button.set_focus_on_click(false);

        let parent_window = ctx.parent_window.clone();
        let repo_model = ctx.repo.clone();
        let client = ctx.client.clone();
        let job_for_logs = job.clone();
        let run_title = ctx.run_title.clone();
        let run_jobs = ctx.run_jobs.clone();
        logs_button.connect_clicked(move |_| {
            let jobs = if run_jobs.iter().any(|other| other.id == job_for_logs.id) {
                run_jobs.as_ref().clone()
            } else {
                vec![job_for_logs.clone()]
            };
            let selected = jobs
                .iter()
                .position(|other| other.id == job_for_logs.id)
                .unwrap_or(0);
            let logs_window = JobLogsWindow::for_run(
                &parent_window,
                repo_model.clone(),
                run_title.clone(),
                jobs,
                selected,
                client.clone(),
            );
            logs_window.present();
        });

        right_box.append(&logs_button);
    }

    header_row.append(&right_box);
    job_box.append(&header_row);

    if !job.steps.is_empty() {
        let steps_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        steps_box.set_margin_start(11);
        steps_box.set_margin_end(8);
        steps_box.set_margin_bottom(8);
        steps_box.set_hexpand(true);

        for (index, step) in job.steps.iter().enumerate() {
            steps_box.append(&create_job_step_row(step, index + 1));
        }

        job_box.append(&steps_box);
    }

    job_box
}

fn create_job_step_row(step: &JobStep, display_number: usize) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    row.set_hexpand(true);
    row.set_margin_top(1);
    row.set_margin_bottom(1);

    let index_label = gtk::Label::new(Some(&display_number.to_string()));
    index_label.add_css_class("mono");
    index_label.add_css_class("dim-label");
    index_label.add_css_class("caption");
    index_label.set_halign(gtk::Align::End);
    index_label.set_valign(gtk::Align::Center);
    index_label.set_width_chars(3);
    row.append(&index_label);

    let dot = build_status_dot(
        step_status_icon(step),
        step_status_class(step),
        STEP_DOT_SIZE,
    );
    let step_status_text = step.friendly_status();
    if !step_status_text.is_empty() {
        crate::ui::utils::describe_control(&dot, &step_status_text);
    }
    row.append(&dot);

    let title = step.name.as_deref().map(str::to_string);
    let fallback = format!("#{display_number}");
    let name_label = gtk::Label::new(Some(title.as_deref().unwrap_or(fallback.as_str())));
    name_label.add_css_class("dim-label");
    name_label.set_halign(gtk::Align::Start);
    name_label.set_hexpand(true);
    name_label.set_valign(gtk::Align::Center);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&name_label);

    let duration_text = step_duration_label_text(step).unwrap_or_else(|| "–".to_string());
    let duration_label = gtk::Label::new(Some(&duration_text));
    duration_label.add_css_class("mono");
    duration_label.add_css_class("dim-label");
    duration_label.add_css_class("caption");
    duration_label.set_halign(gtk::Align::End);
    duration_label.set_valign(gtk::Align::Center);
    if let Some(started_at) = live_start(
        step.duration_string(),
        step.status.as_deref(),
        step.started_at.as_ref(),
    ) {
        start_live_duration(&duration_label, started_at);
    }
    row.append(&duration_label);

    row
}

fn step_status_icon(step: &JobStep) -> &'static str {
    super::formatting::status_presentation(step.status.as_deref(), step.conclusion.as_deref()).0
}

fn step_status_class(step: &JobStep) -> &'static str {
    super::formatting::status_presentation(step.status.as_deref(), step.conclusion.as_deref()).1
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

fn current_retry_widgets(
    job_contexts: &JobContextMap,
    run_id: i64,
    fallback_expander: &gtk::Expander,
    fallback_jobs_box: &gtk::Box,
) -> (gtk::Expander, gtk::Box) {
    let contexts = job_contexts.borrow();
    if let Some(context) = contexts.get(&run_id) {
        (context.expander(), context.jobs_box())
    } else {
        (fallback_expander.clone(), fallback_jobs_box.clone())
    }
}

/// Finds the owning workflow's run list model by walking up from a run expander.
fn workflow_run_list_for_expander(expander: &gtk::Expander) -> Option<WorkflowRunListModel> {
    let mut current = expander.parent();
    while let Some(widget) = current {
        if let Some(candidate) = widget.downcast_ref::<gtk::Expander>()
            && candidate.widget_name().as_str().starts_with("workflow_")
        {
            return get_data_clone(candidate, "actioneer-run-list");
        }
        current = widget.parent();
    }
    None
}

/// Pushes fresh job counts into the workflow-level progress indicator when the
/// jobs belong to the workflow's latest run.
fn update_workflow_progress(
    expander: &gtk::Expander,
    run_id: i64,
    job_summaries: &RunBadgeSummaryMap,
) {
    let summary = job_summaries.borrow().get(&run_id).cloned();
    if let Some(run_list) = workflow_run_list_for_expander(expander) {
        run_list.refresh_progress_from_summary(run_id, summary.as_ref());
    }
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
    parent_window: gtk::Window,
    repo_model: Repo,
    run_branch: Option<String>,
    run_title: String,
    jobs: Arc<Vec<Job>>,
    job_summaries: RunBadgeSummaryMap,
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
            parent_window,
            repo_model,
            branch: run_branch,
            run_title,
            jobs,
            job_summaries,
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
        workflow_id,
        parent_window,
        repo_model,
        background,
        job_contexts,
        job_summaries,
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
        parent_window.clone(),
        repo_model.clone(),
        run_branch.clone(),
        run_title.clone(),
        cached_jobs_for_run(&job_contexts, run_id),
        job_summaries.clone(),
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
                job_summaries.borrow_mut().remove(&run_id);
                update_workflow_progress(&expander, run_id, &job_summaries);
                // The placeholder must be added on background refreshes too: the
                // reload guards key off this box's first child, so leaving it
                // empty would strand the panel blank and unreloadable.
                let no_jobs = tr("No jobs found");
                let label = gtk::Label::new(Some(no_jobs.as_str()));
                label.add_css_class("dim-label");
                label.set_halign(gtk::Align::Start);
                jobs_box.append(&label);
            }
            Ok(jobs) => {
                let summary = JobSummary::from_jobs(jobs.as_ref());
                if summary.is_empty() {
                    job_summaries.borrow_mut().remove(&run_id);
                } else {
                    job_summaries.borrow_mut().insert(run_id, summary);
                }

                update_workflow_progress(&expander, run_id, &job_summaries);

                store_job_refresh_context(
                    &job_contexts,
                    client.clone(),
                    owner.clone(),
                    repo.clone(),
                    workflow_id,
                    run_id,
                    expander.clone(),
                    jobs_box.clone(),
                    parent_window.clone(),
                    repo_model.clone(),
                    run_branch.clone(),
                    run_title.clone(),
                    jobs.clone(),
                    job_summaries.clone(),
                );

                let row_context = JobRowContext {
                    client: client.clone(),
                    parent_window: parent_window.clone(),
                    repo: repo_model.clone(),
                    run_title: run_title.clone(),
                    run_jobs: jobs.clone(),
                };
                for job in jobs.iter() {
                    let job_row = create_job_row_simple(job, Some(row_context.clone()));
                    jobs_box.append(&job_row);
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

                    let parent_window_retry = parent_window.clone();
                    let repo_model_retry = repo_model.clone();
                    let job_summaries_retry = job_summaries.clone();
                    let run_branch_retry = run_branch.clone();
                    let run_title_retry = run_title.clone();
                    // Weak: this button is appended to `jobs_box`, which is the
                    // expander's own child — a strong capture would close a cycle
                    // and keep the run row alive after it is discarded.
                    let expander_retry = expander.downgrade();

                    retry_button.connect_clicked(move |_| {
                        let Some(expander_retry) = expander_retry.upgrade() else {
                            return;
                        };
                        let (current_expander, current_jobs_box) = current_retry_widgets(
                            &job_contexts_retry,
                            run_id,
                            &expander_retry,
                            &jobs_box_retry,
                        );

                        loop {
                            let child_opt = current_jobs_box.first_child();
                            let Some(child) = child_opt else {
                                break;
                            };
                            current_jobs_box.remove(&child);
                        }
                        load_run_jobs(LoadJobsParams {
                            client: client_retry.clone(),
                            owner: owner_retry.clone(),
                            repo: repo_retry.clone(),
                            run_id,
                            expander: current_expander,
                            jobs_box: current_jobs_box,
                            workflow_id,
                            parent_window: parent_window_retry.clone(),
                            repo_model: repo_model_retry.clone(),
                            background: false,
                            job_contexts: job_contexts_retry.clone(),
                            job_summaries: job_summaries_retry.clone(),
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
            workflow_id: context.workflow_id(),
            parent_window: context.parent_window(),
            repo_model: context.repo_model(),
            background: true,
            job_contexts: job_contexts.clone(),
            job_summaries: context.job_summaries(),
            run_branch: context.branch(),
            run_title: context.run_title(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{JobStep, User};
    use crate::ui::test_helpers::run_gtk_test;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn step_stub(status: Option<&str>, started_at: Option<&str>) -> JobStep {
        JobStep {
            name: Some("Build".into()),
            status: status.map(str::to_string),
            conclusion: None,
            number: Some(7),
            started_at: started_at.map(str::to_string),
            completed_at: None,
        }
    }

    #[test]
    fn duration_fallback_only_applies_while_running() {
        // A queued step carries `started_at` (queue time) but must not tick.
        assert_eq!(
            step_duration_label_text(&step_stub(Some("queued"), Some("2020-01-01T00:00:00Z"))),
            None
        );
        // A step abandoned without `completed_at` must not grow forever either.
        assert_eq!(
            step_duration_label_text(&step_stub(Some("completed"), Some("2020-01-01T00:00:00Z"))),
            None
        );
        // An executing step still shows elapsed wall-clock time.
        assert!(
            step_duration_label_text(&step_stub(
                Some("in_progress"),
                Some("2020-01-01T00:00:00Z")
            ))
            .is_some()
        );
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn step_rows_use_contiguous_display_numbers() {
        run_gtk_test("step_rows_use_contiguous_display_numbers", || {
            // GitHub step numbers can be sparse, so rows are numbered by position.
            let row = create_job_step_row(&step_stub(Some("completed"), None), 3);
            let index_label = row
                .first_child()
                .and_then(|child| child.downcast::<gtk::Label>().ok())
                .expect("step row should start with its index label");
            assert_eq!(index_label.text().as_str(), "3");

            let unnamed = JobStep {
                name: None,
                ..step_stub(Some("queued"), None)
            };
            let row = create_job_step_row(&unnamed, 4);
            let name_label = row
                .last_child()
                .and_then(|child| child.prev_sibling())
                .and_then(|child| child.downcast::<gtk::Label>().ok())
                .expect("step row should carry a name label");
            assert_eq!(name_label.text().as_str(), "#4");
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn test_job_row_layout_properties() {
        run_gtk_test("test_job_row_layout_properties", || {
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
            assert!(job_row.has_css_class("job-card"));

            let header = job_row
                .first_child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
                .expect("job header row");

            let dot = header
                .first_child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
                .expect("status dot");
            assert!(dot.has_css_class("status-dot"));
            assert!(dot.has_css_class("success"));

            let name_label = dot
                .next_sibling()
                .and_then(|child| child.downcast::<gtk::Label>().ok())
                .expect("job name label");
            assert_eq!(name_label.text().as_str(), "Test Job");
            assert!(name_label.has_css_class("job-name"));
            assert!(name_label.hexpands());
            assert_eq!(name_label.halign(), gtk::Align::Start);
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn in_flight_job_load_flag_blocks_duplicate_start() {
        run_gtk_test("in_flight_job_load_flag_blocks_duplicate_start", || {
            let expander = gtk::Expander::new(None::<&str>);
            assert!(try_begin_job_load(&expander));
            assert!(!try_begin_job_load(&expander));
            finish_job_load(&expander);
            assert!(try_begin_job_load(&expander));
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn provisional_context_preserves_cached_jobs() {
        run_gtk_test("provisional_context_preserves_cached_jobs", || {
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
            let job_summaries: RunBadgeSummaryMap = Rc::new(RefCell::new(HashMap::new()));
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
                gtk::Window::new(),
                repo.clone(),
                Some("main".into()),
                "CI".into(),
                cached_jobs.clone(),
                job_summaries,
            );

            let preserved = cached_jobs_for_run(&job_contexts, 42);
            assert_eq!(preserved.len(), 1);
            assert_eq!(preserved[0].id, 10);
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn retry_uses_current_context_widgets_when_available() {
        run_gtk_test("retry_uses_current_context_widgets_when_available", || {
            let fallback_expander = gtk::Expander::new(None::<&str>);
            let fallback_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

            let current_expander = gtk::Expander::new(None::<&str>);
            let current_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

            let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
            job_contexts.borrow_mut().insert(
                42,
                JobRefreshContext::from_params(JobRefreshContextParams {
                    client: Arc::new(Mutex::new(
                        GitHubClient::new(None).expect("client should build"),
                    )),
                    owner: "mak".into(),
                    repo: "actioneer".into(),
                    workflow_id: 7,
                    run_id: 42,
                    expander: current_expander.clone(),
                    jobs_box: current_jobs_box.clone(),
                    parent_window: gtk::Window::new(),
                    repo_model: Repo {
                        id: 1,
                        name: "actioneer".into(),
                        full_name: "mak/actioneer".into(),
                        owner: User {
                            login: "mak".into(),
                        },
                        default_branch: Some("main".into()),
                        is_private: false,
                        permissions: None,
                    },
                    branch: Some("main".into()),
                    run_title: "CI".into(),
                    jobs: Arc::new(Vec::new()),
                    job_summaries: Rc::new(RefCell::new(HashMap::new())),
                }),
            );

            let (resolved_expander, resolved_jobs_box) =
                current_retry_widgets(&job_contexts, 42, &fallback_expander, &fallback_jobs_box);

            assert_eq!(resolved_expander, current_expander);
            assert_eq!(resolved_jobs_box, current_jobs_box);
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn test_job_row_renders_step_labels() {
        run_gtk_test("test_job_row_renders_step_labels", || {
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
        });
    }
}
