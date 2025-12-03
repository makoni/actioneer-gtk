use super::WelcomeScreen;
use super::detail_placeholder::schedule_status_page_update;
use super::detail_view::{RepoDetailDeps, RepoDetailPane};
use super::sidebar::{find_label_by_name, row_matches_query};
use crate::api::GitHubClient;
use crate::api::models::{RateLimitInfo, Repo};
use crate::cache::DataCache;
use crate::demo;
use crate::favorites::FavoritesManager;
use crate::notifications::NotificationManager;
use crate::preferences::{Preferences, PreferencesManager};
use crate::storage::TokenStorage;
use crate::ui::auth_window::AuthWindow;
use crate::ui::preferences_window::PreferencesWindow;
use crate::ui::utils::{MainContextChannelExt, update_rate_limit_label};
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
use tracing::{debug, error, info, warn};

mod header_controls;
mod loaders;
mod sidebar_panel;
use header_controls::HeaderControls;
use sidebar_panel::SidebarPanel;

// Import refactored modules
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};

const REPO_STATUS_TTL: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct MainWindow {
    window: adw::ApplicationWindow,
    client: Arc<Mutex<Option<GitHubClient>>>,
    repos: Arc<Mutex<Vec<Repo>>>,
    sidebar_panel: SidebarPanel,
    repo_list: gtk::ListBox,
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
}

impl MainWindow {
    fn is_demo_mode(&self) -> bool {
        *self.demo_mode.lock()
    }

    fn enter_demo_mode(&self) {
        if self.is_demo_mode() {
            info!("Demo mode already active");
            return;
        }

        info!("Entering demo mode with mock data");
        self.stop_background_refresh();

        let repos = demo::enable();
        let rate_info = demo::rate_limit_info();

        match GitHubClient::new(None) {
            Ok(client) => {
                let mut client_guard = self.client.lock();
                *client_guard = Some(client);
            }
            Err(err) => {
                error!("Failed to initialize demo client: {}", err);
                return;
            }
        }

        {
            let mut flag = self.demo_mode.lock();
            *flag = true;
        }

        {
            let mut selected = self.selected_repo_id.lock();
            *selected = None;
        }

        self.favorites.lock().clear();

        self.show_authenticated_ui();
        self.show_header_loading(false);

        self.refresh_repository_view(repos.clone(), rate_info.clone());

        if let Some(first_repo) = repos.first().cloned() {
            let this = self.clone();
            glib::idle_add_local_once(move || {
                this.handle_repo_selection(Some(first_repo));
            });
        } else {
            self.show_detail_placeholder();
        }
    }

    pub fn new(app: &adw::Application) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Actioneer")
            .default_width(1000)
            .default_height(700)
            .build();

        let client = Arc::new(Mutex::new(None));
        let repos = Arc::new(Mutex::new(Vec::new()));

        let sidebar_panel = SidebarPanel::new();
        let repo_list = sidebar_panel.repo_list();
        let search_entry = sidebar_panel.search_entry();

        let header_controls = HeaderControls::new();
        let header_bar = header_controls.header_bar();
        let refresh_button = header_controls.refresh_button();
        let rate_limit_label = header_controls.rate_limit_label();

        let detail_status_page = adw::StatusPage::builder()
            .title("Select a repository")
            .description(
                "Choose a repository from the sidebar to browse its workflows and runs here.",
            )
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
        let favorites_manager = match FavoritesManager::new() {
            Ok(manager) => Some(Arc::new(manager)),
            Err(err) => {
                warn!("Failed to initialize FavoritesManager: {}", err);
                None
            }
        };
        let preferences_manager = match PreferencesManager::new() {
            Ok(manager) => Some(Arc::new(manager)),
            Err(err) => {
                warn!("Failed to initialize PreferencesManager: {}", err);
                None
            }
        };

