mod api;
mod auth;
mod cache;
mod config;
mod demo;
mod favorites;
mod i18n;
mod notifications;
mod preferences;
mod storage;
mod ui;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use gtk4::{IconTheme, gdk, glib};
use i18n::tr;
use libadwaita as adw;
use preferences::{PreferencesManager, ThemePreference};
use std::borrow::Cow;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::runtime::{Builder, Handle};
use tracing::info;
use ui::{MainWindow, style};

pub const APP_ID: &str = "me.spaceinbox.actioneer";
pub const APP_ICON_NAME: &str = APP_ID;

const DEV_ICON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icons");

// Global runtime handle
static RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();

pub fn runtime_handle() -> &'static Handle {
    RUNTIME_HANDLE.get().expect("Runtime not initialized")
}

pub fn apply_text_direction_for_language() {
    let direction = if i18n::current_language_is_rtl() {
        gtk4::TextDirection::Rtl
    } else {
        gtk4::TextDirection::Ltr
    };
    gtk4::Widget::set_default_direction(direction);
}

pub fn resolved_app_id() -> Cow<'static, str> {
    if let Ok(snap_name) =
        std::env::var("SNAP_INSTANCE_NAME").or_else(|_| std::env::var("SNAP_NAME"))
    {
        Cow::Owned(format!("{}_{}", snap_name, APP_ID))
    } else {
        Cow::Borrowed(APP_ID)
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Actioneer for Linux");

    // Start tokio runtime in background thread and keep it alive
    std::thread::spawn(|| {
        let rt = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");
        let handle = rt.handle().clone();

        // Store the handle globally
        RUNTIME_HANDLE
            .set(handle)
            .expect("Failed to set runtime handle");

        // Keep the runtime alive
        rt.block_on(async {
            futures::future::pending::<()>().await;
        })
    });

    // Wait for runtime to be ready
    while RUNTIME_HANDLE.get().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!("Tokio runtime initialized");

    let runtime_app_id = resolved_app_id();

    let app = adw::Application::builder()
        .application_id(runtime_app_id.as_ref())
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();

    let send_test_notification = Arc::new(AtomicBool::new(false));
    let option_flag = send_test_notification.clone();

    let locale_string = Arc::new(std::sync::Mutex::new(None));
    let locale_flag = locale_string.clone();
    let test_notification_help = tr("Send a test notification when the app starts");
    let locale_help = tr("Set the application language (e.g., ru, en, zh_Hans)");

    app.add_main_option(
        "test-notification",
        glib::Char::from(b't'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        test_notification_help.as_str(),
        None,
    );

    app.add_main_option(
        "locale",
        glib::Char::from(b'l'),
        glib::OptionFlags::NONE,
        glib::OptionArg::String,
        locale_help.as_str(),
        Some("LOCALE"),
    );

    app.connect_handle_local_options(move |_app, options| {
        if let Ok(Some(variant)) = options.lookup::<glib::Variant>("locale")
            && let Some(argval) = variant.get::<String>()
        {
            *locale_flag.lock().unwrap() = Some(argval);
        }

        if options.contains("test-notification") {
            option_flag.store(true, Ordering::Relaxed);
        }
        ControlFlow::Continue(())
    });

    let cli_locale = locale_string.lock().unwrap().clone();
    let cli_locale_ref: Option<&str> = cli_locale.as_deref();
    i18n::init(cli_locale_ref);
    let startup_preferences = PreferencesManager::new()
        .map(|manager| manager.get_blocking())
        .unwrap_or_default();
    i18n::apply_language_preference(startup_preferences.language_preference);

    app.connect_startup(|_| {
        apply_text_direction_for_language();
        register_icon_theme_paths();
        gtk4::Window::set_default_icon_name(APP_ICON_NAME);
        style::install_app_css();
    });
    let startup_theme_preference = startup_preferences.theme_preference;
    app.connect_startup(move |_| {
        let style_manager = adw::StyleManager::default();
        match startup_theme_preference {
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
    });

    let activate_flag = send_test_notification.clone();
    app.connect_activate(move |app| build_ui(app, activate_flag.load(Ordering::Relaxed)));

    // Run the application
    app.run();
    Ok(())
}

fn build_ui(app: &adw::Application, send_test_notification: bool) {
    let main_window = MainWindow::new(app);
    if send_test_notification {
        main_window.trigger_test_notification();
    }
    main_window.present();
}

fn register_icon_theme_paths() {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    let theme = IconTheme::for_display(&display);

    // Allow running directly from the source tree by pointing at the repo icons.
    let dev_icons = Path::new(DEV_ICON_DIR);
    if dev_icons.exists() {
        theme.add_search_path(dev_icons);
    }

    // Ensure installed packages (Flatpak/AppImage) can find their bundled icon assets.
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(prefix) = exe_path.parent().and_then(|path| path.parent())
    {
        let share_icons = prefix.join("share/icons");
        if share_icons.exists() {
            theme.add_search_path(&share_icons);
        }

        let usr_share_icons = prefix.join("usr/share/icons");
        if usr_share_icons.exists() {
            theme.add_search_path(&usr_share_icons);
        }

        let meta_gui_icons = prefix.join("meta/gui");
        if meta_gui_icons.exists() {
            theme.add_search_path(&meta_gui_icons);
        }
    }
}
