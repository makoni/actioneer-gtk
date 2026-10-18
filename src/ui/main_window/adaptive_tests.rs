//! Every screen of the main window, in every language, at the size of a phone.
//!
//! Widths depend on the strings as much as on the layout: the English UI fits
//! where the Russian one, with longer words throughout, is cut off at the right
//! edge. So each catalog gets its own test, and each test fails with the chain
//! of widgets that demands the extra width (`horizontal_overflow`).
//!
//! The font matters too, so the tests use Ubuntu Sans, wider than GTK's default
//! (fontconfig substitutes the nearest font where it is not installed).
//!
//! `ACTIONEER_UI_SHOTS=<dir>` saves a PNG of every screen checked, named
//! `<language>-<screen>.png`.

use super::*;
use crate::kernel::i18n::{apply_language_preference, i18n_test_guard, tr};
use crate::services::preferences::LanguagePreference;
use crate::ui::sidebar::repo_from_object;
use crate::ui::test_helpers::{horizontal_overflow, run_gtk_test, save_screenshot};

const PHONE_WIDTH: i32 = 360;
const PHONE_HEIGHT: i32 = 720;
const WIDE_FONT: &str = "Ubuntu Sans 11";
const MAIN_REPO: &str = "demo-org/actioneer-demo-app";

