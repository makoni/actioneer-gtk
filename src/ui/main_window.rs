use super::WelcomeScreen;
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::models::{RateLimitInfo, Repo};
use crate::services::app_services::AppServices;
use crate::services::cache::DataCache;
use crate::services::favorites::FavoritesManager;
use crate::services::gateway::GitHubGateway;
use crate::services::notifications::NotificationManager;
use crate::services::preferences::{Preferences, PreferencesManager, ThemePreference};
use crate::ui::detail_view::RepoDetailPane;
use crate::ui::main_window::layout::create_detail_clamp;
use gio::Menu;
use gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

mod auth;
mod demo_mode;
mod header_controls;
mod loaders;
mod refresh;
mod repo_list;
mod selection;
mod sidebar_panel;
mod window_actions;
use header_controls::HeaderControls;
use sidebar_panel::SidebarPanel;

// Import refactored modules
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};

const REPO_STATUS_TTL: Duration = Duration::from_secs(300);
const MIN_WINDOW_WIDTH: i32 = 860;
const MIN_WINDOW_HEIGHT: i32 = 520;
const HOMEPAGE_URL: &str = "https://github.com/makoni/actioneer-gtk";
const ISSUE_URL: &str = "https://github.com/makoni/actioneer-gtk/issues";
const DONATION_URL: &str = "https://nowpayments.io/donation/makoni";

#[derive(Clone)]
pub struct MainWindow {
    window: adw::ApplicationWindow,
    client: Arc<Mutex<Option<GitHubGateway>>>,
    repos: Arc<Mutex<Vec<Repo>>>,
    sidebar_panel: SidebarPanel,
    repo_store: gio::ListStore,
    repo_filter_model: gtk::FilterListModel,
    repo_selection: gtk::SingleSelection,
    search_entry: gtk::SearchEntry,
    rate_limit_label: gtk::Label,
    refresh_button: gtk::Button,
    header_bar: adw::HeaderBar,
    header_spinner: Rc<RefCell<Option<gtk::Spinner>>>,
    favorites_manager: Option<Arc<FavoritesManager>>,
    favorites: Arc<Mutex<HashSet<i64>>>,
    cache: Arc<DataCache>,
    actions_states: Arc<Mutex<HashMap<i64, RepoActionsState>>>,
    actions_checked_at: Arc<Mutex<HashMap<i64, Instant>>>,
    workflow_counts: Arc<Mutex<HashMap<i64, WorkflowStatusCounts>>>,
    preferences_manager: Option<Arc<PreferencesManager>>,
    selected_repo_id: Arc<Mutex<Option<i64>>>,
    rate_limit_info: Arc<Mutex<Option<RateLimitInfo>>>,
    detail_status_page: adw::StatusPage,
    detail_stack: gtk::Stack,
    root_stack: gtk::Stack,
    active_detail: Rc<RefCell<Option<RepoDetailPane>>>,
    background_refresh_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    handling_selection: Arc<Mutex<bool>>,
    notification_manager: Option<NotificationManager>,
    demo_mode: Arc<Mutex<bool>>,
    /// Kept so the window can rebuild itself on a language change without
    /// constructing a second set of real services.
    services: AppServices,
}

impl MainWindow {
    pub fn new(app: &adw::Application, services: AppServices, start_demo_mode: bool) -> Self {
        crate::ui::style::apply_text_direction_for_language();
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Actioneer")
            .default_width(1000)
            .default_height(700)
            .build();
        window.set_resizable(true);
        window.set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);

        Self::ensure_app_focus_action(app, &window);

        // Everything below comes from the composition root now; the window
        // constructs no service of its own.
        let client = services.gateway.clone();
        let cache = services.cache.clone();
        let repos = Arc::new(Mutex::new(Vec::new()));

        let sidebar_panel = SidebarPanel::new();
        let repo_store = sidebar_panel.repo_store();
        let repo_filter_model = sidebar_panel.filter_model();
        let repo_selection = sidebar_panel.selection();
        let search_entry = sidebar_panel.search_entry();

        let header_controls = HeaderControls::new();
        let header_bar = header_controls.header_bar();
        let refresh_button = header_controls.refresh_button();
        let rate_limit_label = header_controls.rate_limit_label();

