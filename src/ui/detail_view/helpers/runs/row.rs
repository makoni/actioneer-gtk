use super::super::context::{
    JobContextMap, JobRefreshContext, JobRefreshContextParams, RunBadgeSummaryMap,
};
use super::super::formatting::{
    format_run_subtitle, format_run_title, format_run_tooltip, get_run_status_class,
    get_run_status_icon, update_job_summary_badges, update_job_summary_badges_from_summary,
};
use super::super::jobs::{LoadJobsParams, load_run_jobs};
use super::actions::{RunActionContext, create_actions_box};
use crate::api::GitHubClient;
use crate::api::models::{Repo, WorkflowRun};
use crate::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};
use libadwaita as adw;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct RunRowContext {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) repo_model: Repo,
    pub(super) parent_window: adw::ApplicationWindow,
    pub(super) workflow_id: i64,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) job_contexts: JobContextMap,
    pub(super) job_summaries: RunBadgeSummaryMap,
}

impl RunRowContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        client: Arc<Mutex<GitHubClient>>,
        owner: String,
        repo: String,
        repo_model: Repo,
        parent_window: adw::ApplicationWindow,
        workflow_id: i64,
        toast_overlay: adw::ToastOverlay,
        job_contexts: JobContextMap,
        job_summaries: RunBadgeSummaryMap,
    ) -> Self {
        Self {
            client,
            owner,
            repo,
            repo_model,
            parent_window,
            workflow_id,
            toast_overlay,
            job_contexts,
            job_summaries,
        }
    }

    fn actions_context(&self) -> RunActionContext {
        RunActionContext {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            parent_window: self.parent_window.clone(),
            toast_overlay: self.toast_overlay.clone(),
        }
    }
}

pub(crate) fn create_run_expander_row(
    run: &WorkflowRun,
    context: &RunRowContext,
    expand_jobs: bool,
) -> gtk::Box {
    let run_box = create_run_container();
    let row_container = create_row_container();

    let run_title = format_run_title(run);
    let existing_jobs_box = {
        let guard = context.job_contexts.borrow();
        guard.get(&run.id).map(|ctx| ctx.jobs_box())
    };
    let (expander, badges_box) = build_expander(run, &run_title);
    let actions_box = create_actions_box(run, &context.actions_context());

    if let Some(summary) = context.job_summaries.borrow().get(&run.id).cloned() {
        update_job_summary_badges_from_summary(&badges_box, &summary);
    }

    row_container.append(&expander);
    row_container.append(&actions_box);
    run_box.append(&row_container);

    let jobs_box = if let Some(existing) = existing_jobs_box {
        existing.unparent();
        existing
    } else {
        build_jobs_placeholder()
    };
    expander.set_child(Some(&jobs_box));

    let parent_window_for_jobs: gtk::Window = context.parent_window.clone().upcast();
    if expand_jobs {
        rebind_preserved_job_context(
            &context.job_contexts,
            JobRefreshContextParams {
                client: context.client.clone(),
                owner: context.owner.clone(),
                repo: context.repo.clone(),
                workflow_id: context.workflow_id,
                run_id: run.id,
                expander: expander.clone(),
                jobs_box: jobs_box.clone(),
                badges_box: Some(badges_box.clone()),
                parent_window: parent_window_for_jobs.clone(),
                repo_model: context.repo_model.clone(),
                branch: run.head_branch.clone(),
                run_title: run_title.clone(),
                jobs: std::sync::Arc::new(Vec::new()),
                job_summaries: context.job_summaries.clone(),
            },
        );
    }
    attach_job_loader(
        &expander,
        jobs_box,
        badges_box,
        context,
        run,
        parent_window_for_jobs,
        run_title.clone(),
    );

    if expand_jobs {
        // Keep expanded rows expanded immediately to avoid collapse/expand flicker on refresh.
        expander.set_expanded(true);
    }

    run_box
}

fn create_run_container() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("run-row");
    container.add_css_class("hoverless-row");
    container
}

