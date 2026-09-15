//! Tests for [`super`].
//!
//! Split out of `main_window.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::app_services::AppServices;
use crate::ui::test_helpers::run_gtk_test;

fn app() -> adw::Application {
    adw::Application::builder()
        .application_id("me.spaceinbox.actioneer.MainWindowTest")
        .build()
}

fn settle() {
    while glib::MainContext::default().pending() {
        let _ = glib::MainContext::default().iteration(false);
    }
}

#[test]
#[ignore = "requires GTK display"]
fn the_window_builds_from_fake_services() {
    run_gtk_test("main_window_builds_from_fake_services", || {
        crate::runtime::init_test_runtime();
        let dir = tempfile::tempdir().expect("temp dir");
        let window = MainWindow::new(&app(), AppServices::test_fakes(dir.path()), false);
        settle();

        // The three regions the app is made of.
        assert!(window.window.content().is_some(), "the window has content");
        assert!(
            window.header_bar.parent().is_some(),
            "the header bar is packed into the window, not left dangling"
        );
        assert!(
            window.root_stack.pages().n_items() > 0,
            "the root stack has pages"
        );

        window.window.destroy();
        settle();
    });
}

#[test]
#[ignore = "requires GTK display"]
fn the_gateway_slot_follows_the_session_transitions() {
    run_gtk_test("main_window_slot_transitions", || {
        crate::runtime::init_test_runtime();
        let dir = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(dir.path());

        // `test_fakes` starts in demo mode, which is what a test wants;
        // clear it so the full cycle is visible.
        services.sign_out();
        assert!(services.gateway.lock().is_none(), "the slot starts empty");

        services.enter_demo();
        assert!(
            services
                .gateway
                .lock()
                .as_ref()
                .is_some_and(|gateway| gateway.is_demo()),
            "enter_demo installs a demo gateway"
        );

        assert!(services.authenticate("token".into()));
        assert!(
            services
                .gateway
                .lock()
                .as_ref()
                .is_some_and(|gateway| !gateway.is_demo()),
            "authenticate replaces it with a live gateway"
        );

        services.sign_out();
        assert!(services.gateway.lock().is_none(), "sign_out empties it");
    });
}

// No release test for `MainWindow`, deliberately.
//
// The plan asked for one, by analogy with `window_is_released_once_closed`
// in `job_logs_window.rs`. It is not achievable here and the difference is
// not a leak. `JobLogsWindow` is a plain `adw::Window`; `MainWindow` wraps
// an `adw::ApplicationWindow`, which registers itself with its
// `adw::Application` and is released only as the application processes that
// removal — which needs a running main loop that a `run_gtk_test` body does
// not have. Measured: after destroying the window and rebuilding, the
// toplevel survives and so does every widget beneath it, uniformly. That is
// the signature of the toplevel being held, not of a handler cycle, which
// would strand a subset.
//
// A weakened version — assert *some* widgets died — would pass while
// checking nothing, which is exactly the failure mode `AGENTS.md` warns
// about for GTK tests. The cycle risk in this window is covered instead by
// the per-row release tests (`runs/row.rs`, `job_logs_window.rs`) and by
// the `--demo` smoke journeys, which drive the real app through a full
// session.