        let detail_status_page = adw::StatusPage::builder()
            .title(tr("Select a repository"))
            .description(tr(
                "Choose a repository from the sidebar to browse its workflows and runs here.",
            ))
            .icon_name("system-search-symbolic")
            .build();
        let detail_stack = gtk::Stack::new();
        detail_stack.add_named(&detail_status_page, Some("placeholder"));
        detail_stack.set_visible_child_name("placeholder");
        detail_stack.set_vexpand(true);
        detail_stack.set_hexpand(true);
        let root_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(200)
            .build();
        root_stack.set_hexpand(true);
        root_stack.set_vexpand(true);
        let active_detail: Rc<RefCell<Option<RepoDetailPane>>> = Rc::new(RefCell::new(None));
        let favorites_manager = services.favorites.clone();
        let preferences_manager = services.preferences.clone();

        let favorites = Arc::new(Mutex::new(HashSet::new()));
        let actions_states = Arc::new(Mutex::new(HashMap::new()));
        let actions_checked_at = Arc::new(Mutex::new(HashMap::new()));
        let workflow_counts = Arc::new(Mutex::new(HashMap::new()));
        let selected_repo_id = Arc::new(Mutex::new(None));
        let rate_limit_info = Arc::new(Mutex::new(None));
        let background_refresh_task = Arc::new(Mutex::new(None));
        let handling_selection = Arc::new(Mutex::new(false));
        let header_spinner = Rc::new(RefCell::new(None));
        let notification_manager = services.notifications.clone();
        let demo_mode = Arc::new(Mutex::new(false));

        let main_window = Self {
            window: window.clone(),
            client: client.clone(),
            repos: repos.clone(),
            sidebar_panel: sidebar_panel.clone(),
            repo_store: repo_store.clone(),
            repo_filter_model: repo_filter_model.clone(),
            repo_selection: repo_selection.clone(),
            search_entry: search_entry.clone(),
            rate_limit_label: rate_limit_label.clone(),
            refresh_button: refresh_button.clone(),
            header_bar: header_bar.clone(),
            header_spinner: header_spinner.clone(),
            favorites_manager: favorites_manager.clone(),
            favorites: favorites.clone(),
            cache: cache.clone(),
            actions_states: actions_states.clone(),
            actions_checked_at: actions_checked_at.clone(),
            workflow_counts: workflow_counts.clone(),
            preferences_manager: preferences_manager.clone(),
            selected_repo_id: selected_repo_id.clone(),
            rate_limit_info: rate_limit_info.clone(),
            detail_status_page: detail_status_page.clone(),
            detail_stack: detail_stack.clone(),
            root_stack: root_stack.clone(),
            active_detail: active_detail.clone(),
            background_refresh_task: background_refresh_task.clone(),
            handling_selection: handling_selection.clone(),
            notification_manager: notification_manager.clone(),
            demo_mode: demo_mode.clone(),
            services: services.clone(),
        };

