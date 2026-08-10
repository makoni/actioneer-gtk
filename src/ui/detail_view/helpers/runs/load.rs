use super::super::context::{JobContextMap, current_job_context_run_ids};
use super::super::workflows::update_workflow_row_header;
use super::digest::{RunDigestMap, RunDigestStore, update_digest_and_collect_notifications};
use super::filters::summarize_visible_runs;
use super::list::WorkflowRunListModel;
use crate::api::models::{Repo, WorkflowRun};
use crate::api::{GitHubClient, GitHubError};
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::detail_view::helpers::jobs::refresh_jobs_for_workflows;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

#[derive(Clone)]
struct RunErrorContext {
    run_list: WorkflowRunListModel,
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    parent_window: adw::ApplicationWindow,
    expander: gtk::Expander,
    toast_overlay: adw::ToastOverlay,
    job_contexts: JobContextMap,
    workflows_with_active: Arc<Mutex<HashSet<i64>>>,
    workflows_last_loaded: Arc<Mutex<std::collections::HashMap<i64, Instant>>>,
    workflows_loading: Arc<Mutex<HashSet<i64>>>,
    run_digests: Arc<Mutex<RunDigestStore>>,
    workflow_name: String,
    notification_manager: Option<NotificationManager>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    repo_model: Repo,
    run_filters: Arc<Mutex<RunFilters>>,
}

pub(crate) struct LoadRunsParams {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub repo_model: Repo,
    pub workflow_id: i64,
    pub workflow_name: String,
    pub run_list: WorkflowRunListModel,
    pub parent_window: adw::ApplicationWindow,
    pub expander: gtk::Expander,
    pub toast_overlay: adw::ToastOverlay,
    pub job_contexts: JobContextMap,
    pub expanded_run_ids: Vec<i64>,
    pub workflows_with_active: Arc<Mutex<HashSet<i64>>>,
    pub workflows_last_loaded: Arc<Mutex<std::collections::HashMap<i64, Instant>>>,
    pub workflows_loading: Arc<Mutex<HashSet<i64>>>,
    pub background: bool,
    pub run_digests: Arc<Mutex<RunDigestStore>>,
    pub notification_manager: Option<NotificationManager>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub run_filters: Arc<Mutex<RunFilters>>,
}

