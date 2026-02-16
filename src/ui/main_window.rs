use super::WelcomeScreen;
use crate::api::GitHubClient;
use crate::api::models::{RateLimitInfo, Repo};
use crate::cache::{CachePersistenceConfig, DataCache};
use crate::demo;
use crate::favorites::FavoritesManager;
use crate::i18n::tr;
use crate::notifications::NotificationManager;
use crate::preferences::{Preferences, PreferencesManager, ThemePreference};
use crate::storage::TokenStorage;
use crate::ui::auth_window::AuthWindow;
use crate::ui::detail_view::RepoDetailPane;
use crate::ui::preferences_window::PreferencesWindow;
use crate::ui::utils::{MainContextChannelExt, create_detail_clamp};
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

mod demo_mode;
mod header_controls;
mod loaders;
mod refresh;
mod repo_list;
mod selection;
mod sidebar_panel;
use header_controls::HeaderControls;
use sidebar_panel::SidebarPanel;

// Import refactored modules
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};

const REPO_STATUS_TTL: Duration = Duration::from_secs(300);
const MIN_WINDOW_WIDTH: i32 = 860;
const MIN_WINDOW_HEIGHT: i32 = 520;
const HOMEPAGE_URL: &str = "https://github.com/makoni/actioneer-gtk";
const ISSUE_URL: &str = "https://github.com/makoni/actioneer-gtk/issues";