        main_window.ensure_app_actions(app);
        main_window.ensure_app_accels(app);
        main_window.build_ui();
        main_window.restore_preferences();
        main_window.prime_favorites();
        main_window.observe_favorites();
        main_window.setup_focus_handler();
        if start_demo_mode {
            main_window.enter_demo_mode();
        } else {
            main_window.check_authentication();
        }
        main_window
    }

    fn prime_favorites(&self) {
        if let Some(manager) = &self.favorites_manager {
            let favorites = self.favorites.clone();
            let manager = manager.clone();

            let (sender, receiver) =
                glib::MainContext::default().channel::<HashSet<i64>>(glib::Priority::default());
            let favorites_clone = favorites.clone();

            receiver.attach(None, move |favorite_ids| {
                let mut favorites_guard = favorites_clone.lock();
                *favorites_guard = favorite_ids;
                glib::ControlFlow::Break
            });

            crate::runtime::handle().spawn(async move {
                let favorite_ids = manager.get_all().await;
                let _ = sender.send(favorite_ids);
            });
        }
    }

    fn build_ui(&self) {
        let header = self.header_bar.clone();
        let root_stack = self.root_stack.clone();

        self.setup_header_menu(&header);

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.append(&header);

        let sidebar_clamp = self.sidebar_panel.clamp();

        let detail_status_page = self.detail_status_page.clone();
        detail_status_page.set_vexpand(true);
        detail_status_page.set_hexpand(true);
        detail_status_page.set_margin_top(24);
        detail_status_page.set_margin_bottom(24);
        detail_status_page.set_margin_start(24);
        detail_status_page.set_margin_end(24);

        let detail_stack = self.detail_stack.clone();
        if detail_stack.child_by_name("placeholder").is_none() {
            detail_stack.add_named(&detail_status_page, Some("placeholder"));
        }
        detail_stack.set_visible_child_name("placeholder");

        let split_pane = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .wide_handle(true)
            .start_child(&sidebar_clamp)
            .end_child(&detail_stack)
            .shrink_start_child(false)
            .shrink_end_child(true)
            .build();
        // Keep the sidebar at its natural width and let the detail pane use remaining space.
        split_pane.set_resize_start_child(false);
        split_pane.set_resize_end_child(true);
        split_pane.set_position(360);

        main_box.append(&split_pane);

        root_stack.add_named(&main_box, Some("app"));

        let welcome_screen = WelcomeScreen::new();
        let welcome_widget = welcome_screen.widget();
        let welcome_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();
        welcome_scrolled.set_child(Some(welcome_widget));
        let welcome_container = create_detail_clamp(&welcome_scrolled);
        welcome_container.set_hexpand(true);
        welcome_container.set_vexpand(true);
        root_stack.add_named(&welcome_container, Some("welcome"));
        root_stack.set_visible_child_name("welcome");

        self.window.set_content(Some(&root_stack));

        if let Some(manager) = &self.preferences_manager {
            let manager = manager.clone();
            let window_for_size = self.window.clone();
            window_for_size.connect_close_request(move |win| {
                let width = win.width();
                let height = win.height();
                let manager = manager.clone();
                crate::runtime::handle().spawn(async move {
                    if let Err(err) = manager.set_window_size(width, height).await {
                        warn!("Failed to persist window size: {}", err);
                    }
                });
                glib::Propagation::Proceed
            });
        }

        let this = self.clone();
        welcome_screen.connect_signin(move || {
            this.show_auth_window();
        });

        let demo_this = self.clone();
        welcome_screen.connect_demo(move || {
            demo_this.enter_demo_mode();
        });

        let window_for_quit = self.window.clone();
        welcome_screen.connect_quit(move || match window_for_quit.application() {
            Some(app) => {
                app.quit();
            }
            _ => {
                window_for_quit.close();
            }
        });

        let refresh_button = self.refresh_button.clone();
        self.connect_refresh_button(&refresh_button);
        self.connect_search();
        self.connect_repo_selection();
        self.connect_repo_activation();
    }

    fn restore_preferences(&self) {
        if let Some(manager) = &self.preferences_manager {
            let window = self.window.clone();
            let selected_repo_id = self.selected_repo_id.clone();
            let (sender, receiver) =
                glib::MainContext::default().channel::<Preferences>(glib::Priority::default());

            receiver.attach(None, move |prefs| {
                window.set_default_size(prefs.window_width, prefs.window_height);
                let style_manager = adw::StyleManager::default();
                match prefs.theme_preference {
                    ThemePreference::System => {
                        style_manager.set_color_scheme(adw::ColorScheme::Default);
                    }
                    ThemePreference::Light => {
                        style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
                    }
                    ThemePreference::Dark => {
                        style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
                    }
                }
                {
                    let mut selected = selected_repo_id.lock();
                    *selected = prefs.last_selected_repo_id;
                }
                glib::ControlFlow::Break
            });

            let manager = manager.clone();
            crate::runtime::handle().spawn(async move {
                let prefs = manager.get().await;
                let _ = sender.send(prefs);
            });
        }
    }
    fn initialize_client(&self, token: String) -> bool {
        // The slot transition belongs to the services; the UI reaction below
        // belongs here.
        //
        // The demo flag is cleared *after* the swap succeeds, not before:
        // `authenticate` leaves the slot untouched when it fails, so clearing
        // first would leave `is_demo_mode()` answering false while a
        // `DemoBackend` was still installed — and the focus handler and the
        // refresh paths both branch on that flag.
        if self.services.authenticate(token) {
            *self.demo_mode.lock() = false;
            {
                let mut info_guard = self.rate_limit_info.lock();
                *info_guard = None;
            }

            self.update_rate_limit_display(None);
            self.show_authenticated_ui();
            self.load_repositories();
            true
        } else {
            false
        }
    }

    fn show_authenticated_ui(&self) {
        let stack = self.root_stack.clone();
        glib::idle_add_local_once(move || {
            stack.set_visible_child_name("app");
        });
    }

    fn enter_signed_out_state(&self) {
        info!("Switching to signed-out state");
        self.stop_background_refresh();

        if self.is_demo_mode() {
            // Leaving demo mode is just dropping the demo gateway; the slot is
            // replaced (or cleared) by the caller right below.
            *self.demo_mode.lock() = false;
        }

        {
            let mut handling = self.handling_selection.lock();
            *handling = false;
        }

        {
            let mut selected = self.selected_repo_id.lock();
            *selected = None;
        }

        self.services.sign_out();

        {
            let cache = self.cache.clone();
            crate::runtime::handle().spawn(async move {
                cache.clear_all().await;
            });
        }

        {
            let mut repos_guard = self.repos.lock();
            repos_guard.clear();
        }

        {
            let mut favorites_guard = self.favorites.lock();
            favorites_guard.clear();
        }

        if let Some(manager) = &self.favorites_manager {
            let manager = manager.clone();
            crate::runtime::handle().spawn(async move {
                if let Err(err) = manager.clear_all().await {
                    warn!("Failed to clear favorites during sign-out: {}", err);
                }
            });
        }

        self.actions_states.lock().clear();
        self.actions_checked_at.lock().clear();
        self.workflow_counts.lock().clear();

        {
            let mut info_guard = self.rate_limit_info.lock();
            *info_guard = None;
        }

        self.schedule_repo_list_refresh();
        self.show_detail_placeholder();
        self.update_rate_limit_display(None);
        self.show_header_loading(false);

        if let Some(manager) = &self.preferences_manager {
            let manager = manager.clone();
            crate::runtime::handle().spawn(async move {
                if let Err(err) = manager.set_last_selected_repo(None).await {
                    warn!(
                        "Failed to reset stored repo selection during sign-out: {}",
                        err
                    );
                }
            });
        }

        let selection = self.repo_selection.clone();
        glib::idle_add_local_once(move || {
            selection.unselect_all();
        });

        let search_entry = self.search_entry.clone();
        glib::idle_add_local_once(move || {
            search_entry.set_text("");
        });

        let stack = self.root_stack.clone();
        glib::idle_add_local_once(move || {
            stack.set_visible_child_name("welcome");
        });
    }

    fn observe_favorites(&self) {
        if let Some(manager) = &self.favorites_manager {
            let favorites_state = self.favorites.clone();
            let this = self.clone();

            crate::ui::tasks::favorites_observer::observe_favorites(
                manager.clone(),
                favorites_state,
                move || this.schedule_repo_list_refresh(),
            );
        }
    }

    fn connect_refresh_button(&self, button: &gtk::Button) {
        let client_arc = self.client.clone();
        let this = self.clone();

        button.connect_clicked(move |_| {
            if client_arc.lock().is_some() {
                this.load_repositories();
            } else {
                warn!("Cannot refresh: GitHub client not initialized");
            }
        });
    }

    pub fn trigger_test_notification(&self) {
        if let Some(manager) = &self.notification_manager {
            let manager = manager.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Err(err) = manager
                    .notify_message(
                        tr("Actioneer notification test").as_str(),
                        tr("If you see this, notifications are working.").as_str(),
                    )
                    .await
                {
                    error!(error = %err, "Failed to dispatch test notification");
                }
            });
        } else {
            warn!("Notification manager unavailable; cannot send test notification");
        }
    }

    pub fn present(&self) {
        self.window.present();
        let this = self.clone();
        glib::idle_add_local_once(move || {
            this.show_pending_crash_report_dialog();
        });
    }

    fn reload_window_for_language_change(&self) {
        let Some(app) = self.window.application() else {
            return;
        };
        let Ok(app) = app.downcast::<adw::Application>() else {
            return;
        };

        let selected_repo = *self.selected_repo_id.lock();
        let search_query = self.search_entry.text().to_string();
        let window_width = self.window.width();
        let window_height = self.window.height();
        let was_demo_mode = self.is_demo_mode();

        self.stop_background_refresh();

        let replacement = MainWindow::new(&app, self.services.clone(), was_demo_mode);
        {
            let mut replacement_selected = replacement.selected_repo_id.lock();
            *replacement_selected = selected_repo;
        }
        replacement.search_entry.set_text(&search_query);
        replacement
            .window
            .set_default_size(window_width, window_height);

        if was_demo_mode {
            replacement.enter_demo_mode();
            if let Some(repo_id) = selected_repo {
                *replacement.selected_repo_id.lock() = Some(repo_id);
                replacement.ensure_detail_matches_selection();
            }
        }

        replacement.present();
        // The pane's own handlers hold it back, so dropping this window is not
        // enough to release it: cut them the same way a repo switch does.
        if let Some(detail) = self.active_detail.borrow_mut().take() {
            detail.deactivate();
        }
        self.window.close();
    }
}

mod app_actions;
mod layout;
mod rate_limit;

#[cfg(test)]
mod tests;
