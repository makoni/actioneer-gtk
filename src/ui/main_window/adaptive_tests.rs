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
use crate::kernel::i18n::{apply_language_preference, i18n_test_guard};
use crate::services::preferences::LanguagePreference;
use crate::ui::sidebar::repo_from_object;
use crate::ui::test_helpers::{horizontal_overflow, run_gtk_test, save_screenshot};

const PHONE_WIDTH: i32 = 360;
const PHONE_HEIGHT: i32 = 720;
const WIDE_FONT: &str = "Ubuntu Sans 11";
const MAIN_REPO: &str = "demo-org/actioneer-demo-app";

fn settle_for_a_while() {
    for _ in 0..100 {
        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
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
