use super::helpers::{LoadRunsParams, current_job_context_run_ids, refresh_jobs_for_workflows};
use super::{RepoDetailPane, WorkflowListContext};
use crate::api::models::Workflow;
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::utils::channel::Sender as UiChannelSender;
use crate::ui::utils::widget_data::get_data_clone;
use crate::ui::utils::{MainContextChannelExt, try_remove_source};
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

    crate::runtime_handle().spawn(async move {
        let notifier = WorkflowLoadNotifier::new(sender);
        let client_clone = client_for_spawn.lock().clone();
        let result = fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn)
            .await
            .map(Arc::new);
        notifier.finish(result);
    });
}

impl RepoDetailPane {
    pub(super) fn load_workflows(&self) {
        {
            let mut loading_guard = self.loading.lock();
            if *loading_guard {
                info!("Already loading workflows, skipping duplicate request");
                return;
            }
            *loading_guard = true;
        }

        let context = self.workflow_list_context();
        let run_filters = context.run_filters.clone();
        let client = context.client.clone();
        let workflows = self.workflows.clone();
        let owner = context.owner.clone();
        let repo_name = context.repo.clone();
        let list_store = context.store.clone();
        let parent_window = context.parent_window.clone();
        let loading_guard = self.loading.clone();
        let toast_overlay = context.toast_overlay.clone();
        let job_contexts = context.job_contexts.clone();
        let workflows_with_active_runs = context.workflows_with_active_runs.clone();
        let workflows_last_loaded = context.workflows_last_loaded.clone();
        let workflows_loading_runs = context.workflows_loading_runs.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let repo_model = context.repo_model.clone();

        self.show_loading(true);
        let callback_refs = self.clone_for_callbacks();

        let (sender, receiver) = MainContextChannelExt::channel::<WorkflowLoadMessage>(
            &glib::MainContext::default(),
            glib::Priority::default(),
        );

        let client_for_spawn = client.clone();
        let owner_for_spawn = owner.clone();
        let repo_name_for_spawn = repo_name.clone();
        let run_filters_for_ui = run_filters.clone();
        receiver.attach(None, move |message| {
            callback_refs.show_loading(false);
            *loading_guard.lock() = false;

            let result = workflow_load_result(message, "Workflow load");

            let notification_manager_for_ui = notification_manager.clone();
            let preferences_manager_for_ui = preferences_manager.clone();
            let ui_context = WorkflowListContext {
                store: list_store.clone(),
                client: client.clone(),
                owner: owner.clone(),
                repo: repo_name.clone(),
                repo_model: repo_model.clone(),
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
                expand_first_workflow: context.expand_first_workflow.clone(),
                header: context.header.clone(),
            };

            match result {
                Ok(wf_list) => {
                    info!("Loaded {} workflows", wf_list.len());
                    *workflows.lock() = wf_list.clone();

                    super::workflow_list::update_workflows_list(&ui_context, wf_list.as_ref());
                }
                Err(e) => {
                    error!("Failed to load workflows: {}", e);
                    let message = tr("Failed to load workflows: {error}")
                        .replace("{error}", e.to_string().as_str());
                    let overlay = toast_overlay.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let toast = adw::Toast::new(&message);
                        toast.set_timeout(5);
                        overlay.add_toast(toast);
                    });
                }
            }

            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let notifier = WorkflowLoadNotifier::new(sender);
            let client_clone = client_for_spawn.lock().clone();
            let result = fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn)
                .await
                .map(Arc::new);
            notifier.finish(result);
        });
    }

    pub(super) fn connect_refresh_button(&self, button: &gtk::Button) {
        let client = self.client.clone();
        let workflows = self.workflows.clone();
        let context = self.workflow_list_context();
        let run_filters = context.run_filters.clone();
        let owner = context.owner.clone();
        let repo_name = context.repo.clone();
        let list_store = context.store.clone();
        let callback_refs = self.clone_for_callbacks();
        let parent_window = context.parent_window.clone();
        let loading_guard = self.loading.clone();
        let toast_overlay = context.toast_overlay.clone();
        let job_contexts = context.job_contexts.clone();
        let workflows_with_active_runs = context.workflows_with_active_runs.clone();
        let workflows_last_loaded = context.workflows_last_loaded.clone();
        let workflows_loading_runs = context.workflows_loading_runs.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let repo_model = context.repo_model.clone();
        let run_badge_summaries = context.run_badge_summaries.clone();
        let run_filters_for_button = run_filters.clone();

        button.connect_clicked(move |_| {
            {
                let mut guard = loading_guard.lock();
                if *guard {
                    info!("Already loading workflows, ignoring refresh click");
                    return;
                }
                *guard = true;
            }

            let client = client.clone();
            let workflows = workflows.clone();
            let owner = owner.clone();
            let repo_name = repo_name.clone();
            let repo_model = repo_model.clone();
            let store = list_store.clone();
            let callback_refs = callback_refs.clone();
            let parent_window = parent_window.clone();
            let loading_guard = loading_guard.clone();
            let toast_overlay = toast_overlay.clone();
            let workflows_with_active_runs = workflows_with_active_runs.clone();
            let workflows_last_loaded = workflows_last_loaded.clone();
            let workflows_loading_runs = workflows_loading_runs.clone();
            let run_digests = run_digests.clone();
            let notification_manager_handle = notification_manager.clone();
            let preferences_manager_handle = preferences_manager.clone();

            callback_refs.show_loading(true);

            let (sender, receiver) = MainContextChannelExt::channel::<WorkflowLoadMessage>(
                &glib::MainContext::default(),
                glib::Priority::default(),
            );
            let list_store_for_ui = store.clone();
            let run_filters_for_ui = run_filters_for_button.clone();
            let workflows_for_ui = workflows.clone();
            let client_for_ui = client.clone();
            let owner_for_ui = owner.clone();
            let repo_name_for_ui = repo_name.clone();
            let repo_model_for_ui = repo_model.clone();
            let callback_refs_for_ui = callback_refs.clone();
            let parent_window_for_ui = parent_window.clone();
            let toast_overlay_for_ui = toast_overlay.clone();
            let job_contexts_for_ui = job_contexts.clone();
            let workflows_with_active_runs_for_ui = workflows_with_active_runs.clone();
            let run_digests_for_ui = run_digests.clone();
            let notification_manager_for_ui = notification_manager_handle.clone();
            let preferences_manager_for_ui = preferences_manager_handle.clone();
            let run_load_service_for_ui = context.run_load_service.clone();
            let run_badge_summaries_for_ui = run_badge_summaries.clone();
            let header_for_ui = context.header.clone();

            receiver.attach(None, move |message| {
                callback_refs_for_ui.show_loading(false);
                *loading_guard.lock() = false;

                let result = workflow_load_result(message, "Workflow refresh");

                match result {
                    Ok(wf_list) => {
                        info!("Refreshed {} workflows", wf_list.len());
                        *workflows_for_ui.lock() = wf_list.clone();

                        let ui_context = WorkflowListContext {
                            store: list_store_for_ui.clone(),
                            client: client_for_ui.clone(),
                            owner: owner_for_ui.clone(),
                            repo: repo_name_for_ui.clone(),
                            repo_model: repo_model_for_ui.clone(),
                            parent_window: parent_window_for_ui.clone(),
                            toast_overlay: toast_overlay_for_ui.clone(),
                            job_contexts: job_contexts_for_ui.clone(),
                            run_badge_summaries: run_badge_summaries_for_ui.clone(),
                            workflows_with_active_runs: workflows_with_active_runs_for_ui.clone(),
                            workflows_last_loaded: workflows_last_loaded.clone(),
                            workflows_loading_runs: workflows_loading_runs.clone(),
                            run_digests: run_digests_for_ui.clone(),
                            notification_manager: notification_manager_for_ui.clone(),
                            preferences_manager: preferences_manager_for_ui.clone(),
                            run_filters: run_filters_for_ui.clone(),
                            run_load_service: run_load_service_for_ui.clone(),
                            expand_first_workflow: Rc::new(Cell::new(false)),
                            header: header_for_ui.clone(),
                        };

                        super::workflow_list::update_workflows_list(&ui_context, wf_list.as_ref());
                    }
                    Err(e) => {
                        error!("Failed to refresh workflows: {}", e);
                    }
                }

                glib::ControlFlow::Break
            });

            let client_for_spawn = client.clone();
            let owner_for_spawn = owner.clone();
            let repo_name_for_spawn = repo_name.clone();
            crate::runtime_handle().spawn(async move {
                let notifier = WorkflowLoadNotifier::new(sender);
                let client_clone = client_for_spawn.lock().clone();
                let result = fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn)
                    .await
                    .map(Arc::new);
                notifier.finish(result);
            });
        });
    }

    pub(super) fn start_auto_refresh(&self) {
        let auto_refresh_source = self.auto_refresh_source.clone();
        let auto_refresh_interval = self.auto_refresh_interval.clone();
        let refresh_active = self.refresh_active.clone();
        let list_context = self.workflow_list_context();
        let repo_full_name = self.repo.full_name.clone();
        let lifecycle_token = Rc::downgrade(&self.lifecycle_token);
        let silent_refresh = SilentWorkflowRefreshState {
            workflows: self.workflows.clone(),
            loading: self.loading.clone(),
            workflows_last_silent_refresh: self.workflows_last_silent_refresh.clone(),
        };

        if let Some(prefs_mgr) = &self.preferences_manager {
            let workflow_view_weak = self.workflow_view.downgrade();
            let (sender, receiver) =
                glib::MainContext::default().channel::<u64>(glib::Priority::default());

            receiver.attach(None, move |interval| {
                if !should_listen_for_refresh_updates(
                    workflow_view_weak.upgrade().is_some(),
                    lifecycle_token.upgrade().is_some(),
                    refresh_active.load(Ordering::Relaxed),
                ) {
                    cancel_auto_refresh_timer_slot(&auto_refresh_source);
                    *auto_refresh_interval.lock() = None;
                    return glib::ControlFlow::Break;
                }

                configure_auto_refresh_timer_slot(
                    &auto_refresh_source,
                    &auto_refresh_interval,
                    &refresh_active,
                    &silent_refresh,
                    &list_context,
                    &repo_full_name,
                    interval,
                );
                glib::ControlFlow::Continue
            });

            let prefs_mgr = prefs_mgr.clone();
            crate::runtime_handle().spawn(async move {
                let mut updates = prefs_mgr.subscribe();
                if sender.send(updates.borrow().refresh_interval).is_err() {
                    return;
                }

                while updates.changed().await.is_ok() {
                    if sender.send(updates.borrow().refresh_interval).is_err() {
                        break;
                    }
                }
            });
        } else {
            configure_auto_refresh_timer_slot(
                &auto_refresh_source,
                &auto_refresh_interval,
                &self.refresh_active,
                &silent_refresh,
                &list_context,
                &repo_full_name,
                DEFAULT_AUTO_REFRESH_INTERVAL_SECS,
            );
        }
    }

    fn cancel_auto_refresh_timer(&self) {
        cancel_auto_refresh_timer_slot(&self.auto_refresh_source);
    }

    pub(super) fn teardown_refresh_timers(&self) {
        self.cancel_auto_refresh_timer();
        *self.auto_refresh_interval.lock() = None;
        super::helpers::clear_follow_up_refresh_timers(&self.workflow_store);
    }

    fn refresh_runs_background(context: &WorkflowListContext) {
        let client = context.client.clone();
        let owner = context.owner.clone();
        let repo = context.repo.clone();
        let repo_model = context.repo_model.clone();
        let parent_window = context.parent_window.clone();
        let toast_overlay = context.toast_overlay.clone();
        let workflows_with_active = context.workflows_with_active_runs.clone();
        let workflows_loading = context.workflows_loading_runs.clone();
        let job_contexts = context.job_contexts.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let workflows_last_loaded = context.workflows_last_loaded.clone();
        let run_load_service = context.run_load_service.clone();
        let run_filters_arc = context.run_filters.clone();

        let previous_active = context.workflows_with_active_runs.lock().clone();
        let mut observed_active: HashSet<i64> = HashSet::new();

        for row in super::workflow_list::collect_workflow_rows(&context.store) {
            visit_expanders(&row, &mut |expander, workflow_id, is_active| {
                if is_active {
                    observed_active.insert(workflow_id);
                }

                if !should_refresh_workflow_runs_in_background(expander.is_expanded(), is_active) {
                    return;
                }

                if let Some(run_list) = run_list_for_expander(expander) {
                    let mut preserved_runs: Vec<i64> =
                        run_list.expanded_run_ids().into_iter().collect();
                    if preserved_runs.is_empty() {
                        preserved_runs = current_job_context_run_ids(&job_contexts, workflow_id);
                    }
                    let workflow_label = get_data_clone(expander, "actioneer-workflow-name")
                        .unwrap_or_else(|| {
                            format!("{}/{} • Workflow {}", owner, repo, workflow_id)
                        });

                    let notification_manager_clone = notification_manager.clone();
                    let preferences_manager_clone = preferences_manager.clone();
                    let repo_model_clone = repo_model.clone();

                    run_load_service.request(LoadRunsParams {
                        client: client.clone(),
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                        repo_model: repo_model_clone,
                        workflow_id,
                        workflow_name: workflow_label,
                        run_list,
                        parent_window: parent_window.clone(),
                        expander: expander.clone(),
                        toast_overlay: toast_overlay.clone(),
                        job_contexts: job_contexts.clone(),
                        expanded_run_ids: preserved_runs,
                        workflows_with_active: workflows_with_active.clone(),
                        workflows_last_loaded: workflows_last_loaded.clone(),
                        workflows_loading: workflows_loading.clone(),
                        background: true,
                        run_digests: run_digests.clone(),
                        notification_manager: notification_manager_clone,
                        preferences_manager: preferences_manager_clone,
                        run_filters: run_filters_arc.clone(),
                    });
                }
            });
        }

        let job_refresh_targets = union_active_workflows(&observed_active, &previous_active);

        refresh_jobs_for_workflows(&job_contexts, &job_refresh_targets);

        *workflows_with_active.lock() = observed_active;
    }

    fn clone_for_callbacks(&self) -> CallbackRefs {
        CallbackRefs {
            refresh_button: self.refresh_button.clone(),
            buttons_box: self.buttons_box.clone(),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::{
        AutoRefreshTick, SILENT_WORKFLOW_REFRESH_TTL, WorkflowLoadMessage, auto_refresh_tick,
        should_listen_for_refresh_updates, should_reconfigure_auto_refresh,
        should_refresh_workflow_runs_in_background, should_skip_silent_workflow_refresh,
        workflow_load_result, workflow_load_task_dropped_error,
    };
    use crate::api::models::Workflow;
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
    client: &GitHubClient,
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
