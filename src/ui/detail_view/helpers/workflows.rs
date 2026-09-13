use super::RunLoadService;
use super::context::{JobContextMap, RunBadgeSummaryMap};
use super::formatting::{
    format_workflow_meta, get_run_status_class, get_run_status_icon, workflow_status_text,
};
use super::runs::{
    LoadRunsParams, RunActionContext, RunDigestStore, RunRowContext, WorkflowRunListModel,
    confirm_and_cancel_run,
};
use super::status_dot::{WORKFLOW_DOT_SIZE, build_status_dot, set_status_dot_state};
use super::workflow_follow_up::{FollowUpRefreshParams, schedule_follow_up_refresh};
use crate::domain::formatting::running_duration_string;
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::models::{
    JobSummary, Repo, Workflow, WorkflowDispatchInput, WorkflowDispatchInputType,
    WorkflowDispatchInputValue, WorkflowRun, build_dispatch_inputs_payload,
};
use crate::services::gateway::GitHubGateway;
use crate::services::notifications::NotificationManager;
use crate::services::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::detail_view::header_state::DetailHeaderState;
use crate::ui::job_logs_window::JobLogsWindow;
use crate::ui::utils::widget_data::{set_data, steal_data};
use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tracing::{error, info};

#[derive(Clone)]
pub(crate) struct WorkflowRowContext {
    pub client: Arc<Mutex<GitHubGateway>>,
    pub owner: String,
    pub repo: String,
    pub repo_model: Repo,
    pub parent_window: adw::ApplicationWindow,
    pub toast_overlay: adw::ToastOverlay,
    pub job_contexts: JobContextMap,
    pub run_badge_summaries: RunBadgeSummaryMap,
    pub workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>,
    pub workflows_last_loaded: Arc<Mutex<std::collections::HashMap<i64, std::time::Instant>>>,
    pub workflows_loading_runs: Arc<Mutex<HashSet<i64>>>,
    pub run_digests: Arc<Mutex<RunDigestStore>>,
    pub notification_manager: Option<NotificationManager>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub run_filters: Arc<Mutex<RunFilters>>,
    pub run_load_service: RunLoadService,
    pub header: DetailHeaderState,
}

