use super::helpers::{LoadRunsParams, current_job_context_run_ids, refresh_jobs_for_workflows};
use super::{RepoDetailPane, WorkflowListContext};
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::runtime::channel::Sender as UiChannelSender;
use crate::services::api::GitHubError;
use crate::services::api::models::Workflow;
use crate::services::gateway::GitHubGateway;
use crate::ui::detail_view::source::try_remove_source;
use crate::ui::utils::widget_data::get_data_clone;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

mod scan;
use scan::{union_active_workflows, visit_expanders};

const DEFAULT_AUTO_REFRESH_INTERVAL_SECS: u64 = 10;
const SILENT_WORKFLOW_REFRESH_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct SilentWorkflowRefreshState {
    workflows: Arc<Mutex<Arc<Vec<Workflow>>>>,
    loading: Arc<Mutex<bool>>,
    workflows_last_silent_refresh: Arc<Mutex<Option<Instant>>>,
}

enum WorkflowLoadMessage {
    Finished(Result<Arc<Vec<Workflow>>, GitHubError>),
    Dropped,
}

struct WorkflowLoadNotifier {
    sender: Option<UiChannelSender<WorkflowLoadMessage>>,
}

impl WorkflowLoadNotifier {
    fn new(sender: UiChannelSender<WorkflowLoadMessage>) -> Self {
        Self {
            sender: Some(sender),
        }
    }

    fn finish(mut self, result: Result<Arc<Vec<Workflow>>, GitHubError>) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(WorkflowLoadMessage::Finished(result));
        }
    }
}

impl Drop for WorkflowLoadNotifier {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(WorkflowLoadMessage::Dropped);
        }
    }
}

fn should_skip_silent_workflow_refresh(last_refresh: Option<Instant>, now: Instant) -> bool {
    last_refresh
        .map(|previous| now.duration_since(previous) < SILENT_WORKFLOW_REFRESH_TTL)
        .unwrap_or(false)
}

fn should_refresh_workflow_runs_in_background(expander_expanded: bool, is_active: bool) -> bool {
    expander_expanded || is_active
}

fn workflow_load_task_dropped_error(task_name: &str) -> GitHubError {
    GitHubError::ApiError(format!("{task_name} ended without returning a result"))
}

