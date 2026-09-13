use super::context::{
    JobContextMap, JobRefreshContext, JobRefreshContextParams, RunBadgeSummaryMap,
};
use super::formatting::friendly_status;
use super::formatting::{get_job_status_class, get_job_status_icon};
use super::runs::WorkflowRunListModel;
use super::status_dot::{JOB_DOT_SIZE, STEP_DOT_SIZE, build_status_dot};
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::GitHubError;
use crate::services::api::models::{Job, JobStep, JobSummary, Repo};
use crate::services::gateway::GitHubGateway;
use crate::ui::job_logs_window::JobLogsWindow;
use crate::ui::utils::duration::{job_duration_label, start_live_duration, step_duration_label};
use crate::ui::utils::widget_data::{get_data_clone, get_data_copy, set_data};
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

const JOB_LOAD_IN_FLIGHT_KEY: &str = "actioneer-job-load-in-flight";

pub(super) struct LoadJobsParams {
    pub(super) client: Arc<Mutex<GitHubGateway>>,
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
    pub(super) client: Arc<Mutex<GitHubGateway>>,
    pub(super) parent_window: gtk::Window,
    pub(super) repo: Repo,
    pub(super) run_title: String,
    /// Every job of the run, so the log window can offer the others in its
    /// sidebar instead of trapping the reader in the one row they clicked.
    pub(super) run_jobs: Arc<Vec<Job>>,
}

/// Inset of a job card's contents, on both sides. Equal by design: the header
/// and the step rows share it, so the card's text column and its trailing
/// durations sit the same distance from the card's edges.
const JOB_CARD_INSET: i32 = 11;

pub(super) fn create_job_row_simple(job: &Job, context: Option<JobRowContext>) -> gtk::Box {
    let job_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    job_box.set_hexpand(true);
    job_box.set_overflow(gtk::Overflow::Hidden);
    job_box.add_css_class("job-card");

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    header_row.set_valign(gtk::Align::Center);
    header_row.set_hexpand(true);
    header_row.set_margin_start(JOB_CARD_INSET);
    header_row.set_margin_end(JOB_CARD_INSET);
    header_row.set_margin_top(8);
    header_row.set_margin_bottom(8);

    let dot = build_status_dot(
        get_job_status_icon(job),
        get_job_status_class(job),
        JOB_DOT_SIZE,
    );
    let job_status_text = friendly_status(job.status.as_deref(), job.conclusion.as_deref());
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

    let (duration, live) = job_duration_label(job);
    if let Some(duration) = duration {
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.add_css_class("mono");
        duration_label.add_css_class("dim-label");
        duration_label.add_css_class("caption");
        duration_label.set_valign(gtk::Align::Center);
        if let Some(started_at) = live {
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
        steps_box.set_margin_start(JOB_CARD_INSET);
        steps_box.set_margin_end(JOB_CARD_INSET);
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
    let step_status_text = friendly_status(step.status.as_deref(), step.conclusion.as_deref());
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

    let (duration, live) = step_duration_label(step);
    let duration_label = gtk::Label::new(Some(duration.as_deref().unwrap_or("–")));
    duration_label.add_css_class("mono");
    duration_label.add_css_class("dim-label");
    duration_label.add_css_class("caption");
    duration_label.set_halign(gtk::Align::End);
    duration_label.set_valign(gtk::Align::Center);
    if let Some(started_at) = live {
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
    client: Arc<Mutex<GitHubGateway>>,
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
                        crate::ui::error_text::user_message_from(&e).as_str(),
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

    crate::runtime::handle().spawn(async move {
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
mod tests;
