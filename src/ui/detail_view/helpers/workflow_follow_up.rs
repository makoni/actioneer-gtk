use super::context::JobContextMap;
use super::run_loader::RunLoadService;
use super::runs::{LoadRunsParams, RunDigestStore, WorkflowRunListModel};
use crate::api::GitHubClient;
use crate::api::models::Repo;
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::utils::widget_data::{set_data, steal_data};
use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

const FOLLOW_UP_SOURCE_KEY: &str = "actioneer-follow-up-refresh";

#[derive(Clone)]
pub(super) struct FollowUpRefreshParams {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) repo_model: Repo,
    pub(super) workflow_id: i64,
    pub(super) workflow_name: String,
    pub(super) run_list: WorkflowRunListModel,
    pub(super) parent_window: adw::ApplicationWindow,
    pub(super) status_badge: Option<gtk::Label>,
    pub(super) expander: glib::WeakRef<gtk::Expander>,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) job_contexts: JobContextMap,
    pub(super) workflows_with_active: Arc<Mutex<HashSet<i64>>>,
    pub(super) workflows_last_loaded: Arc<Mutex<HashMap<i64, Instant>>>,
    pub(super) workflows_loading: Arc<Mutex<HashSet<i64>>>,
    pub(super) run_digests: Arc<Mutex<RunDigestStore>>,
    pub(super) notification_manager: Option<NotificationManager>,
    pub(super) preferences_manager: Option<Arc<PreferencesManager>>,
    pub(super) run_filters: Arc<Mutex<RunFilters>>,
    pub(super) run_load_service: RunLoadService,
}

pub(super) fn schedule_follow_up_refresh(params: FollowUpRefreshParams) {
    let params_rc = Rc::new(params);
    let Some(expander) = params_rc.expander.upgrade() else {
        return;
    };

    if let Some(existing) = steal_data::<glib::SourceId, _>(&expander, FOLLOW_UP_SOURCE_KEY) {
        existing.remove();
    }

    let prefs_mgr = params_rc.preferences_manager.clone();
    let params_for_async = params_rc.clone();

    glib::MainContext::default().spawn_local(async move {
        let interval_secs = match prefs_mgr {
            Some(manager) => manager.get().await.refresh_interval,
            None => crate::preferences::Preferences::default().refresh_interval,
        };

        if interval_secs == 0 {
            info!("Auto-refresh disabled; skipping follow-up refresh timer");
            return;
        }

        let interval_secs = interval_secs.max(1);
        let params_for_timer = params_for_async.clone();
        let expander_weak = params_for_async.expander.clone();
        let source_id = glib::timeout_add_seconds_local(interval_secs as u32, move || {
            let Some(expander) = expander_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let preserved_runs: Vec<i64> = params_for_timer
                .run_list
                .expanded_run_ids()
                .into_iter()
                .collect();

            params_for_timer.run_load_service.request(LoadRunsParams {
                client: params_for_timer.client.clone(),
                owner: params_for_timer.owner.clone(),
                repo: params_for_timer.repo.clone(),
                repo_model: params_for_timer.repo_model.clone(),
                workflow_id: params_for_timer.workflow_id,
                workflow_name: params_for_timer.workflow_name.clone(),
                run_list: params_for_timer.run_list.clone(),
                parent_window: params_for_timer.parent_window.clone(),
                status_badge: params_for_timer.status_badge.clone(),
                expander,
                toast_overlay: params_for_timer.toast_overlay.clone(),
                job_contexts: params_for_timer.job_contexts.clone(),
                expanded_run_ids: preserved_runs,
                workflows_with_active: params_for_timer.workflows_with_active.clone(),
                workflows_last_loaded: params_for_timer.workflows_last_loaded.clone(),
                workflows_loading: params_for_timer.workflows_loading.clone(),
                background: true,
                run_digests: params_for_timer.run_digests.clone(),
                notification_manager: params_for_timer.notification_manager.clone(),
                preferences_manager: params_for_timer.preferences_manager.clone(),
                run_filters: params_for_timer.run_filters.clone(),
            });

            glib::ControlFlow::Continue
        });

        if let Some(expander_for_handle) = params_for_async.expander.upgrade() {
            set_data(&expander_for_handle, FOLLOW_UP_SOURCE_KEY, source_id);
        }
    });
}

pub(crate) fn clear_follow_up_refresh_timers(store: &gio::ListStore) {
    for row in super::super::workflow_list::collect_workflow_rows(store) {
        clear_follow_up_from_widget(&row);
    }
}

fn clear_follow_up_from_widget(widget: &gtk::Widget) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && let Some(existing) = steal_data::<glib::SourceId, _>(expander, FOLLOW_UP_SOURCE_KEY)
    {
        existing.remove();
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        clear_follow_up_from_widget(&current);
        child = current.next_sibling();
    }
}
