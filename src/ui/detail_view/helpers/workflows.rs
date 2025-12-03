use super::context::{JobContextMap, take_job_context_run_ids};
use super::runs::{LoadRunsParams, RunDigestStore, load_workflow_runs};
use crate::api::GitHubClient;
use crate::api::models::{Repo, Workflow};
use crate::cache::DataCache;
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{error, info};

#[derive(Clone)]
pub(crate) struct WorkflowRowContext {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub repo_model: Repo,
    pub parent_window: adw::ApplicationWindow,
    pub cache: Arc<DataCache>,
    pub toast_overlay: adw::ToastOverlay,
    pub job_contexts: JobContextMap,
    pub workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>,
    pub run_digests: Arc<Mutex<RunDigestStore>>,
    pub notification_manager: Option<NotificationManager>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub run_filters: Arc<Mutex<RunFilters>>,
}

pub(crate) struct WorkflowRowSettings {
    pub should_expand: bool,
    pub initial_expanded_run_ids: Vec<i64>,
}

pub(crate) fn create_workflow_expander_row(
    workflow: &Workflow,
    context: &WorkflowRowContext,
    settings: WorkflowRowSettings,
) -> gtk::ListBoxRow {
    let should_expand = settings.should_expand;
    let initial_expanded_run_ids = Rc::new(RefCell::new(Some(settings.initial_expanded_run_ids)));
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let repo_model = context.repo_model.clone();
    let parent_window = context.parent_window.clone();
    let cache = context.cache.clone();
    let toast_overlay = context.toast_overlay.clone();
    let job_contexts = context.job_contexts.clone();
    let workflows_with_active = context.workflows_with_active_runs.clone();
    let run_digests = context.run_digests.clone();
    let notification_manager = context.notification_manager.clone();
    let preferences_manager = context.preferences_manager.clone();
    let run_filters = context.run_filters.clone();
    let run_filters_for_signal = run_filters.clone();

    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let workflow_display_name = format!("{}/{} • {}", owner, repo, workflow.name);

    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_box.set_margin_top(8);
    header_box.set_margin_bottom(8);
    header_box.set_margin_start(12);
    header_box.set_margin_end(12);

    let workflow_name_label = gtk::Label::new(Some(&workflow.name));
    workflow_name_label.set_halign(gtk::Align::Start);
    workflow_name_label.set_hexpand(true);
    header_box.append(&workflow_name_label);

    let trigger_btn = gtk::Button::from_icon_name("media-playback-start-symbolic");
    trigger_btn.set_tooltip_text(Some("Trigger workflow"));
    trigger_btn.add_css_class("flat");
    trigger_btn.add_css_class("circular");
    trigger_btn.set_valign(gtk::Align::Center);
    header_box.append(&trigger_btn);

    let status_badge = gtk::Label::new(None);
    status_badge.add_css_class("caption");
    status_badge.add_css_class("badge");
    status_badge.set_halign(gtk::Align::End);
    status_badge.set_visible(false);
    header_box.append(&status_badge);

    let expander = gtk::Expander::new(None);
    expander.set_label_widget(Some(&header_box));
    expander.set_widget_name(&format!("workflow_{}", workflow.id));
    unsafe {
        expander.set_data("actioneer-workflow-name", workflow.name.clone());
        expander.set_data("actioneer-workflow-id", workflow.id);
    }

    let runs_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    runs_box.set_margin_start(24);
    runs_box.set_margin_top(8);
    runs_box.set_margin_bottom(8);

    let placeholder = gtk::Label::new(Some("Click to load runs..."));
    placeholder.add_css_class("dim-label");
    placeholder.set_halign(gtk::Align::Start);
    placeholder.set_margin_top(4);
    placeholder.set_margin_bottom(4);
    runs_box.append(&placeholder);

    expander.set_child(Some(&runs_box));
    main_box.append(&expander);

    let client_for_trigger = client.clone();
    let owner_for_trigger = owner.clone();
    let repo_for_trigger = repo.clone();
    let workflow_id_for_trigger = workflow.id;
    let workflow_name_for_trigger = workflow.name.clone();
    let workflow_display_for_trigger = workflow_display_name.clone();
    let parent_window_for_trigger = parent_window.clone();
    let toast_overlay_for_trigger = toast_overlay.clone();
    let cache_for_trigger = cache.clone();
    let expander_for_trigger = expander.clone();
    let runs_box_for_trigger = runs_box.clone();
    let job_contexts_for_trigger = job_contexts.clone();
    let run_digests_shared = run_digests.clone();
    let notification_manager_shared = notification_manager.clone();
    let preferences_manager_shared = preferences_manager.clone();
    let run_digests_for_trigger = run_digests_shared.clone();
    let notification_manager_for_trigger = notification_manager_shared.clone();
    let preferences_manager_for_trigger = preferences_manager_shared.clone();
    let workflows_with_active_shared = workflows_with_active.clone();

    let workflow_id = workflow.id;
    let owner_string = owner.clone();
    let repo_string = repo.clone();
    let workflow_display_shared = workflow_display_name.clone();
    let client_shared = client.clone();
    let runs_box_shared = runs_box.clone();
    let parent_window_shared = parent_window.clone();
    let status_badge_shared = status_badge.clone();
    let cache_shared = cache.clone();
    let toast_overlay_shared = toast_overlay.clone();
    let repo_model_shared = repo_model.clone();
    let repo_model_for_signal = repo_model.clone();
    let repo_model_for_trigger = repo_model.clone();

    let owner_for_signal = owner_string.clone();
    let repo_for_signal = repo_string.clone();
    let workflow_display_for_signal = workflow_display_shared.clone();
    let client_for_signal = client_shared.clone();
    let runs_box_for_signal = runs_box_shared.clone();
    let parent_window_for_signal = parent_window_shared.clone();
    let status_badge_for_signal = status_badge_shared.clone();
    let cache_for_signal = cache_shared.clone();
    let toast_overlay_for_signal = toast_overlay_shared.clone();
    let job_contexts_shared = job_contexts.clone();
    let job_contexts_for_signal = job_contexts_shared.clone();
    let initial_expanded_runs_shared = initial_expanded_run_ids.clone();
    let initial_expanded_runs_for_signal = initial_expanded_runs_shared.clone();
    let workflows_with_active_for_signal = workflows_with_active_shared.clone();
    let run_digests_for_signal = run_digests_shared.clone();
    let notification_manager_for_signal = notification_manager_shared.clone();
    let preferences_manager_for_signal = preferences_manager_shared.clone();

    let is_programmatic_expand = Rc::new(Cell::new(false));
    let is_programmatic_for_signal = is_programmatic_expand.clone();

    expander.connect_expanded_notify(move |exp| {
        if !exp.is_expanded() {
            return;
        }

        if is_programmatic_for_signal.get() {
            return;
        }

        let force_refresh = unsafe {
            exp.steal_data::<bool>("actioneer-force-refresh")
                .unwrap_or(false)
        };

        let mut should_load = force_refresh;
        if !should_load {
            match runs_box_for_signal.first_child() {
                None => should_load = true,
                Some(child) if child.is::<gtk::Label>() => should_load = true,
                _ => {}
            }
        }

        if should_load {
            let preserved_runs = {
                let mut initial_opt = initial_expanded_runs_for_signal.borrow_mut();
                if let Some(vec) = initial_opt.take() {
                    vec
                } else {
                    drop(initial_opt);
                    take_job_context_run_ids(&job_contexts_for_signal, workflow_id)
                }
            };

            load_workflow_runs(LoadRunsParams {
                client: client_for_signal.clone(),
                owner: owner_for_signal.clone(),
                repo: repo_for_signal.clone(),
                repo_model: repo_model_for_signal.clone(),
                workflow_id,
                workflow_name: workflow_display_for_signal.clone(),
                runs_box: runs_box_for_signal.clone(),
                parent_window: parent_window_for_signal.clone(),
                status_badge: Some(status_badge_for_signal.clone()),
                expander: exp.clone(),
                cache: cache_for_signal.clone(),
                toast_overlay: toast_overlay_for_signal.clone(),
                bypass_cache: force_refresh,
                job_contexts: job_contexts_for_signal.clone(),
                expanded_run_ids: preserved_runs,
                workflows_with_active: workflows_with_active_for_signal.clone(),
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

        if let Some(child) = runs_box.first_child()
            && child.is::<gtk::Label>()
        {
            let preserved_runs = {
                let mut initial_opt = initial_expanded_runs_shared.borrow_mut();
                if let Some(vec) = initial_opt.take() {
                    vec
                } else {
                    drop(initial_opt);
                    take_job_context_run_ids(&job_contexts_shared, workflow_id)
                }
            };

            load_workflow_runs(LoadRunsParams {
                client: client_shared.clone(),
                owner: owner_string.clone(),
                repo: repo_string.clone(),
                repo_model: repo_model_shared.clone(),
                workflow_id,
                workflow_name: workflow_display_shared.clone(),
                runs_box: runs_box_shared.clone(),
                parent_window: parent_window_shared.clone(),
                status_badge: Some(status_badge_shared.clone()),
                expander: expander.clone(),
                cache: cache_shared.clone(),
                toast_overlay: toast_overlay_shared.clone(),
                bypass_cache: true,
                job_contexts: job_contexts_shared.clone(),
                expanded_run_ids: preserved_runs,
                workflows_with_active: workflows_with_active_shared.clone(),
                background: false,
                run_digests: run_digests_shared.clone(),
                notification_manager: notification_manager_shared.clone(),
                preferences_manager: preferences_manager_shared.clone(),
                run_filters: run_filters_for_initial.clone(),
            });
        }
    }

    let run_filters_for_trigger = run_filters.clone();

    trigger_btn.connect_clicked(move |_| {
        let run_filters_for_dialog = run_filters_for_trigger.clone();
        let expander = expander_for_trigger.clone();
        let runs_box = runs_box_for_trigger.clone();
        let cache = cache_for_trigger.clone();
        let client = client_for_trigger.clone();
        let owner = owner_for_trigger.clone();
        let repo = repo_for_trigger.clone();
        let workflow_id = workflow_id_for_trigger;
        let toast_overlay = toast_overlay_for_trigger.clone();
        let workflow_name = workflow_name_for_trigger.clone();
        let workflow_display = workflow_display_for_trigger.clone();
        let parent_window = parent_window_for_trigger.clone();
        let job_contexts = job_contexts_for_trigger.clone();
        let workflows_with_active_button = workflows_with_active_shared.clone();
        let run_digests = run_digests_for_trigger.clone();
        let notification_manager = notification_manager_for_trigger.clone();
        let preferences_manager = preferences_manager_for_trigger.clone();
        let repo_model_for_dialog = repo_model_for_trigger.clone();

        let dialog = gtk::Dialog::with_buttons(
            Some("Trigger Workflow"),
            Some(&parent_window),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[("Cancel", gtk::ResponseType::Cancel), ("Trigger", gtk::ResponseType::Accept)],
        );
        dialog.set_default_response(gtk::ResponseType::Accept);
        dialog.set_modal(true);

        let content_area = dialog.content_area();
        content_area.set_margin_start(12);
        content_area.set_margin_end(12);
        content_area.set_margin_top(12);
        content_area.set_margin_bottom(12);
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);

        let info_label = gtk::Label::new(Some(&format!(
            "This will trigger the \"{}\" workflow.\n\nTriggered runs typically appear within 10-30 seconds.",
            workflow_name
        )));
        info_label.set_wrap(true);
        info_label.set_xalign(0.0);
        vbox.append(&info_label);

        let branch_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let branch_label = gtk::Label::new(Some("Branch or ref:"));
        branch_label.set_xalign(0.0);
        branch_box.append(&branch_label);

        let branch_dropdown = gtk::DropDown::from_strings(&["main", "master"]);
        branch_dropdown.set_selected(0);
        branch_box.append(&branch_dropdown);

        let client_for_branches = client.clone();
        let owner_for_branches = owner.clone();
        let repo_for_branches = repo.clone();
        let dropdown_for_branches = branch_dropdown.clone();

        let (branch_sender, branch_receiver) = glib::MainContext::default()
            .channel::<Vec<String>>(glib::Priority::default());

        branch_receiver.attach(None, move |branch_names| {
            let str_refs: Vec<&str> = branch_names.iter().map(|s| s.as_str()).collect();
            dropdown_for_branches.set_model(Some(&gtk::StringList::new(&str_refs)));
            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_guard = client_for_branches.lock().clone();
            let branch_result = client_guard
                .list_branches(&owner_for_branches, &repo_for_branches)
                .await;

            if let Ok(branches) = branch_result {
                let branch_names: Vec<String> =
                    branches.iter().map(|b| b.name.clone()).collect();
                let _ = branch_sender.send(branch_names);
            }
        });

        vbox.append(&branch_box);
        content_area.append(&vbox);

        let client_clone = client.clone();
        let owner_clone = owner.clone();
        let repo_clone = repo.clone();
        let toast_overlay_clone = toast_overlay.clone();
        let workflow_name_clone = workflow_name.clone();
        let workflow_display_rc = Rc::new(workflow_display.clone());
        let cache_clone = cache.clone();
        let expander_clone = expander.clone();
        let runs_box_clone = runs_box.clone();
        let parent_window_clone = parent_window.clone();
        let workflows_with_active_clone = workflows_with_active_button.clone();
        let run_digests_clone = run_digests.clone();
        let notification_manager_rc = Rc::new(notification_manager.clone());
        let preferences_manager_rc = Rc::new(preferences_manager.clone());

        let run_filters_for_response = run_filters_for_dialog.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let selected_branch = branch_dropdown
                    .selected_item()
                    .and_then(|obj| obj.downcast::<gtk::StringObject>().ok())
                    .map(|so| so.string().to_string())
                    .unwrap_or_else(|| "main".to_string());

                let client = client_clone.clone();
                let owner = owner_clone.clone();
                let repo = repo_clone.clone();
                let branch = selected_branch.clone();
                let workflow_id_str = workflow_id.to_string();

                let (sender, receiver) = glib::MainContext::default()
                    .channel::<Result<String, String>>(glib::Priority::default());

                crate::runtime_handle().spawn(async move {
                    let client_guard = client.lock().clone();
                    let dispatch_result = client_guard
                        .dispatch_workflow(&owner, &repo, &workflow_id_str, &branch, None)
                        .await;

                    match dispatch_result {
                        Ok(_) => {
                            let _ = sender.send(Ok(branch));
                        }
                        Err(e) => {
                            let _ = sender
                                .send(Err(format!("Failed to trigger workflow: {}", e)));
                        }
                    }
                });

                let toast_overlay = toast_overlay_clone.clone();
                let workflow_name = workflow_name_clone.clone();
                let cache = cache_clone.clone();
                let owner = owner_clone.clone();
                let repo = repo_clone.clone();
                let expander = expander_clone.clone();
                let runs_box = runs_box_clone.clone();
                let client = client_clone.clone();
                let parent_window = parent_window_clone.clone();
                let job_contexts = job_contexts.clone();
                let workflows_with_active = workflows_with_active_clone.clone();
                let run_digests_for_reload = run_digests_clone.clone();
                let workflow_display_for_closure = workflow_display_rc.clone();
                let notification_manager_for_closure = notification_manager_rc.clone();
                let preferences_manager_for_closure = preferences_manager_rc.clone();
                let repo_model_for_closure = repo_model_for_dialog.clone();

                let run_filters_for_reload = run_filters_for_response.clone();
                receiver.attach(None, move |result| {
                    let workflows_with_active = workflows_with_active.clone();
                    let workflow_display_handle = workflow_display_for_closure.clone();
                    let notification_manager_handle = notification_manager_for_closure.clone();
                    let preferences_manager_handle = preferences_manager_for_closure.clone();
                    match result {
                        Ok(branch_name) => {
                            info!("Workflow triggered successfully on branch: {}", branch_name);

                            let cache = cache.clone();
                            let cache_key = format!("{}/{}", owner, repo);
                            let expander = expander.clone();
                            let runs_box = runs_box.clone();
                            let client_for_reload = client.clone();
                            let owner_for_reload = owner.clone();
                            let repo_for_reload = repo.clone();
                            let parent_window_for_reload = parent_window.clone();
                            let toast_overlay_for_reload = toast_overlay.clone();
                            let cache_for_reload = cache.clone();
                            let job_contexts_for_reload = job_contexts.clone();
                            let workflows_with_active_for_reload = workflows_with_active.clone();
                            let run_digests_for_refresh = run_digests_for_reload.clone();
                            let workflow_display_for_reload =
                                workflow_display_handle.as_ref().clone();
                            let notification_manager_for_reload =
                                notification_manager_handle.as_ref().clone();
                            let preferences_manager_for_reload =
                                preferences_manager_handle.as_ref().clone();
                            let repo_model_for_reload = repo_model_for_closure.clone();

                            crate::runtime_handle().spawn(async move {
                                cache.store_runs(Vec::new(), &cache_key, workflow_id).await;
                            });

                            let run_filters_for_idle = run_filters_for_reload.clone();
                            glib::idle_add_local_once(move || {
                                let workflows_with_active = workflows_with_active_for_reload.clone();
                                if expander.is_expanded() {
                                    info!("Reloading runs after workflow trigger");
                                    loop {
                                        let child_opt = runs_box.first_child();
                                        let Some(child) = child_opt else {
                                            break;
                                        };
                                        runs_box.remove(&child);
                                    }
                                    let spinner = gtk::Spinner::new();
                                    spinner.start();
                                    spinner.set_margin_top(8);
                                    spinner.set_margin_bottom(8);
                                    runs_box.append(&spinner);

                                    let preserved_runs = take_job_context_run_ids(&job_contexts_for_reload, workflow_id);

                                    let workflow_display_for_runs =
                                        workflow_display_for_reload.clone();
                                    let notification_manager_for_runs =
                                        notification_manager_for_reload.clone();
                                    let preferences_manager_for_runs =
                                        preferences_manager_for_reload.clone();

                                    load_workflow_runs(LoadRunsParams {
                                        client: client_for_reload.clone(),
                                        owner: owner_for_reload.clone(),
                                        repo: repo_for_reload.clone(),
                                        repo_model: repo_model_for_reload.clone(),
                                        workflow_id,
                                        workflow_name: workflow_display_for_runs,
                                        runs_box: runs_box.clone(),
                                        parent_window: parent_window_for_reload.clone(),
                                        status_badge: None,
                                        expander: expander.clone(),
                                        cache: cache_for_reload.clone(),
                                        toast_overlay: toast_overlay_for_reload.clone(),
                                        bypass_cache: true,
                                        job_contexts: job_contexts_for_reload.clone(),
                                        expanded_run_ids: preserved_runs,
                                        workflows_with_active,
                                        background: false,
                                        run_digests: run_digests_for_refresh.clone(),
                                        notification_manager: notification_manager_for_runs,
                                        preferences_manager: preferences_manager_for_runs,
                                        run_filters: run_filters_for_idle.clone(),
                                    });
                                }
                            });

                            let toast_overlay = toast_overlay.clone();
                            let workflow_name = workflow_name.clone();
                            let branch = branch_name.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let toast = adw::Toast::new(&format!(
                                    "✓ Workflow '{}' triggered on branch '{}'",
                                    workflow_name, branch
                                ));
                                toast.set_timeout(3);
                                toast_overlay.add_toast(toast);
                            });
                        }
                        Err(error_msg) => {
                            error!("{}", error_msg);

                            let toast_overlay = toast_overlay.clone();
                            let error = error_msg.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let toast = adw::Toast::new(&format!("✗ Failed to trigger workflow: {}", error));
                                toast.set_timeout(5);
                                toast_overlay.add_toast(toast);
                            });
                        }
                    }
                    glib::ControlFlow::Break
                });
            }

            dialog.close();
        });

        dialog.present();
    });

    row.set_child(Some(&main_box));
    row
}
