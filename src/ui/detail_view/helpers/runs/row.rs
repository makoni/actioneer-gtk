use super::super::context::{
    JobContextMap, JobRefreshContext, JobRefreshContextParams, RunBadgeSummaryMap,
};
use super::super::formatting::{
    format_run_title, format_run_tooltip, get_run_status_class, get_run_status_icon,
    populate_run_meta,
};
use super::super::jobs::{LoadJobsParams, load_run_jobs};
use super::super::status_dot::{RUN_DOT_SIZE, build_status_dot};
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

    pub(super) fn actions_context(&self) -> RunActionContext {
        RunActionContext {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            repo_model: self.repo_model.clone(),
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
    let expander = build_expander(run);
    let actions_box = create_actions_box(run, &context.actions_context());

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

    // Highlight the row while the run is expanded. The reference must be weak:
    // `run_box` owns the expander, so a strong clone here would form a cycle
    // that keeps the row (and its refresh timer) alive after it is discarded.
    let run_box_weak = run_box.downgrade();
    expander.connect_expanded_notify(move |exp| {
        let Some(run_box) = run_box_weak.upgrade() else {
            return;
        };
        if exp.is_expanded() {
            run_box.add_css_class("expanded");
        } else {
            run_box.remove_css_class("expanded");
        }
    });

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
    container.add_css_class("run-item");
    container.add_css_class("hoverless-row");
    container
}

fn create_row_container() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    container.set_margin_start(4);
    container.set_margin_end(8);
    container.set_margin_top(9);
    container.set_margin_bottom(9);
    container.set_hexpand(true);
    container.set_valign(gtk::Align::Center);
    container
}

/// Run title without the trailing run number (the number is rendered separately).
fn run_title_base(run: &WorkflowRun) -> String {
    run.display_title
        .as_ref()
        .or(run.name.as_ref())
        .cloned()
        .unwrap_or_else(|| tr("Workflow Run"))
}

