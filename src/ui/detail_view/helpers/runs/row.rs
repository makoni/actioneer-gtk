use super::super::context::JobContextMap;
use super::super::formatting::{
    format_run_subtitle, format_run_title, get_run_status_class, get_run_status_icon,
};
use super::super::jobs::{LoadJobsParams, load_run_jobs};
use super::actions::{RunActionContext, create_actions_box};
use crate::api::GitHubClient;
use crate::api::models::{Repo, WorkflowRun};
use crate::cache::DataCache;
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
    pub(super) cache: Arc<DataCache>,
    pub(super) workflow_id: i64,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) job_contexts: JobContextMap,
}

impl RunRowContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        client: Arc<Mutex<GitHubClient>>,
        owner: String,
        repo: String,
        repo_model: Repo,
        parent_window: adw::ApplicationWindow,
        cache: Arc<DataCache>,
        workflow_id: i64,
        toast_overlay: adw::ToastOverlay,
        job_contexts: JobContextMap,
    ) -> Self {
        Self {
            client,
            owner,
            repo,
            repo_model,
            parent_window,
            cache,
            workflow_id,
            toast_overlay,
            job_contexts,
        }
    }

    fn actions_context(&self) -> RunActionContext {
        RunActionContext {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            parent_window: self.parent_window.clone(),
            cache: self.cache.clone(),
            workflow_id: self.workflow_id,
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
    let (expander, badges_box) = build_expander(run, &run_title);
    let actions_box = create_actions_box(run, &context.actions_context());

    row_container.append(&expander);
    row_container.append(&actions_box);
    run_box.append(&row_container);

    let jobs_box = build_jobs_placeholder();
    expander.set_child(Some(&jobs_box));

    let parent_window_for_jobs: gtk::Window = context.parent_window.clone().upcast();
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
        let expander_for_expand = expander.clone();
        glib::idle_add_local_once(move || {
            expander_for_expand.set_expanded(true);
        });
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
    text_box.append(&subtitle_label);

    let subtitle_label_weak = subtitle_label.downgrade();
    let run_for_timer = run.clone();
    glib::timeout_add_seconds_local(60, move || match subtitle_label_weak.upgrade() {
        Some(label) => {
            label.set_text(&format_run_subtitle(&run_for_timer));
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

    let placeholder = gtk::Label::new(Some("Click to load jobs..."));
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
    let cache = context.cache.clone();
    let job_contexts_for_load = context.job_contexts.clone();
    let job_contexts_for_remove = context.job_contexts.clone();
    let repo_model = context.repo_model.clone();
    let badges_box_for_load = badges_box.clone();
    let parent_window_for_load = parent_window.clone();
    let run_title_for_load = run_title.clone();

    expander.connect_expanded_notify(move |exp| {
        if !exp.is_expanded() {
            job_contexts_for_remove.borrow_mut().remove(&run_id);
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
                jobs_box: jobs_box.clone(),
                badges_box: Some(badges_box_for_load.clone()),
                cache: cache.clone(),
                workflow_id,
                parent_window: parent_window_for_load.clone(),
                repo_model: repo_model.clone(),
                background: false,
                bypass_cache: false,
                job_contexts: job_contexts_for_load.clone(),
                run_branch: run_branch.clone(),
                run_title: run_title_for_load.clone(),
            });
        }
    });
}