/// Builds the "Trigger Workflow" dialog shell (form content is attached and the
/// response handler wired by the caller). The trigger action starts disabled
/// until branches and inputs have loaded.
fn build_trigger_dialog(heading: &str) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(Some(heading), None);
    dialog.add_response("cancel", tr("Cancel").as_str());
    dialog.add_response("trigger", tr("Trigger").as_str());
    dialog.set_response_appearance("trigger", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("trigger"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("trigger", false);
    dialog
}

pub(crate) struct WorkflowRowSettings {
    pub should_expand: bool,
    pub initial_expanded_run_ids: Vec<i64>,
    pub is_first: bool,
}

enum DispatchInputWidget {
    Text(adw::EntryRow),
    Choice(adw::ComboRow, Vec<String>),
    Boolean(adw::SwitchRow),
}

struct DispatchInputField {
    input: WorkflowDispatchInput,
    widget: DispatchInputWidget,
}

impl DispatchInputField {
    fn current_value(&self) -> WorkflowDispatchInputValue {
        match &self.widget {
            DispatchInputWidget::Text(row) => {
                WorkflowDispatchInputValue::String(row.text().to_string())
            }
            DispatchInputWidget::Choice(row, options) => {
                let index = row.selected() as usize;
                let value = options.get(index).cloned().unwrap_or_else(String::new);
                WorkflowDispatchInputValue::String(value)
            }
            DispatchInputWidget::Boolean(row) => {
                WorkflowDispatchInputValue::Boolean(row.is_active())
            }
        }
    }
}

fn dispatch_input_title(input: &WorkflowDispatchInput) -> String {
    if input.required {
        format!("{} *", input.name)
    } else {
        input.name.clone()
    }
}

fn dispatch_input_subtitle(input: &WorkflowDispatchInput) -> Option<String> {
    match (input.description.as_ref(), input.required) {
        (Some(description), true) => {
            Some(tr("{description} (required)").replace("{description}", description.as_str()))
        }
        (Some(description), false) => Some(description.clone()),
        (None, true) => Some(tr("Required")),
        (None, false) => None,
    }
}

/// Live widgets of a workflow row that update in place when the workflow's
/// latest run changes (initial load, expansion, background refreshes).
#[derive(Clone)]
pub(crate) struct WorkflowRowHeader {
    pub status_dot: gtk::Box,
    pub meta_label: gtk::Label,
    pub progress_row: gtk::Box,
    pub progress_bar: gtk::ProgressBar,
    pub progress_label: gtk::Label,
    // Weak: `WorkflowRunListModel` holds this header (`set_row_header`) and the
    // trigger button's handler owns that same model, so strong references here
    // would form a cycle that keeps the row — and the whole pane — alive.
    trigger_btn: glib::WeakRef<gtk::Button>,
    cancel_btn: glib::WeakRef<gtk::Button>,
    elapsed: Rc<RefCell<ElapsedTicker>>,
}

impl WorkflowRowHeader {
    fn trigger_btn(&self) -> Option<gtk::Button> {
        self.trigger_btn.upgrade()
    }

    fn cancel_btn(&self) -> Option<gtk::Button> {
        self.cancel_btn.upgrade()
    }
}

#[derive(Default)]
struct ElapsedTicker {
    source: Option<glib::SourceId>,
    started_at: Option<String>,
    counts: Option<(i32, i32)>,
}

fn render_progress_label(
    label: &gtk::Label,
    started_at: &Option<String>,
    counts: &Option<(i32, i32)>,
) {
    let elapsed = running_duration_string(started_at.as_ref());
    let text = match (counts, elapsed) {
        (Some((done, total)), Some(elapsed)) if *total > 0 => {
            format!("{done} / {total} · {elapsed}")
        }
        (Some((done, total)), None) if *total > 0 => format!("{done} / {total}"),
        (None, Some(elapsed)) => elapsed,
        _ => String::new(),
    };
    label.set_text(&text);
    label.set_visible(!text.is_empty());
}

fn stop_elapsed_ticker(header: &WorkflowRowHeader) {
    let source = header.elapsed.borrow_mut().source.take();
    if let Some(source) = source {
        let _ = crate::ui::utils::try_remove_source(source);
    }
}

fn start_elapsed_ticker(
    header: &WorkflowRowHeader,
    started_at: Option<String>,
    counts: Option<(i32, i32)>,
) {
    {
        let mut state = header.elapsed.borrow_mut();
        state.started_at = started_at;
        state.counts = counts;
        if state.source.is_some() {
            return;
        }
    }

    let progress_label = header.progress_label.downgrade();
    let progress_bar = header.progress_bar.downgrade();
    let progress_row = header.progress_row.downgrade();
    let elapsed = header.elapsed.clone();

    let source = glib::timeout_add_seconds_local(1, move || {
        let (Some(label), Some(bar), Some(row)) = (
            progress_label.upgrade(),
            progress_bar.upgrade(),
            progress_row.upgrade(),
        ) else {
            elapsed.borrow_mut().source = None;
            return glib::ControlFlow::Break;
        };

        if !row.is_visible() {
            elapsed.borrow_mut().source = None;
            return glib::ControlFlow::Break;
        }

        let (started_at, counts) = {
            let state = elapsed.borrow();
            (state.started_at.clone(), state.counts)
        };

        render_progress_label(&label, &started_at, &counts);
        if counts.is_none() {
            bar.pulse();
        }

        glib::ControlFlow::Continue
    });

    header.elapsed.borrow_mut().source = Some(source);
}

/// Refreshes a workflow row's status dot, meta line, progress indicator and
/// action buttons from the workflow's latest run.
pub(crate) fn update_workflow_row_header(
    header: &WorkflowRowHeader,
    latest: Option<&WorkflowRun>,
    summary: Option<&JobSummary>,
) {
    match latest {
        Some(run) => {
            set_status_dot_state(
                &header.status_dot,
                get_run_status_icon(run),
                get_run_status_class(run),
            );
            let (status_text, _) = workflow_status_text(run);
            crate::ui::utils::describe_control(&header.status_dot, &status_text);

            let meta = format_workflow_meta(run);
            if meta.is_empty() {
                header.meta_label.set_text(tr("No runs yet").as_str());
            } else {
                header.meta_label.set_text(&meta);
            }

            let active = run.is_active();
            if let Some(trigger) = header.trigger_btn() {
                trigger.set_visible(!active);
            }
            if let Some(cancel) = header.cancel_btn() {
                cancel.set_visible(active && run.is_cancellable());
            }

            if active {
                header.progress_row.set_visible(true);
                let counts = summary
                    .filter(|s| !s.is_empty())
                    .map(|s| (s.completed, s.completed + s.running + s.queued));
                if let Some((done, total)) = counts
                    && total > 0
                {
                    header.progress_bar.set_fraction(done as f64 / total as f64);
                }
                render_progress_label(&header.progress_label, &run.run_started_at, &counts);
                start_elapsed_ticker(header, run.run_started_at.clone(), counts);
            } else {
                header.progress_row.set_visible(false);
                stop_elapsed_ticker(header);
            }
        }
        None => {
            set_status_dot_state(&header.status_dot, "window-minimize-symbolic", "idle");
            crate::ui::utils::describe_control(&header.status_dot, tr("No runs yet").as_str());
            header.meta_label.set_text(tr("No runs yet").as_str());
            if let Some(trigger) = header.trigger_btn() {
                trigger.set_visible(true);
            }
            if let Some(cancel) = header.cancel_btn() {
                cancel.set_visible(false);
            }
            header.progress_row.set_visible(false);
            stop_elapsed_ticker(header);
        }
    }
}

/// Basename of a workflow file path, e.g. `.github/workflows/ci.yml` → `ci.yml`.
fn workflow_file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn create_workflow_expander_row(
    workflow: &Workflow,
    context: &WorkflowRowContext,
    settings: WorkflowRowSettings,
) -> gtk::Box {
    let should_expand = settings.should_expand;
    let initial_expanded_run_ids = Rc::new(RefCell::new(Some(settings.initial_expanded_run_ids)));
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let repo_model = context.repo_model.clone();
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let job_contexts = context.job_contexts.clone();
    let workflows_with_active = context.workflows_with_active_runs.clone();
    let workflows_last_loaded = context.workflows_last_loaded.clone();
    let workflows_loading_runs = context.workflows_loading_runs.clone();
    let run_digests = context.run_digests.clone();
    let notification_manager = context.notification_manager.clone();
    let preferences_manager = context.preferences_manager.clone();
    let run_filters = context.run_filters.clone();
    let run_filters_for_signal = run_filters.clone();
    let run_load_service = context.run_load_service.clone();
    let run_load_service_for_signal = run_load_service.clone();

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    main_box.add_css_class("workflow-item");
    if settings.is_first {
        main_box.add_css_class("workflow-item-first");
    }
    let workflow_display_name = format!("{}/{} • {}", owner, repo, workflow.name);
    let workflow_path = workflow.path.clone();
    let workflow_file = workflow_file_name(&workflow.path).to_string();

    // ---- Row header: chevron (expander arrow) + status dot + title/meta + actions
    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header_box.set_margin_start(6);
    header_box.set_margin_end(10);
    header_box.set_margin_top(12);
    header_box.set_margin_bottom(12);
    header_box.set_valign(gtk::Align::Center);

    let status_dot = build_status_dot("window-minimize-symbolic", "idle", WORKFLOW_DOT_SIZE);
    header_box.append(&status_dot);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text_box.set_hexpand(true);
    text_box.set_valign(gtk::Align::Center);

    let workflow_name_label = gtk::Label::new(Some(&workflow.name));
    workflow_name_label.set_halign(gtk::Align::Start);
    workflow_name_label.set_hexpand(true);
    workflow_name_label.set_ellipsize(pango::EllipsizeMode::End);
    workflow_name_label.add_css_class("workflow-title");
    text_box.append(&workflow_name_label);

    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    meta_box.set_halign(gtk::Align::Start);

    let file_label = gtk::Label::new(Some(&workflow_file));
    file_label.add_css_class("workflow-file");
    file_label.add_css_class("dim-label");
    file_label.add_css_class("caption");
    meta_box.append(&file_label);

    let meta_separator = gtk::Label::new(Some("·"));
    meta_separator.add_css_class("dim-label");
    meta_separator.add_css_class("caption");
    meta_box.append(&meta_separator);

    let meta_label = gtk::Label::new(Some(tr("No runs yet").as_str()));
    meta_label.add_css_class("dim-label");
    meta_label.add_css_class("caption");
    meta_label.set_halign(gtk::Align::Start);
    meta_label.set_ellipsize(pango::EllipsizeMode::End);
    meta_box.append(&meta_label);

    text_box.append(&meta_box);
    header_box.append(&text_box);

    // ---- Trailing actions: logs, trigger (play) / cancel (stop while running)
    let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions_box.set_valign(gtk::Align::Center);
    actions_box.set_halign(gtk::Align::End);

    let logs_btn = gtk::Button::from_icon_name("text-x-generic-symbolic");
    crate::ui::utils::describe_control(&logs_btn, tr("View logs").as_str());
    logs_btn.add_css_class("row-action-btn");
    logs_btn.set_valign(gtk::Align::Center);
    logs_btn.set_focus_on_click(false);
    actions_box.append(&logs_btn);

    let trigger_btn = gtk::Button::from_icon_name("media-playback-start-symbolic");
    crate::ui::utils::describe_control(&trigger_btn, tr("Trigger workflow").as_str());
    trigger_btn.add_css_class("row-action-btn");
    trigger_btn.add_css_class("run-action");
    trigger_btn.set_valign(gtk::Align::Center);
    trigger_btn.set_focus_on_click(false);
    actions_box.append(&trigger_btn);

    let cancel_btn = gtk::Button::from_icon_name("process-stop-symbolic");
    crate::ui::utils::describe_control(&cancel_btn, tr("Cancel run").as_str());
    cancel_btn.add_css_class("row-action-btn");
    cancel_btn.add_css_class("cancel-action");
    cancel_btn.set_valign(gtk::Align::Center);
    cancel_btn.set_focus_on_click(false);
    cancel_btn.set_visible(false);
    actions_box.append(&cancel_btn);

    header_box.append(&actions_box);

    let expander = gtk::Expander::new(None);
    // Horizontal breathing room around the disclosure arrow: `margin_start`
    // insets the arrow from the card edge, the header's own `margin_start`
    // (set above) leaves a gap between the arrow and the row content.
    expander.set_margin_start(6);
    expander.set_label_widget(Some(&header_box));
    expander.set_widget_name(&format!("workflow_{}", workflow.id));
    set_data(&expander, "actioneer-workflow-name", workflow.name.clone());
    set_data(&expander, "actioneer-workflow-id", workflow.id);

    // ---- Expanded area: progress + "Recent runs" sub-header + runs card
    let detail_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    detail_box.add_css_class("workflow-detail");
    detail_box.set_margin_start(62);
    detail_box.set_margin_end(14);
    detail_box.set_margin_bottom(14);

    let progress_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    progress_row.set_valign(gtk::Align::Center);
    progress_row.set_visible(false);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.add_css_class("workflow-progress");
    progress_bar.set_hexpand(true);
    progress_bar.set_valign(gtk::Align::Center);
    progress_row.append(&progress_bar);

    let progress_label = gtk::Label::new(None);
    progress_label.add_css_class("mono");
    progress_label.add_css_class("dim-label");
    progress_label.add_css_class("caption");
    progress_row.append(&progress_label);

    detail_box.append(&progress_row);

    let subheader = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    subheader.set_margin_top(2);

    let (recent_heading, recent_suppress_tracking) =
        crate::ui::utils::section_heading(&tr("Recent runs"));
    let recent_label = gtk::Label::new(Some(&recent_heading));
    if recent_suppress_tracking {
        recent_label.add_css_class("no-tracking");
    }
    recent_label.add_css_class("section-label");
    recent_label.set_halign(gtk::Align::Start);
    recent_label.set_hexpand(true);
    subheader.append(&recent_label);

    let counts_label = gtk::Label::new(None);
    counts_label.add_css_class("dim-label");
    counts_label.add_css_class("caption");
    counts_label.set_visible(false);
    subheader.append(&counts_label);

    let counts_separator = gtk::Label::new(Some("·"));
    counts_separator.add_css_class("dim-label");
    counts_separator.add_css_class("caption");
    subheader.append(&counts_separator);
    counts_label
        .bind_property("visible", &counts_separator, "visible")
        .sync_create()
        .build();

    let actions_url = format!(
        "https://github.com/{}/{}/actions/workflows/{}",
        owner, repo, workflow_file
    );
    let all_link = gtk::LinkButton::with_label(&actions_url, tr("All on GitHub").as_str());
    all_link.add_css_class("caption");
    all_link.set_valign(gtk::Align::Center);
    subheader.append(&all_link);

    detail_box.append(&subheader);

    let run_row_context = RunRowContext::new(
        client.clone(),
        owner.clone(),
        repo.clone(),
        repo_model.clone(),
        parent_window.clone(),
        workflow.id,
        toast_overlay.clone(),
        job_contexts.clone(),
        context.run_badge_summaries.clone(),
    );
    let run_list = WorkflowRunListModel::new(run_row_context);

    let runs_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    runs_card.add_css_class("runs-card");
    runs_card.set_overflow(gtk::Overflow::Hidden);
    runs_card.append(&run_list.widget());
    detail_box.append(&runs_card);

    expander.set_child(Some(&detail_box));

    let row_header = WorkflowRowHeader {
        status_dot,
        meta_label,
        progress_row,
        progress_bar,
        progress_label,
        trigger_btn: trigger_btn.downgrade(),
        cancel_btn: cancel_btn.downgrade(),
        elapsed: Rc::new(RefCell::new(ElapsedTicker::default())),
    };
    run_list.set_row_header(row_header.clone());
    run_list.set_detail_header(context.header.clone());
    run_list.set_counts_label(counts_label);
    set_data(&expander, "actioneer-run-list", run_list.clone());
    main_box.append(&expander);

    // Render the header from the latest run we already know about (if any).
    {
        let latest = context.header.latest_run(workflow.id);
        let summary = latest
            .as_ref()
            .and_then(|run| context.run_badge_summaries.borrow().get(&run.id).cloned());
        update_workflow_row_header(&row_header, latest.as_ref(), summary.as_ref());
    }

    // Highlight the row while the workflow is expanded. The reference must be
    // weak: `main_box` owns the expander, so a strong clone here would form a
    // cycle that keeps the row (and its elapsed ticker) alive after a rebuild.
    let main_box_weak = main_box.downgrade();
    expander.connect_expanded_notify(move |exp| {
        let Some(main_box) = main_box_weak.upgrade() else {
            return;
        };
        if exp.is_expanded() {
            main_box.add_css_class("expanded");
        } else {
            main_box.remove_css_class("expanded");
        }
    });

    // Cancel the workflow's latest (running) run.
    {
        let header_state = context.header.clone();
        let workflow_id_for_cancel = workflow.id;
        let client_for_cancel = client.clone();
        let owner_for_cancel = owner.clone();
        let repo_for_cancel = repo.clone();
        let repo_model_for_cancel = repo_model.clone();
        let parent_window_for_cancel = parent_window.clone();
        let toast_overlay_for_cancel = toast_overlay.clone();
        cancel_btn.connect_clicked(move |btn| {
            let Some(run) = header_state.latest_run(workflow_id_for_cancel) else {
                return;
            };
            if !run.is_cancellable() {
                return;
            }
            let action_context = RunActionContext {
                client: client_for_cancel.clone(),
                owner: owner_for_cancel.clone(),
                repo: repo_for_cancel.clone(),
                repo_model: repo_model_for_cancel.clone(),
                parent_window: parent_window_for_cancel.clone(),
                toast_overlay: toast_overlay_for_cancel.clone(),
            };
            confirm_and_cancel_run(&run, &action_context, btn);
        });
    }

    // Open the logs of the latest run's most relevant job.
    {
        let header_state = context.header.clone();
        let workflow_id_for_logs = workflow.id;
        let client_for_logs = client.clone();
        let owner_for_logs = owner.clone();
        let repo_for_logs = repo.clone();
        let repo_model_for_logs = repo_model.clone();
        let parent_window_for_logs = parent_window.clone();
        let toast_overlay_for_logs = toast_overlay.clone();
        logs_btn.connect_clicked(move |btn| {
            let Some(run) = header_state.latest_run(workflow_id_for_logs) else {
                toast_overlay_for_logs.add_toast(adw::Toast::new(tr("No runs yet").as_str()));
                return;
            };

            // Guard against a double click opening two log windows.
            btn.set_sensitive(false);
            let btn_for_result = btn.clone();

            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<Result<Vec<crate::services::api::models::Job>, String>>(
                        glib::Priority::default(),
                    );

            let parent_window = parent_window_for_logs.clone();
            let repo_model = repo_model_for_logs.clone();
            let toast_overlay = toast_overlay_for_logs.clone();
            let client_for_window = client_for_logs.clone();
            let run_title = super::formatting::format_run_title(&run);
            receiver.attach(None, move |result| {
                btn_for_result.set_sensitive(true);
                match result {
                    Ok(jobs) => {
                        // A run has no log of its own — it is the set of its job
                        // logs — so the window lists them all and preselects the
                        // one a reader most likely wants.
                        match crate::ui::job_logs_window::most_relevant_job(&jobs) {
                            Some(selected) => {
                                let logs_window = JobLogsWindow::for_run(
                                    &parent_window,
                                    repo_model.clone(),
                                    run_title.clone(),
                                    jobs,
                                    selected,
                                    client_for_window.clone(),
                                );
                                logs_window.present();
                            }
                            None => {
                                toast_overlay
                                    .add_toast(adw::Toast::new(tr("No jobs found").as_str()));
                            }
                        }
                    }
                    Err(message) => {
                        error!("Failed to load jobs for logs: {}", message);
                        toast_overlay
                            .add_toast(adw::Toast::new(tr("Unable to load jobs").as_str()));
                    }
                }
                glib::ControlFlow::Break
            });

            let client = client_for_logs.clone();
            let owner = owner_for_logs.clone();
            let repo = repo_for_logs.clone();
            crate::runtime::handle().spawn(async move {
                let client_guard = client.lock().clone();
                let result = client_guard
                    .list_jobs(&owner, &repo, run.id)
                    .await
                    .map_err(|err| err.to_string());
                let _ = sender.send(result);
            });
        });
    }

    let client_for_trigger = client.clone();
    let owner_for_trigger = owner.clone();
    let repo_for_trigger = repo.clone();
    let workflow_id_for_trigger = workflow.id;
    let workflow_name_for_trigger = workflow.name.clone();
    let workflow_display_for_trigger = workflow_display_name.clone();
    let parent_window_for_trigger = parent_window.clone();
    let toast_overlay_for_trigger = toast_overlay.clone();
    let expander_for_trigger = expander.downgrade();
    let job_contexts_for_trigger = job_contexts.clone();
    let run_digests_shared = run_digests.clone();
    let notification_manager_shared = notification_manager.clone();
    let preferences_manager_shared = preferences_manager.clone();
    let run_digests_for_trigger = run_digests_shared.clone();
    let notification_manager_for_trigger = notification_manager_shared.clone();
    let preferences_manager_for_trigger = preferences_manager_shared.clone();
    let workflows_with_active_shared = workflows_with_active.clone();
    let workflows_loading_shared = workflows_loading_runs.clone();
    let workflows_last_loaded_shared = workflows_last_loaded.clone();

    let workflow_id = workflow.id;
    let owner_string = owner.clone();
    let repo_string = repo.clone();
    let workflow_display_shared = workflow_display_name.clone();
    let client_shared = client.clone();
    let run_list_shared = run_list.clone();
    let parent_window_shared = parent_window.clone();
    let toast_overlay_shared = toast_overlay.clone();
    let repo_model_shared = repo_model.clone();
    let repo_model_for_signal = repo_model.clone();
    let repo_model_for_trigger = repo_model.clone();

    let owner_for_signal = owner_string.clone();
    let repo_for_signal = repo_string.clone();
    let workflow_display_for_signal = workflow_display_shared.clone();
    let client_for_signal = client_shared.clone();
    let run_list_for_signal = run_list_shared.clone();
    let parent_window_for_signal = parent_window_shared.clone();
    let toast_overlay_for_signal = toast_overlay_shared.clone();
    let job_contexts_shared = job_contexts.clone();
    let job_contexts_for_signal = job_contexts_shared.clone();
    let initial_expanded_runs_shared = initial_expanded_run_ids.clone();
    let initial_expanded_runs_for_signal = initial_expanded_runs_shared.clone();
    let workflows_with_active_for_signal = workflows_with_active_shared.clone();
    let workflows_loading_for_signal = workflows_loading_shared.clone();
    let workflows_last_loaded_for_signal = workflows_last_loaded_shared.clone();
    let run_digests_for_signal = run_digests_shared.clone();
    let notification_manager_for_signal = notification_manager_shared.clone();
    let preferences_manager_for_signal = preferences_manager_shared.clone();
    let run_load_service_for_initial = run_load_service.clone();

    let is_programmatic_expand = Rc::new(Cell::new(false));
    let is_programmatic_for_signal = is_programmatic_expand.clone();

    expander.connect_expanded_notify(move |exp| {
        if !exp.is_expanded() {
            return;
        }

        if is_programmatic_for_signal.get() {
            return;
        }

        let force_refresh = steal_data::<bool, _>(exp, "actioneer-force-refresh").unwrap_or(false);

        let mut should_load = force_refresh;
        if !should_load {
            should_load = run_list_for_signal.should_load_runs();
        }

        if should_load {
            let preserved_runs = {
                let mut initial_opt = initial_expanded_runs_for_signal.borrow_mut();
                if let Some(vec) = initial_opt.take() {
                    vec
                } else {
                    drop(initial_opt);
                    run_list_for_signal.expanded_run_ids().into_iter().collect()
                }
            };

            run_load_service_for_signal.request(LoadRunsParams {
                client: client_for_signal.clone(),
                owner: owner_for_signal.clone(),
                repo: repo_for_signal.clone(),
                repo_model: repo_model_for_signal.clone(),
                workflow_id,
                workflow_name: workflow_display_for_signal.clone(),
                run_list: run_list_for_signal.clone(),
                parent_window: parent_window_for_signal.clone(),
                expander: exp.clone(),
                toast_overlay: toast_overlay_for_signal.clone(),
                job_contexts: job_contexts_for_signal.clone(),
                expanded_run_ids: preserved_runs,
                workflows_with_active: workflows_with_active_for_signal.clone(),
                workflows_last_loaded: workflows_last_loaded_for_signal.clone(),
                workflows_loading: workflows_loading_for_signal.clone(),
                background: false,
                run_digests: run_digests_for_signal.clone(),
                notification_manager: notification_manager_for_signal.clone(),
                preferences_manager: preferences_manager_for_signal.clone(),
                run_filters: run_filters_for_signal.clone(),
            });
        }
    });

    let run_filters_for_initial = run_filters.clone();

    if should_expand {
        is_programmatic_expand.set(true);
        expander.set_expanded(true);
        is_programmatic_expand.set(false);

        if run_list_shared.should_load_runs() {
            let preserved_runs = {
                let mut initial_opt = initial_expanded_runs_shared.borrow_mut();
                if let Some(vec) = initial_opt.take() {
                    vec
                } else {
                    drop(initial_opt);
                    run_list_shared.expanded_run_ids().into_iter().collect()
                }
            };

            run_load_service_for_initial.request(LoadRunsParams {
                client: client_shared.clone(),
                owner: owner_string.clone(),
                repo: repo_string.clone(),
                repo_model: repo_model_shared.clone(),
                workflow_id,
                workflow_name: workflow_display_shared.clone(),
                run_list: run_list_shared.clone(),
                parent_window: parent_window_shared.clone(),
                expander: expander.clone(),
                toast_overlay: toast_overlay_shared.clone(),
                job_contexts: job_contexts_shared.clone(),
                expanded_run_ids: preserved_runs,
                workflows_with_active: workflows_with_active_shared.clone(),
                workflows_last_loaded: workflows_last_loaded_shared.clone(),
                workflows_loading: workflows_loading_shared.clone(),
                background: false,
                run_digests: run_digests_shared.clone(),
                notification_manager: notification_manager_shared.clone(),
                preferences_manager: preferences_manager_shared.clone(),
                run_filters: run_filters_for_initial.clone(),
            });
        }
    }

    // The dispatch dialog is ~550 lines on its own; it lives in `workflows/trigger.rs`.
    connect_trigger_button(
        &trigger_btn,
        TriggerContext {
            client: client_for_trigger,
            owner: owner_for_trigger,
            repo: repo_for_trigger,
            repo_model: repo_model_for_trigger,
            workflow_id: workflow_id_for_trigger,
            workflow_name: workflow_name_for_trigger,
            workflow_display: workflow_display_for_trigger,
            parent_window: parent_window_for_trigger,
            toast_overlay: toast_overlay_for_trigger,
            expander: expander_for_trigger,
            job_contexts: job_contexts_for_trigger,
            run_digests: run_digests_for_trigger,
            notification_manager: notification_manager_for_trigger,
            preferences_manager: preferences_manager_for_trigger,
            run_filters: run_filters.clone(),
            run_load_service: run_load_service.clone(),
            workflows_with_active_shared: workflows_with_active_shared.clone(),
            workflows_loading_shared: workflows_loading_shared.clone(),
            workflows_last_loaded: workflows_last_loaded_shared.clone(),
            workflow_path: workflow_path.clone(),
            run_list: run_list_shared.clone(),
        },
    );

    main_box
}

mod trigger;
use trigger::{TriggerContext, connect_trigger_button};

#[cfg(test)]
mod tests;