pub(crate) fn load_workflow_runs(params: LoadRunsParams) {
    let LoadRunsParams {
        client,
        owner,
        repo,
        repo_model,
        workflow_id,
        workflow_name,
        run_list,
        parent_window,
        expander,
        toast_overlay,
        job_contexts,
        expanded_run_ids,
        workflows_with_active,
        workflows_last_loaded,
        workflows_loading,
        background,
        run_digests,
        notification_manager,
        preferences_manager,
        run_filters,
    } = params;

    let now = Instant::now();
    {
        let mut last_loaded = workflows_last_loaded.lock();
        if background
            && let Some(previous) = last_loaded.get(&workflow_id)
            && now.duration_since(*previous) < Duration::from_millis(1200)
        {
            debug!(workflow_id, "Background load skipped (debounced)");
            return;
        }
        last_loaded.insert(workflow_id, now);
    }

    {
        let mut in_flight = workflows_loading.lock();
        if !in_flight.insert(workflow_id) {
            debug!(
                workflow_id,
                background, "Run load already in flight; skipping duplicate request"
            );
            return;
        }
    }

    let expanded_run_ids: HashSet<i64> = expanded_run_ids.into_iter().collect();
    let expanded_run_ids = Rc::new(expanded_run_ids);

    if !background {
        run_list.show_loading();
        // in-flight tracking handled above
    }

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Arc<Vec<WorkflowRun>>, GitHubError>>(glib::Priority::default());

    let client_for_spawn = client.clone();
    let owner_for_spawn = owner.clone();
    let repo_for_spawn = repo.clone();
    let parent_window_clone = parent_window.clone();
    let expander_for_retry = expander.clone();
    let task_run_list = run_list.clone();
    let job_contexts_for_retry = job_contexts.clone();
    let toast_overlay_for_retry = toast_overlay.clone();
    let run_digests_for_ui = run_digests.clone();
    let background_for_ui = background;
    let workflows_last_loaded_for_ui = workflows_last_loaded.clone();
    let workflows_loading_for_ui = workflows_loading.clone();

    receiver.attach(None, move |result| {
        let requested_expanded_run_ids = expanded_run_ids.clone();
        let expander_expanded = expander.is_expanded();

        match result {
            Ok(runs) if runs.is_empty() => {
                task_run_list.set_runs(runs.clone());
                {
                    let mut digests = run_digests_for_ui.lock();
                    digests.insert(workflow_id, RunDigestMap::new());
                }

                if let Some(header) = task_run_list.row_header() {
                    update_workflow_row_header(&header, None, None);
                }
                if let Some(detail) = task_run_list.detail_header() {
                    detail.record_latest_run(workflow_id, None);
                }

                if should_render_run_list(background_for_ui, expander_expanded, true) {
                    task_run_list.show_empty();
                }
            }
            Ok(runs) => {
                task_run_list.set_runs(runs.clone());

                let (changed, notification_requests) = {
                    let mut digests = run_digests_for_ui.lock();
                    update_digest_and_collect_notifications(
                        &mut digests,
                        workflow_id,
                        runs.as_ref(),
                    )
                };

                if changed {
                    prune_stale_job_contexts(&job_contexts, workflow_id, runs.as_ref());
                    if let (Some(manager), Some(notification_requests)) =
                        (notification_manager.clone(), notification_requests)
                    {
                        let workflow_label = workflow_name.clone();
                        let preferences_manager = preferences_manager.clone();

                        crate::runtime_handle().spawn(async move {
                            let notifications_enabled = match preferences_manager {
                                Some(manager) => manager.get().await.enable_notifications,
                                None => true,
                            };

                            if !notifications_enabled {
                                debug!(
                                    workflow = workflow_label.as_str(),
                                    "Notifications disabled in preferences"
                                );
                                return;
                            }

                            let total = notification_requests.len();
                            debug!(
                                workflow = workflow_label.as_str(),
                                count = total,
                                "Sending workflow completion notification(s)"
                            );
                            for (run_title, status, conclusion) in notification_requests {
                                let notify_result = manager
                                    .notify_workflow_completed(
                                        &workflow_label,
                                        &run_title,
                                        &status,
                                        conclusion.as_deref(),
                                    )
                                    .await;

                                if let Err(err) = notify_result {
                                    warn!(
                                        "Failed to send workflow completion notification: {}",
                                        err
                                    );
                                }
                            }
                        });
                    }
                }

                if let Some(latest_run) = runs.first() {
                    let summary = task_run_list
                        .job_summaries()
                        .borrow()
                        .get(&latest_run.id)
                        .cloned();
                    if let Some(header) = task_run_list.row_header() {
                        update_workflow_row_header(&header, Some(latest_run), summary.as_ref());
                    }
                    if let Some(detail) = task_run_list.detail_header() {
                        detail.record_latest_run(workflow_id, Some(latest_run));
                    }
                }

                let has_active_runs =
                    update_expander_activity(&expander, workflow_id, runs.as_ref());

                {
                    let mut active = workflows_with_active.lock();
                    if has_active_runs {
                        active.insert(workflow_id);
                    } else {
                        active.remove(&workflow_id);
                    }
                }

                if changed && !has_active_runs {
                    let mut targets = HashSet::new();
                    targets.insert(workflow_id);
                    refresh_jobs_for_workflows(&job_contexts, &targets);
                }

                if should_render_run_list(background_for_ui, expander_expanded, changed) {
                    let filters_snapshot = run_filters.lock().clone();
                    let summary = summarize_visible_runs(runs.as_ref(), &filters_snapshot);
                    let current_expanded_run_ids = task_run_list.expanded_run_ids();
                    let current_job_context_ids =
                        current_job_context_run_ids(&job_contexts, workflow_id);
                    // For background refreshes the live UI state is authoritative —
                    // the list was never cleared so current_expanded + job_contexts
                    // are accurate. Using the stale dispatch-time snapshot would
                    // re-expand rows the user collapsed between dispatch and response.
                    let requested = if background_for_ui {
                        &HashSet::new()
                    } else {
                        requested_expanded_run_ids.as_ref()
                    };
                    let preserved_expanded_run_ids = resolve_preserved_expanded_run_ids(
                        requested,
                        &current_expanded_run_ids,
                        &current_job_context_ids,
                        runs.as_ref(),
                    );

                    if summary.visible_runs.is_empty() {
                        task_run_list.show_filtered_placeholder();
                    } else {
                        task_run_list.show_runs(
                            summary.visible_runs.len(),
                            summary.filtered_total,
                            runs.len(),
                            &summary.visible_runs,
                            &preserved_expanded_run_ids,
                        );
                    }
                }
            }
            Err(error) => {
                if background_for_ui {
                    error!(
                        "Background run refresh failed for workflow {}: {}",
                        workflow_id, error
                    );
                } else {
                    let error_context = RunErrorContext {
                        run_list: task_run_list.clone(),
                        client: client.clone(),
                        owner: owner.clone(),
                        repo: repo.clone(),
                        parent_window: parent_window_clone.clone(),
                        expander: expander_for_retry.clone(),
                        toast_overlay: toast_overlay_for_retry.clone(),
                        job_contexts: job_contexts_for_retry.clone(),
                        workflows_with_active: workflows_with_active.clone(),
                        workflows_last_loaded: workflows_last_loaded_for_ui.clone(),
                        workflows_loading: workflows_loading_for_ui.clone(),
                        run_digests: run_digests_for_ui.clone(),
                        workflow_name: workflow_name.clone(),
                        notification_manager: notification_manager.clone(),
                        preferences_manager: preferences_manager.clone(),
                        repo_model: repo_model.clone(),
                        run_filters: run_filters.clone(),
                    };

                    show_error_state(error, workflow_id, error_context);
                }
            }
        }

        {
            let mut in_flight = workflows_loading_for_ui.lock();
            in_flight.remove(&workflow_id);
        }

        glib::ControlFlow::Break
    });

    crate::runtime_handle().spawn(async move {
        let client_guard = client_for_spawn.lock().clone();
        let result = client_guard
            .list_runs(&owner_for_spawn, &repo_for_spawn, workflow_id)
            .await
            .map(Arc::new);

        let _ = sender.send(result);
    });
}