fn build_expander(run: &WorkflowRun) -> gtk::Expander {
    let expander = gtk::Expander::new(None);
    expander.set_hexpand(true);
    expander.set_valign(gtk::Align::Center);
    expander.set_widget_name(&format!("run_{}", run.id));
    // Horizontal breathing room around the disclosure arrow: `margin_start`
    // insets the arrow from the row edge, the header's `margin_start` below
    // leaves a gap between the arrow and the row content.
    expander.set_margin_start(6);

    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    header_box.set_hexpand(true);
    header_box.set_valign(gtk::Align::Center);
    header_box.set_margin_start(6);

    let status_dot = build_status_dot(
        get_run_status_icon(run),
        get_run_status_class(run),
        RUN_DOT_SIZE,
    );
    let tooltip = format_run_tooltip(run);
    if !tooltip.is_empty() {
        crate::ui::utils::describe_control(&status_dot, &tooltip);
    }
    header_box.append(&status_dot);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_hexpand(true);
    text_box.set_valign(gtk::Align::Center);

    let title_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_line.set_halign(gtk::Align::Start);

    let has_number = run.run_number.is_some();
    if let Some(num) = run.run_number {
        let num_label = gtk::Label::new(Some(&format!("#{num}")));
        num_label.add_css_class("run-number");
        title_line.append(&num_label);
    }

    let title_label = gtk::Label::new(Some(&run_title_base(run)));
    title_label.set_halign(gtk::Align::Start);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(pango::EllipsizeMode::End);
    if has_number {
        title_label.add_css_class("dim-label");
    } else {
        title_label.add_css_class("run-number");
    }
    title_line.append(&title_label);
    text_box.append(&title_line);

    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    meta_box.set_halign(gtk::Align::Start);
    populate_run_meta(&meta_box, run);
    meta_box.set_tooltip_text(Some(&format_run_tooltip(run)));
    text_box.append(&meta_box);

    let meta_box_weak = meta_box.downgrade();
    let run_for_timer = run.clone();
    glib::timeout_add_seconds_local(60, move || match meta_box_weak.upgrade() {
        Some(meta_box) => {
            populate_run_meta(&meta_box, &run_for_timer);
            meta_box.set_tooltip_text(Some(&format_run_tooltip(&run_for_timer)));
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });

    header_box.append(&text_box);
    expander.set_label_widget(Some(&header_box));

    expander
}

fn build_jobs_placeholder() -> gtk::Box {
    let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Keeps the job cards aligned under the run title: the title sits 6px
    // further in than before (the header's disclosure gap), so the jobs shift
    // by the same amount to stay in line.
    jobs_box.set_margin_start(42);
    jobs_box.set_margin_end(12);
    jobs_box.set_margin_top(2);
    jobs_box.set_margin_bottom(12);
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
    use crate::ui::test_helpers::run_gtk_test;
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

    fn run_stub() -> WorkflowRun {
        WorkflowRun {
            id: 42,
            run_number: Some(128),
            workflow_id: Some(7),
            name: Some("CI".into()),
            display_title: Some("Bundle flatpak manifest".into()),
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
        }
    }

    fn context_stub() -> RunRowContext {
        RunRowContext::new(
            client_stub(),
            "mak".into(),
            "actioneer".into(),
            repo_stub(),
            adw::ApplicationWindow::builder().build(),
            7,
            adw::ToastOverlay::new(),
            Rc::new(RefCell::new(HashMap::new())),
            Rc::new(RefCell::new(HashMap::new())),
        )
    }

    fn params_for(
        expander: &gtk::Expander,
        jobs_box: &gtk::Box,
        jobs: Arc<Vec<Job>>,
    ) -> JobRefreshContextParams {
        JobRefreshContextParams {
            client: client_stub(),
            owner: "mak".into(),
            repo: "actioneer".into(),
            workflow_id: 7,
            run_id: 42,
            expander: expander.clone(),
            jobs_box: jobs_box.clone(),
            parent_window: gtk::Window::new(),
            repo_model: repo_stub(),
            branch: Some("main".into()),
            run_title: "CI".into(),
            jobs,
            job_summaries: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn run_row_is_released_when_dropped() {
        run_gtk_test("run_row_is_released_when_dropped", || {
            // Regression guard: the row's own box must never be captured strongly by
            // a handler on a widget it owns. Such a cycle is invisible at runtime —
            // it silently keeps the row alive, which in turn defeats the weak-ref
            // guard on its 60s refresh timer and leaves it ticking forever.
            let mut weaks: Vec<(String, glib::WeakRef<gtk::Widget>)> = Vec::new();
            let weak = {
                let context = context_stub();
                // A running run with a start also runs the wall-clock duration
                // ticker; the release must survive it.
                let mut run = run_stub();
                run.status = Some("in_progress".into());
                run.run_started_at = Some("2024-01-01T00:00:00Z".into());
                let row = create_run_expander_row(&run, &context, false);
                crate::ui::test_helpers::collect_widget_weaks(
                    &row.clone().upcast::<gtk::Widget>(),
                    &mut weaks,
                );
                row.downgrade()
            };

            while glib::MainContext::default().pending() {
                let _ = glib::MainContext::default().iteration(false);
            }

            assert!(
                weak.upgrade().is_none(),
                "run row outlived its last strong reference — a signal handler is \
                 holding it in a reference cycle"
            );

            // Checking the outer box alone is not enough: a cycle pinning only the
            // expander leaves `meta_box` alive, and with it the 60s refresh timer
            // whose weak upgrade then never fails.
            let survivors: Vec<&str> = weaks
                .iter()
                .filter(|(_, weak)| weak.upgrade().is_some())
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(
                survivors.is_empty(),
                "widgets outlived the discarded run row: {survivors:?} — a signal \
                 handler is holding them in a reference cycle. This covers the \
                 widget tree only: a leak that pins no widget is invisible here."
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn run_row_renders_number_dot_and_meta() {
        run_gtk_test("run_row_renders_number_dot_and_meta", || {
            let context = context_stub();
            let row = create_run_expander_row(&run_stub(), &context, false);

            assert!(row.has_css_class("run-item"));

            let row_container = row
                .first_child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
                .expect("row container");
            let expander = row_container
                .first_child()
                .and_then(|child| child.downcast::<gtk::Expander>().ok())
                .expect("run expander");
            assert_eq!(expander.widget_name().as_str(), "run_42");

            let header = expander
                .label_widget()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
                .expect("header box");
            let dot = header
                .first_child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
                .expect("status dot");
            assert!(dot.has_css_class("status-dot"));
            assert!(dot.has_css_class("accent"));
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn remove_job_context_only_removes_matching_expander() {
        run_gtk_test("remove_job_context_only_removes_matching_expander", || {
            let old_expander = gtk::Expander::new(None);
            let new_expander = gtk::Expander::new(None);
            let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            jobs_box.append(&gtk::Spinner::new());
            let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));

            job_contexts.borrow_mut().insert(
                42,
                JobRefreshContext::from_params(params_for(
                    &new_expander,
                    &jobs_box,
                    Arc::new(Vec::new()),
                )),
            );

            remove_job_context_if_current(&job_contexts, 42, &old_expander);
            assert!(job_contexts.borrow().contains_key(&42));

            remove_job_context_if_current(&job_contexts, 42, &new_expander);
            assert!(!job_contexts.borrow().contains_key(&42));
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn rebind_preserved_job_context_skips_placeholders() {
        run_gtk_test("rebind_preserved_job_context_skips_placeholders", || {
            let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
            let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            jobs_box.append(&gtk::Label::new(Some("placeholder")));

            rebind_preserved_job_context(
                &job_contexts,
                params_for(&gtk::Expander::new(None), &jobs_box, Arc::new(Vec::new())),
            );

            assert!(job_contexts.borrow().is_empty());
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn rebind_preserved_job_context_keeps_cached_jobs() {
        run_gtk_test("rebind_preserved_job_context_keeps_cached_jobs", || {
            let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
            let old_expander = gtk::Expander::new(None);
            let old_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            old_jobs_box.append(&gtk::Spinner::new());

            let cached_jobs = Arc::new(vec![Job {
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
                JobRefreshContext::from_params(params_for(
                    &old_expander,
                    &old_jobs_box,
                    cached_jobs,
                )),
            );

            let new_expander = gtk::Expander::new(None);
            let new_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            new_jobs_box.append(&gtk::Spinner::new());

            rebind_preserved_job_context(
                &job_contexts,
                params_for(&new_expander, &new_jobs_box, Arc::new(Vec::new())),
            );

            let contexts = job_contexts.borrow();
            let rebound = contexts.get(&42).expect("context preserved");
            assert_eq!(rebound.jobs().len(), 1);
            assert!(rebound.matches_expander(&new_expander));
        });
    }
}
