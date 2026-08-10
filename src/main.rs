mod api;
mod auth;
mod cache;
mod config;
mod crash_report;
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
use std::backtrace::Backtrace;
use std::borrow::Cow;
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
const DEFAULT_TOKIO_WORKER_THREADS: usize = 4;
const TOKIO_WORKER_THREADS_ENV: &str = "ACTIONEER_TOKIO_WORKER_THREADS";

// Global runtime handle
static RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();
static CURRENT_SESSION_ID: OnceLock<String> = OnceLock::new();

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

fn available_parallelism_count() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(DEFAULT_TOKIO_WORKER_THREADS)
}

fn default_tokio_worker_threads_for(available_parallelism: usize) -> usize {
    available_parallelism.clamp(1, DEFAULT_TOKIO_WORKER_THREADS)
}

fn parse_tokio_worker_threads_override(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|threads| *threads > 0)
}

fn tokio_worker_threads() -> usize {
    let default_workers = default_tokio_worker_threads_for(available_parallelism_count());
    match std::env::var(TOKIO_WORKER_THREADS_ENV) {
        Ok(value) => parse_tokio_worker_threads_override(&value).unwrap_or_else(|| {
            warn!(
                env_var = TOKIO_WORKER_THREADS_ENV,
                value = %value,
                default_workers,
                "Ignoring invalid Tokio worker thread override"
            );
            default_workers
        }),
        Err(_) => default_workers,
    }
}

fn mark_current_session_clean(reason: &str) {
    if let Some(session_id) = CURRENT_SESSION_ID.get()
        && let Err(err) = crash_report::mark_session_clean(session_id)
    {
        warn!(
            reason,
            session_id,
            error = %err,
            "Failed to mark session as clean"
        );
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShutdownSignal {
    Hangup,
    Interrupt,
    Quit,
    Terminate,
}

#[cfg(unix)]
impl ShutdownSignal {
    fn all() -> [Self; 4] {
        [Self::Hangup, Self::Interrupt, Self::Quit, Self::Terminate]
    }

    fn name(self) -> &'static str {
        match self {
            Self::Hangup => "SIGHUP",
            Self::Interrupt => "SIGINT",
            Self::Quit => "SIGQUIT",
            Self::Terminate => "SIGTERM",
        }
    }

    fn kind(self) -> tokio::signal::unix::SignalKind {
        match self {
            Self::Hangup => tokio::signal::unix::SignalKind::hangup(),
            Self::Interrupt => tokio::signal::unix::SignalKind::interrupt(),
            Self::Quit => tokio::signal::unix::SignalKind::quit(),
            Self::Terminate => tokio::signal::unix::SignalKind::terminate(),
        }
    }
}

#[cfg(unix)]
fn install_unix_signal_handlers(_app: &adw::Application) {
    use tokio::signal::unix::signal;

    let _runtime_guard = runtime_handle().enter();
    let mut handles = Vec::with_capacity(ShutdownSignal::all().len());
    for shutdown_signal in ShutdownSignal::all() {
        let handle = match signal(shutdown_signal.kind()) {
            Ok(handle) => handle,
            Err(err) => {
                warn!(signal = shutdown_signal.name(), error = %err, "Failed to register shutdown signal handler");
                return;
            }
        };
        handles.push((shutdown_signal, handle));
    }

    let [
        (signal_hangup, mut sig_hangup),
        (signal_interrupt, mut sig_interrupt),
        (signal_quit, mut sig_quit),
        (signal_terminate, mut sig_terminate),
    ]: [(ShutdownSignal, tokio::signal::unix::Signal); 4] = handles
        .try_into()
        .expect("shutdown signal registration count should match enum");

    runtime_handle().spawn(async move {
        let signal_name = tokio::select! {
            _ = sig_hangup.recv() => signal_hangup.name(),
            _ = sig_interrupt.recv() => signal_interrupt.name(),
            _ = sig_quit.recv() => signal_quit.name(),
            _ = sig_terminate.recv() => signal_terminate.name(),
        };

        glib::MainContext::default().invoke(move || {
            info!(
                signal = signal_name,
                "Received termination signal, quitting cleanly"
            );
            mark_current_session_clean(signal_name);
            if let Some(app) = gio::Application::default() {
                app.quit();
            }
        });
    });
}

fn main() -> anyhow::Result<()> {
    let cli_locale = parse_cli_locale_arg().and_then(|locale| i18n::parse_locale_string(&locale));
    i18n::init(cli_locale.as_deref());

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    match crash_report::initialize_session_lifecycle() {
        Ok(marker) => {
            let _ = CURRENT_SESSION_ID.set(marker.session_id);
        }
        Err(err) => {
            warn!("Failed to initialize crash session lifecycle: {}", err);
        }
    }
    install_panic_hook();

    info!("Starting Actioneer for Linux");
    let runtime_app_id = resolved_app_id();
    notifications::initialize_portal_env(runtime_app_id.as_ref());
    let available_parallelism = available_parallelism_count();
    let default_runtime_workers = default_tokio_worker_threads_for(available_parallelism);
    let runtime_worker_threads = tokio_worker_threads();
    info!(
        available_parallelism,
        default_runtime_workers,
        runtime_worker_threads,
        env_var = TOKIO_WORKER_THREADS_ENV,
        "Configuring Tokio runtime"
    );

    // Start tokio runtime in background thread and keep it alive
    std::thread::spawn(move || {
        let rt = Builder::new_multi_thread()
            .worker_threads(runtime_worker_threads)
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

    let app = adw::Application::builder()
        .application_id(runtime_app_id.as_ref())
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();
    #[cfg(unix)]
    install_unix_signal_handlers(&app);
    app.connect_shutdown(|_| {
        mark_current_session_clean("shutdown");
    });

    let send_test_notification = Arc::new(AtomicBool::new(false));
    let option_flag = send_test_notification.clone();

    let start_demo_mode = Arc::new(AtomicBool::new(false));
    let demo_option_flag = start_demo_mode.clone();

    let test_notification_help = tr("Send a test notification when the app starts");
    let locale_help = tr("Set the application language (e.g., ru, en, zh_Hans)");
    let demo_help = tr("Start the app with sample demo data");

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

    app.add_main_option(
        "demo",
        glib::Char::from(b'd'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        demo_help.as_str(),
        None,
    );

    app.connect_handle_local_options(move |_app, options| {
        if options.contains("test-notification") {
            option_flag.store(true, Ordering::Relaxed);
        }
        if options.contains("demo") {
            demo_option_flag.store(true, Ordering::Relaxed);
        }
        ControlFlow::Continue(())
    });

    let startup_preferences = PreferencesManager::new()
        .map(|manager| manager.get_blocking())
        .unwrap_or_default();
    if cli_locale.is_none() {
        i18n::apply_language_preference(startup_preferences.language_preference);
    }

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
    let activate_demo = start_demo_mode.clone();
    app.connect_activate(move |app| {
        build_ui(
            app,
            activate_flag.load(Ordering::Relaxed),
            activate_demo.load(Ordering::Relaxed),
        )
    });

    // Run the application
    app.run();
    Ok(())
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = panic_info.payload().downcast_ref::<String>() {
            msg.clone()
        } else {
            "non-string panic payload".to_string()
        };
        let backtrace = Backtrace::force_capture();

        tracing::error!(
            panic_location = %location,
            panic_payload = %payload,
            backtrace = %backtrace,
            "Unhandled panic"
        );
        let session_id = CURRENT_SESSION_ID.get().map(String::as_str);
        if let Err(err) = crate::crash_report::persist_panic_report(
            &location,
            &payload,
            &backtrace.to_string(),
            session_id,
        ) {
            tracing::error!("Failed to persist crash report from panic hook: {}", err);
            eprintln!("Failed to persist crash report from panic hook: {err}");
        }
        eprintln!("Unhandled panic at {location}: {payload}\nBacktrace:\n{backtrace}");
    }));
}

fn parse_cli_locale_arg() -> Option<String> {
    extract_cli_locale_arg(std::env::args().skip(1))
}

fn extract_cli_locale_arg<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--locale" || arg == "-l" {
            return iter.next().map(|next| next.as_ref().to_string());
        }
        if let Some(locale) = arg.strip_prefix("--locale=") {
            return Some(locale.to_string());
        }
        if let Some(locale) = arg.strip_prefix("-l=") {
            return Some(locale.to_string());
        }
    }
    None
}

