use crate::api::models::{Repo, Workflow};
use crate::api::{GitHubClient, GitHubError};
use crate::cache::DataCache;
use crate::favorites::FavoritesManager;
use crate::notifications::NotificationManager;
use crate::preferences::{PreferencesManager, RunFilterPreferences};
use crate::ui::utils::{MainContextChannelExt, create_detail_clamp};
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tracing::{error, info, warn};

mod filter_controls;
mod helpers;
use filter_controls::{FilterChips, FilterControls};
use helpers::{
    JobContextMap, LoadRunsParams, RunDigestStore, WorkflowRowContext, WorkflowRowSettings,
    create_workflow_expander_row, current_job_context_run_ids, load_workflow_runs,
    refresh_jobs_for_workflows, take_job_context_run_ids,
};

#[derive(Clone)]
pub struct RepoDetailPane {
    parent: adw::ApplicationWindow,
    repo: Repo,
    client: Arc<Mutex<GitHubClient>>,
    workflows: Arc<Mutex<Vec<Workflow>>>,
    favorites_manager: Option<Arc<FavoritesManager>>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    cache: Arc<DataCache>,
    favorites: Arc<Mutex<HashSet<i64>>>,
    favorite_button: gtk::ToggleButton,
    refresh_button: gtk::Button,
    buttons_box: gtk::Box,
    filter_controls: gtk::Box,
    list_box: gtk::ListBox,
    root: gtk::Box,
    toast_overlay: adw::ToastOverlay,
    filter_chips: FilterChips,
    run_filters: Arc<Mutex<RunFilters>>,
    filter_guard: Rc<Cell<bool>>,
    loading: Arc<Mutex<bool>>, // Guard against re-entrant loads
    auto_refresh_source: Arc<Mutex<Option<glib::SourceId>>>, // Auto-refresh timer
    workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>, // Track workflows needing refresh
    job_contexts: JobContextMap,
    run_digests: Arc<Mutex<RunDigestStore>>,
    notification_manager: Option<NotificationManager>,
}

#[derive(Clone)]
pub struct RepoDetailDeps {
    pub favorites_manager: Option<Arc<FavoritesManager>>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub cache: Arc<DataCache>,
    pub favorites: Arc<Mutex<HashSet<i64>>>,
    pub notification_manager: Option<NotificationManager>,
}

#[derive(Clone)]
struct WorkflowListContext {
    list_box: gtk::ListBox,
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    repo_model: Repo,
    parent_window: adw::ApplicationWindow,
    cache: Arc<DataCache>,
    toast_overlay: adw::ToastOverlay,
    job_contexts: JobContextMap,
    workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>,
    run_digests: Arc<Mutex<RunDigestStore>>,
    notification_manager: Option<NotificationManager>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    run_filters: Arc<Mutex<RunFilters>>,
}

#[derive(Debug, Clone)]
pub(crate) struct RunFilters {
    pub include_success: bool,
    pub include_failed: bool,
    pub include_running: bool,
}

impl Default for RunFilters {
    fn default() -> Self {
        Self {
            include_success: true,
            include_failed: true,
            include_running: true,
        }
    }
}

impl From<RunFilterPreferences> for RunFilters {
    fn from(prefs: RunFilterPreferences) -> Self {
        Self {
            include_success: prefs.show_success,
            include_failed: prefs.show_failed,
            include_running: prefs.show_running,
        }
    }
}

impl From<RunFilters> for RunFilterPreferences {
    fn from(filters: RunFilters) -> Self {
        Self {
            show_success: filters.include_success,
            show_failed: filters.include_failed,
            show_running: filters.include_running,
            default_branch_only: false,
        }
    }
}

#[derive(Copy, Clone)]
enum FilterKind {
    Success,
    Failed,
    Running,
}

