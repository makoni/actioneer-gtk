use super::RunLoadService;
use super::context::JobContextMap;
use super::runs::{LoadRunsParams, RunDigestStore, RunRowContext, WorkflowRunListModel};
use crate::api::GitHubClient;
use crate::api::models::{
    Repo, Workflow, WorkflowDispatchInput, WorkflowDispatchInputType, WorkflowDispatchInputValue,
    build_dispatch_inputs_payload,
};
use crate::notifications::NotificationManager;
use crate::preferences::PreferencesManager;
use crate::ui::detail_view::RunFilters;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

#[derive(Clone)]
pub(crate) struct WorkflowRowContext {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub repo_model: Repo,
    pub parent_window: adw::ApplicationWindow,
    pub toast_overlay: adw::ToastOverlay,
    pub job_contexts: JobContextMap,
    pub workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>,
    pub workflows_last_loaded: Arc<Mutex<std::collections::HashMap<i64, std::time::Instant>>>,
    pub workflows_loading_runs: Arc<Mutex<HashSet<i64>>>,
    pub run_digests: Arc<Mutex<RunDigestStore>>,
    pub notification_manager: Option<NotificationManager>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub run_filters: Arc<Mutex<RunFilters>>,
    pub run_load_service: RunLoadService,
}

pub(crate) struct WorkflowRowSettings {
    pub should_expand: bool,
    pub initial_expanded_run_ids: Vec<i64>,
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
        (Some(description), true) => Some(format!("{} (required)", description)),
        (Some(description), false) => Some(description.clone()),
        (None, true) => Some("Required".to_string()),
        (None, false) => None,
    }
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
    let workflow_display_name = format!("{}/{} • {}", owner, repo, workflow.name);
    let workflow_path = workflow.path.clone();

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
    trigger_btn.set_focus_on_click(false);
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

    let run_row_context = RunRowContext::new(
        client.clone(),
        owner.clone(),
        repo.clone(),
        repo_model.clone(),
        parent_window.clone(),
        workflow.id,
        toast_overlay.clone(),
        job_contexts.clone(),
    );
    let run_list = WorkflowRunListModel::new(run_row_context);
    expander.set_child(Some(&run_list.widget()));
    unsafe {
        expander.set_data("actioneer-run-list", run_list.clone());
    }
    main_box.append(&expander);

    let card = workflow_row_card(&main_box);
    card.add_css_class("workflow-row");

    let client_for_trigger = client.clone();
    let owner_for_trigger = owner.clone();
    let repo_for_trigger = repo.clone();
    let workflow_id_for_trigger = workflow.id;
    let workflow_name_for_trigger = workflow.name.clone();
    let workflow_display_for_trigger = workflow_display_name.clone();
    let parent_window_for_trigger = parent_window.clone();
    let toast_overlay_for_trigger = toast_overlay.clone();
    let expander_for_trigger = expander.clone();
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
    let status_badge_shared = status_badge.clone();
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
    let status_badge_for_signal = status_badge_shared.clone();
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

        let force_refresh = unsafe {
            exp.steal_data::<bool>("actioneer-force-refresh")
                .unwrap_or(false)
        };

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
                status_badge: Some(status_badge_for_signal.clone()),
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
                status_badge: Some(status_badge_shared.clone()),
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

    let run_filters_for_trigger = run_filters.clone();
    let run_load_service_for_trigger = run_load_service.clone();

    trigger_btn.connect_clicked(move |_| {
        let run_filters_for_dialog = run_filters_for_trigger.clone();
        let expander = expander_for_trigger.clone();
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
        let workflows_loading_for_trigger = workflows_loading_shared.clone();
        let run_digests = run_digests_for_trigger.clone();
        let notification_manager = notification_manager_for_trigger.clone();
        let preferences_manager = preferences_manager_for_trigger.clone();
        let repo_model_for_dialog = repo_model_for_trigger.clone();
        let run_load_service_for_dialog = run_load_service_for_trigger.clone();

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

        let branch_model = gtk::StringList::new(&["Loading branches..."]);
        let branch_dropdown = gtk::DropDown::new(Some(branch_model.clone()), None::<&gtk::Expression>);
        branch_dropdown.set_selected(0);
        branch_dropdown.set_sensitive(false);
        branch_box.append(&branch_dropdown);

        let inputs_placeholder = gtk::Label::new(Some("Loading workflow inputs..."));
        inputs_placeholder.set_wrap(true);
        inputs_placeholder.set_xalign(0.0);

        let inputs_group = adw::PreferencesGroup::builder().title("Inputs").build();
        inputs_group.set_visible(false);

        if let Some(trigger_button) = dialog
            .widget_for_response(gtk::ResponseType::Accept)
            .and_then(|w| w.downcast::<gtk::Button>().ok())
        {
            trigger_button.set_sensitive(false);
        }

        let input_fields: Rc<RefCell<Vec<DispatchInputField>>> = Rc::new(RefCell::new(Vec::new()));
        let branches_loaded = Rc::new(Cell::new(false));
        let branches_available = Rc::new(Cell::new(false));
        let inputs_loaded = Rc::new(Cell::new(false));

        let client_for_branches = client.clone();
        let owner_for_branches = owner.clone();
        let repo_for_branches = repo.clone();
        let dropdown_for_branches = branch_dropdown.clone();
        let repo_model_for_branches = repo_model_for_dialog.clone();

        let (branch_sender, branch_receiver) = glib::MainContext::default()
            .channel::<Vec<String>>(glib::Priority::default());

        let trigger_button_for_branches = dialog
            .widget_for_response(gtk::ResponseType::Accept)
            .and_then(|w| w.downcast::<gtk::Button>().ok());
        let branches_loaded_for_branches = branches_loaded.clone();
        let branches_available_for_branches = branches_available.clone();
        let inputs_loaded_for_branches = inputs_loaded.clone();

        branch_receiver.attach(None, move |branch_names| {
            let str_refs: Vec<&str> = branch_names.iter().map(|s| s.as_str()).collect();
            let model = gtk::StringList::new(&str_refs);

            dropdown_for_branches.set_model(Some(&model));
            dropdown_for_branches.set_sensitive(!str_refs.is_empty());

            if !str_refs.is_empty() {
                dropdown_for_branches.set_selected(0);
            }

            branches_loaded_for_branches.set(true);
            branches_available_for_branches.set(!str_refs.is_empty());

            if let Some(btn) = trigger_button_for_branches.as_ref() {
                let should_enable = branches_loaded_for_branches.get()
                    && inputs_loaded_for_branches.get()
                    && branches_available_for_branches.get();
                btn.set_sensitive(should_enable);
            }
            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_guard = client_for_branches.lock().clone();
            let branch_result = client_guard
                .list_branches(&owner_for_branches, &repo_for_branches)
                .await;

            let branch_names: Vec<String> = match branch_result {
                Ok(branches) if !branches.is_empty() => {
                    branches.iter().map(|b| b.name.clone()).collect()
                }
                Ok(_) => {
                    let fallback = repo_model_for_branches
                        .default_branch
                        .clone()
                        .unwrap_or_else(|| "main".to_string());
                    vec![fallback]
                }
                Err(_) => {
                    let fallback = repo_model_for_branches
                        .default_branch
                        .clone()
                        .unwrap_or_else(|| "main".to_string());
                    vec![fallback]
                }
            };

            let _ = branch_sender.send(branch_names);
        });

        let client_for_inputs = client.clone();
        let owner_for_inputs = owner.clone();
        let repo_for_inputs = repo.clone();
        let workflow_path_for_inputs = workflow_path.clone();
        let default_ref_for_inputs = repo_model_for_dialog
            .default_branch
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let inputs_group_for_loader = inputs_group.clone();
        let inputs_placeholder_for_loader = inputs_placeholder.clone();
        let input_fields_for_loader = input_fields.clone();
        let trigger_button_for_inputs = dialog
            .widget_for_response(gtk::ResponseType::Accept)
            .and_then(|w| w.downcast::<gtk::Button>().ok());
        let branches_loaded_for_inputs = branches_loaded.clone();
        let branches_available_for_inputs = branches_available.clone();
        let inputs_loaded_for_inputs = inputs_loaded.clone();
        let toast_overlay_for_inputs = toast_overlay.clone();

        let (inputs_sender, inputs_receiver) = glib::MainContext::default()
            .channel::<Result<Vec<WorkflowDispatchInput>, String>>(glib::Priority::default());

        inputs_receiver.attach(None, move |result| {
            input_fields_for_loader.borrow_mut().clear();
            match result {
                Ok(inputs) if inputs.is_empty() => {
                    inputs_placeholder_for_loader.set_text("No inputs defined for this workflow.");
                    inputs_placeholder_for_loader.set_visible(true);
                    inputs_group_for_loader.set_visible(false);
                }
                Ok(inputs) => {
                    inputs_placeholder_for_loader.set_visible(false);
                    inputs_group_for_loader.set_visible(true);

                    for input in inputs {
                        let title = dispatch_input_title(&input);
                        let subtitle = dispatch_input_subtitle(&input);

                        match input.input_type {
                            WorkflowDispatchInputType::Choice if !input.options.is_empty() => {
                                let options = input.options.clone();
                                let str_refs: Vec<&str> =
                                    options.iter().map(|s| s.as_str()).collect();
                                let model = gtk::StringList::new(&str_refs);
                                let row = adw::ComboRow::builder()
                                    .title(title)
                                    .model(&model)
                                    .build();
                                if let Some(subtitle) = subtitle {
                                    row.set_subtitle(&subtitle);
                                }
                                let selected = input
                                    .default_as_string()
                                    .and_then(|value| options.iter().position(|opt| opt == &value))
                                    .unwrap_or(0);
                                row.set_selected(selected as u32);
                                inputs_group_for_loader.add(&row);
                                input_fields_for_loader.borrow_mut().push(DispatchInputField {
                                    input,
                                    widget: DispatchInputWidget::Choice(row, options),
                                });
                            }
                            WorkflowDispatchInputType::Boolean => {
                                let row = adw::SwitchRow::builder().title(title).build();
                                if let Some(subtitle) = subtitle {
                                    row.set_subtitle(&subtitle);
                                }
                                if let Some(WorkflowDispatchInputValue::Boolean(default_value)) =
                                    input.default_value.as_ref()
                                {
                                    row.set_active(*default_value);
                                }
                                inputs_group_for_loader.add(&row);
                                input_fields_for_loader.borrow_mut().push(DispatchInputField {
                                    input,
                                    widget: DispatchInputWidget::Boolean(row),
                                });
                            }
                            _ => {
                                let row = adw::EntryRow::builder().title(title).build();
                                if let Some(subtitle) = subtitle {
                                    row.set_tooltip_text(Some(&subtitle));
                                }
                                if let Some(default_value) = input.default_as_string() {
                                    row.set_text(&default_value);
                                }
                                inputs_group_for_loader.add(&row);
                                input_fields_for_loader.borrow_mut().push(DispatchInputField {
                                    input,
                                    widget: DispatchInputWidget::Text(row),
                                });
                            }
                        }
                    }
                }
                Err(error) => {
                    inputs_placeholder_for_loader
                        .set_text("Workflow inputs could not be loaded.");
                    inputs_placeholder_for_loader.set_visible(true);
                    inputs_group_for_loader.set_visible(false);

                    let toast_overlay = toast_overlay_for_inputs.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let toast = adw::Toast::new(&format!(
                            "✗ Failed to load workflow inputs: {}",
                            error
                        ));
                        toast.set_timeout(5);
                        toast_overlay.add_toast(toast);
                    });
                }
            }

            inputs_loaded_for_inputs.set(true);
            if let Some(btn) = trigger_button_for_inputs.as_ref() {
                let should_enable = branches_loaded_for_inputs.get()
                    && inputs_loaded_for_inputs.get()
                    && branches_available_for_inputs.get();
                btn.set_sensitive(should_enable);
            }

            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_guard = client_for_inputs.lock().clone();
            let inputs_result = client_guard
                .get_workflow_dispatch_inputs(
                    &owner_for_inputs,
                    &repo_for_inputs,
                    &workflow_path_for_inputs,
                    Some(&default_ref_for_inputs),
                )
                .await;

            let _ = inputs_sender.send(
                inputs_result.map_err(|error| format!("Failed to load workflow inputs: {}", error)),
            );
        });

        vbox.append(&branch_box);
        vbox.append(&inputs_placeholder);
        vbox.append(&inputs_group);

        content_area.append(&vbox);

        let client_clone = client.clone();
        let owner_clone = owner.clone();
        let repo_clone = repo.clone();
        let toast_overlay_clone = toast_overlay.clone();
        let workflow_name_clone = workflow_name.clone();
        let workflow_display_rc = Rc::new(workflow_display.clone());
        let expander_clone = expander.clone();
        let run_list_clone = run_list.clone();
        let parent_window_clone = parent_window.clone();
        let workflows_with_active_clone = workflows_with_active_button.clone();
        let run_digests_clone = run_digests.clone();
        let notification_manager_rc = Rc::new(notification_manager.clone());
        let preferences_manager_rc = Rc::new(preferences_manager.clone());
        let workflows_last_loaded_clone = workflows_last_loaded.clone();
        let run_load_service_for_response = run_load_service_for_dialog.clone();

        let run_filters_for_response = run_filters_for_dialog.clone();
        let run_load_service_handle = run_load_service_for_response.clone();
        let input_fields_for_dialog = input_fields.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let selected_branch = branch_dropdown
                    .selected_item()
                    .and_then(|obj| obj.downcast::<gtk::StringObject>().ok())
                    .map(|so| so.string().to_string())
                    .unwrap_or_else(|| {
                        repo_model_for_dialog
                            .default_branch
                            .clone()
                            .unwrap_or_else(|| "main".to_string())
                    });

                let client = client_clone.clone();
                let owner = owner_clone.clone();
                let repo = repo_clone.clone();
                let branch = selected_branch.clone();
                let workflow_id_str = workflow_id.to_string();

                let input_fields = input_fields_for_dialog.borrow();
                let mut inputs = Vec::new();
                let mut input_values = HashMap::new();
                for field in input_fields.iter() {
                    inputs.push(field.input.clone());
                    input_values.insert(field.input.name.clone(), field.current_value());
                }
                drop(input_fields);

                let inputs_payload = match build_dispatch_inputs_payload(&inputs, &input_values) {
                    Ok(payload) => payload,
                    Err(error) => {
                        let toast_overlay = toast_overlay_clone.clone();
                        glib::MainContext::default().spawn_local(async move {
                            let toast = adw::Toast::new(&format!("✗ {}", error));
                            toast.set_timeout(5);
                            toast_overlay.add_toast(toast);
                        });
                        return;
                    }
                };

                let (sender, receiver) = glib::MainContext::default()
                    .channel::<Result<String, String>>(glib::Priority::default());

                crate::runtime_handle().spawn(async move {
                    let client_guard = client.lock().clone();
                    let dispatch_result = client_guard
                        .dispatch_workflow(
                            &owner,
                            &repo,
                            &workflow_id_str,
                            &branch,
                            inputs_payload,
                        )
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
                let owner = owner_clone.clone();
                let repo = repo_clone.clone();
                let expander = expander_clone.clone();
                let run_list_for_reload = run_list_clone.clone();
                let client = client_clone.clone();
                let parent_window = parent_window_clone.clone();
                let job_contexts = job_contexts.clone();
                let workflows_with_active = workflows_with_active_clone.clone();
                let run_digests_for_reload = run_digests_clone.clone();
                let workflow_display_for_closure = workflow_display_rc.clone();
                let notification_manager_for_closure = notification_manager_rc.clone();
                let preferences_manager_for_closure = preferences_manager_rc.clone();
                let repo_model_for_closure = repo_model_for_dialog.clone();
                let workflows_loading_for_receiver = workflows_loading_for_trigger.clone();
                let workflows_last_loaded_for_receiver = workflows_last_loaded_clone.clone();

                let run_filters_for_reload = run_filters_for_response.clone();
                let run_load_service_for_closure = run_load_service_handle.clone();
                receiver.attach(None, move |result| {
                    let workflows_loading_for_runs = workflows_loading_for_receiver.clone();
                    let workflows_loading_for_idle = workflows_loading_for_runs.clone();
                    let workflows_loading_for_follow_up = workflows_loading_for_runs.clone();
                    let workflows_last_loaded_for_runs = workflows_last_loaded_for_receiver.clone();
                    let workflows_last_loaded_for_idle = workflows_last_loaded_for_runs.clone();
                    let workflows_last_loaded_for_follow_up = workflows_last_loaded_for_runs.clone();
                    let workflows_with_active = workflows_with_active.clone();
                    let workflow_display_handle = workflow_display_for_closure.clone();
                    let notification_manager_handle = notification_manager_for_closure.clone();
                    let preferences_manager_handle = preferences_manager_for_closure.clone();
                    let run_load_service_for_idle = run_load_service_for_closure.clone();
                    let run_load_service_for_follow_up = run_load_service_for_closure.clone();
                    match result {
                        Ok(branch_name) => {
                            info!("Workflow triggered successfully on branch: {}", branch_name);

                            let expander = expander.clone();
                            let client_for_reload = client.clone();
                            let owner_for_reload = owner.clone();
                            let repo_for_reload = repo.clone();
                            let parent_window_for_reload = parent_window.clone();
                            let toast_overlay_for_reload = toast_overlay.clone();
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

                            let client_for_idle = client_for_reload.clone();
                            let owner_for_idle = owner_for_reload.clone();
                            let repo_for_idle = repo_for_reload.clone();
                            let repo_model_for_idle = repo_model_for_reload.clone();
                            let workflow_display_for_idle = workflow_display_for_reload.clone();
                            let parent_window_for_idle = parent_window_for_reload.clone();
                            let toast_overlay_for_idle = toast_overlay_for_reload.clone();
                            let job_contexts_for_idle = job_contexts_for_reload.clone();
                            let workflows_with_active_idle =
                                workflows_with_active_for_reload.clone();
                            let run_digests_for_idle = run_digests_for_refresh.clone();
                            let notification_manager_idle =
                                notification_manager_for_reload.clone();
                            let preferences_manager_idle =
                                preferences_manager_for_reload.clone();
                            let run_filters_for_idle = run_filters_for_reload.clone();
                            let run_list_handle = run_list_for_reload.clone();
                            let expander_for_idle = expander.clone();
                            glib::idle_add_local_once(move || {
                                let workflows_with_active = workflows_with_active_idle.clone();
                                if expander_for_idle.is_expanded() {
                                    info!("Refreshing runs in background after workflow trigger");

                                    let preserved_runs: Vec<i64> = run_list_handle
                                        .expanded_run_ids()
                                        .into_iter()
                                        .collect();

                                    let workflow_display_for_runs =
                                        workflow_display_for_idle.clone();
                                    let notification_manager_for_runs =
                                        notification_manager_idle.clone();
                                    let preferences_manager_for_runs =
                                        preferences_manager_idle.clone();

                                    run_load_service_for_idle.request(LoadRunsParams {
                                        client: client_for_idle.clone(),
                                        owner: owner_for_idle.clone(),
                                        repo: repo_for_idle.clone(),
                                        repo_model: repo_model_for_idle.clone(),
                                        workflow_id,
                                        workflow_name: workflow_display_for_runs,
                                        run_list: run_list_handle.clone(),
                                        parent_window: parent_window_for_idle.clone(),
                                        status_badge: None,
                                        expander: expander_for_idle.clone(),
                                        toast_overlay: toast_overlay_for_idle.clone(),
                                        job_contexts: job_contexts_for_idle.clone(),
                                        expanded_run_ids: preserved_runs,
                                        workflows_with_active,
                                        workflows_last_loaded: workflows_last_loaded_for_idle.clone(),
                                        workflows_loading: workflows_loading_for_idle.clone(),
                                        background: true,
                                        run_digests: run_digests_for_idle.clone(),
                                        notification_manager: notification_manager_for_runs,
                                        preferences_manager: preferences_manager_for_runs,
                                        run_filters: run_filters_for_idle.clone(),
                                    });
                                }
                            });

                            let follow_up_params = FollowUpRefreshParams {
                                client: client_for_reload.clone(),
                                owner: owner_for_reload.clone(),
                                repo: repo_for_reload.clone(),
                                repo_model: repo_model_for_reload.clone(),
                                workflow_id,
                                workflow_name: workflow_display_for_reload.clone(),
                                run_list: run_list_for_reload.clone(),
                                parent_window: parent_window_for_reload.clone(),
                                status_badge: None,
                                expander: expander.clone(),
                                toast_overlay: toast_overlay_for_reload.clone(),
                                job_contexts: job_contexts_for_reload.clone(),
                                workflows_with_active: workflows_with_active_for_reload.clone(),
                                workflows_last_loaded: workflows_last_loaded_for_follow_up.clone(),
                                workflows_loading: workflows_loading_for_follow_up.clone(),
                                run_digests: run_digests_for_refresh.clone(),
                                notification_manager: notification_manager_for_reload.clone(),
                                preferences_manager: preferences_manager_for_reload.clone(),
                                run_filters: run_filters_for_reload.clone(),
                                run_load_service: run_load_service_for_follow_up.clone(),
                            };
                            schedule_follow_up_refresh(follow_up_params);

                            let toast_overlay = toast_overlay.clone();
                            let workflow_name = workflow_name.clone();
                            let branch = branch_name.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let toast = adw::Toast::new(&format!(
                                    "✓ Workflow '{}' triggered on branch '{}'. The run will appear once GitHub reports it.",
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

    card
}

pub(crate) fn workflow_row_card<W: IsA<gtk::Widget>>(child: &W) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("workflow-card");
    card.add_css_class("card");
    card.add_css_class("background");
    card.set_margin_start(6);
    card.set_margin_end(6);
    card.set_margin_top(4);
    card.set_margin_bottom(4);
    card.set_hexpand(true);
    card.set_vexpand(false);
    card.set_overflow(gtk::Overflow::Hidden);
    card.append(child);
    card
}

#[derive(Clone)]
struct FollowUpRefreshParams {
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    repo_model: Repo,
    workflow_id: i64,
    workflow_name: String,
    run_list: WorkflowRunListModel,
    parent_window: adw::ApplicationWindow,
    status_badge: Option<gtk::Label>,
    expander: gtk::Expander,
    toast_overlay: adw::ToastOverlay,
    job_contexts: JobContextMap,
    workflows_with_active: Arc<Mutex<HashSet<i64>>>,
    workflows_last_loaded: Arc<Mutex<HashMap<i64, Instant>>>,
    workflows_loading: Arc<Mutex<HashSet<i64>>>,
    run_digests: Arc<Mutex<RunDigestStore>>,
    notification_manager: Option<NotificationManager>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    run_filters: Arc<Mutex<RunFilters>>,
    run_load_service: RunLoadService,
}

fn schedule_follow_up_refresh(params: FollowUpRefreshParams) {
    const SOURCE_KEY: &str = "actioneer-follow-up-refresh";

    let params_rc = Rc::new(params);
    let expander = params_rc.expander.clone();

    if let Some(existing) = unsafe { expander.steal_data::<glib::SourceId>(SOURCE_KEY) } {
        existing.remove();
    }

    // Resolve refresh interval from preferences; fall back to the default auto-refresh interval.
    let prefs_mgr = params_rc.preferences_manager.clone();
    let params_for_async = params_rc.clone();

    glib::MainContext::default().spawn_local(async move {
        let interval_secs = match prefs_mgr {
            Some(manager) => manager.get().await.refresh_interval,
            None => crate::preferences::Preferences::default().refresh_interval,
        };

        // If the user disabled auto-refresh, skip scheduling.
        if interval_secs == 0 {
            info!("Auto-refresh disabled; skipping follow-up refresh timer");
            return;
        }

        let interval_secs = interval_secs.max(1);
        let params_for_timer = params_for_async.clone();
        let params_for_handle = params_for_async;
        let source_id = glib::timeout_add_seconds_local(interval_secs as u32, move || {
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
                expander: params_for_timer.expander.clone(),
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

        unsafe {
            params_for_handle.expander.set_data(SOURCE_KEY, source_id);
        }
    });
}