fn workflow_load_result(
    message: WorkflowLoadMessage,
    task_name: &str,
) -> Result<Arc<Vec<Workflow>>, GitHubError> {
    match message {
        WorkflowLoadMessage::Finished(result) => result,
        WorkflowLoadMessage::Dropped => Err(workflow_load_task_dropped_error(task_name)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoRefreshTick {
    Stop,
    Skip,
    Refresh,
}

fn auto_refresh_tick(window_visible: bool, refresh_active: bool) -> AutoRefreshTick {
    if !refresh_active {
        AutoRefreshTick::Stop
    } else if !window_visible {
        AutoRefreshTick::Skip
    } else {
        AutoRefreshTick::Refresh
    }
}

fn should_listen_for_refresh_updates(
    workflow_view_alive: bool,
    lifecycle_alive: bool,
    refresh_active: bool,
) -> bool {
    workflow_view_alive && lifecycle_alive && refresh_active
}

fn cancel_auto_refresh_timer_slot(auto_refresh_source: &Arc<Mutex<Option<glib::SourceId>>>) {
    if let Some(source_id) = auto_refresh_source.lock().take() {
        let _ = try_remove_source(source_id);
    }
}

fn should_reconfigure_auto_refresh(
    current_interval: Option<u64>,
    has_timer: bool,
    requested_interval: u64,
) -> bool {
    current_interval != Some(requested_interval) || !has_timer
}

fn configure_auto_refresh_timer_slot(
    auto_refresh_source: &Arc<Mutex<Option<glib::SourceId>>>,
    auto_refresh_interval: &Arc<Mutex<Option<u64>>>,
    refresh_active: &Arc<AtomicBool>,
    silent_refresh: &SilentWorkflowRefreshState,
    list_context: &WorkflowListContext,
    repo_full_name: &str,
    refresh_interval_secs: u64,
) {
    let has_timer = auto_refresh_source.lock().is_some();
    let current_interval = *auto_refresh_interval.lock();
    if !should_reconfigure_auto_refresh(current_interval, has_timer, refresh_interval_secs) {
        return;
    }

    list_context
        .header
        .set_refresh_interval(refresh_interval_secs);

    cancel_auto_refresh_timer_slot(auto_refresh_source);

    if refresh_interval_secs == 0 {
        *auto_refresh_interval.lock() = Some(0);
        info!(
            "Auto-refresh disabled for {} (interval = 0)",
            repo_full_name
        );
        return;
    }

    let client = list_context.client.clone();
    let owner = list_context.owner.clone();
    let repo_name = list_context.repo.clone();
    let repo_model = list_context.repo_model.clone();
    let parent_window = list_context.parent_window.clone();
    let list_store = list_context.store.clone();
    let workflows_with_active = list_context.workflows_with_active_runs.clone();
    let job_contexts = list_context.job_contexts.clone();
    let run_badge_summaries = list_context.run_badge_summaries.clone();
    let toast_overlay = list_context.toast_overlay.clone();
    let run_digests = list_context.run_digests.clone();
    let notification_manager = list_context.notification_manager.clone();
    let preferences_manager = list_context.preferences_manager.clone();
    let workflows_loading_runs = list_context.workflows_loading_runs.clone();
    let workflows_last_loaded = list_context.workflows_last_loaded.clone();
    let run_filters = list_context.run_filters.clone();
    let run_load_service = list_context.run_load_service.clone();
    let header_state = list_context.header.clone();
    let silent_refresh = silent_refresh.clone();
    let repo_full_name = repo_full_name.to_string();
    let refresh_active = refresh_active.clone();

    info!(
        "Starting auto-refresh timer with interval: {} seconds",
        refresh_interval_secs
    );

    let source_id = glib::timeout_add_seconds_local(refresh_interval_secs as u32, move || {
        match auto_refresh_tick(
            parent_window.is_visible(),
            refresh_active.load(Ordering::Relaxed),
        ) {
            AutoRefreshTick::Stop => {
                info!("Stopping auto-refresh for inactive pane");
                return glib::ControlFlow::Break;
            }
            AutoRefreshTick::Skip => {
                info!("Skipping auto-refresh while window is hidden");
                return glib::ControlFlow::Continue;
            }
            AutoRefreshTick::Refresh => {}
        }

        info!("Auto-refreshing workflow runs in background");

        let background_context = WorkflowListContext {
            store: list_store.clone(),
            client: client.clone(),
            owner: owner.clone(),
            repo: repo_name.clone(),
            repo_model: repo_model.clone(),
            parent_window: parent_window.clone(),
            toast_overlay: toast_overlay.clone(),
            job_contexts: job_contexts.clone(),
            run_badge_summaries: run_badge_summaries.clone(),
            workflows_with_active_runs: workflows_with_active.clone(),
            workflows_last_loaded: workflows_last_loaded.clone(),
            workflows_loading_runs: workflows_loading_runs.clone(),
            run_digests: run_digests.clone(),
            notification_manager: notification_manager.clone(),
            preferences_manager: preferences_manager.clone(),
            run_filters: run_filters.clone(),
            run_load_service: run_load_service.clone(),
            expand_first_workflow: Rc::new(Cell::new(false)),
            header: header_state.clone(),
        };

        refresh_workflows_silent_with_state(
            silent_refresh.loading.clone(),
            silent_refresh.workflows_last_silent_refresh.clone(),
            silent_refresh.workflows.clone(),
            background_context.clone(),
            repo_full_name.clone(),
        );
        RepoDetailPane::refresh_runs_background(&background_context);

        glib::ControlFlow::Continue
    });

    *auto_refresh_source.lock() = Some(source_id);
    *auto_refresh_interval.lock() = Some(refresh_interval_secs);
}

fn refresh_workflows_silent_with_state(
    loading: Arc<Mutex<bool>>,
    workflows_last_silent_refresh: Arc<Mutex<Option<Instant>>>,
    workflows: Arc<Mutex<Arc<Vec<Workflow>>>>,
    context: WorkflowListContext,
    repo_full_name: String,
) {
    if *loading.lock() {
        info!("Already loading workflows, skipping silent refresh");
        return;
    }

    let now = Instant::now();
    {
        let mut last_refresh = workflows_last_silent_refresh.lock();
        if should_skip_silent_workflow_refresh(*last_refresh, now) {
            info!(
                "Skipping silent workflow refresh for {} due to TTL",
                repo_full_name
            );
            return;
        }
        *last_refresh = Some(now);
    }

    {
        let mut loading_guard = loading.lock();
        *loading_guard = true;
    }

    let run_filters = context.run_filters.clone();
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo_name = context.repo.clone();
    let list_store = context.store.clone();
    let parent_window = context.parent_window.clone();
    let loading_guard = loading.clone();
    let toast_overlay = context.toast_overlay.clone();
    let job_contexts = context.job_contexts.clone();
    let workflows_with_active_runs = context.workflows_with_active_runs.clone();
    let workflows_last_loaded = context.workflows_last_loaded.clone();
    let workflows_loading_runs = context.workflows_loading_runs.clone();
    let run_digests = context.run_digests.clone();
    let notification_manager = context.notification_manager.clone();
    let preferences_manager = context.preferences_manager.clone();
    let repo_model = context.repo_model.clone();

    let (sender, receiver) = MainContextChannelExt::channel::<WorkflowLoadMessage>(
        &glib::MainContext::default(),
        glib::Priority::default(),
    );

    let client_for_spawn = client.clone();
    let owner_for_spawn = owner.clone();
    let repo_name_for_spawn = repo_name.clone();

    let run_filters_for_ui = run_filters.clone();
    let workflows_last_silent_refresh_for_ui = workflows_last_silent_refresh.clone();
    receiver.attach(None, move |message| {
        let run_digests = run_digests.clone();
        let notification_manager_handle = notification_manager.clone();
        let preferences_manager_handle = preferences_manager.clone();
        let repo_model_for_ui = repo_model.clone();
        *loading_guard.lock() = false;

        let result = match message {
            WorkflowLoadMessage::Finished(result) => result,
            WorkflowLoadMessage::Dropped => {
                *workflows_last_silent_refresh_for_ui.lock() = None;
                Err(workflow_load_task_dropped_error("Silent workflow refresh"))
            }
        };

        let notification_manager_for_ui = notification_manager_handle.clone();
        let preferences_manager_for_ui = preferences_manager_handle.clone();
        let ui_context = WorkflowListContext {
            store: list_store.clone(),
            client: client.clone(),
            owner: owner.clone(),
            repo: repo_name.clone(),
            repo_model: repo_model_for_ui.clone(),
            parent_window: parent_window.clone(),
            toast_overlay: toast_overlay.clone(),
            job_contexts: job_contexts.clone(),
            run_badge_summaries: context.run_badge_summaries.clone(),
            workflows_with_active_runs: workflows_with_active_runs.clone(),
            workflows_last_loaded: workflows_last_loaded.clone(),
            workflows_loading_runs: workflows_loading_runs.clone(),
            run_digests: run_digests.clone(),
            notification_manager: notification_manager_for_ui.clone(),
            preferences_manager: preferences_manager_for_ui.clone(),
            run_filters: run_filters_for_ui.clone(),
            run_load_service: context.run_load_service.clone(),
            expand_first_workflow: Rc::new(Cell::new(false)),
            header: context.header.clone(),
        };

        match result {
            Ok(wf_list) => {
                context.header.note_refreshed();
                let current = workflows.lock().clone();
                if super::workflow_list::workflows_differ(current.as_ref(), wf_list.as_ref()) {
                    info!("Silent refresh detected workflow changes");
                    *workflows.lock() = wf_list.clone();
                    super::workflow_list::update_workflows_list(&ui_context, wf_list.as_ref());
                } else {
                    // The workflow set is unchanged, so no rows are rebuilt — but
                    // their status dots and meta lines still need the newest runs,
                    // otherwise a collapsed row stays stale until a manual refresh.
                    super::workflow_list::fetch_latest_runs_summary(&ui_context);
                }
            }
            Err(e) => {
                if !matches!(e, GitHubError::ApiError(ref msg) if msg.contains("Not modified")) {
                    warn!("Silent workflow refresh failed: {}", e);
                }
            }
        }

        glib::ControlFlow::Break
    });

    crate::runtime::handle().spawn(async move {
        let notifier = WorkflowLoadNotifier::new(sender);
        let client_clone = client_for_spawn.lock().clone();
        let result = fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn)
            .await
            .map(Arc::new);
        notifier.finish(result);
    });
}
mod pane;

#[cfg(test)]
mod tests {
    use super::{
        AutoRefreshTick, SILENT_WORKFLOW_REFRESH_TTL, WorkflowLoadMessage, auto_refresh_tick,
        should_listen_for_refresh_updates, should_reconfigure_auto_refresh,
        should_refresh_workflow_runs_in_background, should_skip_silent_workflow_refresh,
        workflow_load_result, workflow_load_task_dropped_error,
    };
    use crate::services::api::models::Workflow;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn silent_workflow_refresh_honors_ttl() {
        let now = Instant::now();
        assert!(!should_skip_silent_workflow_refresh(None, now));
        assert!(should_skip_silent_workflow_refresh(
            Some(now - SILENT_WORKFLOW_REFRESH_TTL + std::time::Duration::from_secs(1)),
            now
        ));
        assert!(!should_skip_silent_workflow_refresh(
            Some(now - SILENT_WORKFLOW_REFRESH_TTL - std::time::Duration::from_secs(1)),
            now
        ));
    }

    #[test]
    fn background_run_refresh_skips_collapsed_inactive_workflows() {
        assert!(should_refresh_workflow_runs_in_background(true, false));
        assert!(should_refresh_workflow_runs_in_background(false, true));
        assert!(!should_refresh_workflow_runs_in_background(false, false));
    }

    #[test]
    fn auto_refresh_reconfigure_skips_matching_live_timer() {
        assert!(!should_reconfigure_auto_refresh(Some(10), true, 10));
        assert!(should_reconfigure_auto_refresh(Some(30), true, 10));
        assert!(should_reconfigure_auto_refresh(Some(10), false, 10));
    }

    #[test]
    fn auto_refresh_tick_respects_inactive_panes() {
        assert_eq!(auto_refresh_tick(true, false), AutoRefreshTick::Stop);
        assert_eq!(auto_refresh_tick(false, false), AutoRefreshTick::Stop);
        assert_eq!(auto_refresh_tick(false, true), AutoRefreshTick::Skip);
        assert_eq!(auto_refresh_tick(true, true), AutoRefreshTick::Refresh);
    }

    #[test]
    fn refresh_updates_require_live_active_pane() {
        assert!(should_listen_for_refresh_updates(true, true, true));
        assert!(!should_listen_for_refresh_updates(false, true, true));
        assert!(!should_listen_for_refresh_updates(true, false, true));
        assert!(!should_listen_for_refresh_updates(true, true, false));
    }

    #[test]
    fn dropped_workflow_load_becomes_api_error() {
        let error = workflow_load_result(WorkflowLoadMessage::Dropped, "Workflow load")
            .expect_err("dropped task should fail");

        assert_eq!(
            error.to_string(),
            workflow_load_task_dropped_error("Workflow load").to_string()
        );
    }

    #[test]
    fn finished_workflow_load_returns_payload() {
        let workflows = Arc::new(vec![Workflow {
            id: 1,
            name: "CI".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
        }]);
        let result = workflow_load_result(
            WorkflowLoadMessage::Finished(Ok(workflows.clone())),
            "Workflow load",
        )
        .expect("finished task should succeed");

        assert_eq!(result.len(), 1);
    }
}

#[derive(Clone)]
struct CallbackRefs {
    refresh_button: gtk::Button,
    buttons_box: gtk::Box,
}

impl CallbackRefs {
    fn show_loading(&self, loading: bool) {
        let refresh_button = self.refresh_button.clone();
        let buttons_box = self.buttons_box.clone();

        glib::idle_add_local_once(move || {
            if loading {
                refresh_button.set_visible(false);

                let mut child = buttons_box.first_child();
                while let Some(widget) = child.as_ref() {
                    let next = widget.next_sibling();
                    if widget.widget_name().as_str() == "detail-spinner" {
                        buttons_box.remove(widget);
                    }
                    child = next;
                }

                let spinner = gtk::Spinner::new();
                spinner.start();
                spinner.set_tooltip_text(Some(tr("Loading workflows...").as_str()));
                spinner.set_widget_name("detail-spinner");
                spinner.set_size_request(24, 24);
                buttons_box.prepend(&spinner);
                spinner.set_visible(true);
            } else {
                let mut child = buttons_box.first_child();
                while let Some(widget) = child.as_ref() {
                    let next = widget.next_sibling();
                    if widget.widget_name().as_str() == "detail-spinner" {
                        buttons_box.remove(widget);
                    }
                    child = next;
                }

                refresh_button.set_visible(true);
            }
        });
    }
}

async fn fetch_workflows(
    client: &GitHubGateway,
    owner: &str,
    repo: &str,
) -> Result<Vec<Workflow>, GitHubError> {
    client.list_workflows(owner, repo).await
}

pub(super) fn parse_expander_widget_name(name: &str) -> Option<(i64, bool)> {
    scan::parse_expander_widget_name(name)
}

pub(super) fn run_list_for_expander(
    expander: &gtk::Expander,
) -> Option<super::helpers::WorkflowRunListModel> {
    scan::run_list_for_expander(expander)
}

pub(super) fn visit_workflow_expanders<F: FnMut(&gtk::Expander, i64, bool)>(
    widget: &gtk::Widget,
    f: &mut F,
) {
    scan::visit_expanders(widget, f);
}