impl RepoDetailPane {
    pub fn new(
        parent: adw::ApplicationWindow,
        repo: Repo,
        client: Arc<Mutex<GitHubClient>>,
        deps: RepoDetailDeps,
    ) -> Self {
        info!("Creating RepoDetailPane for: {}", repo.full_name);
        let workflows = Arc::new(Mutex::new(Vec::new()));
        let job_contexts = Rc::new(RefCell::new(HashMap::new()));

        let favorite_button = gtk::ToggleButton::new();
        favorite_button.set_icon_name("emblem-favorite-symbolic");
        favorite_button.add_css_class("flat");
        favorite_button.set_tooltip_text(Some("Toggle favorite"));

        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Refresh workflows"));
        refresh_button.add_css_class("flat");

        let buttons_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons_box.set_valign(gtk::Align::Center);

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.set_margin_top(12);
        list_box.set_margin_bottom(12);
        list_box.set_margin_start(12);
        list_box.set_margin_end(12);
        list_box.set_valign(gtk::Align::Fill);
        list_box.set_vexpand(true);
        // Create ToastOverlay to wrap the content for showing feedback
        let toast_overlay = adw::ToastOverlay::new();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);

        let filter_controls = FilterControls::new();
        let filter_chips = filter_controls.chips.clone();
        let filter_controls_widget = filter_controls.widget();
        let run_digests = Arc::new(Mutex::new(HashMap::new()));
        let run_filters = Arc::new(Mutex::new(RunFilters::default()));
        let filter_guard = Rc::new(Cell::new(false));
        let notification_manager = deps
            .notification_manager
            .or_else(|| Some(NotificationManager::new("me.spaceinbox.actioneer")));

        let pane = Self {
            parent: parent.clone(),
            repo: repo.clone(),
            client: client.clone(),
            workflows: workflows.clone(),
            favorites_manager: deps.favorites_manager.clone(),
            preferences_manager: deps.preferences_manager.clone(),
            cache: deps.cache.clone(),
            favorites: deps.favorites.clone(),
            favorite_button: favorite_button.clone(),
            refresh_button: refresh_button.clone(),
            buttons_box: buttons_box.clone(),
            filter_controls: filter_controls_widget.clone(),
            list_box: list_box.clone(),
            root: root.clone(),
            toast_overlay: toast_overlay.clone(),
            filter_chips: filter_chips.clone(),
            run_filters: run_filters.clone(),
            filter_guard: filter_guard.clone(),
            loading: Arc::new(Mutex::new(false)),
            auto_refresh_source: Arc::new(Mutex::new(None)),
            workflows_with_active_runs: Arc::new(Mutex::new(HashSet::new())),
            job_contexts: job_contexts.clone(),
            run_digests: run_digests.clone(),
            notification_manager: notification_manager.clone(),
        };

