use super::helpers::{
    LoadRunsParams, current_job_context_run_ids, load_workflow_runs, refresh_jobs_for_workflows,
};
use super::{RepoDetailPane, WorkflowListContext};
use crate::api::models::Workflow;
use crate::api::{GitHubClient, GitHubError};
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use std::collections::HashSet;
use tracing::{error, info, warn};

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
        let list_box = context.list_box.clone();
        let parent_window = context.parent_window.clone();
        let loading_guard = self.loading.clone();
        let cache = context.cache.clone();
        let toast_overlay = context.toast_overlay.clone();
        let job_contexts = context.job_contexts.clone();
        let workflows_with_active_runs = context.workflows_with_active_runs.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let repo_model = context.repo_model.clone();

        self.show_loading(true);
        let callback_refs = self.clone_for_callbacks();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Vec<Workflow>, GitHubError>>(glib::Priority::default());

        let client_for_spawn = client.clone();
        let owner_for_spawn = owner.clone();
        let repo_name_for_spawn = repo_name.clone();
        let cache_for_spawn = cache.clone();
        let cache_for_ui = cache.clone();

        let run_filters_for_ui = run_filters.clone();
        receiver.attach(None, move |result| {
            callback_refs.show_loading(false);
            *loading_guard.lock() = false;

            let notification_manager_for_ui = notification_manager.clone();
            let preferences_manager_for_ui = preferences_manager.clone();
            let ui_context = WorkflowListContext {
                list_box: list_box.clone(),
                client: client.clone(),
                owner: owner.clone(),
                repo: repo_name.clone(),
                repo_model: repo_model.clone(),
                parent_window: parent_window.clone(),
                cache: cache_for_ui.clone(),
                toast_overlay: toast_overlay.clone(),
                job_contexts: job_contexts.clone(),
                workflows_with_active_runs: workflows_with_active_runs.clone(),
                run_digests: run_digests.clone(),
                notification_manager: notification_manager_for_ui.clone(),
                preferences_manager: preferences_manager_for_ui.clone(),
                run_filters: run_filters_for_ui.clone(),
            };

            match result {
                Ok(wf_list) => {
                    info!("Loaded {} workflows", wf_list.len());
                    *workflows.lock() = wf_list.clone();

                    let cache_store = cache.clone();
                    let cache_key = format!("{}/{}", owner, repo_name);
                    let wf_list_cache = wf_list.clone();
                    crate::runtime_handle().spawn(async move {
                        cache_store.store_workflows(wf_list_cache, &cache_key).await;
                    });

                    super::workflow_list::update_workflows_list(&ui_context, &wf_list);
                }
                Err(e) => {
                    error!("Failed to load workflows: {}", e);
                    *workflows.lock() = Vec::new();
                    super::workflow_list::update_workflows_list(&ui_context, &[]);
                    let message = format!("Failed to load workflows: {}", e);
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
            let cache_key = format!("{}/{}", owner_for_spawn, repo_name_for_spawn);

            if let Some(cached_workflows) = cache_for_spawn.workflows(&cache_key).await {
                info!("Using cached workflows for {}", cache_key);
                let _ = sender.send(Ok(cached_workflows));
                return;
            }

            let client_clone = client_for_spawn.lock().clone();
            let result =
                fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn).await;
            let _ = sender.send(result);
        });
    }

    pub fn refresh_workflows_silent(&self) {
        {
            let mut loading_guard = self.loading.lock();
            if *loading_guard {
                info!("Already loading workflows, skipping silent refresh");
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
        let list_box = context.list_box.clone();
        let parent_window = context.parent_window.clone();
        let loading_guard = self.loading.clone();
        let cache = context.cache.clone();
        let toast_overlay = context.toast_overlay.clone();
        let job_contexts = context.job_contexts.clone();
        let workflows_with_active_runs = context.workflows_with_active_runs.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let repo_model = context.repo_model.clone();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Vec<Workflow>, GitHubError>>(glib::Priority::default());

        let client_for_spawn = client.clone();
        let owner_for_spawn = owner.clone();
        let repo_name_for_spawn = repo_name.clone();

        let run_filters_for_ui = run_filters.clone();
        receiver.attach(None, move |result| {
            let run_digests = run_digests.clone();
            let notification_manager_handle = notification_manager.clone();
            let preferences_manager_handle = preferences_manager.clone();
            let repo_model_for_ui = repo_model.clone();
            *loading_guard.lock() = false;

            let notification_manager_for_ui = notification_manager_handle.clone();
            let preferences_manager_for_ui = preferences_manager_handle.clone();
            let ui_context = WorkflowListContext {
                list_box: list_box.clone(),
                client: client.clone(),
                owner: owner.clone(),
                repo: repo_name.clone(),
                repo_model: repo_model_for_ui.clone(),
                parent_window: parent_window.clone(),
                cache: cache.clone(),
                toast_overlay: toast_overlay.clone(),
                job_contexts: job_contexts.clone(),
                workflows_with_active_runs: workflows_with_active_runs.clone(),
                run_digests: run_digests.clone(),
                notification_manager: notification_manager_for_ui.clone(),
                preferences_manager: preferences_manager_for_ui.clone(),
                run_filters: run_filters_for_ui.clone(),
            };

            match result {
                Ok(wf_list) => {
                    let current = workflows.lock().clone();
                    if super::workflow_list::workflows_differ(&current, &wf_list) {
                        info!("Silent refresh detected workflow changes");
                        *workflows.lock() = wf_list.clone();
                        super::workflow_list::update_workflows_list(&ui_context, &wf_list);
                    }
                }
                Err(e) => {
                    if !matches!(e, GitHubError::ApiError(ref msg) if msg.contains("Not modified"))
                    {
                        warn!("Silent workflow refresh failed: {}", e);
                    }
                }
            }

            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_clone = client_for_spawn.lock().clone();
            let result =
                fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn).await;
            let _ = sender.send(result);
        });
    }

    pub(super) fn connect_refresh_button(&self, button: &gtk::Button) {
        let client = self.client.clone();
        let workflows = self.workflows.clone();
        let context = self.workflow_list_context();
        let run_filters = context.run_filters.clone();
        let owner = context.owner.clone();
        let repo_name = context.repo.clone();
        let list_box = context.list_box.clone();
        let callback_refs = self.clone_for_callbacks();
        let parent_window = context.parent_window.clone();
        let loading_guard = self.loading.clone();
        let cache = context.cache.clone();
        let toast_overlay = context.toast_overlay.clone();
        let job_contexts = context.job_contexts.clone();
        let workflows_with_active_runs = context.workflows_with_active_runs.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let repo_model = context.repo_model.clone();
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
            let list_box = list_box.clone();
            let callback_refs = callback_refs.clone();
            let parent_window = parent_window.clone();
            let loading_guard = loading_guard.clone();
            let cache = cache.clone();
            let toast_overlay = toast_overlay.clone();
            let workflows_with_active_runs = workflows_with_active_runs.clone();
            let run_digests = run_digests.clone();
            let notification_manager_handle = notification_manager.clone();
            let preferences_manager_handle = preferences_manager.clone();

            callback_refs.show_loading(true);

            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<Vec<Workflow>, GitHubError>>(glib::Priority::default());
            let list_box_for_ui = list_box.clone();
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
            let cache_for_ui = cache.clone();

            receiver.attach(None, move |result| {
                callback_refs_for_ui.show_loading(false);
                *loading_guard.lock() = false;

                match result {
                    Ok(wf_list) => {
                        info!("Refreshed {} workflows", wf_list.len());
                        *workflows_for_ui.lock() = wf_list.clone();

                        let ui_context = WorkflowListContext {
                            list_box: list_box_for_ui.clone(),
                            client: client_for_ui.clone(),
                            owner: owner_for_ui.clone(),
                            repo: repo_name_for_ui.clone(),
                            repo_model: repo_model_for_ui.clone(),
                            parent_window: parent_window_for_ui.clone(),
                            cache: cache_for_ui.clone(),
                            toast_overlay: toast_overlay_for_ui.clone(),
                            job_contexts: job_contexts_for_ui.clone(),
                            workflows_with_active_runs: workflows_with_active_runs_for_ui.clone(),
                            run_digests: run_digests_for_ui.clone(),
                            notification_manager: notification_manager_for_ui.clone(),
                            preferences_manager: preferences_manager_for_ui.clone(),
                            run_filters: run_filters_for_ui.clone(),
                        };

                        super::workflow_list::update_workflows_list(&ui_context, &wf_list);
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
            let cache_for_spawn = cache.clone();

            crate::runtime_handle().spawn(async move {
                let cache_key = format!("{}/{}", owner_for_spawn, repo_name_for_spawn);
                cache_for_spawn.clear_repo(&cache_key).await;

                let client_clone = client_for_spawn.lock().clone();
                let result =
                    fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn).await;
                let _ = sender.send(result);
            });
        });
    }

    pub(super) fn start_auto_refresh(&self) {
        let refresh_interval_secs = if let Some(prefs_mgr) = &self.preferences_manager {
            let handle = crate::runtime_handle().clone();
            let prefs_mgr = prefs_mgr.clone();

            handle.spawn(async move { prefs_mgr.get().await.refresh_interval });
            5u64
        } else {
            5u64
        };

        if refresh_interval_secs == 0 {
            info!("Auto-refresh disabled (interval = 0)");
            return;
        }

        let list_context = self.workflow_list_context();
        let client = list_context.client.clone();
        let owner = list_context.owner.clone();
        let repo_name = list_context.repo.clone();
        let repo_model = list_context.repo_model.clone();
        let parent_window = list_context.parent_window.clone();
        let list_box = list_context.list_box.clone();
        let workflows_with_active = list_context.workflows_with_active_runs.clone();
        let auto_refresh_source = self.auto_refresh_source.clone();
        let job_contexts = list_context.job_contexts.clone();
        let cache = list_context.cache.clone();
        let toast_overlay = list_context.toast_overlay.clone();
        let run_digests = list_context.run_digests.clone();
        let notification_manager = list_context.notification_manager.clone();
        let preferences_manager = list_context.preferences_manager.clone();
        let run_filters = list_context.run_filters.clone();

        info!(
            "Starting auto-refresh timer with interval: {} seconds",
            refresh_interval_secs
        );

        let source_id = glib::timeout_add_seconds_local(refresh_interval_secs as u32, move || {
            info!("Auto-refreshing workflow runs in background");

            let background_context = WorkflowListContext {
                list_box: list_box.clone(),
                client: client.clone(),
                owner: owner.clone(),
                repo: repo_name.clone(),
                repo_model: repo_model.clone(),
                parent_window: parent_window.clone(),
                cache: cache.clone(),
                toast_overlay: toast_overlay.clone(),
                job_contexts: job_contexts.clone(),
                workflows_with_active_runs: workflows_with_active.clone(),
                run_digests: run_digests.clone(),
                notification_manager: notification_manager.clone(),
                preferences_manager: preferences_manager.clone(),
                run_filters: run_filters.clone(),
            };

            Self::refresh_runs_background(&background_context);

            glib::ControlFlow::Continue
        });

        *auto_refresh_source.lock() = Some(source_id);
    }

    fn refresh_runs_background(context: &WorkflowListContext) {
        let list_box = context.list_box.clone();
        let client = context.client.clone();
        let owner = context.owner.clone();
        let repo = context.repo.clone();
        let repo_model = context.repo_model.clone();
        let parent_window = context.parent_window.clone();
        let cache = context.cache.clone();
        let toast_overlay = context.toast_overlay.clone();
        let workflows_with_active = context.workflows_with_active_runs.clone();
        let job_contexts = context.job_contexts.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();
        let run_filters_arc = context.run_filters.clone();

        let mut observed_active: HashSet<i64> = HashSet::new();

        let mut child = list_box.first_child();
        while let Some(widget) = child.as_ref() {
            let next_sibling = widget.next_sibling();

            if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>()
                && let Some(row_child) = row.child()
                && let Some(box_widget) = row_child.downcast_ref::<gtk::Box>()
            {
                let mut inner_child = box_widget.first_child();
                while let Some(widget) = inner_child.as_ref() {
                    let next = widget.next_sibling();

                    if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
                        if let Some((workflow_id, is_active)) =
                            parse_expander_widget_name(expander.widget_name().as_str())
                        {
                            if is_active {
                                observed_active.insert(workflow_id);
                            }

                            if let Some(child_widget) = expander.child()
                                && let Ok(runs_box) = child_widget.downcast::<gtk::Box>()
                            {
                                let status_badge = Self::status_badge_for_expander(expander);
                                let preserved_runs =
                                    current_job_context_run_ids(&job_contexts, workflow_id);
                                let workflow_label = unsafe {
                                    expander
                                        .data::<String>("actioneer-workflow-name")
                                        .map(|name_ptr| name_ptr.as_ref().clone())
                                }
                                .unwrap_or_else(|| {
                                    format!("{}/{} • Workflow {}", owner, repo, workflow_id)
                                });

                                let notification_manager_clone = notification_manager.clone();
                                let preferences_manager_clone = preferences_manager.clone();
                                let repo_model_clone = repo_model.clone();

                                load_workflow_runs(LoadRunsParams {
                                    client: client.clone(),
                                    owner: owner.to_string(),
                                    repo: repo.to_string(),
                                    repo_model: repo_model_clone,
                                    workflow_id,
                                    workflow_name: workflow_label,
                                    runs_box,
                                    parent_window: parent_window.clone(),
                                    status_badge,
                                    expander: expander.clone(),
                                    cache: cache.clone(),
                                    toast_overlay: toast_overlay.clone(),
                                    bypass_cache: true,
                                    job_contexts: job_contexts.clone(),
                                    expanded_run_ids: preserved_runs,
                                    workflows_with_active: workflows_with_active.clone(),
                                    background: true,
                                    run_digests: run_digests.clone(),
                                    notification_manager: notification_manager_clone,
                                    preferences_manager: preferences_manager_clone,
                                    run_filters: run_filters_arc.clone(),
                                });
                            }
                        }
                    }
                    inner_child = next;
                }
            }
            child = next_sibling;
        }

        refresh_jobs_for_workflows(&job_contexts, &observed_active);

        *workflows_with_active.lock() = observed_active;
    }

    pub(super) fn status_badge_for_expander(expander: &gtk::Expander) -> Option<gtk::Label> {
        expander
            .label_widget()
            .and_then(|widget| widget.downcast::<gtk::Box>().ok())
            .and_then(|header| {
                let mut child = header.first_child();
                while let Some(widget) = child.as_ref() {
                    if let Ok(label) = widget.clone().downcast::<gtk::Label>()
                        && label.has_css_class("badge")
                    {
                        return Some(label);
                    }
                    child = widget.next_sibling();
                }
                None
            })
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
                spinner.set_tooltip_text(Some("Loading workflows..."));
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
                spinner.set_tooltip_text(Some("Loading workflows..."));
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

fn parse_expander_widget_name(name: &str) -> Option<(i64, bool)> {
    let (base, is_active) = if let Some(stripped) = name.strip_suffix("_ACTIVE") {
        (stripped, true)
    } else {
        (name, false)
    };

    let workflow_id = base.strip_prefix("workflow_")?.parse::<i64>().ok()?;

    Some((workflow_id, is_active))
}

#[cfg(test)]
mod tests {
    use super::parse_expander_widget_name;

    #[test]
    fn parses_active_expander_names() {
        let result = parse_expander_widget_name("workflow_123_ACTIVE");
        assert_eq!(result, Some((123, true)));
    }

    #[test]
    fn parses_inactive_expander_names() {
        let result = parse_expander_widget_name("workflow_5");
        assert_eq!(result, Some((5, false)));
    }

    #[test]
    fn rejects_invalid_prefixes() {
        assert_eq!(parse_expander_widget_name("foo"), None);
    }

    #[test]
    fn rejects_non_numeric_ids() {
        assert_eq!(parse_expander_widget_name("workflow_bar"), None);
    }
}