        let favorites = Arc::new(Mutex::new(HashSet::new()));
        let actions_states = Arc::new(Mutex::new(HashMap::new()));
        let actions_checked_at = Arc::new(Mutex::new(HashMap::new()));
        let workflow_counts = Arc::new(Mutex::new(HashMap::new()));
        let selected_repo_id = Arc::new(Mutex::new(None));
        let rate_limit_info = Arc::new(Mutex::new(None));
        let background_refresh_task = Arc::new(Mutex::new(None));
        let handling_selection = Arc::new(Mutex::new(false));
        let header_spinner = Rc::new(RefCell::new(None));
        let notification_manager = Some(NotificationManager::new("me.spaceinbox.actioneer"));
        let demo_mode = Arc::new(Mutex::new(false));

        let main_window = Self {
            window: window.clone(),
            client: client.clone(),
            repos: repos.clone(),
            sidebar_panel: sidebar_panel.clone(),
            repo_list: repo_list.clone(),
            search_entry: search_entry.clone(),
            rate_limit_label: rate_limit_label.clone(),
            refresh_button: refresh_button.clone(),
            header_bar: header_bar.clone(),
            header_spinner: header_spinner.clone(),
            favorites_manager: favorites_manager.clone(),
            favorites: favorites.clone(),
            cache: Arc::new(DataCache::new()),
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
        };

        main_window.build_ui();
        main_window.restore_preferences();
        main_window.prime_favorites();
        main_window.observe_favorites();
        main_window.setup_focus_handler();
        main_window.check_authentication();
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

