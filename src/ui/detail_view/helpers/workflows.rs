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
        let _ = crate::ui::detail_view::source::try_remove_source(source);
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
    let initial_expanded_run_ids = Rc::new(RefCell::new(Some(
        settings.initial_expanded_run_ids.clone(),
    )));
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

    // The widget tree itself is built in `workflows/row_widgets.rs`; what stays
    // here is the behaviour wired onto it.
    let RowWidgets {
        logs_btn,
        trigger_btn,
        cancel_btn,
        expander,
        run_list,
    } = build_row_widgets(&main_box, context, workflow);

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

    // Cancelling the latest run lives in `workflows/cancel_button.rs`.
    connect_cancel_button(
        &cancel_btn,
        context,
        workflow,
        &client,
        &owner,
        &repo,
        &repo_model,
        &parent_window,
        &toast_overlay,
    );

    // Opening the logs of the latest run lives in `workflows/logs_button.rs`.
    connect_logs_button(
        &logs_btn,
        context,
        workflow,
        &client,
        &owner,
        &repo,
        &repo_model,
        &parent_window,
        &toast_overlay,
    );

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

mod cancel_button;
mod logs_button;
mod row_widgets;
mod trigger;
use cancel_button::connect_cancel_button;
use logs_button::connect_logs_button;
use row_widgets::{RowWidgets, build_row_widgets};
use trigger::{TriggerContext, connect_trigger_button};

#[cfg(test)]
mod tests;