fn build_ui(app: &adw::Application, send_test_notification: bool, start_demo_mode: bool) {
    let main_window = MainWindow::new(app, start_demo_mode);
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::ShutdownSignal;
    use super::{
        default_tokio_worker_threads_for, extract_cli_locale_arg,
        parse_tokio_worker_threads_override,
    };

    #[test]
    fn extracts_locale_from_long_option_with_value() {
        let args = vec!["--locale", "ru"];
        assert_eq!(extract_cli_locale_arg(args), Some("ru".to_string()));
    }

    #[test]
    fn extracts_locale_from_long_option_equals() {
        let args = vec!["--locale=fr"];
        assert_eq!(extract_cli_locale_arg(args), Some("fr".to_string()));
    }

    #[test]
    fn extracts_locale_from_short_option_with_value() {
        let args = vec!["-l", "zh_Hans"];
        assert_eq!(extract_cli_locale_arg(args), Some("zh_Hans".to_string()));
    }

    #[test]
    fn returns_none_when_locale_is_missing() {
        let args = vec!["--locale"];
        assert_eq!(extract_cli_locale_arg(args), None);
    }

    #[test]
    fn caps_default_tokio_workers_for_high_parallelism_hosts() {
        assert_eq!(default_tokio_worker_threads_for(24), 4);
    }

    #[test]
    fn keeps_small_parallelism_for_default_tokio_workers() {
        assert_eq!(default_tokio_worker_threads_for(2), 2);
        assert_eq!(default_tokio_worker_threads_for(1), 1);
    }

    #[test]
    fn rejects_invalid_tokio_worker_override_values() {
        assert_eq!(parse_tokio_worker_threads_override("0"), None);
        assert_eq!(parse_tokio_worker_threads_override("abc"), None);
        assert_eq!(parse_tokio_worker_threads_override(" "), None);
    }

    #[test]
    fn parses_valid_tokio_worker_override_values() {
        assert_eq!(parse_tokio_worker_threads_override("6"), Some(6));
        assert_eq!(parse_tokio_worker_threads_override(" 3 "), Some(3));
    }

    #[cfg(unix)]
    #[test]
    fn clean_shutdown_signal_set_covers_terminal_and_session_signals() {
        let names = ShutdownSignal::all()
            .into_iter()
            .map(ShutdownSignal::name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["SIGHUP", "SIGINT", "SIGQUIT", "SIGTERM"]);
    }
}