            crate::runtime_handle().spawn(async move {
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
            .shrink_end_child(false)
            .build();
        // Keep the sidebar at its natural width and let the detail pane use remaining space.
        split_pane.set_resize_start_child(false);
        split_pane.set_resize_end_child(true);
        split_pane.set_position(360);

        main_box.append(&split_pane);

        root_stack.add_named(&main_box, Some("app"));

        let welcome_screen = WelcomeScreen::new();
        let welcome_widget = welcome_screen.widget();
        welcome_widget.set_margin_top(48);
        welcome_widget.set_margin_bottom(48);
        welcome_widget.set_margin_start(48);
        welcome_widget.set_margin_end(48);
        root_stack.add_named(welcome_widget, Some("welcome"));
        root_stack.set_visible_child_name("welcome");

        self.window.set_content(Some(&root_stack));

        if let Some(manager) = &self.preferences_manager {
            let manager = manager.clone();
            let window_for_size = self.window.clone();
            window_for_size.connect_close_request(move |win| {
                let width = win.width();
                let height = win.height();
                let manager = manager.clone();
                crate::runtime_handle().spawn(async move {
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
    }

    fn restore_preferences(&self) {
        if let Some(manager) = &self.preferences_manager {
            let window = self.window.clone();
            let selected_repo_id = self.selected_repo_id.clone();
            let (sender, receiver) =
                glib::MainContext::default().channel::<Preferences>(glib::Priority::default());

            receiver.attach(None, move |prefs| {
                window.set_default_size(prefs.window_width, prefs.window_height);
                {
                    let mut selected = selected_repo_id.lock();
                    *selected = prefs.last_selected_repo_id;
                }
                glib::ControlFlow::Break
            });

            let manager = manager.clone();
            crate::runtime_handle().spawn(async move {
                let prefs = manager.get().await;
                let _ = sender.send(prefs);
            });
        }
    }

    fn setup_header_menu(&self, header: &adw::HeaderBar) {
        self.ensure_window_actions();

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Application menu")
            .build();
        menu_button.add_css_class("flat");

        let menu = Menu::new();
        menu.append(Some("Preferences"), Some("win.open_preferences"));
        if cfg!(debug_assertions) {
            menu.append(
                Some("Send test notification"),
                Some("win.send_test_notification"),
            );
        }
        menu.append(Some("Sign out"), Some("win.sign_out"));

        menu_button.set_menu_model(Some(&menu));
        header.pack_end(&menu_button);
    }

    fn ensure_window_actions(&self) {
        let window = self.window.clone();

        if window.lookup_action("open_preferences").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("open_preferences", None);
            action.connect_activate(move |_, _| {
                this.open_preferences_window();
            });
            window.add_action(&action);
        }

        if window.lookup_action("sign_out").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("sign_out", None);
            action.connect_activate(move |_, _| {
                this.show_sign_out_dialog();
            });
            window.add_action(&action);
        }

        if cfg!(debug_assertions) && window.lookup_action("send_test_notification").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("send_test_notification", None);
            action.connect_activate(move |_, _| {
                this.dispatch_test_notification();
            });
            window.add_action(&action);
        }
    }

    fn open_preferences_window(&self) {
        if let Some(manager) = &self.preferences_manager {
            let parent = self.window.clone();
            let window = PreferencesWindow::new(&parent, manager.clone());
            window.present();
        } else {
            warn!("Preferences unavailable; preferences manager failed to initialize");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                "Preferences are currently unavailable.",
            );
            dialog.connect_response(|dialog, _| dialog.close());
            dialog.present();
        }
    }

    fn show_sign_out_dialog(&self) {
        let parent = self.window.clone();
        let this = self.clone();

        let dialog = gtk::MessageDialog::new(
            Some(&parent),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Warning,
            gtk::ButtonsType::YesNo,
            "Are you sure you want to sign out?\n\nYou will need to sign in again to continue.",
        );

        dialog.connect_response(move |dialog, response| {
            dialog.close();

            if response == gtk::ResponseType::Yes {
                match TokenStorage::new() {
                    Ok(storage) => match storage.delete_token() {
                        Err(err) => {
                            error!("Failed to delete token: {}", err);
                        }
                        _ => {
                            info!("Signed out successfully");
                            this.enter_signed_out_state();
                        }
                    },
                    Err(err) => {
                        error!("Failed to access token storage: {}", err);
                    }
                }
            }
        });

        dialog.present();
    }

    fn dispatch_test_notification(&self) {
        match self.notification_manager.clone() {
            Some(manager) => {
                crate::runtime_handle().spawn(async move {
                    if let Err(err) = manager
                        .notify_message(
                            "Actioneer notification test",
                            "If you can read this, GNOME notifications are working.",
                        )
                        .await
                    {
                        warn!("Failed to dispatch test notification: {}", err);
                    }
                });
            }
            None => {
                warn!("Notifications unavailable; could not send test notification");
                let dialog = gtk::MessageDialog::new(
                    Some(&self.window),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::Ok,
                    "Notifications are currently unavailable.",
                );
                dialog.connect_response(|dialog, _| dialog.close());
                dialog.present();
            }
        }
    }

    fn check_authentication(&self) {
        match TokenStorage::new() {
            Ok(storage) => {
                if !storage.has_token() {
                    info!("No token found, presenting welcome screen");
                    self.enter_signed_out_state();
                    return;
                }

                match storage.get_token() {
                    Ok(token) => {
                        info!("Found existing token, initializing client");
                        if !self.initialize_client(token) {
                            self.enter_signed_out_state();
                        }
                    }
                    Err(_) => {
                        info!(
                            "Failed to retrieve token despite presence flag; showing welcome screen"
                        );
                        self.enter_signed_out_state();
                    }
                }
            }
            Err(e) => {
                error!("Failed to access token storage: {}", e);
                self.enter_signed_out_state();
            }
        }
    }

    fn show_auth_window(&self) {
        let auth_window = AuthWindow::new();
        let this = self.clone();
        auth_window.present(Some(&self.window), move || {
            this.handle_sign_in_success();
        });
    }

    fn handle_sign_in_success(&self) {
        match TokenStorage::new() {
            Ok(storage) => match storage.get_token() {
                Ok(token) => {
                    if !self.initialize_client(token) {
                        self.enter_signed_out_state();
                    }
                }
                Err(err) => {
                    error!("Token unavailable after sign-in: {}", err);
                    self.enter_signed_out_state();
                }
            },
            Err(err) => {
                error!("Failed to reopen token storage after sign-in: {}", err);
                self.enter_signed_out_state();
            }
        }
    }

    fn initialize_client(&self, token: String) -> bool {
        if self.is_demo_mode() {
            demo::disable();
            *self.demo_mode.lock() = false;
        }

        match GitHubClient::new(Some(token)) {
            Ok(client) => {
                {
                    let mut client_guard = self.client.lock();
                    *client_guard = Some(client);
                }

                {
                    let mut info_guard = self.rate_limit_info.lock();
                    *info_guard = None;
                }

                self.update_rate_limit_display(None);
                self.show_authenticated_ui();
                self.load_repositories();
                true
            }
            Err(e) => {
                error!("Failed to create GitHub client: {}", e);
                false
            }
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
            demo::disable();
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

        {
            let mut client_guard = self.client.lock();
            *client_guard = None;
        }

        {
            let cache = self.cache.clone();
            crate::runtime_handle().spawn(async move {
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
            crate::runtime_handle().spawn(async move {
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
            crate::runtime_handle().spawn(async move {
                if let Err(err) = manager.set_last_selected_repo(None).await {
                    warn!(
                        "Failed to reset stored repo selection during sign-out: {}",
                        err
                    );
                }
            });
        }

        let list_box = self.repo_list.clone();
        glib::idle_add_local_once(move || {
            list_box.unselect_all();
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

    fn setup_focus_handler(&self) {
        let this = self.clone();
        self.window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                return;
            }

            if this.is_demo_mode() {
                return;
            }

            let storage = match TokenStorage::new() {
                Ok(storage) => storage,
                Err(err) => {
                    error!("Failed to access token storage during focus check: {}", err);
                    this.enter_signed_out_state();
                    return;
                }
            };

            let token_result = storage.get_token();
            match token_result {
                Ok(token) => {
                    let needs_client = this.client.lock().is_none();
                    if needs_client {
                        info!("Token available after auth, initializing client");
                        if !this.initialize_client(token) {
                            this.enter_signed_out_state();
                        }
                    }
                }
                Err(_) => {
                    let had_client = {
                        let mut guard = this.client.lock();
                        let had = guard.is_some();
                        *guard = None;
                        had
                    };

                    if had_client {
                        info!("Token missing after focus, returning to welcome screen");
                    }

                    this.enter_signed_out_state();
                }
            }
        });
    }

    fn ensure_detail_matches_selection(&self) {
        let action = {
            let selected_id = *self.selected_repo_id.lock();
            let active_id = self
                .active_detail
                .borrow()
                .as_ref()
                .map(|pane| pane.repo().id);

            match (selected_id, active_id) {
                (Some(sel), Some(active)) if sel == active => None,
                (Some(sel), _) => {
                    let repo = {
                        let repos = self.repos.lock();
                        repos.iter().find(|repo| repo.id == sel).cloned()
                    };
                    Some(repo)
                }
                (None, Some(_)) => Some(None),
                _ => None,
            }
        };

        if let Some(repo_opt) = action {
            let this = self.clone();
            glib::idle_add_local_once(move || {
                if *this.handling_selection.lock() {
                    return;
                }

                match repo_opt {
                    Some(repo) => this.handle_repo_selection(Some(repo)),
                    None => this.handle_repo_selection(None),
                }
            });
        }
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

    fn connect_search(&self) {
        let list_box = self.repo_list.clone();

        self.search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_lowercase();
            let query = text.clone();
            list_box.set_filter_func(move |row: &gtk::ListBoxRow| row_matches_query(row, &query));
            list_box.invalidate_filter();
        });
    }

    fn connect_repo_selection(&self) {
        let window = self.clone();

        self.repo_list
            .connect_selected_rows_changed(move |list_box| {
                let window = window.clone();
                let repo_name = list_box
                    .selected_row()
                    .and_then(|row| row.child())
                    .and_then(|child| find_label_by_name(&child, "repo-name-label"))
                    .map(|label| label.text().to_string());

                let repo = match repo_name {
                    Some(name) => {
                        let repos = window.repos.lock();
                        repos.iter().find(|repo| repo.full_name == name).cloned()
                    }
                    None => None,
                };

                // Check if selection actually changed
                let current_selection = *window.selected_repo_id.lock();
                let new_selection = repo.as_ref().map(|r| r.id);

                // Check if we're already handling a selection
                if *window.handling_selection.lock() {
                    info!("Already handling selection, ignoring signal");
                    return;
                }

                info!(
                    "Selection signal: current={:?}, new={:?}, repo={:?}",
                    current_selection,
                    new_selection,
                    repo.as_ref().map(|r| r.full_name.as_str())
                );

                // Ignore transient deselection events if we have an active detail pane
                // This happens during widget manipulation (stack remove/add)
                if new_selection.is_none() && window.active_detail.borrow().is_some() {
                    info!("Ignoring transient deselection (detail pane is active)");
                    return;
                }

                if current_selection != new_selection {
                    window.handle_repo_selection(repo);
                }
            });
    }

    fn handle_repo_selection(&self, repo: Option<Repo>) {
        *self.handling_selection.lock() = true;

        match repo {
            Some(repo) => {
                info!("Handling repo selection: {}", repo.full_name);
                let repo_id = repo.id;
                *self.selected_repo_id.lock() = Some(repo_id);
                if let Some(manager) = &self.preferences_manager {
                    let manager = manager.clone();
                    crate::runtime_handle().spawn(async move {
                        if let Err(err) = manager.set_last_selected_repo(Some(repo_id)).await {
                            warn!("Failed to persist selected repo: {}", err);
                        }
                    });
                }
                self.start_background_refresh(repo.clone());
                self.present_repo_detail(repo);
            }
            None => {
                info!("Deselecting repo");
                *self.selected_repo_id.lock() = None;
                if let Some(manager) = &self.preferences_manager {
                    let manager = manager.clone();
                    crate::runtime_handle().spawn(async move {
                        if let Err(err) = manager.set_last_selected_repo(None).await {
                            warn!("Failed to clear selected repo preference: {}", err);
                        }
                    });
                }
                self.stop_background_refresh();
                self.show_detail_placeholder();
            }
        }

        *self.handling_selection.lock() = false;
    }

    fn present_repo_detail(&self, repo: Repo) {
        info!("Creating detail pane for: {}", repo.full_name);

        // Check if we already have a pane for this repo to avoid recreating
        {
            let active = self.active_detail.borrow();
            if let Some(existing_pane) = active.as_ref()
                && existing_pane.repo().id == repo.id
            {
                info!("Pane already exists for this repo, skipping creation");
                return;
            }
        }

        let client_opt = {
            let guard = self.client.lock();
            guard.clone()
        };

        match client_opt {
            Some(client) => {
                let deps = RepoDetailDeps {
                    favorites_manager: self.favorites_manager.clone(),
                    preferences_manager: self.preferences_manager.clone(),
                    cache: self.cache.clone(),
                    favorites: self.favorites.clone(),
                    notification_manager: self.notification_manager.clone(),
                };

                let pane = RepoDetailPane::new(
                    self.window.clone(),
                    repo,
                    Arc::new(Mutex::new(client)),
                    deps,
                );
                let stack = self.detail_stack.clone();
                let active_detail = self.active_detail.clone();

                glib::idle_add_local_once(move || {
                    if let Some(existing_child) = stack.child_by_name("detail") {
                        stack.remove(&existing_child);
                    }

                    let widget = pane.widget();
                    stack.add_named(&widget, Some("detail"));
                    stack.set_visible_child_name("detail");
                    active_detail.borrow_mut().replace(pane);
                });
            }
            None => {
                warn!("Cannot show repository details without an authenticated client");
                self.show_detail_placeholder();
            }
        }
    }

    fn show_detail_placeholder(&self) {
        let stack = self.detail_stack.clone();
        let active_detail = self.active_detail.clone();
        let status_page = self.detail_status_page.clone();

        glib::idle_add_local_once(move || {
            if let Some(detail_child) = stack.child_by_name("detail") {
                stack.remove(&detail_child);
            }
            stack.set_visible_child_name("placeholder");
            active_detail.borrow_mut().take();
        });

        schedule_status_page_update(status_page, None);
    }

    fn start_background_refresh(&self, _repo: Repo) {
        // Stop any existing refresh task
        self.stop_background_refresh();

        let preferences_manager = match &self.preferences_manager {
            Some(manager) => manager.clone(),
            None => return,
        };

        let client_arc = self.client.clone();
        let active_detail = self.active_detail.clone();
        let rate_limit_label = self.rate_limit_label.clone();

        let (sender, receiver) =
            glib::MainContext::default().channel::<()>(glib::Priority::default());

        receiver.attach(None, move |_| {
            // Trigger a refresh on the active detail pane
            if let Some(pane) = active_detail.borrow().as_ref() {
                pane.refresh_workflows_silent();
            }
            glib::ControlFlow::Continue
        });

        let (rate_sender, rate_receiver) =
            glib::MainContext::default().channel::<RateLimitInfo>(glib::Priority::default());

        let rate_label_clone = rate_limit_label.clone();
        rate_receiver.attach(None, move |info| {
            update_rate_limit_label(&rate_label_clone, Some(info));
            glib::ControlFlow::Continue
        });

        let handle = crate::runtime_handle().spawn(async move {
            loop {
                // Get the current refresh interval
                let interval = preferences_manager.get().await.refresh_interval;

                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

                // Check if we still have a client
                let client_opt = {
                    let guard = client_arc.lock();
                    guard.clone()
                };

                if let Some(client) = client_opt {
                    // Signal the UI to refresh
                    if sender.send(()).is_err() {
                        break;
                    }

                    // Update rate limit display
                    if let Some(rate_info) = client.rate_limit_info()
                        && rate_sender.send(rate_info).is_err()
                    {
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        *self.background_refresh_task.lock() = Some(handle);
    }

    fn stop_background_refresh(&self) {
        if let Some(handle) = self.background_refresh_task.lock().take() {
            handle.abort();
        }
    }

    fn show_header_loading(&self, loading: bool) {
        let header = self.header_bar.clone();
        let refresh_button = self.refresh_button.clone();
        let spinner_ref = self.header_spinner.clone();

        glib::idle_add_local_once(move || {
            if loading {
                info!("🔄 Showing header loading spinner");
                // Hide refresh button
                refresh_button.set_visible(false);

                // Remove any existing spinner
                if let Some(old_spinner) = spinner_ref.borrow_mut().take() {
                    header.remove(&old_spinner);
                }

                // Create and add new spinner
                let spinner = gtk::Spinner::new();
                spinner.start(); // Start animation
                spinner.set_size_request(24, 24);
                spinner.set_tooltip_text(Some("Loading repositories..."));
                header.pack_start(&spinner);
                spinner.set_visible(true); // Ensure visible

                // Store reference
                *spinner_ref.borrow_mut() = Some(spinner);
            } else {
                info!("✅ Hiding header loading spinner");
                // Remove spinner if it exists
                match spinner_ref.borrow_mut().take() {
                    Some(spinner) => {
                        info!("Removing spinner from header");
                        header.remove(&spinner);
                    }
                    _ => {
                        debug!("No header spinner present when hiding header loader");
                    }
                }

                // Show refresh button
                refresh_button.set_visible(true);
            }
        });
    }

    pub fn present(&self) {
        self.window.present();
    }
}