#[derive(Clone)]
pub struct MainWindow {
    window: adw::ApplicationWindow,
    client: Arc<Mutex<Option<GitHubClient>>>,
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
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        crate::apply_text_direction_for_language();
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Actioneer")
            .default_width(1000)
            .default_height(700)
            .build();
        window.set_resizable(true);
        window.set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);

        Self::ensure_app_focus_action(app, &window);

        let client = Arc::new(Mutex::new(None));
        let repos = Arc::new(Mutex::new(Vec::new()));
        let cache = Arc::new(
            CachePersistenceConfig::for_app(crate::APP_ID)
                .map_or_else(DataCache::new, DataCache::with_persistence),
        );
        if cache.has_persistence() {
            let cache_clone = cache.clone();
            crate::runtime_handle().spawn(async move {
                if cache_clone.hydrate_from_disk().await {
                    info!("Loaded cache snapshot from disk");
                }
            });
        }

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
        let notification_manager = Some(NotificationManager::for_application(app));
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
        };

        main_window.ensure_app_actions(app);
        main_window.ensure_app_accels(app);
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
            .tooltip_text(tr("Application menu"))
            .build();
        menu_button.add_css_class("flat");

        let menu = Menu::new();
        let preferences_label = tr("Preferences");
        let shortcuts_label = tr("Keyboard Shortcuts");
        let help_label = tr("Help");
        let sign_out_label = tr("Sign out");
        let report_issue_label = tr("Report Issue");
        let about_label = tr("About Actioneer");
        let quit_label = tr("Quit");
        menu.append(Some(preferences_label.as_str()), Some("app.preferences"));
        menu.append(Some(shortcuts_label.as_str()), Some("app.shortcuts"));
        menu.append(Some(help_label.as_str()), Some("app.help"));
        if cfg!(debug_assertions) {
            let test_notification_label = tr("Send test notification");
            menu.append(
                Some(test_notification_label.as_str()),
                Some("win.send_test_notification"),
            );
        }
        menu.append(Some(sign_out_label.as_str()), Some("win.sign_out"));
        menu.append(Some(report_issue_label.as_str()), Some("app.report_issue"));
        menu.append(Some(about_label.as_str()), Some("app.about"));
        menu.append(Some(quit_label.as_str()), Some("app.quit"));

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

    fn ensure_app_actions(&self, app: &adw::Application) {
        let replace_action = |name: &str, action: &gio::SimpleAction| {
            if app.lookup_action(name).is_some() {
                app.remove_action(name);
            }
            app.add_action(action);
        };

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("preferences", None);
            action.connect_activate(move |_, _| {
                this.open_preferences_window();
            });
            replace_action("preferences", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("about", None);
            action.connect_activate(move |_, _| {
                this.open_about_window();
            });
            replace_action("about", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("shortcuts", None);
            action.connect_activate(move |_, _| {
                this.open_shortcuts_window();
            });
            replace_action("shortcuts", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("help", None);
            action.connect_activate(move |_, _| {
                this.open_help_window();
            });
            replace_action("help", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("report_issue", None);
            action.connect_activate(move |_, _| {
                this.open_report_issue();
            });
            replace_action("report_issue", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("refresh", None);
            action.connect_activate(move |_, _| {
                if this.client.lock().is_some() {
                    this.load_repositories();
                } else {
                    warn!("Cannot refresh: GitHub client not initialized");
                }
            });
            replace_action("refresh", &action);
        }

        if app.lookup_action("quit").is_none() {
            let app_clone = app.clone();
            let action = gio::SimpleAction::new("quit", None);
            action.connect_activate(move |_, _| {
                app_clone.quit();
            });
            app.add_action(&action);
        }

        let this = self.clone();
        let action = gio::SimpleAction::new("reload-ui", None);
        action.connect_activate(move |_, _| {
            this.reload_window_for_language_change();
        });
        replace_action("reload-ui", &action);
    }

    fn ensure_app_accels(&self, app: &adw::Application) {
        app.set_accels_for_action("app.refresh", &["F5"]);
        app.set_accels_for_action("app.quit", &["<Primary>q"]);
        app.set_accels_for_action("app.preferences", &["<Primary>comma"]);
        app.set_accels_for_action("app.shortcuts", &["<Primary>question", "<Primary>slash"]);
        app.set_accels_for_action("app.help", &["F1"]);
    }

    fn ensure_app_focus_action(app: &adw::Application, window: &adw::ApplicationWindow) {
        if app.lookup_action("focus-main-window").is_some() {
            return;
        }

        let window_weak = window.downgrade();
        let action = gio::SimpleAction::new("focus-main-window", None);
        action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.present();
            }
        });

        app.add_action(&action);
    }

    fn open_preferences_window(&self) {
        if let Some(manager) = &self.preferences_manager {
            let parent = self.window.clone();
            let window = PreferencesWindow::new(&parent, manager.clone());
            window.present();
        } else {
            warn!("Preferences unavailable; preferences manager failed to initialize");
            let unavailable_message = tr("Preferences are currently unavailable.");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                unavailable_message.as_str(),
            );
            dialog.connect_response(|dialog, _| dialog.close());
            dialog.present();
        }
    }

    fn open_about_window(&self) {
        let about = adw::AboutWindow::builder()
            .transient_for(&self.window)
            .application_name("Actioneer")
            .application_icon(crate::APP_ICON_NAME)
            .developer_name("Sergey Armodin")
            .version(env!("CARGO_PKG_VERSION"))
            .website(HOMEPAGE_URL)
            .issue_url(ISSUE_URL)
            .license_type(gtk::License::MitX11)
            .build();
        about.present();
    }

    fn open_shortcuts_window(&self) {
        let window = gtk::ShortcutsWindow::builder()
            .transient_for(&self.window)
            .modal(true)
            .default_width(460)
            .default_height(340)
            .build();
        let section = gtk::ShortcutsSection::builder()
            .title(tr("General"))
            .build();
        let group = gtk::ShortcutsGroup::builder()
            .title(tr("Application"))
            .build();

        group.append(&Self::shortcut_item(
            tr("Refresh repositories").as_str(),
            "F5",
        ));
        group.append(&Self::shortcut_item(
            tr("Open preferences").as_str(),
            "<Primary>comma",
        ));
        group.append(&Self::shortcut_item(
            tr("Show keyboard shortcuts").as_str(),
            "<Primary>question",
        ));
        group.append(&Self::shortcut_item(tr("Open help").as_str(), "F1"));
        group.append(&Self::shortcut_item(
            tr("Quit application").as_str(),
            "<Primary>q",
        ));

        section.append(&group);
        window.set_child(Some(&section));
        window.present();
    }

    fn shortcut_item(title: &str, accelerator: &str) -> gtk::ShortcutsShortcut {
        gtk::ShortcutsShortcut::builder()
            .title(title)
            .accelerator(accelerator)
            .build()
    }

    fn open_help_window(&self) {
        let window = adw::Window::builder()
            .title(tr("Actioneer Help"))
            .transient_for(&self.window)
            .modal(true)
            .default_width(620)
            .default_height(520)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let help_text = tr("Actioneer Help\n\n\
Getting started\n\
1. Sign in with your GitHub account on the welcome screen.\n\
2. Pick a repository in the left sidebar.\n\
3. Expand a workflow to inspect recent runs.\n\n\
Useful actions\n\
- Refresh repository/workflow status with F5.\n\
- Trigger manual workflow runs from the play button.\n\
- Open Preferences with Ctrl+, to adjust refresh/notifications.\n\
- Open Keyboard Shortcuts with Ctrl+? for quick references.\n\n\
Troubleshooting\n\
- If no repos appear, verify your token and network access.\n\
- For expired logs, retry from the latest run or trigger a new run.\n\
- Use Report Issue from the menu to send diagnostics and steps.");
        let label = gtk::Label::new(Some(help_text.as_str()));
        label.set_wrap(true);
        label.set_selectable(false);
        label.set_xalign(0.0);
        content.append(&label);

        let issue_hint_text = format!(
            "{}\n{}",
            tr("Need more help? Use “Report Issue” in the app menu or visit:"),
            ISSUE_URL
        );
        let issue_hint = gtk::Label::new(Some(issue_hint_text.as_str()));
        issue_hint.set_wrap(true);
        issue_hint.set_xalign(0.0);
        issue_hint.add_css_class("dim-label");
        content.append(&issue_hint);

        scrolled.set_child(Some(&content));

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_vexpand(true);
        container.set_hexpand(true);
        container.append(&scrolled);

        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        button_row.set_margin_top(12);
        button_row.set_margin_bottom(18);
        button_row.set_margin_start(18);
        button_row.set_margin_end(18);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        button_row.append(&spacer);

        let close_button = gtk::Button::with_label(tr("Close").as_str());
        close_button.add_css_class("suggested-action");
        let window_for_close = window.clone();
        close_button.connect_clicked(move |_| {
            window_for_close.close();
        });
        button_row.append(&close_button);

        container.append(&button_row);
        window.set_content(Some(&container));
        window.present();
    }

    fn open_report_issue(&self) {
        if let Err(err) = open::that(ISSUE_URL) {
            error!("Failed to open issue tracker URL: {}", err);
            let open_issue_error = tr("Failed to open issue tracker in the browser.");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Error,
                gtk::ButtonsType::Ok,
                open_issue_error.as_str(),
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
            tr("Are you sure you want to sign out?\n\nYou will need to sign in again to continue.")
                .as_str(),
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
        info!("Debug: dispatching test notification action");
        match self.notification_manager.clone() {
            Some(manager) => {
                crate::runtime_handle().spawn(async move {
                    match manager
                        .notify_message(
                            tr("Actioneer notification test").as_str(),
                            tr("If you can read this, GNOME notifications are working.").as_str(),
                        )
                        .await
                    {
                        Ok(()) => info!("Debug: test notification dispatched"),
                        Err(err) => warn!("Failed to dispatch test notification: {}", err),
                    }
                });
            }
            None => {
                warn!("Notifications unavailable; could not send test notification");
                let notifications_unavailable = tr("Notifications are currently unavailable.");
                let dialog = gtk::MessageDialog::new(
                    Some(&self.window),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::Ok,
                    notifications_unavailable.as_str(),
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

    fn handle_auth_failure(&self) {
        info!("Authentication failed; clearing token and returning to welcome");

        match TokenStorage::new() {
            Ok(storage) => {
                if let Err(err) = storage.delete_token() {
                    warn!("Failed to delete token after auth failure: {}", err);
                }
            }
            Err(err) => warn!(
                "Token storage unavailable during auth failure handling: {}",
                err
            ),
        }

        self.enter_signed_out_state();
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
    }

    fn reload_window_for_language_change(&self) {
        let Some(app) = self.window.application() else {
            return;
        };
        let Ok(app) = app.downcast::<adw::Application>() else {
            return;
        };

        let replacement = MainWindow::new(&app);
        replacement.present();
        self.window.close();
    }
}
