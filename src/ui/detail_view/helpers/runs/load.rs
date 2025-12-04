use super::super::context::JobContextMap;
use super::super::formatting::update_workflow_status_badge;
use super::digest::{RunDigestMap, RunDigestStore, collect_completed_notifications, digest_runs};
use super::filters::run_matches_filters;
use super::list::WorkflowRunListModel;
use crate::api::models::{Repo, WorkflowRun};
use crate::api::{GitHubClient, GitHubError};
use crate::cache::DataCache;
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

const MAX_VISIBLE_RUNS: usize = 10;

#[derive(Clone)]
struct RunErrorContext {
    run_list: WorkflowRunListModel,
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    parent_window: adw::ApplicationWindow,
    expander: gtk::Expander,
    cache: Arc<DataCache>,
    toast_overlay: adw::ToastOverlay,
    job_contexts: JobContextMap,
    workflows_with_active: Arc<Mutex<HashSet<i64>>>,
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
    pub status_badge: Option<gtk::Label>,
    pub expander: gtk::Expander,
    pub cache: Arc<DataCache>,
    pub toast_overlay: adw::ToastOverlay,
    pub bypass_cache: bool,
    pub job_contexts: JobContextMap,
    pub expanded_run_ids: Vec<i64>,
    pub workflows_with_active: Arc<Mutex<HashSet<i64>>>,
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
        status_badge,
        expander,
        cache,
        toast_overlay,
        bypass_cache,
        job_contexts,
        expanded_run_ids,
        workflows_with_active,
        background,
        run_digests,
        notification_manager,
        preferences_manager,
        run_filters,
    } = params;

    let expanded_run_ids: HashSet<i64> = expanded_run_ids.into_iter().collect();
    let expanded_run_ids = Rc::new(expanded_run_ids);

    if !background {
        run_list.show_loading();
    }

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Arc<Vec<WorkflowRun>>, GitHubError>>(glib::Priority::default());

    let client_for_spawn = client.clone();
    let owner_for_spawn = owner.clone();
    let repo_for_spawn = repo.clone();
    let cache_for_spawn = cache.clone();
    let parent_window_clone = parent_window.clone();
    let expander_for_retry = expander.clone();
    let task_run_list = run_list.clone();
    let job_contexts_for_retry = job_contexts.clone();
    let toast_overlay_for_retry = toast_overlay.clone();
    let run_digests_for_ui = run_digests.clone();
    let background_for_ui = background;

    receiver.attach(None, move |result| {
        let expanded_run_ids_for_ui = expanded_run_ids.clone();
        let expander_expanded = expander.is_expanded();

        match result {
            Ok(runs) if runs.is_empty() => {
                {
                    let mut digests = run_digests_for_ui.lock();
                    digests.insert(workflow_id, RunDigestMap::new());
                }

                if should_render_run_list(background_for_ui, expander_expanded, true) {
                    task_run_list.show_empty();
                }
            }
            Ok(runs) => {
                let digest = digest_runs(runs.as_ref());
                let (previous_digest, changed) = {
                    let mut digests = run_digests_for_ui.lock();
                    let previous = digests.insert(workflow_id, digest.clone());
                    let changed = match &previous {
                        Some(prev) => prev != &digest,
                        None => true,
                    };
                    (previous, changed)
                };

                if changed {
                    prune_stale_job_contexts(&job_contexts, workflow_id, runs.as_ref());
                    store_runs_async(
                        cache.clone(),
                        owner.clone(),
                        repo.clone(),
                        workflow_id,
                        runs.as_ref(),
                    );

                    if let (Some(prev), Some(manager)) =
                        (previous_digest.as_ref(), notification_manager.clone())
                    {
                        let notification_requests = collect_completed_notifications(prev, &runs);
                        if !notification_requests.is_empty() {
                            let window_is_active = parent_window_clone.is_active();
                            let workflow_label = workflow_name.clone();
                            let preferences_manager = preferences_manager.clone();
                            crate::runtime_handle().spawn(async move {
                                if window_is_active {
                                    debug!(
                                        workflow = workflow_label.as_str(),
                                        "Skipping notification because window is active"
                                    );
                                    return;
                                }

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
                }

                if let Some(ref badge) = status_badge
                    && let Some(latest_run) = runs.first()
                {
                    update_workflow_status_badge(badge, latest_run);
                    badge.set_visible(true);
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

                if should_render_run_list(background_for_ui, expander_expanded, changed) {
                    let filters_snapshot = run_filters.lock().clone();
                    let summary = summarize_visible_runs(runs.as_ref(), &filters_snapshot);

                    if summary.visible_runs.is_empty() {
                        task_run_list.show_filtered_placeholder();
                    } else {
                        task_run_list.show_runs(
                            summary.visible_runs.len(),
                            summary.filtered_total,
                            runs.len(),
                            &summary.visible_runs,
                            &expanded_run_ids_for_ui,
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
                        cache: cache.clone(),
                        toast_overlay: toast_overlay_for_retry.clone(),
                        job_contexts: job_contexts_for_retry.clone(),
                        workflows_with_active: workflows_with_active.clone(),
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

        glib::ControlFlow::Break
    });

    crate::runtime_handle().spawn(async move {
        let cache_key = format!("{}/{}", owner_for_spawn, repo_for_spawn);

        if !bypass_cache {
            if let Some(cached_runs) = cache_for_spawn.runs(&cache_key, workflow_id).await {
                if !cached_runs.is_empty() {
                    info!("Using cached runs for workflow {}", workflow_id);
                    let _ = sender.send(Ok(cached_runs));
                    return;
                }
                info!(
                    "Cache invalidated for workflow {}, fetching fresh data",
                    workflow_id
                );
            }
        } else {
            info!("Bypassing run cache for workflow {}", workflow_id);
        }

        let client_guard = client_for_spawn.lock().clone();
        if bypass_cache {
            client_guard.invalidate_runs_cache(&owner_for_spawn, &repo_for_spawn, workflow_id);
        }
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

fn store_runs_async(
    cache: Arc<DataCache>,
    owner: String,
    repo: String,
    workflow_id: i64,
    runs: &[WorkflowRun],
) {
    let cache_key = format!("{}/{}", owner, repo);
    let runs_cache = Arc::new(runs.to_vec());
    crate::runtime_handle().spawn(async move {
        cache.store_runs(runs_cache, &cache_key, workflow_id).await;
    });
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

    let widget_name = expander.widget_name();
    let base_name = widget_name.as_str().trim_end_matches("_ACTIVE");

    if has_active_runs {
        expander.set_widget_name(&format!("{}_ACTIVE", base_name));
        info!("Workflow {} has active runs", workflow_id);
    } else {
        expander.set_widget_name(base_name);
        info!("Workflow {} has no active runs", workflow_id);
    }

    has_active_runs
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
        cache,
        toast_overlay,
        job_contexts,
        workflows_with_active,
        run_digests,
        workflow_name,
        notification_manager,
        preferences_manager,
        repo_model,
        run_filters,
    } = context;

    let detail = format!("Error: {}", error);
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
            status_badge: None,
            expander: expander.clone(),
            cache: cache.clone(),
            toast_overlay: toast_overlay.clone(),
            bypass_cache: true,
            job_contexts: job_contexts.clone(),
            expanded_run_ids: Vec::new(),
            workflows_with_active: workflows_with_active.clone(),
            background: false,
            run_digests: run_digests.clone(),
            notification_manager: notification_manager.clone(),
            preferences_manager: preferences_manager.clone(),
            run_filters: run_filters.clone(),
        });
    });
}

fn summarize_visible_runs(runs: &[WorkflowRun], filters: &RunFilters) -> RunDisplaySummary {
    let mut filtered_total = 0;
    let mut visible_runs = Vec::new();

    for run in runs {
        if run_matches_filters(run, filters) {
            filtered_total += 1;
            if visible_runs.len() < MAX_VISIBLE_RUNS {
                visible_runs.push(run.clone());
            }
        }
    }

    RunDisplaySummary {
        filtered_total,
        visible_runs,
    }
}

struct RunDisplaySummary {
    filtered_total: usize,
    visible_runs: Vec<WorkflowRun>,
}

#[cfg(test)]
mod tests {
    use super::{should_render_run_list, summarize_visible_runs};
    use crate::api::models::WorkflowRun;
    use crate::ui::detail_view::RunFilters;

    fn run_with_status(id: i64, status: &str, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            name: Some(format!("run-{id}")),
            display_title: Some(format!("Run {id}")),
            head_branch: Some("main".into()),
            status: Some(status.into()),
            conclusion: conclusion.map(|c| c.into()),
            run_started_at: Some("2024-01-01T00:00:00Z".into()),
            event: Some("push".into()),
            created_at: Some("2024-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:10:00Z".into()),
            html_url: Some("https://example.com".into()),
        }
    }

    #[test]
    fn limits_visible_runs_to_ten() {
        let runs: Vec<_> = (0..15)
            .map(|i| run_with_status(i, "completed", Some("success")))
            .collect();
        let summary = summarize_visible_runs(&runs, &RunFilters::default());
        assert_eq!(summary.filtered_total, 15);
        assert_eq!(summary.visible_runs.len(), 10);
    }

    #[test]
    fn respects_filters_when_collecting_runs() {
        let runs = vec![
            run_with_status(1, "completed", Some("success")),
            run_with_status(2, "completed", Some("failure")),
            run_with_status(3, "in_progress", None),
        ];

        let filters = RunFilters {
            include_failed: false,
            ..RunFilters::default()
        };
        let summary = summarize_visible_runs(&runs, &filters);
        assert_eq!(summary.filtered_total, 2);
        assert_eq!(summary.visible_runs.len(), 2);
        assert_eq!(summary.visible_runs[0].id, 1);
        assert_eq!(summary.visible_runs[1].id, 3);
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
}