fn prune_stale_job_contexts(job_contexts: &JobContextMap, workflow_id: i64, runs: &[WorkflowRun]) {
    let active_run_ids: HashSet<i64> = runs.iter().map(|run| run.id).collect();
    let mut contexts = job_contexts.borrow_mut();
    contexts.retain(|_, ctx| {
        if ctx.workflow_id() != workflow_id {
            return true;
        }
        active_run_ids.contains(&ctx.run_id())
    });
}

fn resolve_preserved_expanded_run_ids(
    requested_expanded_run_ids: &HashSet<i64>,
    current_expanded_run_ids: &HashSet<i64>,
    current_job_context_ids: &[i64],
    runs: &[WorkflowRun],
) -> HashSet<i64> {
    let valid_run_ids: HashSet<i64> = runs.iter().map(|run| run.id).collect();

    requested_expanded_run_ids
        .iter()
        .copied()
        .chain(current_expanded_run_ids.iter().copied())
        .chain(current_job_context_ids.iter().copied())
        .filter(|run_id| valid_run_ids.contains(run_id))
        .collect()
}

fn should_render_run_list(
    background_refresh: bool,
    expander_expanded: bool,
    digest_changed: bool,
) -> bool {
    if !background_refresh {
        return true;
    }

    if digest_changed {
        return true;
    }

    expander_expanded
}

fn update_expander_activity(
    expander: &gtk::Expander,
    workflow_id: i64,
    runs: &[WorkflowRun],
) -> bool {
    let has_active_runs = runs.iter().any(|run| {
        matches!(
            run.status.as_deref(),
            Some("in_progress") | Some("queued") | Some("waiting")
        )
    });

    set_expander_active(expander, has_active_runs);
    info!(
        workflow_id,
        has_active_runs, "Updated workflow activity state"
    );

    has_active_runs
}

