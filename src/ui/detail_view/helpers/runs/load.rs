use super::super::context::JobContextMap;
use super::super::formatting::update_workflow_status_badge;
use super::row::{RunRowContext, create_run_expander_row};
use crate::api::models::{Repo, WorkflowRun};
use crate::api::{GitHubClient, GitHubError};
use crate::cache::DataCache;
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct RunDigest {
    pub(crate) id: i64,
    pub(crate) status: Option<String>,
    pub(crate) conclusion: Option<String>,
    pub(crate) updated_at: Option<String>,
}

pub(crate) type RunDigestMap = HashMap<i64, RunDigest>;
pub(crate) type RunDigestStore = HashMap<i64, RunDigestMap>;

#[derive(Clone)]
struct RunErrorContext {
    runs_box: gtk::Box,
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
}

pub(crate) struct LoadRunsParams {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub repo_model: Repo,
    pub workflow_id: i64,
    pub workflow_name: String,
    pub runs_box: gtk::Box,
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
}

pub(crate) fn load_workflow_runs(params: LoadRunsParams) {
    let LoadRunsParams {
        client,
        owner,
        repo,
        repo_model,
        workflow_id,
        workflow_name,
        runs_box,
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
    } = params;

    let expanded_run_ids: HashSet<i64> = expanded_run_ids.into_iter().collect();
    let expanded_run_ids = std::rc::Rc::new(expanded_run_ids);

    if !background {
        clear_runs_box(&runs_box);
        append_spinner(&runs_box);
    }

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Vec<WorkflowRun>, GitHubError>>(glib::Priority::default());

    let client_for_spawn = client.clone();
    let owner_for_spawn = owner.clone();
    let repo_for_spawn = repo.clone();
    let parent_window_clone = parent_window.clone();
    let expander_for_retry = expander.clone();
    let cache_for_spawn = cache.clone();
    let job_contexts_for_retry = job_contexts.clone();
    let toast_overlay_for_retry = toast_overlay.clone();
    let run_digests_for_ui = run_digests.clone();
    let background_for_ui = background;

    receiver.attach(None, move |result| {
        let expanded_run_ids_for_ui = expanded_run_ids.clone();
        let mut should_rebuild_ui = true;
        if background_for_ui && !expander.is_expanded() {
            should_rebuild_ui = false;
        }

        if !background_for_ui {
            clear_runs_box(&runs_box);
        }

        match result {
            Ok(runs) if runs.is_empty() => {
                {
                    let mut digests = run_digests_for_ui.lock();
                    digests.insert(workflow_id, RunDigestMap::new());
                }

                if should_rebuild_ui {
                    if background_for_ui {
                        clear_runs_box(&runs_box);
                    }
                    append_empty_runs_state(&runs_box);
                }
            }
            Ok(runs) => {
                let digest = digest_runs(&runs);
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
                    prune_stale_job_contexts(&job_contexts, workflow_id, &runs);
                    store_runs_async(
                        cache.clone(),
                        owner.clone(),
                        repo.clone(),
                        workflow_id,
                        &runs,
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

                let has_active_runs = update_expander_activity(&expander, workflow_id, &runs);

                {
                    let mut active = workflows_with_active.lock();
                    if has_active_runs {
                        active.insert(workflow_id);
                    } else {
                        active.remove(&workflow_id);
                    }
                }
                if !background_for_ui || (should_rebuild_ui && changed) {
                    if background_for_ui {
                        clear_runs_box(&runs_box);
                    }

                    append_runs_header(&runs_box, runs.len());

                    for run in runs.iter().take(10) {
                        let expand_jobs = expanded_run_ids_for_ui.contains(&run.id);
                        let row_context = RunRowContext {
                            client: client.clone(),
                            owner: owner.clone(),
                            repo: repo.clone(),
                            repo_model: repo_model.clone(),
                            parent_window: parent_window_clone.clone(),
                            cache: cache.clone(),
                            workflow_id,
                            toast_overlay: toast_overlay.clone(),
                            job_contexts: job_contexts.clone(),
                        };
                        let row = create_run_expander_row(run, &row_context, expand_jobs);
                        runs_box.append(&row);
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
                        runs_box: runs_box.clone(),
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
                    };

                    append_error_state(error, workflow_id, error_context);
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
        let result = client_guard
            .list_runs(&owner_for_spawn, &repo_for_spawn, workflow_id)
            .await;

        if let Ok(ref runs) = result {
            cache_for_spawn
                .store_runs(runs.clone(), &cache_key, workflow_id)
                .await;
        }

        let _ = sender.send(result);
    });
}

fn clear_runs_box(runs_box: &gtk::Box) {
    loop {
        let child_opt = runs_box.first_child();
        let Some(child) = child_opt else {
            break;
        };
        runs_box.remove(&child);
    }
}

fn append_spinner(runs_box: &gtk::Box) {
    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(8);
    spinner.set_margin_bottom(8);
    runs_box.append(&spinner);
}

fn append_empty_runs_state(runs_box: &gtk::Box) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.set_halign(gtk::Align::Start);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);

    let label = gtk::Label::new(Some("No recent runs"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    vbox.append(&label);

    let info_label = gtk::Label::new(Some("Triggered runs may take 10-30 seconds to appear"));
    info_label.add_css_class("dim-label");
    info_label.add_css_class("caption");
    info_label.set_halign(gtk::Align::Start);
    vbox.append(&info_label);

    runs_box.append(&vbox);
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
    let runs_cache = runs.to_vec();
    crate::runtime_handle().spawn(async move {
        cache.store_runs(runs_cache, &cache_key, workflow_id).await;
    });
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

fn digest_runs(runs: &[WorkflowRun]) -> RunDigestMap {
    runs.iter()
        .map(|run| {
            (
                run.id,
                RunDigest {
                    id: run.id,
                    status: run.status.clone(),
                    conclusion: run.conclusion.clone(),
                    updated_at: run.updated_at.clone(),
                },
            )
        })
        .collect()
}

fn collect_completed_notifications(
    previous: &RunDigestMap,
    runs: &[WorkflowRun],
) -> Vec<(String, String, Option<String>)> {
    runs.iter()
        .filter(|run| is_completed_status(run.status.as_ref()))
        .filter_map(|run| {
            let prior = previous.get(&run.id)?;

            let prev_completed = is_completed_status(prior.status.as_ref());
            let conclusion_changed = prior.conclusion != run.conclusion;

            if !prev_completed || conclusion_changed {
                let title = build_run_notification_title(run);
                let status = run
                    .status
                    .clone()
                    .unwrap_or_else(|| "completed".to_string());
                let conclusion = run.conclusion.clone();
                Some((title, status, conclusion))
            } else {
                None
            }
        })
        .collect()
}

fn is_completed_status(status: Option<&String>) -> bool {
    status
        .map(|value| value.eq_ignore_ascii_case("completed"))
        .unwrap_or(false)
}

fn build_run_notification_title(run: &WorkflowRun) -> String {
    let base = run
        .display_title
        .clone()
        .or(run.name.clone())
        .unwrap_or_else(|| {
            run.run_number
                .map(|num| format!("Run #{}", num))
                .unwrap_or_else(|| format!("Run {}", run.id))
        });

    if let Some(branch) = run.head_branch.as_ref().filter(|b| !b.is_empty()) {
        format!("{} ({})", base, branch)
    } else {
        base
    }
}

fn append_runs_header(runs_box: &gtk::Box, run_count: usize) {
    let count_label = gtk::Label::new(Some(&format!("Recent runs ({})", run_count)));
    count_label.add_css_class("dim-label");
    count_label.add_css_class("caption");
    count_label.set_halign(gtk::Align::Start);
    count_label.set_margin_bottom(8);
    runs_box.append(&count_label);
}

fn append_error_state(error: GitHubError, workflow_id: i64, context: RunErrorContext) {
    let RunErrorContext {
        runs_box,
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
    } = context;

    error!("Failed to load runs: {}", error);

    let error_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    error_box.set_halign(gtk::Align::Start);
    error_box.set_margin_top(8);
    error_box.set_margin_bottom(8);

    let error_label = gtk::Label::new(Some("Unable to load workflow runs"));
    error_label.add_css_class("dim-label");
    error_label.set_halign(gtk::Align::Start);
    error_box.append(&error_label);

    let detail_label = gtk::Label::new(Some(&format!("Error: {}", error)));
    detail_label.add_css_class("caption");
    detail_label.add_css_class("dim-label");
    detail_label.set_halign(gtk::Align::Start);
    error_box.append(&detail_label);

    let retry_button = gtk::Button::with_label("Retry");
    retry_button.add_css_class("suggested-action");
    retry_button.set_halign(gtk::Align::Start);
    retry_button.set_margin_top(8);

    let runs_box_retry = runs_box.clone();
    let client_retry = client.clone();
    let owner_retry = owner.clone();
    let repo_retry = repo.clone();
    let parent_window_retry = parent_window.clone();
    let expander_retry = expander.clone();
    let cache_retry = cache.clone();
    let toast_overlay_retry = toast_overlay.clone();
    let job_contexts_retry = job_contexts.clone();
    let workflows_with_active_retry = workflows_with_active.clone();
    let run_digests_retry = run_digests.clone();
    let workflow_name_retry = workflow_name.clone();
    let notification_manager_retry = notification_manager.clone();
    let preferences_manager_retry = preferences_manager.clone();
    let repo_model_retry = repo_model.clone();

    retry_button.connect_clicked(move |_| {
        clear_runs_box(&runs_box_retry);
        load_workflow_runs(LoadRunsParams {
            client: client_retry.clone(),
            owner: owner_retry.clone(),
            repo: repo_retry.clone(),
            repo_model: repo_model_retry.clone(),
            workflow_id,
            workflow_name: workflow_name_retry.clone(),
            runs_box: runs_box_retry.clone(),
            parent_window: parent_window_retry.clone(),
            status_badge: None,
            expander: expander_retry.clone(),
            cache: cache_retry.clone(),
            toast_overlay: toast_overlay_retry.clone(),
            bypass_cache: true,
            job_contexts: job_contexts_retry.clone(),
            expanded_run_ids: Vec::new(),
            workflows_with_active: workflows_with_active_retry.clone(),
            background: false,
            run_digests: run_digests_retry.clone(),
            notification_manager: notification_manager_retry.clone(),
            preferences_manager: preferences_manager_retry.clone(),
        });
    });

    error_box.append(&retry_button);
    runs_box.append(&error_box);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn append_empty_runs_creates_notice() {
        let Some(_guard) = gtk_test_guard("append_empty_runs_creates_notice") else {
            return;
        };

        let runs_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        append_empty_runs_state(&runs_box);

        assert!(runs_box.first_child().is_some());
    }

    fn build_run(id: i64, status: &str, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            name: Some(format!("Run {}", id)),
            display_title: Some(format!("Run {}", id)),
            head_branch: Some("main".to_string()),
            status: Some(status.to_string()),
            conclusion: conclusion.map(|c| c.to_string()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            html_url: None,
        }
    }

    #[test]
    fn digest_runs_treats_reordering_as_unchanged() {
        let run_a = build_run(1, "completed", Some("success"));
        let run_b = build_run(2, "in_progress", None);

        let digest_first = digest_runs(&[run_a.clone(), run_b.clone()]);
        let digest_second = digest_runs(&[run_b, run_a]);

        assert_eq!(digest_first, digest_second);
    }
}