        pane.build_ui();
        pane.connect_filter_chips();
        pane.restore_run_filter_preferences();
        pane.setup_favorite_button();
        pane.observe_favorites();
        pane.load_workflows();
        pane.start_auto_refresh();
        pane
    }

    pub fn widget(&self) -> gtk::Widget {
        self.toast_overlay.clone().upcast::<gtk::Widget>()
    }

    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    fn workflow_list_context(&self) -> WorkflowListContext {
        WorkflowListContext {
            list_box: self.list_box.clone(),
            client: self.client.clone(),
            owner: self.repo.owner.login.clone(),
            repo: self.repo.name.clone(),
            repo_model: self.repo.clone(),
            parent_window: self.parent.clone(),
            cache: self.cache.clone(),
            toast_overlay: self.toast_overlay.clone(),
            job_contexts: self.job_contexts.clone(),
            workflows_with_active_runs: self.workflows_with_active_runs.clone(),
            run_digests: self.run_digests.clone(),
            notification_manager: self.notification_manager.clone(),
            preferences_manager: self.preferences_manager.clone(),
            run_filters: self.run_filters.clone(),
        }
    }

    fn build_ui(&self) {
        // Header section with repo info, favorite and refresh buttons
        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_box.set_margin_top(24);
        header_box.set_margin_bottom(12);
        header_box.set_margin_start(24);
        header_box.set_margin_end(24);

        // Left side: repo info
        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        let repo_label = gtk::Label::new(Some(&self.repo.full_name));
        repo_label.add_css_class("title-2");
        repo_label.set_halign(gtk::Align::Start);
        info_box.append(&repo_label);

        if self.repo.is_private {
            let private_label = gtk::Label::new(Some("Private Repository"));
            private_label.add_css_class("dim-label");
            private_label.add_css_class("caption");
            private_label.set_halign(gtk::Align::Start);
            info_box.append(&private_label);
        }

        header_box.append(&info_box);

        // Right side: chips + buttons
        let buttons_box = self.buttons_box.clone();
        buttons_box.set_valign(gtk::Align::Center);
        buttons_box.set_halign(gtk::Align::End);
        buttons_box.set_spacing(6);

        let chips_row = self.filter_controls.clone();
        buttons_box.append(&chips_row);

        let refresh_button = self.refresh_button.clone();
        refresh_button.set_valign(gtk::Align::Center);
        buttons_box.append(&refresh_button);

        let favorite_button = self.favorite_button.clone();
        favorite_button.set_valign(gtk::Align::Center);
        buttons_box.append(&favorite_button);

        header_box.append(&buttons_box);

        self.root.append(&header_box);

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.set_margin_start(12);
        separator.set_margin_end(12);
        self.root.append(&separator);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        scrolled.set_propagate_natural_height(false);
        scrolled.set_child(Some(&self.list_box));

        let clamp = create_detail_clamp(&scrolled);

        self.root.append(&clamp);

        // Set the root as the child of toast_overlay so toasts can be shown
        self.toast_overlay.set_child(Some(&self.root));

        self.connect_refresh_button(&refresh_button);
        self.connect_workflow_selected();
    }

    fn connect_filter_chips(&self) {
        let chips = self.filter_chips.clone();
        self.attach_filter_chip_handler(&chips.success, FilterKind::Success);
        self.attach_filter_chip_handler(&chips.failed, FilterKind::Failed);
        self.attach_filter_chip_handler(&chips.running, FilterKind::Running);
    }

    fn attach_filter_chip_handler(&self, button: &gtk::ToggleButton, kind: FilterKind) {
        let pane = self.clone();
        button.connect_toggled(move |btn| {
            pane.on_filter_chip_toggled(kind, btn.is_active());
        });
    }

    fn on_filter_chip_toggled(&self, kind: FilterKind, active: bool) {
        if self.filter_guard.get() {
            return;
        }

        {
            let mut filters = self.run_filters.lock();
            match kind {
                FilterKind::Success => filters.include_success = active,
                FilterKind::Failed => filters.include_failed = active,
                FilterKind::Running => filters.include_running = active,
            }
        }

        self.persist_run_filters();
        self.refresh_visible_runs_with_filters();
    }

    fn persist_run_filters(&self) {
        if let Some(manager) = &self.preferences_manager {
            let manager = manager.clone();
            let filters: RunFilterPreferences = self.run_filters.lock().clone().into();
            crate::runtime_handle().spawn(async move {
                if let Err(err) = manager.set_run_filters(filters).await {
                    warn!("Failed to persist run filters: {}", err);
                }
            });
        }
    }

    fn restore_run_filter_preferences(&self) {
        if let Some(manager) = &self.preferences_manager {
            let (sender, receiver) =
                glib::MainContext::default().channel::<RunFilters>(glib::Priority::default());
            let manager = manager.clone();
            crate::runtime_handle().spawn(async move {
                let prefs = manager.get().await;
                let _ = sender.send(RunFilters::from(prefs.run_filters));
            });

            let pane = self.clone();
            receiver.attach(None, move |filters| {
                pane.apply_saved_filters(filters);
                pane.refresh_visible_runs_with_filters();
                glib::ControlFlow::Break
            });
        } else {
            self.apply_saved_filters(RunFilters::default());
            self.refresh_visible_runs_with_filters();
        }
    }

    fn apply_saved_filters(&self, filters: RunFilters) {
        let normalized = filters.clone();

        {
            let mut guard = self.run_filters.lock();
            *guard = normalized.clone();
        }

        self.filter_guard.set(true);
        self.filter_chips
            .success
            .set_active(normalized.include_success);
        self.filter_chips
            .failed
            .set_active(normalized.include_failed);
        self.filter_chips
            .running
            .set_active(normalized.include_running);
        self.filter_guard.set(false);
    }

    fn refresh_visible_runs_with_filters(&self) {
        let context = self.workflow_list_context();
        let run_filters_arc = self.run_filters.clone();
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

        let mut child = self.list_box.first_child();
        while let Some(widget) = child.as_ref() {
            let next = widget.next_sibling();

            if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>()
                && let Some(row_child) = row.child()
                && let Some(box_widget) = row_child.downcast_ref::<gtk::Box>()
            {
                let mut inner = box_widget.first_child();
                while let Some(expander_widget) = inner.as_ref() {
                    let next_inner = expander_widget.next_sibling();
                    if let Some(expander) = expander_widget.downcast_ref::<gtk::Expander>()
                        && expander.is_expanded()
                    {
                        if let Some(workflow_id_ptr) =
                            unsafe { expander.data::<i64>("actioneer-workflow-id") }
                        {
                            let workflow_id = unsafe { *workflow_id_ptr.as_ref() };
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

                                load_workflow_runs(LoadRunsParams {
                                    client: context.client.clone(),
                                    owner: owner.clone(),
                                    repo: repo.clone(),
                                    repo_model: repo_model.clone(),
                                    workflow_id,
                                    workflow_name: workflow_label,
                                    runs_box,
                                    parent_window: parent_window.clone(),
                                    status_badge,
                                    expander: expander.clone(),
                                    cache: cache.clone(),
                                    toast_overlay: toast_overlay.clone(),
                                    bypass_cache: false,
                                    job_contexts: job_contexts.clone(),
                                    expanded_run_ids: preserved_runs,
                                    workflows_with_active: workflows_with_active.clone(),
                                    background: false,
                                    run_digests: run_digests.clone(),
                                    notification_manager: notification_manager.clone(),
                                    preferences_manager: preferences_manager.clone(),
                                    run_filters: run_filters_arc.clone(),
                                });
                            }
                        }
                    }
                    inner = next_inner;
                }
            }

            child = next;
        }
    }

    fn setup_favorite_button(&self) {
        let button = self.favorite_button.clone();

        if let Some(manager) = &self.favorites_manager {
            let repo_id = self.repo.id;
            let manager_for_toggle = manager.clone();
            let favorites_state = self.favorites.clone();

            button.connect_toggled(move |button| {
                let is_active = button.is_active();
                update_detail_favorite_button(button, is_active);

                let manager = manager_for_toggle.clone();
                let favorites_state = favorites_state.clone();
                let button_clone = button.clone();
                let (sender, receiver) = glib::MainContext::default()
                    .channel::<Result<(), anyhow::Error>>(glib::Priority::default());

                receiver.attach(None, move |result| {
                    match result {
                        Ok(()) => {
                            let mut favorites = favorites_state.lock();
                            if is_active {
                                favorites.insert(repo_id);
                            } else {
                                favorites.remove(&repo_id);
                            }
                        }
                        Err(err) => {
                            warn!("Failed to update favorite {}: {}", repo_id, err);
                            let revert_state = !is_active;
                            button_clone.set_active(revert_state);
                            update_detail_favorite_button(&button_clone, revert_state);
                        }
                    }

                    glib::ControlFlow::Break
                });

                crate::runtime_handle().spawn(async move {
                    let outcome = if is_active {
                        manager.add_favorite(repo_id).await
                    } else {
                        manager.remove_favorite(repo_id).await
                    };

                    let _ = sender.send(outcome);
                });
            });
        } else {
            button.set_sensitive(false);
            button.set_tooltip_text(Some("Favorites unavailable"));
        }
    }

    fn observe_favorites(&self) {
        if let Some(manager) = &self.favorites_manager {
            let receiver = manager.subscribe();
            let button = self.favorite_button.clone();
            let repo_id = self.repo.id;

            let (sender, receiver_channel) =
                glib::MainContext::default().channel::<bool>(glib::Priority::default());

            receiver_channel.attach(None, move |is_favorite| {
                if button.is_active() != is_favorite {
                    button.set_active(is_favorite);
                }
                update_detail_favorite_button(&button, is_favorite);

                glib::ControlFlow::Continue
            });

            crate::runtime_handle().spawn(async move {
                let mut receiver_local = receiver;

                if sender
                    .send(receiver_local.borrow().contains(&repo_id))
                    .is_err()
                {
                    return;
                }

                loop {
                    if receiver_local.changed().await.is_err() {
                        break;
                    }

                    let is_favorite = receiver_local.borrow().contains(&repo_id);
                    if sender.send(is_favorite).is_err() {
                        break;
                    }
                }
            });
        } else {
            update_detail_favorite_button(&self.favorite_button, false);
            self.favorite_button.set_sensitive(false);
        }
    }

    fn load_workflows(&self) {
        // Guard against re-entrant calls
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

        // Show loading spinner
        self.show_loading(true);
        let callback_refs = self.clone_for_callbacks();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Vec<Workflow>, GitHubError>>(glib::Priority::default());

        // Clone for spawn closure
        let client_for_spawn = client.clone();
        let owner_for_spawn = owner.clone();
        let repo_name_for_spawn = repo_name.clone();
        let cache_for_spawn = cache.clone();
        let cache_for_ui = cache.clone();

        let run_filters_for_ui = run_filters.clone();
        receiver.attach(None, move |result| {
            // Hide loading spinner
            callback_refs.show_loading(false);

            // Clear loading flag
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

                    // Store workflows in cache
                    let cache_store = cache.clone();
                    let cache_key = format!("{}/{}", owner, repo_name);
                    let wf_list_cache = wf_list.clone();
                    crate::runtime_handle().spawn(async move {
                        cache_store.store_workflows(wf_list_cache, &cache_key).await;
                    });

                    update_workflows_list(&ui_context, &wf_list);
                }
                Err(e) => {
                    error!("Failed to load workflows: {}", e);
                    *workflows.lock() = Vec::new();
                    update_workflows_list(&ui_context, &[]);
                    let message = format!("Failed to load workflows: {}", e);
                    let toast_overlay = toast_overlay.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let toast = adw::Toast::new(&message);
                        toast.set_timeout(5);
                        toast_overlay.add_toast(toast);
                    });
                }
            }

            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let cache_key = format!("{}/{}", owner_for_spawn, repo_name_for_spawn);

            // Try cache first
            if let Some(cached_workflows) = cache_for_spawn.workflows(&cache_key).await {
                info!("Using cached workflows for {}", cache_key);
                let _ = sender.send(Ok(cached_workflows));
                return;
            }

            // Cache miss - fetch from API
            let client_clone = client_for_spawn.lock().clone();
            let result =
                fetch_workflows(&client_clone, &owner_for_spawn, &repo_name_for_spawn).await;
            let _ = sender.send(result);
        });
    }

    pub fn refresh_workflows_silent(&self) {
        // Guard against re-entrant calls
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

        // Clone for spawn
        let client_for_spawn = client.clone();
        let owner_for_spawn = owner.clone();
        let repo_name_for_spawn = repo_name.clone();

        let run_filters_for_ui = run_filters.clone();
        receiver.attach(None, move |result| {
            let run_digests = run_digests.clone();
            let notification_manager_handle = notification_manager.clone();
            let preferences_manager_handle = preferences_manager.clone();
            let repo_model_for_ui = repo_model.clone();
            // Clear loading flag
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
                    // Only update if there are changes (ETag will prevent unnecessary updates)
                    let current = workflows.lock().clone();
                    if workflows_differ(&current, &wf_list) {
                        info!("Silent refresh detected workflow changes");
                        *workflows.lock() = wf_list.clone();
                        update_workflows_list(&ui_context, &wf_list);
                    }
                }
                Err(e) => {
                    // Silent refresh failures are logged but not shown to user
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

    fn connect_refresh_button(&self, button: &gtk::Button) {
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
            // Guard against re-entrant calls
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

            // Show loading spinner
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
                // Hide loading spinner
                callback_refs_for_ui.show_loading(false);

                // Clear loading flag
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

                        update_workflows_list(&ui_context, &wf_list);
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

    fn connect_workflow_selected(&self) {
        // Workflows are now expanded in-place, no need to open a window
        // The row activation will be handled by the expander widget
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
                // Hide refresh button and add spinner
                refresh_button.set_visible(false);

                // Remove any existing spinner first
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
                // Remove spinner and show refresh button
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

    /// Start auto-refresh timer for active runs
    fn start_auto_refresh(&self) {
        // Get refresh interval from preferences (default 5 seconds)
        let refresh_interval_secs = if let Some(prefs_mgr) = &self.preferences_manager {
            // Try to get current preferences
            let handle = crate::runtime_handle().clone();
            let prefs_mgr = prefs_mgr.clone();

            // Spawn a task to get preferences (it's async)
            handle.spawn(async move { prefs_mgr.get().await.refresh_interval });

            // For now, use default while we wait
            5u64
        } else {
            5u64
        };

        // Don't auto-refresh if interval is 0 (disabled)
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

        info!(
            "Starting auto-refresh timer with interval: {} seconds",
            refresh_interval_secs
        );

        // Schedule periodic refresh
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
                run_filters: list_context.run_filters.clone(),
            };

            Self::refresh_runs_background(&background_context);

            glib::ControlFlow::Continue
        });

        *auto_refresh_source.lock() = Some(source_id);
    }

    /// Refresh runs for all workflows in the background.
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

            if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>() {
                let row_child_opt = row.child();
                if let Some(row_child) = row_child_opt
                    && let Some(box_widget) = row_child.downcast_ref::<gtk::Box>()
                {
                    let mut inner_child = box_widget.first_child();
                    while let Some(widget) = inner_child.as_ref() {
                        let next = widget.next_sibling();

                        if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
                            let (workflow_id_opt, is_active) = {
                                let name = expander.widget_name();
                                let name_str = name.as_str();
                                let is_active = name_str.ends_with("_ACTIVE");
                                let base_name = name_str.trim_end_matches("_ACTIVE");
                                let workflow_id_opt = base_name
                                    .strip_prefix("workflow_")
                                    .and_then(|id_str| id_str.parse::<i64>().ok());
                                (workflow_id_opt, is_active)
                            };

                            if let Some(workflow_id) = workflow_id_opt {
                                if is_active {
                                    observed_active.insert(workflow_id);
                                }

                                if let Some(child_widget) = expander.child()
                                    && let Ok(runs_box) = child_widget.downcast::<gtk::Box>()
                                {
                                    let status_badge = Self::status_badge_for_expander(expander);
                                    let preserved_runs =
                                        current_job_context_run_ids(&job_contexts, workflow_id);
                                    let workflow_label_stored = unsafe {
                                        expander
                                            .data::<String>("actioneer-workflow-name")
                                            .map(|name_ptr| name_ptr.as_ref().clone())
                                    };
                                    let workflow_label = workflow_label_stored
                                        .map(|name| format!("{}/{} • {}", owner, repo, name))
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
            }
            child = next_sibling;
        }

        refresh_jobs_for_workflows(&job_contexts, &observed_active);

        *workflows_with_active.lock() = observed_active;
    }

    fn status_badge_for_expander(expander: &gtk::Expander) -> Option<gtk::Label> {
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
}

// Helper struct for callback closures
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

                // Remove any existing spinner first
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
                // Remove all spinners
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

fn update_workflows_list(context: &WorkflowListContext, workflows: &[Workflow]) {
    let list_box = context.list_box.clone();

    // First, collect which workflows are currently expanded
    let mut expanded_ids = HashSet::new();
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

                if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
                    && expander.is_expanded()
                {
                    let name = expander.widget_name();
                    if let Some(id) = name
                        .as_str()
                        .strip_prefix("workflow_")
                        .and_then(|id_str| id_str.parse::<i64>().ok())
                    {
                        info!("Preserving expansion for workflow ID {}", id);
                        expanded_ids.insert(id);
                    }
                }

                inner_child = next;
            }
        }
        child = next_sibling;
    }

    info!("💾 Preserved {} expanded workflow(s)", expanded_ids.len());

    // Remove any job refresh contexts for workflows that are no longer visible
    let visible_workflows: HashSet<i64> = workflows.iter().map(|w| w.id).collect();
    {
        let mut contexts = context.job_contexts.borrow_mut();
        contexts.retain(|_, ctx| visible_workflows.contains(&ctx.workflow_id()));
    }
    {
        let mut active = context.workflows_with_active_runs.lock();
        active.retain(|id| visible_workflows.contains(id));
    }
    {
        let mut digests = context.run_digests.lock();
        digests.retain(|workflow_id, _| visible_workflows.contains(workflow_id));
    }

    // Clear the list
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if workflows.is_empty() {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);

        let placeholder = gtk::Label::new(Some("No workflows found."));
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(24);
        placeholder.set_margin_bottom(24);
        placeholder.set_margin_start(12);
        placeholder.set_margin_end(12);

        row.set_child(Some(&placeholder));
        list_box.append(&row);
        return;
    }

    let base_row_context = WorkflowRowContext {
        client: context.client.clone(),
        owner: context.owner.clone(),
        repo: context.repo.clone(),
        repo_model: context.repo_model.clone(),
        parent_window: context.parent_window.clone(),
        cache: context.cache.clone(),
        toast_overlay: context.toast_overlay.clone(),
        job_contexts: context.job_contexts.clone(),
        workflows_with_active_runs: context.workflows_with_active_runs.clone(),
        run_digests: context.run_digests.clone(),
        notification_manager: context.notification_manager.clone(),
        preferences_manager: context.preferences_manager.clone(),
        run_filters: context.run_filters.clone(),
    };

    for workflow in workflows {
        let should_expand = expanded_ids.contains(&workflow.id);
        let preserved_run_ids = take_job_context_run_ids(&context.job_contexts, workflow.id);

        let row_context = base_row_context.clone();
        let settings = WorkflowRowSettings {
            should_expand,
            initial_expanded_run_ids: preserved_run_ids,
        };

        let expander_row = create_workflow_expander_row(workflow, &row_context, settings);
        list_box.append(&expander_row);
    }
}

fn update_detail_favorite_button(button: &gtk::ToggleButton, is_active: bool) {
    if is_active {
        button.remove_css_class("flat");
        button.add_css_class("suggested-action");
        button.set_opacity(1.0);
    } else {
        button.remove_css_class("suggested-action");
        button.add_css_class("flat");
        button.set_opacity(0.5);
    }
}

fn workflows_differ(a: &[Workflow], b: &[Workflow]) -> bool {
    if a.len() != b.len() {
        return true;
    }

    let a_ids: std::collections::HashSet<_> = a.iter().map(|w| w.id).collect();
    let b_ids: std::collections::HashSet<_> = b.iter().map(|w| w.id).collect();

    a_ids != b_ids
}
