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
use crate::ui::test_helpers::{
    find_widget, horizontal_overflow, present_window_at, run_gtk_test, save_screenshot,
    save_screenshot_to, settle_main_loop, toplevel_windows, wait_for_new_toplevel,
};

const PHONE_WIDTH: i32 = 360;
const PHONE_HEIGHT: i32 = 720;
/// A phone on its side: wide enough to collapse nothing more, and as low as
/// GNOME asks adaptive apps to go.
const LANDSCAPE_WIDTH: i32 = 720;
const LANDSCAPE_HEIGHT: i32 = 294;
const WIDE_FONT: &str = "Ubuntu Sans 11";
const MAIN_REPO: &str = "demo-org/actioneer-demo-app";

/// A crash report line long enough to need wrapping, and with no spaces in
/// the path, so it has to break inside words.
const LONG_REPORT: &str = "thread 'main' panicked at /home/user/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/some-crate-1.2.3/src/lib.rs:42:13\n";

fn settle_for_a_while() {
    settle_main_loop(50);
}

/// The look a test lays the app out in: its language, the wide font, and for
/// store screenshots the light style, Yaru icons and no window buttons.
///
/// All of it is process-wide — every GTK test shares one worker — so it is put
/// back however the test ends; otherwise the tests after this one would lay
/// out in its font or language, and pass or fail by the order they ran in.
struct LookScope {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl LookScope {
    fn enter(language: LanguagePreference) -> Self {
        let guard = i18n_test_guard();
        apply_language_preference(language);
        crate::ui::style::apply_text_direction_for_language();
        crate::runtime::init_test_runtime();
        crate::ui::style::install_app_css();
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_font_name(Some(WIDE_FONT));
        }
        Self { _guard: guard }
    }

    /// The desktop store screenshots' look. Call once the window exists: it
    /// applies the stored theme when it is built, which would win.
    fn store_look(&self) {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_icon_theme_name(Some("Yaru"));
            settings.set_gtk_decoration_layout(Some(":"));
        }
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    }
}

impl Drop for LookScope {
    fn drop(&mut self) {
        apply_language_preference(LanguagePreference::En);
        crate::ui::style::apply_text_direction_for_language();
        if let Some(settings) = gtk::Settings::default() {
            for property in [
                "gtk-font-name",
                "gtk-icon-theme-name",
                "gtk-decoration-layout",
            ] {
                settings.reset_property(property);
            }
        }
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
    }
}

/// A demo-mode main window, presented at the given size with its
/// repositories loaded.
fn demo_window(services: AppServices, width: i32, height: i32) -> MainWindow {
    let app = adw::Application::builder()
        .application_id("me.spaceinbox.actioneer.PhoneLayoutTest")
        .build();
    let window = MainWindow::new(&app, services, true);
    // The stored window size arrives asynchronously; let it land first.
    settle_for_a_while();
    present_window_at(&window.window, width, height);
    window
}

/// Opens `MAIN_REPO` as a tap or Enter would.
fn open_main_repo(window: &MainWindow) {
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
}

/// Clicks the first button in the window described as `description` (the
/// tooltip `describe_control` sets).
fn click_control(window: &MainWindow, description: &str) {
    find_widget::<gtk::Button>(&window.window, |button| {
        button.tooltip_text().as_deref() == Some(description)
    })
    .unwrap_or_else(|| panic!("no button described as {description:?}"))
    .emit_clicked();
}

/// Opens the logs of the run whose control is first in the window; they
/// arrive in a window of their own.
fn open_job_logs(window: &MainWindow) -> Option<gtk::Window> {
    let before = toplevel_windows();
    click_control(window, &tr("View logs"));
    wait_for_new_toplevel(&before)
}

/// Lays the window out, saves a screenshot and records what overflows.
fn inspect(window: &MainWindow, code: &str, screen: &str, problems: &mut Vec<String>) {
    settle_for_a_while();
    save_screenshot(&window.window, &format!("{code}-{screen}"));
    for overflow in horizontal_overflow(&window.window) {
        problems.push(format!("{screen}: {overflow}"));
    }
}

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
    // Too tall is cut off as surely as too wide; on a phone on its side it is
    // height that runs out.
    let (needed, _, _, _) = content.measure(gtk::Orientation::Vertical, content.width());
    if needed > content.height() + 1 {
        problems.push(format!(
            "{screen}: needs {needed} px of height, has {}",
            content.height()
        ));
    }
    dialog.force_close();
    settle_for_a_while();
}

type OpenDialog = dyn Fn(&MainWindow);

/// The dialogs reached from the window and its menu. Each is hosted inside
/// the window, as a bottom sheet on a phone.
fn inspect_window_dialogs(window: &MainWindow, code: &str, problems: &mut Vec<String>) {
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
        open(window);
        inspect_dialog(window, code, screen, problems);
    }
    WidgetExt::activate_action(&window.window, "win.sign_out", None).ok();
    inspect_dialog(window, code, "sign-out", problems);
}

