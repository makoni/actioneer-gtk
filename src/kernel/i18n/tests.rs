//! Tests for [`super`].
//!
//! Split out of `i18n.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::{
    LanguagePreference, apply_language_preference, current_effective_language,
    current_language_is_rtl, init, is_rtl_language, normalize_system_locale, parse_locale_string,
    parse_po_catalog, resolve_initial_language, resolve_language_preference, resolve_locale_dir,
    resolve_po_dir, set_effective_language, tr,
};
use std::path::PathBuf;

#[test]
fn prefers_explicit_locale_dir() {
    let dev_dir = PathBuf::from("/tmp/actioneer-dev-locale");
    let actual = resolve_locale_dir(
        Some("/custom/locale".to_string()),
        Some("/snap/actioneer/current".to_string()),
        true,
        true,
        &dev_dir,
    );
    assert_eq!(actual, "/custom/locale");
}

#[test]
fn uses_snap_locale_path_when_available() {
    let dev_dir = PathBuf::from("/tmp/actioneer-dev-locale");
    let actual = resolve_locale_dir(
        None,
        Some("/snap/actioneer/current".to_string()),
        false,
        true,
        &dev_dir,
    );
    assert_eq!(actual, "/snap/actioneer/current/usr/share/locale");
}

#[test]
fn falls_back_to_dev_locale_dir() {
    let dev_dir = PathBuf::from("/tmp/actioneer-dev-locale");
    let actual = resolve_locale_dir(None, None, false, true, &dev_dir);
    assert_eq!(actual, "/tmp/actioneer-dev-locale");
}

#[test]
fn falls_back_to_dev_po_dir() {
    let dev_dir = PathBuf::from("/tmp/actioneer-dev-po");
    let actual = resolve_po_dir(None, false, true, &dev_dir);
    assert_eq!(actual, dev_dir);
}

#[test]
fn normalizes_supported_locales() {
    assert_eq!(
        normalize_system_locale("ru_RU.UTF-8"),
        Some("ru".to_string())
    );
    assert_eq!(
        normalize_system_locale("de_DE.UTF-8"),
        Some("de".to_string())
    );
    assert_eq!(
        normalize_system_locale("nl_NL.UTF-8"),
        Some("nl".to_string())
    );
    assert_eq!(
        normalize_system_locale("pt_PT.UTF-8"),
        Some("pt_BR".to_string())
    );
    assert_eq!(
        normalize_system_locale("zh_CN.UTF-8"),
        Some("zh_Hans".to_string())
    );
    assert_eq!(
        normalize_system_locale("it_IT.UTF-8"),
        Some("it".to_string())
    );
    assert_eq!(
        normalize_system_locale("ja_JP.UTF-8"),
        Some("ja".to_string())
    );
}

#[test]
fn explicit_language_preference_is_preserved() {
    assert_eq!(
        resolve_language_preference(LanguagePreference::Fr),
        "fr".to_string()
    );
    assert_eq!(
        resolve_language_preference(LanguagePreference::De),
        "de".to_string()
    );
    assert_eq!(
        resolve_language_preference(LanguagePreference::Nl),
        "nl".to_string()
    );
    assert_eq!(
        resolve_language_preference(LanguagePreference::It),
        "it".to_string()
    );
    assert_eq!(
        resolve_language_preference(LanguagePreference::Ja),
        "ja".to_string()
    );
}

#[test]
fn identifies_rtl_languages() {
    assert!(is_rtl_language("ar"));
    assert!(is_rtl_language("ur"));
    assert!(!is_rtl_language("ru"));
}

#[test]
fn tr_uses_selected_language_catalog() {
    let _guard = super::i18n_test_guard();
    init(None);
    let previous = current_effective_language();
    let changed = apply_language_preference(LanguagePreference::Ru);
    assert!(changed || current_effective_language() == "ru");
    let translated = tr("Welcome to Actioneer");
    assert_ne!(translated, "Welcome to Actioneer");
    set_effective_language(previous.clone());
    // SAFETY: process-local env vars in test process.
    unsafe {
        std::env::set_var("LANGUAGE", previous.clone());
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", previous);
    }
}

#[test]
fn tr_uses_italian_catalog() {
    let _guard = super::i18n_test_guard();
    init(None);
    let previous = current_effective_language();
    let changed = apply_language_preference(LanguagePreference::It);
    assert!(changed || current_effective_language() == "it");
    let translated = tr("Welcome to Actioneer");
    assert_ne!(translated, "Welcome to Actioneer");
    set_effective_language(previous.clone());
    // SAFETY: process-local env vars in test process.
    unsafe {
        std::env::set_var("LANGUAGE", previous.clone());
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", previous);
    }
}