/// Toggles the `_ACTIVE` suffix used by the background refresh scanner.
pub(crate) fn set_expander_active(expander: &gtk::Expander, active: bool) {
    let widget_name = expander.widget_name();
    let base_name = widget_name.as_str().trim_end_matches("_ACTIVE");

    if active {
        expander.set_widget_name(&format!("{}_ACTIVE", base_name));
    } else {
        expander.set_widget_name(base_name);
    }
}

fn show_error_state(error: GitHubError, workflow_id: i64, context: RunErrorContext) {
    error!("Failed to load runs: {}", error);

    let RunErrorContext {
        run_list,
        client,
        owner,
        repo,
        parent_window,
        expander,
        toast_overlay,
        job_contexts,
        workflows_with_active,
        workflows_last_loaded,
        workflows_loading,
        run_digests,
        workflow_name,
        notification_manager,
        preferences_manager,
        repo_model,
        run_filters,
    } = context;

    let detail =
        crate::i18n::tr("Error: {message}").replace("{message}", error.to_string().as_str());
    let retry_run_list = run_list.clone();
    run_list.show_error(detail, move || {
        load_workflow_runs(LoadRunsParams {
            client: client.clone(),
            owner: owner.clone(),
            repo: repo.clone(),
            repo_model: repo_model.clone(),
            workflow_id,
            workflow_name: workflow_name.clone(),
            run_list: retry_run_list.clone(),
            parent_window: parent_window.clone(),
            expander: expander.clone(),
            toast_overlay: toast_overlay.clone(),
            job_contexts: job_contexts.clone(),
            expanded_run_ids: Vec::new(),
            workflows_with_active: workflows_with_active.clone(),
            workflows_last_loaded: workflows_last_loaded.clone(),
            workflows_loading: workflows_loading.clone(),
            background: false,
            run_digests: run_digests.clone(),
            notification_manager: notification_manager.clone(),
            preferences_manager: preferences_manager.clone(),
            run_filters: run_filters.clone(),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::{resolve_preserved_expanded_run_ids, should_render_run_list};
    use crate::api::models::WorkflowRun;
    use std::collections::HashSet;

    fn run_stub(id: i64) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            workflow_id: Some(1),
            name: Some(format!("Run {id}")),
            display_title: None,
            head_branch: Some("main".into()),
            status: Some("in_progress".into()),
            conclusion: None,
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            actor: None,

            head_commit: None,

            triggering_actor: None,

            html_url: None,
        }
    }

    #[test]
    fn background_refresh_updates_collapsed_rows_when_data_changes() {
        assert!(should_render_run_list(true, false, true));
    }

    #[test]
    fn background_refresh_skips_collapsed_rows_without_changes() {
        assert!(!should_render_run_list(true, false, false));
    }

    #[test]
    fn background_refresh_updates_expanded_rows_even_without_changes() {
        assert!(should_render_run_list(true, true, false));
    }

    #[test]
    fn foreground_refresh_always_updates() {
        assert!(should_render_run_list(false, false, false));
    }

    #[test]
    fn preserved_expanded_runs_use_current_ui_state_and_job_contexts() {
        let requested = HashSet::from([10]);
        let current = HashSet::from([20]);
        let contexts = vec![30];
        let runs = vec![run_stub(20), run_stub(30), run_stub(40)];

        let preserved = resolve_preserved_expanded_run_ids(&requested, &current, &contexts, &runs);

        assert_eq!(preserved, HashSet::from([20, 30]));
    }

    #[test]
    fn preserved_expanded_runs_keep_requested_ids_when_still_valid() {
        let requested = HashSet::from([10]);
        let current = HashSet::new();
        let contexts = Vec::new();
        let runs = vec![run_stub(10), run_stub(20)];

        let preserved = resolve_preserved_expanded_run_ids(&requested, &current, &contexts, &runs);

        assert_eq!(preserved, HashSet::from([10]));
    }
}
