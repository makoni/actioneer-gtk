use crate::api::GitHubClient;
use crate::api::models::{Repo, Workflow};
use crate::favorites::FavoritesManager;
use crate::i18n::tr;
use crate::notifications::NotificationManager;
use crate::preferences::{PreferencesManager, RunFilterPreferences};
use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tracing::info;

mod content;
mod favorite_controls;
mod filter_controls;
mod helpers;
mod run_filters;
mod workflow_list;
mod workflow_refresh;
use favorite_controls::{observe_favorites, setup_favorite_button};
use filter_controls::{FilterChips, FilterControls};
use helpers::{JobContextMap, RunDigestStore, RunLoadService};

#[derive(Clone)]
pub struct RepoDetailPane {
    parent: adw::ApplicationWindow,
    repo: Repo,
    client: Arc<Mutex<GitHubClient>>,
    workflows: Arc<Mutex<Arc<Vec<Workflow>>>>,
    favorites_manager: Option<Arc<FavoritesManager>>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    favorites: Arc<Mutex<HashSet<i64>>>,
    favorite_button: gtk::ToggleButton,
    refresh_button: gtk::Button,
    buttons_box: gtk::Box,
    filter_controls: gtk::Box,
    workflow_view: gtk::ListView,
    workflow_store: gio::ListStore,
    root: gtk::Box,
    toast_overlay: adw::ToastOverlay,
    filter_chips: FilterChips,
    run_filters: Arc<Mutex<RunFilters>>,
    filter_guard: Rc<Cell<bool>>,
    loading: Arc<Mutex<bool>>, // Guard against re-entrant loads
    auto_refresh_source: Arc<Mutex<Option<glib::SourceId>>>, // Auto-refresh timer
    workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>, // Track workflows needing refresh
    workflows_last_loaded: Arc<Mutex<HashMap<i64, std::time::Instant>>>, // Debounce per-workflow loads
    job_contexts: JobContextMap,
    run_digests: Arc<Mutex<RunDigestStore>>,
    notification_manager: Option<NotificationManager>,
    workflows_loading_runs: Arc<Mutex<HashSet<i64>>>, // Track in-flight run loads
    run_load_service: RunLoadService,
}

#[derive(Clone)]
pub struct RepoDetailDeps {
    pub favorites_manager: Option<Arc<FavoritesManager>>,
    pub preferences_manager: Option<Arc<PreferencesManager>>,
    pub favorites: Arc<Mutex<HashSet<i64>>>,
    pub notification_manager: Option<NotificationManager>,
}

#[derive(Clone)]
struct WorkflowListContext {
    store: gio::ListStore,
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    repo_model: Repo,
    parent_window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    job_contexts: JobContextMap,
    workflows_with_active_runs: Arc<Mutex<HashSet<i64>>>,
    workflows_last_loaded: Arc<Mutex<HashMap<i64, std::time::Instant>>>,
    run_digests: Arc<Mutex<RunDigestStore>>,
    notification_manager: Option<NotificationManager>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    workflows_loading_runs: Arc<Mutex<HashSet<i64>>>,
    run_filters: Arc<Mutex<RunFilters>>,
    run_load_service: RunLoadService,
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
        let workflows = Arc::new(Mutex::new(Arc::new(Vec::new())));
        let job_contexts = Rc::new(RefCell::new(HashMap::new()));

