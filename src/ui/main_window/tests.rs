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
        // `true`, not `false`. The `false` path runs `check_authentication()`,
        // which reads the developer's real system keyring — forbidden by
        // AGENTS.md — and, on a machine where the lookup is slower than the
        // test, its reply handler lands late and calls
        // `enter_signed_out_state()`, tearing down what the test just asserted.
        let window = MainWindow::new(&app(), AppServices::test_fakes(dir.path()), true);
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

/// A demo-mode window on `services`, presented at the given size with its
/// repositories loaded. Shared with `adaptive_tests`.
pub(super) fn demo_window(services: AppServices, width: i32, height: i32) -> MainWindow {
    crate::runtime::init_test_runtime();
    let window = MainWindow::new(&app(), services, true);
    // The demo data and the stored window size arrive asynchronously; let
    // them land before overriding the size, or it overrides the size under
    // test.
    crate::ui::test_helpers::settle_main_loop(100);
    crate::ui::test_helpers::present_window_at(&window.window, width, height);
    crate::ui::test_helpers::settle_main_loop(100);
    assert!(
        crate::ui::sidebar::find_first_repo_index(&window.repo_filter_model).is_some(),
        "the demo repositories loaded"
    );
    window
}

#[test]
#[ignore = "requires GTK display"]
fn a_phone_sized_window_shows_one_page_at_a_time() {
    run_gtk_test("main_window_phone_sized", || {
        let dir = tempfile::tempdir().expect("temp dir");
        let window = demo_window(AppServices::test_fakes(dir.path()), 360, 720);

        assert_eq!(
            window.window.size_request(),
            (MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT),
            "the window can shrink to a phone"
        );
        // Not `== 360`: the default size includes the client-side decorations.
        assert!(window.window.width() <= 360, "the window fits a phone");
        assert!(window.split_view.is_collapsed(), "the split view collapsed");
        assert!(
            !window.rate_limit_box.is_visible(),
            "the rate limit gave way to the back button"
        );
        let header_text = window.rate_limit_label.label();
        assert!(
            crate::ui::test_helpers::find_widget::<gtk::Label>(&window.window, |label| {
                label.label() == header_text
            })
            .is_some(),
            "the rate limit is still shown somewhere: {header_text:?}"
        );
        // Demo mode opens a repository by itself. That is a restore, not a
        // request from the user, so it must leave the list on screen.
        assert!(window.active_detail.borrow().is_some(), "a repo is open");
        assert!(
            !window.split_view.shows_content(),
            "opening a repo by restore navigated away from the list"
        );

        let position = crate::ui::sidebar::find_first_repo_index(&window.repo_filter_model)
            .expect("a repository row");
        window
            .sidebar_panel
            .repo_list()
            .emit_by_name::<()>("activate", &[&position]);
        settle();
        assert!(
            window.split_view.shows_content(),
            "activating a repository opened its page"
        );

        window.window.destroy();
        settle();
    });
}

#[test]
#[ignore = "requires GTK display"]
fn a_desktop_window_keeps_list_and_detail_side_by_side() {
    run_gtk_test("main_window_desktop_sized", || {
        let dir = tempfile::tempdir().expect("temp dir");
        let window = demo_window(AppServices::test_fakes(dir.path()), 1000, 700);

        assert!(!window.split_view.is_collapsed());
        assert!(window.rate_limit_box.is_visible());

        window.window.destroy();
        settle();
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
