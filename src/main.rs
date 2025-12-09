mod api;
mod auth;
mod cache;
mod config;
mod demo;
mod favorites;
mod notifications;
mod preferences;
mod storage;
mod ui;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use gtk4::{IconTheme, gdk, glib};
use libadwaita as adw;
use std::fs;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::runtime::{Builder, Handle};
use tracing::{info, warn};
use ui::{MainWindow, style};

pub const APP_ID: &str = "me.spaceinbox.actioneer";
pub const APP_ICON_NAME: &str = APP_ID;

const DEV_ICON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icons");

// Global runtime handle
static RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();

pub fn runtime_handle() -> &'static Handle {
    RUNTIME_HANDLE.get().expect("Runtime not initialized")
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

    install_snap_desktop_entry();

    // Create GTK application
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();

    let send_test_notification = Arc::new(AtomicBool::new(false));
    let option_flag = send_test_notification.clone();
    app.add_main_option(
        "test-notification",
        glib::Char::from(b't'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Send a test notification when the app starts",
        None,
    );
    app.connect_handle_local_options(move |_app, options| {
        if options.contains("test-notification") {
            option_flag.store(true, Ordering::Relaxed);
        }
        ControlFlow::Continue(())
    });

    app.connect_startup(|_| {
        register_icon_theme_paths();
        gtk4::Window::set_default_icon_name(APP_ICON_NAME);
        style::install_app_css();
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

fn install_snap_desktop_entry() {
    let Ok(snap_dir) = std::env::var("SNAP") else {
        return;
    };

    let source = Path::new(&snap_dir).join("meta/gui/me.spaceinbox.actioneer.desktop");
    if !source.exists() {
        warn!("Snap desktop file not found at {}", source.display());
        return;
    }

    let home_dir = std::env::var("SNAP_REAL_HOME")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let dest_dir = Path::new(&home_dir).join(".local/share/applications");
    let dest = dest_dir.join("me.spaceinbox.actioneer.desktop");

    let Ok(raw) = fs::read_to_string(&source) else {
        warn!("Failed to read snap desktop file {}", source.display());
        return;
    };

    let icon_path = "/snap/actioneer/current/meta/gui/me.spaceinbox.actioneer.svg";
    let exec_path = "/snap/bin/actioneer";

    let mut rewritten = String::new();
    for line in raw.lines() {
        if line.starts_with("Icon=") {
            rewritten.push_str(&format!("Icon={icon_path}\n"));
        } else if line.starts_with("Exec=") {
            rewritten.push_str(&format!("Exec={exec_path}\n"));
        } else if line.starts_with("TryExec=") {
            rewritten.push_str(&format!("TryExec={exec_path}\n"));
        } else {
            rewritten.push_str(line);
            rewritten.push('\n');
        }
    }

    let needs_write = fs::read_to_string(&dest).map_or(true, |existing| existing != rewritten);
    if needs_write {
        if let Err(err) = fs::create_dir_all(&dest_dir) {
            warn!(error = %err, "Failed to create desktop dir {}", dest_dir.display());
            return;
        }

        if let Err(err) = fs::write(&dest, rewritten) {
            warn!(error = %err, "Failed to write desktop file {}", dest.display());
        } else {
            info!("Installed desktop entry at {}", dest.display());
        }
    }
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
    }
}