        let favorite_button = gtk::ToggleButton::new();
        favorite_button.set_icon_name("emblem-favorite-symbolic");
        favorite_button.add_css_class("flat");
        favorite_button.set_tooltip_text(Some(tr("Toggle favorite").as_str()));

        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some(tr("Refresh workflows").as_str()));
        refresh_button.add_css_class("flat");

        let buttons_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons_box.set_valign(gtk::Align::Center);

        let workflow_store = gio::ListStore::new::<gtk::Widget>();
        let workflow_selection = gtk::NoSelection::new(Some(workflow_store.clone()));
        let workflow_factory = gtk::SignalListItemFactory::new();
        workflow_factory.connect_bind(|_, list_item| {
            let Some(row) = list_item
                .item()
                .and_then(|obj| obj.downcast::<gtk::Widget>().ok())
            else {
                return;
            };
            row.unparent();
            list_item.set_child(Some(&row));
        });
        workflow_factory.connect_unbind(|_, list_item| {
            if let Some(child) = list_item.child() {
                child.unparent();
                list_item.set_child(None::<&gtk::Widget>);
            }
        });
        let workflow_view = gtk::ListView::new(Some(workflow_selection), Some(workflow_factory));
        workflow_view.add_css_class("boxed-list");
        workflow_view.add_css_class("hoverless-list");
        workflow_view.set_single_click_activate(false);
        workflow_view.set_margin_top(12);
        workflow_view.set_margin_bottom(12);
        workflow_view.set_margin_start(12);
        workflow_view.set_margin_end(12);
        workflow_view.set_valign(gtk::Align::Fill);
        workflow_view.set_vexpand(true);
        // Create ToastOverlay to wrap the content for showing feedback
        let toast_overlay = adw::ToastOverlay::new();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled_window.set_propagate_natural_height(true);

        let viewport = gtk::Viewport::builder()
            .scroll_to_focus(false)
            .hexpand(true)
            .vexpand(true)
            .build();
        viewport.set_child(Some(&root));
        scrolled_window.set_child(Some(&viewport));
        toast_overlay.set_child(Some(&scrolled_window));

        let filter_controls = FilterControls::new();
        let filter_chips = filter_controls.chips.clone();
        let filter_controls_widget = filter_controls.widget();
        let run_digests = Arc::new(Mutex::new(HashMap::new()));
        let run_filters = Arc::new(Mutex::new(RunFilters::default()));
        let workflows_last_loaded = Arc::new(Mutex::new(HashMap::new()));
        let workflows_loading_runs = Arc::new(Mutex::new(HashSet::new()));
        let filter_guard = Rc::new(Cell::new(false));
        let notification_manager = deps
            .notification_manager
            .or_else(|| {
                parent
                    .application()
                    .map(|app| NotificationManager::for_application(&app))
            })
            .or_else(|| {
                Some(NotificationManager::new(
                    crate::resolved_app_id().into_owned(),
                ))
            });
        let run_load_service = RunLoadService::new(
            workflows_last_loaded.clone(),
            workflows_loading_runs.clone(),
        );

        let pane = Self {
            parent: parent.clone(),
            repo: repo.clone(),
            client: client.clone(),
            workflows: workflows.clone(),
            favorites_manager: deps.favorites_manager.clone(),
            preferences_manager: deps.preferences_manager.clone(),
            favorites: deps.favorites.clone(),
            favorite_button: favorite_button.clone(),
            refresh_button: refresh_button.clone(),
            buttons_box: buttons_box.clone(),
            filter_controls: filter_controls_widget.clone(),
            workflow_view: workflow_view.clone(),
            workflow_store: workflow_store.clone(),
            root: root.clone(),
            toast_overlay: toast_overlay.clone(),
            filter_chips: filter_chips.clone(),
            run_filters: run_filters.clone(),
            filter_guard: filter_guard.clone(),
            loading: Arc::new(Mutex::new(false)),
            auto_refresh_source: Arc::new(Mutex::new(None)),
            workflows_with_active_runs: Arc::new(Mutex::new(HashSet::new())),
            workflows_loading_runs: workflows_loading_runs.clone(),
            workflows_last_loaded: workflows_last_loaded.clone(),
            job_contexts: job_contexts.clone(),
            run_digests: run_digests.clone(),
            run_load_service: run_load_service.clone(),
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
        self.build_header();
        self.attach_run_list();

        let refresh_button = self.refresh_button.clone();
        self.connect_refresh_button(&refresh_button);
        self.connect_workflow_selected();
    }

    fn build_header(&self) {
        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_box.set_margin_top(24);
        header_box.set_margin_bottom(12);
        header_box.set_margin_start(24);
        header_box.set_margin_end(24);

        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        let repo_label = gtk::Label::new(Some(&self.repo.full_name));
        repo_label.add_css_class("title-2");
        repo_label.set_halign(gtk::Align::Start);
        info_box.append(&repo_label);

        if self.repo.is_private {
            let private_label = gtk::Label::new(Some(tr("Private Repository").as_str()));
            private_label.add_css_class("dim-label");
            private_label.add_css_class("caption");
            private_label.set_halign(gtk::Align::Start);
            info_box.append(&private_label);
        }

        header_box.append(&info_box);

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
    }

    fn connect_workflow_selected(&self) {
        // Workflows are now expanded in-place, no need to open a window
        // The row activation will be handled by the expander widget
    }
}