fn create_row_container() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    container.set_margin_start(12);
    container.set_margin_end(12);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_hexpand(true);
    container.set_valign(gtk::Align::Center);
    container
}

fn build_expander(run: &WorkflowRun, run_title: &str) -> (gtk::Expander, gtk::Box) {
    let expander = gtk::Expander::new(None);
    expander.set_hexpand(true);
    expander.set_valign(gtk::Align::Center);
    expander.set_widget_name(&format!("run_{}", run.id));

    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_box.set_hexpand(true);

    let status_icon = gtk::Image::from_icon_name(get_run_status_icon(run));
    let status_class = get_run_status_class(run);
    if !status_class.is_empty() {
        status_icon.add_css_class(status_class);
    }
    status_icon.set_valign(gtk::Align::Center);
    header_box.append(&status_icon);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_hexpand(true);

    let title_label = gtk::Label::new(Some(run_title));
    title_label.set_halign(gtk::Align::Start);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(pango::EllipsizeMode::End);
    title_label.add_css_class("title-4");
    text_box.append(&title_label);

    let subtitle_label = gtk::Label::new(Some(&format_run_subtitle(run)));
    subtitle_label.set_halign(gtk::Align::Start);
    subtitle_label.add_css_class("dim-label");
    subtitle_label.set_ellipsize(pango::EllipsizeMode::End);
    subtitle_label.set_tooltip_text(Some(&format_run_tooltip(run)));
    text_box.append(&subtitle_label);

    let subtitle_label_weak = subtitle_label.downgrade();
    let run_for_timer = run.clone();
    glib::timeout_add_seconds_local(60, move || match subtitle_label_weak.upgrade() {
        Some(label) => {
            label.set_text(&format_run_subtitle(&run_for_timer));
            label.set_tooltip_text(Some(&format_run_tooltip(&run_for_timer)));
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });

    header_box.append(&text_box);

    let badges_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    badges_box.set_halign(gtk::Align::End);
    badges_box.set_valign(gtk::Align::Center);
    header_box.append(&badges_box);

    expander.set_label_widget(Some(&header_box));

    (expander, badges_box)
}

fn build_jobs_placeholder() -> gtk::Box {
    let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    jobs_box.set_margin_start(24);
    jobs_box.set_margin_end(12);
    jobs_box.set_margin_top(4);
    jobs_box.set_margin_bottom(4);
    jobs_box.set_hexpand(true);

    let placeholder = gtk::Label::new(Some(tr("Click to load jobs...").as_str()));
    placeholder.add_css_class("dim-label");
    placeholder.set_halign(gtk::Align::Start);
    jobs_box.append(&placeholder);

    jobs_box
}

fn attach_job_loader(
    expander: &gtk::Expander,
    jobs_box: gtk::Box,
    badges_box: gtk::Box,
    context: &RunRowContext,
    run: &WorkflowRun,
    parent_window: gtk::Window,
    run_title: String,
) {
    let run_id = run.id;
    let run_branch = run.head_branch.clone();

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let workflow_id = context.workflow_id;
    let job_contexts_for_load = context.job_contexts.clone();
    let job_contexts_for_remove = context.job_contexts.clone();
    let repo_model = context.repo_model.clone();
    let badges_box_for_load = badges_box.clone();
    let job_summaries_for_load = context.job_summaries.clone();
    let parent_window_for_load = parent_window.clone();
    let run_title_for_load = run_title.clone();

    expander.connect_expanded_notify(move |exp| {
        if !exp.is_expanded() {
            let expander = exp.clone();
            let job_contexts = job_contexts_for_remove.clone();
            glib::idle_add_local_once(move || {
                if expander.is_expanded() {
                    return;
                }

                remove_job_context_if_current(&job_contexts, run_id, &expander);
            });
            return;
        }

        if let Some(child) = jobs_box.first_child()
            && child.is::<gtk::Label>()
        {
            load_run_jobs(LoadJobsParams {
                client: client.clone(),
                owner: owner.clone(),
                repo: repo.clone(),
                run_id,
                expander: exp.clone(),
                jobs_box: jobs_box.clone(),
                badges_box: Some(badges_box_for_load.clone()),
                workflow_id,
                parent_window: parent_window_for_load.clone(),
                repo_model: repo_model.clone(),
                background: false,
                job_contexts: job_contexts_for_load.clone(),
                job_summaries: job_summaries_for_load.clone(),
                run_branch: run_branch.clone(),
                run_title: run_title_for_load.clone(),
            });
        }
    });
}

fn rebind_preserved_job_context(job_contexts: &JobContextMap, params: JobRefreshContextParams) {
    let already_loaded = params
        .jobs_box
        .first_child()
        .is_some_and(|child| !child.is::<gtk::Label>());

    if !already_loaded {
        return;
    }

    let previous_jobs = {
        let contexts = job_contexts.borrow();
        contexts
            .get(&params.run_id)
            .map(|context| context.jobs())
            .unwrap_or_else(|| std::sync::Arc::new(Vec::new()))
    };

    if let Some(ref badges_box) = params.badges_box {
        update_job_summary_badges(badges_box, previous_jobs.as_ref());
    }

    let mut params = params;
    params.jobs = previous_jobs;

    job_contexts
        .borrow_mut()
        .insert(params.run_id, JobRefreshContext::from_params(params));
}

fn remove_job_context_if_current(
    job_contexts: &JobContextMap,
    run_id: i64,
    expander: &gtk::Expander,
) {
    let should_remove = {
        let contexts = job_contexts.borrow();
        contexts
            .get(&run_id)
            .is_some_and(|context| context.matches_expander(expander))
    };

    if should_remove {
        job_contexts.borrow_mut().remove(&run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::client::GitHubClient;
    use crate::api::models::{Job, Repo, User};
    use crate::ui::test_helpers::gtk_test_guard;
    use parking_lot::Mutex;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Arc;

    fn repo_stub() -> Repo {
        Repo {
            id: 1,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        }
    }

    fn client_stub() -> Arc<Mutex<GitHubClient>> {
        Arc::new(Mutex::new(
            GitHubClient::new(None).expect("client stub should build"),
        ))
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn remove_job_context_only_removes_matching_expander() {
        let Some(_guard) = gtk_test_guard("remove_job_context_only_removes_matching_expander")
        else {
            return;
        };

        let old_expander = gtk::Expander::new(None);
        let new_expander = gtk::Expander::new(None);
        let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        jobs_box.append(&gtk::Spinner::new());
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));

        job_contexts.borrow_mut().insert(
            42,
            JobRefreshContext::from_params(JobRefreshContextParams {
                client: client_stub(),
                owner: "mak".into(),
                repo: "actioneer".into(),
                workflow_id: 7,
                run_id: 42,
                expander: new_expander.clone(),
                jobs_box: jobs_box.clone(),
                badges_box: None,
                parent_window: gtk::Window::new(),
                repo_model: repo_stub(),
                branch: Some("main".into()),
                run_title: "CI".into(),
                jobs: std::sync::Arc::new(Vec::new()),
                job_summaries: Rc::new(RefCell::new(HashMap::new())),
            }),
        );

        remove_job_context_if_current(&job_contexts, 42, &old_expander);
        assert!(job_contexts.borrow().contains_key(&42));

        remove_job_context_if_current(&job_contexts, 42, &new_expander);
        assert!(!job_contexts.borrow().contains_key(&42));
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn rebind_preserved_job_context_skips_placeholders() {
        let Some(_guard) = gtk_test_guard("rebind_preserved_job_context_skips_placeholders") else {
            return;
        };

        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        jobs_box.append(&gtk::Label::new(Some("placeholder")));

        rebind_preserved_job_context(
            &job_contexts,
            JobRefreshContextParams {
                client: client_stub(),
                owner: "mak".into(),
                repo: "actioneer".into(),
                workflow_id: 7,
                run_id: 42,
                expander: gtk::Expander::new(None),
                jobs_box,
                badges_box: None,
                parent_window: gtk::Window::new(),
                repo_model: repo_stub(),
                branch: Some("main".into()),
                run_title: "CI".into(),
                jobs: std::sync::Arc::new(Vec::new()),
                job_summaries: Rc::new(RefCell::new(HashMap::new())),
            },
        );

        assert!(job_contexts.borrow().is_empty());
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn rebind_preserved_job_context_restores_badges_from_cached_jobs() {
        let Some(_guard) =
            gtk_test_guard("rebind_preserved_job_context_restores_badges_from_cached_jobs")
        else {
            return;
        };

        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let old_expander = gtk::Expander::new(None);
        let new_expander = gtk::Expander::new(None);
        let old_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        old_jobs_box.append(&gtk::Spinner::new());

        let cached_jobs = std::sync::Arc::new(vec![Job {
            id: 1,
            run_id: 42,
            status: Some("in_progress".into()),
            conclusion: None,
            started_at: None,
            completed_at: None,
            name: Some("Build".into()),
            steps: Vec::new(),
            html_url: None,
        }]);

        job_contexts.borrow_mut().insert(
            42,
            JobRefreshContext::from_params(JobRefreshContextParams {
                client: client_stub(),
                owner: "mak".into(),
                repo: "actioneer".into(),
                workflow_id: 7,
                run_id: 42,
                expander: old_expander,
                jobs_box: old_jobs_box,
                badges_box: None,
                parent_window: gtk::Window::new(),
                repo_model: repo_stub(),
                branch: Some("main".into()),
                run_title: "CI".into(),
                jobs: cached_jobs,
                job_summaries: Rc::new(RefCell::new(HashMap::new())),
            }),
        );

        let new_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        new_jobs_box.append(&gtk::Spinner::new());
        let new_badges_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        rebind_preserved_job_context(
            &job_contexts,
            JobRefreshContextParams {
                client: client_stub(),
                owner: "mak".into(),
                repo: "actioneer".into(),
                workflow_id: 7,
                run_id: 42,
                expander: new_expander,
                jobs_box: new_jobs_box,
                badges_box: Some(new_badges_box.clone()),
                parent_window: gtk::Window::new(),
                repo_model: repo_stub(),
                branch: Some("main".into()),
                run_title: "CI".into(),
                jobs: std::sync::Arc::new(Vec::new()),
                job_summaries: Rc::new(RefCell::new(HashMap::new())),
            },
        );

        assert!(new_badges_box.first_child().is_some());
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn create_run_expander_row_restores_cached_badges_without_job_context() {
        let Some(_guard) =
            gtk_test_guard("create_run_expander_row_restores_cached_badges_without_job_context")
        else {
            return;
        };

        let run = WorkflowRun {
            id: 42,
            run_number: Some(1),
            workflow_id: Some(7),
            name: Some("CI".into()),
            display_title: Some("CI".into()),
            head_branch: Some("main".into()),
            head_commit: None,
            status: Some("in_progress".into()),
            conclusion: None,
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: None,
            actor: None,
            triggering_actor: None,
        };
        let job_summaries = Rc::new(RefCell::new(HashMap::new()));
        job_summaries.borrow_mut().insert(
            42,
            crate::api::models::JobSummary {
                queued: 1,
                running: 2,
                completed: 3,
            },
        );

        let context = RunRowContext::new(
            client_stub(),
            "mak".into(),
            "actioneer".into(),
            repo_stub(),
            adw::ApplicationWindow::builder().build(),
            7,
            adw::ToastOverlay::new(),
            Rc::new(RefCell::new(HashMap::new())),
            job_summaries,
        );

        let row = create_run_expander_row(&run, &context, false);
        let row_container = row
            .first_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("row container");
        let expander = row_container
            .first_child()
            .and_then(|child| child.downcast::<gtk::Expander>().ok())
            .expect("expander");
        let header = expander
            .label_widget()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("header box");
        let badges_box = header
            .last_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("badges box");

        assert!(badges_box.first_child().is_some());
    }
}