fn check_phone_layout(language: LanguagePreference, code: &str) {
    let _look = LookScope::enter(language);
    let dir = tempfile::tempdir().expect("temp dir");
    let window = demo_window(
        AppServices::test_fakes(dir.path()),
        PHONE_WIDTH,
        PHONE_HEIGHT,
    );

    let mut problems = Vec::new();
    inspect(&window, code, "list", &mut problems);
    assert!(
        window.split_view.is_collapsed(),
        "{code}: a phone collapses the split view"
    );

    open_main_repo(&window);
    inspect(&window, code, "detail", &mut problems);
    assert!(
        window.split_view.shows_content(),
        "{code}: the detail page opened"
    );

    inspect_window_dialogs(&window, code, &mut problems);

    // Dialogs and windows opened from the detail pane.
    for (screen, control) in [
        ("trigger", "Trigger workflow"),
        ("cancel-run", "Cancel run"),
        ("rerun", "Re-run workflow"),
    ] {
        click_control(&window, &tr(control));
        inspect_dialog(&window, code, screen, &mut problems);
    }
    match open_job_logs(&window) {
        Some(logs) => {
            present_window_at(&logs, PHONE_WIDTH, PHONE_HEIGHT);
            save_screenshot(&logs, &format!("{code}-job-logs"));
            for overflow in horizontal_overflow(&logs) {
                problems.push(format!("job-logs: {overflow}"));
            }
            logs.destroy();
        }
        None => problems.push("job-logs: no window opened".to_string()),
    }

    window.window.destroy();
    settle_for_a_while();

    assert!(
        problems.is_empty(),
        "{code}: content is cut off at {PHONE_WIDTH} px:\n{}",
        problems.join("\n")
    );
}

/// The dialogs on a phone on its side, where the height runs out first.
fn check_landscape_dialogs(language: LanguagePreference, code: &str) {
    let _look = LookScope::enter(language);
    let dir = tempfile::tempdir().expect("temp dir");
    let window = demo_window(
        AppServices::test_fakes(dir.path()),
        LANDSCAPE_WIDTH,
        LANDSCAPE_HEIGHT,
    );

    let mut problems = Vec::new();
    inspect_window_dialogs(&window, &format!("{code}-landscape"), &mut problems);

    window.window.destroy();
    settle_for_a_while();

    assert!(
        problems.is_empty(),
        "{code}: a dialog does not fit {LANDSCAPE_WIDTH}x{LANDSCAPE_HEIGHT}:\n{}",
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

// Two languages are enough here: height comes from the number of lines, and
// these two wrap the most — Russian on long words, German on long compounds.
#[test]
#[ignore = "requires GTK display"]
fn landscape_dialogs_en() {
    run_gtk_test("landscape_dialogs_en", || {
        check_landscape_dialogs(LanguagePreference::En, "en")
    });
}

#[test]
#[ignore = "requires GTK display"]
fn landscape_dialogs_ru() {
    run_gtk_test("landscape_dialogs_ru", || {
        check_landscape_dialogs(LanguagePreference::Ru, "ru")
    });
}

#[test]
#[ignore = "requires GTK display"]
fn landscape_dialogs_de() {
    run_gtk_test("landscape_dialogs_de", || {
        check_landscape_dialogs(LanguagePreference::De, "de")
    });
}

/// Renders the phone screenshots the AppStream data ships
/// (`data/screenshots/en-mobile-*.png`), when `ACTIONEER_STORE_SHOTS=<dir>` is
/// set; otherwise does nothing, so it is harmless in the `--ignored` run.
///
/// Light style, English, Ubuntu Sans and the Yaru icons, like the desktop
/// screenshots; no window buttons, as on a phone's shell; drawn at twice the
/// size. `scripts/render-mobile-screenshots.sh` runs it and adds the frame
/// (rounded corners, shadow).
#[test]
#[ignore = "requires GTK display"]
fn store_screenshots() {
    let Some(dir) = std::env::var_os("ACTIONEER_STORE_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("screenshot directory");
    run_gtk_test("store_screenshots", move || {
        let look = LookScope::enter(LanguagePreference::En);

        let temp = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(temp.path());
        // One favourite, so the list shows both states of the heart.
        let main_repo_id = crate::demo::DemoBackend::new()
            .seed()
            .0
            .into_iter()
            .find(|repo| repo.full_name == MAIN_REPO)
            .expect("the main demo repository")
            .id;
        if let Some(favorites) = services.favorites.clone() {
            crate::runtime::handle()
                .block_on(favorites.add_favorite(main_repo_id))
                .expect("favourite saved");
        }

        let window = demo_window(services, PHONE_WIDTH, PHONE_HEIGHT);
        look.store_look();
        settle_main_loop(200);
        // No caret blinking in the search field.
        gtk::prelude::GtkWindowExt::set_focus(&window.window, None::<&gtk::Widget>);
        settle_for_a_while();
        let save = |widget: &gtk::Widget, name: &str| {
            save_screenshot_to(widget, &dir.join(name), 2.0);
        };
        save(window.window.upcast_ref(), "en-mobile-1.png");

        open_main_repo(&window);
        settle_main_loop(200);
        // Open the newest run, so its jobs and steps show.
        if let Some(run) =
            find_widget::<gtk::Widget>(&window.window, |widget| widget.has_css_class("run-item"))
            && let Some(expander) = find_widget::<gtk::Expander>(&run, |_| true)
        {
            expander.set_expanded(true);
        }
        settle_main_loop(300);
        save(window.window.upcast_ref(), "en-mobile-2.png");

        let logs = open_job_logs(&window).expect("the logs window opened");
        present_window_at(&logs, PHONE_WIDTH, PHONE_HEIGHT);
        settle_main_loop(300);
        save(logs.upcast_ref(), "en-mobile-3.png");

        logs.destroy();
        window.window.destroy();
        settle_for_a_while();
    });
}