#[test]
fn tr_uses_japanese_catalog() {
    let _guard = super::i18n_test_guard();
    init(None);
    let previous = current_effective_language();
    let changed = apply_language_preference(LanguagePreference::Ja);
    assert!(changed || current_effective_language() == "ja");
    let translated = tr("Welcome to Actioneer");
    assert_ne!(translated, "Welcome to Actioneer");
    set_effective_language(previous.clone());
    // SAFETY: process-local env vars in test process.
    unsafe {
        std::env::set_var("LANGUAGE", previous.clone());
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", previous);
    }
}

#[test]
fn current_language_rtl_reflects_effective_language() {
    let _guard = super::i18n_test_guard();
    init(None);
    let previous = current_effective_language();
    set_effective_language("ar".to_string());
    assert!(current_language_is_rtl());
    set_effective_language("en".to_string());
    assert!(!current_language_is_rtl());
    set_effective_language(previous);
}

#[test]
fn parses_po_catalog_entries() {
    let source = r#"
msgid ""
msgstr ""
"Language: en\n"

msgid "Preferences"
msgstr "Настройки"

msgid "Actioneer Help"
msgstr ""
"Справка "
"Actioneer"
"#;

    let parsed = parse_po_catalog(source);
    assert_eq!(
        parsed.get("Preferences").cloned(),
        Some("Настройки".to_string())
    );
    assert_eq!(
        parsed.get("Actioneer Help").cloned(),
        Some("Справка Actioneer".to_string())
    );
}

#[test]
fn keeps_first_translation_when_msgid_is_duplicated() {
    let source = r#"
msgid "Sign out"
msgstr "Cerrar sesión"

msgid "Sign out"
msgstr "Sign out"
"#;

    let parsed = parse_po_catalog(source);
    assert_eq!(
        parsed.get("Sign out").cloned(),
        Some("Cerrar sesión".to_string())
    );
}

#[test]
fn parses_locale_string_ru() {
    assert_eq!(parse_locale_string("ru"), Some("ru".to_string()));
}

#[test]
fn parses_locale_string_en_us() {
    assert_eq!(parse_locale_string("en_US"), Some("en".to_string()));
}

#[test]
fn parses_locale_string_zh_cn() {
    assert_eq!(parse_locale_string("zh_CN"), Some("zh_Hans".to_string()));
}

#[test]
fn parses_locale_string_pt_br() {
    assert_eq!(parse_locale_string("pt_BR"), Some("pt_BR".to_string()));
}

#[test]
fn parses_locale_string_de_de() {
    assert_eq!(parse_locale_string("de_DE"), Some("de".to_string()));
}

#[test]
fn parses_locale_string_nl_nl() {
    assert_eq!(parse_locale_string("nl_NL"), Some("nl".to_string()));
}

#[test]
fn parses_locale_string_it() {
    assert_eq!(parse_locale_string("it"), Some("it".to_string()));
    assert_eq!(parse_locale_string("it_IT"), Some("it".to_string()));
}

#[test]
fn parses_locale_string_ja() {
    assert_eq!(parse_locale_string("ja"), Some("ja".to_string()));
    assert_eq!(parse_locale_string("ja_JP"), Some("ja".to_string()));
}

#[test]
fn parses_locale_string_lowercase() {
    assert_eq!(parse_locale_string("RU"), Some("ru".to_string()));
    assert_eq!(parse_locale_string("en_gb"), Some("en".to_string()));
}

#[test]
fn rejects_invalid_locale_string() {
    assert_eq!(parse_locale_string("invalid"), None);
}

#[test]
fn resolve_initial_language_uses_cli_locale() {
    let result = resolve_initial_language(Some("ru"));
    assert_eq!(result, "ru");
}

#[test]
fn resolve_initial_language_falls_back_to_env_or_system() {
    let _guard = super::i18n_test_guard();
    unsafe {
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", "fr");
    }
    let result = resolve_initial_language(None);
    assert_eq!(result, "fr");
    unsafe {
        std::env::remove_var("ACTIONEER_EFFECTIVE_LANG");
    }
}

#[test]
fn cli_locale_takes_precedence_over_env() {
    let _guard = super::i18n_test_guard();
    unsafe {
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", "fr");
    }
    let result = resolve_initial_language(Some("ru"));
    assert_eq!(result, "ru");
    unsafe {
        std::env::remove_var("ACTIONEER_EFFECTIVE_LANG");
    }
}

#[test]
fn apply_language_preference_does_not_mutate_environment() {
    let _guard = super::i18n_test_guard();
    unsafe {
        std::env::set_var("LANGUAGE", "en");
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", "en");
    }

    let _ = apply_language_preference(LanguagePreference::Ru);

    assert_eq!(std::env::var("LANGUAGE").ok().as_deref(), Some("en"));
    assert_eq!(
        std::env::var("ACTIONEER_EFFECTIVE_LANG").ok().as_deref(),
        Some("en")
    );

    unsafe {
        std::env::remove_var("LANGUAGE");
        std::env::remove_var("ACTIONEER_EFFECTIVE_LANG");
    }
}
