use crate::api::GitHubClient;
use crate::api::models::{Repo, Workflow};
use crate::cache::DataCache;
use crate::favorites::FavoritesManager;
use crate::notifications::NotificationManager;
use crate::preferences::{PreferencesManager, RunFilterPreferences};
use crate::ui::utils::create_detail_clamp;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tracing::info;

mod favorite_controls;
mod filter_controls;
mod helpers;
mod run_filters;
mod workflow_list;
use favorite_controls::{observe_favorites, setup_favorite_button};
use filter_controls::{FilterChips, FilterControls};
use helpers::{JobContextMap, RunDigestStore};

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
        setup_favorite_button(
            &pane.favorite_button,
            pane.repo.id,
            pane.favorites_manager.clone(),
            pane.favorites.clone(),
        );
        observe_favorites(
            &pane.favorite_button,
            pane.repo.id,
            pane.favorites_manager.clone(),
        );
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

    fn connect_workflow_selected(&self) {
        // Workflows are now expanded in-place, no need to open a window
        // The row activation will be handled by the expander widget
    }
}