/// Runs the main loop for a moment: demo data and stored preferences arrive
/// from the Tokio side, and dialogs animate in. Not "until idle": in demo mode
/// running jobs tick their timers every second, so it never is.
fn settle_for_a_while() {
    let context = glib::MainContext::default();
    for _ in 0..50 {
        while context.pending() {
            let _ = context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Switches the UI language for one test and puts English back however the
/// test ends, so a failure does not leak a language into the next test.
struct LanguageScope {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl LanguageScope {
    fn enter(language: LanguagePreference) -> Self {
        let guard = i18n_test_guard();
        apply_language_preference(language);
        crate::ui::style::apply_text_direction_for_language();
        Self { _guard: guard }
    }
}

impl Drop for LanguageScope {
    fn drop(&mut self) {
        apply_language_preference(LanguagePreference::En);
        crate::ui::style::apply_text_direction_for_language();
    }
}

/// Lays the window out, saves a screenshot and records what overflows.
fn inspect(window: &MainWindow, code: &str, screen: &str, problems: &mut Vec<String>) {
    settle_for_a_while();
    save_screenshot(&window.window, &format!("{code}-{screen}"));
    for overflow in horizontal_overflow(&window.window) {
        problems.push(format!("{screen}: {overflow}"));
    }
}

/// A crash report line long enough to need wrapping, and with no spaces in
/// the path, so it has to break inside words.
const LONG_REPORT: &str = "thread 'main' panicked at /home/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/some-crate-1.2.3/src/lib.rs:42:13\n";

type OpenDialog = dyn Fn(&MainWindow);

/// Checks the dialog the last action opened, then closes it.
///
/// Measured against the dialog's own content, not the window: on a phone a
/// dialog is a bottom sheet a few pixels wider than the window it covers — its
/// rounded corners and shadow — which is libadwaita's frame, not overflow.
fn inspect_dialog(window: &MainWindow, code: &str, screen: &str, problems: &mut Vec<String>) {
    settle_for_a_while();
    let Some(dialog) = window.window.visible_dialog() else {
        problems.push(format!("{screen}: no dialog opened"));
        return;
    };
    save_screenshot(&window.window, &format!("{code}-{screen}"));
    let content = dialog.child().unwrap_or_else(|| dialog.clone().upcast());
    for overflow in horizontal_overflow(&content) {
        problems.push(format!("{screen}: {overflow}"));
    }
    dialog.force_close();
    settle_for_a_while();
}

/// Clicks the first button in the window described as `description` (the
/// tooltip `describe_control` sets).
fn click_control(window: &MainWindow, description: &str) {
    fn find(widget: &gtk::Widget, description: &str) -> Option<gtk::Button> {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.is_mapped()
            && button.tooltip_text().as_deref() == Some(description)
        {
            return Some(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = find(&current, description) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    let button = find(window.window.upcast_ref(), description)
        .unwrap_or_else(|| panic!("no button described as {description:?}"));
    button.emit_clicked();
}

/// Opens a run's logs, which arrive in a window of their own, and checks it at
/// the phone's size.
fn check_job_logs_window(window: &MainWindow, code: &str, problems: &mut Vec<String>) {
    let before: Vec<gtk::Window> = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .collect();
    click_control(window, &tr("View logs"));
    let mut logs = None;
    for _ in 0..20 {
        settle_for_a_while();
        logs = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.is_visible() && !before.contains(w));
        if logs.is_some() {
            break;
        }
    }
    let Some(logs) = logs else {
        problems.push("job-logs: no window opened".to_string());
        return;
    };
    logs.set_default_size(PHONE_WIDTH, PHONE_HEIGHT);
    settle_for_a_while();
    save_screenshot(&logs, &format!("{code}-job-logs"));
    for overflow in horizontal_overflow(&logs) {
        problems.push(format!("job-logs: {overflow}"));
    }
    logs.destroy();
    settle_for_a_while();
}

fn check_phone_layout(language: LanguagePreference, code: &str) {
    let _language = LanguageScope::enter(language);
    crate::runtime::init_test_runtime();
    crate::ui::style::install_app_css();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_font_name(Some(WIDE_FONT));
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let app = adw::Application::builder()
        .application_id("me.spaceinbox.actioneer.PhoneLayoutTest")
        .build();
    let window = MainWindow::new(&app, AppServices::test_fakes(dir.path()), true);
    // The stored window size arrives asynchronously; let it land first.
    settle_for_a_while();
    window.window.set_default_size(PHONE_WIDTH, PHONE_HEIGHT);
    window.window.present();

    let mut problems = Vec::new();
    inspect(&window, code, "list", &mut problems);
    assert!(
        window.split_view.is_collapsed(),
        "{code}: a phone collapses the split view"
    );

    let position = (0..window.repo_filter_model.n_items())
        .find(|&index| {
            window
                .repo_filter_model
                .item(index)
                .and_then(|item| repo_from_object(&item))
                .is_some_and(|repo| repo.full_name == MAIN_REPO)
        })
        .expect("the main demo repository is listed");
    window
        .sidebar_panel
        .repo_list()
        .emit_by_name::<()>("activate", &[&position]);
    inspect(&window, code, "detail", &mut problems);
    assert!(
        window.split_view.shows_content(),
        "{code}: the detail page opened"
    );

    // The dialogs. Each is hosted inside the window, as a bottom sheet at
    // this width, so checking the window checks the dialog.
    let dialogs: [(&str, &OpenDialog); 6] = [
        ("preferences", &|w| w.open_preferences_window()),
        ("about", &|w| w.open_about_window()),
        ("shortcuts", &|w| w.open_shortcuts_window()),
        ("help", &|w| w.open_help_window()),
        ("crash-report", &|w| {
            w.build_crash_report_dialog(LONG_REPORT.repeat(20))
                .present(Some(&w.window))
        }),
        ("sign-in", &|w| {
            crate::ui::auth_window::AuthWindow::new()
                .dialog_showing_code("ABCD-1234")
                .present(Some(&w.window))
        }),
    ];
    for (screen, open) in dialogs {
        open(&window);
        inspect_dialog(&window, code, screen, &mut problems);
    }
    WidgetExt::activate_action(&window.window, "win.sign_out", None).ok();
    inspect_dialog(&window, code, "sign-out", &mut problems);

    // Dialogs and windows opened from the detail pane.
    for (screen, control) in [
        ("trigger", "Trigger workflow"),
        ("cancel-run", "Cancel run"),
        ("rerun", "Re-run workflow"),
    ] {
        click_control(&window, &tr(control));
        inspect_dialog(&window, code, screen, &mut problems);
    }
    check_job_logs_window(&window, code, &mut problems);

    window.window.destroy();
    settle_for_a_while();

    assert!(
        problems.is_empty(),
        "{code}: content is cut off at {PHONE_WIDTH} px:\n{}",
        problems.join("\n")
    );
}

macro_rules! phone_layout_tests {
    ($($test:ident => $language:ident, $code:literal;)*) => {$(
        #[test]
        #[ignore = "requires GTK display"]
        fn $test() {
            run_gtk_test(stringify!($test), || {
                check_phone_layout(LanguagePreference::$language, $code)
            });
        }
    )*};
}

phone_layout_tests! {
    phone_layout_en => En, "en";
    phone_layout_de => De, "de";
    phone_layout_nl => Nl, "nl";
    phone_layout_zh_hans => ZhHans, "zh_Hans";
    phone_layout_hi => Hi, "hi";
    phone_layout_es => Es, "es";
    phone_layout_fr => Fr, "fr";
    phone_layout_ar => Ar, "ar";
    phone_layout_bn => Bn, "bn";
    phone_layout_pt_br => PtBr, "pt_BR";
    phone_layout_ru => Ru, "ru";
    phone_layout_ur => Ur, "ur";
    phone_layout_it => It, "it";
    phone_layout_ja => Ja, "ja";
}
