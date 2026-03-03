use crate::preferences::LanguagePreference;
use gettextrs::{
    LocaleCategory, bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

pub const GETTEXT_PACKAGE: &str = "actioneer";

static EFFECTIVE_LANGUAGE: OnceLock<RwLock<String>> = OnceLock::new();
static PO_TRANSLATIONS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
#[cfg(test)]
static I18N_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn i18n_test_guard() -> std::sync::MutexGuard<'static, ()> {
    I18N_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn init(cli_locale: Option<&str>) {
    let _ = setlocale(LocaleCategory::LcAll, "");
    let locale_dir = locale_dir();
    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let initial_language = resolve_initial_language(cli_locale);
    set_language_env_for_process(&initial_language);
    set_effective_language(initial_language);
    let _ = PO_TRANSLATIONS.get_or_init(load_po_translations);
}

pub fn tr(message: &str) -> String {
    if let Some(translation) = lookup_po_translation(message) {
        return translation;
    }
    gettext(message)
}

pub fn apply_language_preference(preference: LanguagePreference) -> bool {
    let effective_language = resolve_language_preference(preference);
    let changed = current_effective_language() != effective_language;

    set_effective_language(effective_language);
    let locale = locale_for_setlocale(current_effective_language().as_str()).unwrap_or("");
    let _ = setlocale(LocaleCategory::LcAll, locale);
    changed
}

fn set_language_env_for_process(effective_language: &str) {
    // SAFETY: invoked during startup initialization before worker threads are spawned.
    unsafe {
        std::env::set_var("LANGUAGE", effective_language);
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", effective_language);
    }
}

fn locale_for_setlocale(language: &str) -> Option<&'static str> {
    match language {
        "en" => Some("en_US.UTF-8"),
        "de" => Some("de_DE.UTF-8"),
        "nl" => Some("nl_NL.UTF-8"),
        "zh_Hans" => Some("zh_CN.UTF-8"),
        "hi" => Some("hi_IN.UTF-8"),
        "es" => Some("es_ES.UTF-8"),
        "fr" => Some("fr_FR.UTF-8"),
        "ar" => Some("ar_SA.UTF-8"),
        "bn" => Some("bn_BD.UTF-8"),
        "pt_BR" => Some("pt_BR.UTF-8"),
        "ru" => Some("ru_RU.UTF-8"),
        "ur" => Some("ur_PK.UTF-8"),
        _ => None,
    }
}

pub fn resolve_initial_language(cli_locale: Option<&str>) -> String {
    if let Some(locale_str) = cli_locale
        && let Some(effective_lang) = parse_locale_string(locale_str)
    {
        return effective_lang;
    }

    std::env::var("ACTIONEER_EFFECTIVE_LANG")
        .ok()
        .unwrap_or_else(|| resolve_language_preference(LanguagePreference::System))
}

pub fn parse_locale_string(locale: &str) -> Option<String> {
    let normalized = locale.to_lowercase().trim().to_string();

    match normalized.as_str() {
        "ru" => Some("ru".to_string()),
        "en" | "en_us" | "en_gb" => Some("en".to_string()),
        "de" | "de_de" | "de_at" | "de_ch" => Some("de".to_string()),
        "nl" | "nl_nl" | "nl_be" => Some("nl".to_string()),
        "zh_hans" | "zh_cn" => Some("zh_Hans".to_string()),
        "hi" => Some("hi".to_string()),
        "es" | "es_es" | "es_mx" => Some("es".to_string()),
        "fr" | "fr_fr" | "fr_ca" => Some("fr".to_string()),
        "ar" => Some("ar".to_string()),
        "bn" => Some("bn".to_string()),
        "pt_br" | "pt" => Some("pt_BR".to_string()),
        "ur" => Some("ur".to_string()),
        _ => None,
    }
}

pub fn resolve_language_preference(preference: LanguagePreference) -> String {
    match preference {
        LanguagePreference::System => resolve_system_language().unwrap_or_else(|| "en".to_string()),
        other => language_code(other).to_string(),
    }
}

pub fn language_code(language: LanguagePreference) -> &'static str {
    match language {
        LanguagePreference::System => "system",
        LanguagePreference::En => "en",
        LanguagePreference::De => "de",
        LanguagePreference::Nl => "nl",
        LanguagePreference::ZhHans => "zh_Hans",
        LanguagePreference::Hi => "hi",
        LanguagePreference::Es => "es",
        LanguagePreference::Fr => "fr",
        LanguagePreference::Ar => "ar",
        LanguagePreference::Bn => "bn",
        LanguagePreference::PtBr => "pt_BR",
        LanguagePreference::Ru => "ru",
        LanguagePreference::Ur => "ur",
    }
}

pub fn is_rtl_language(language: &str) -> bool {
    let normalized = language.to_ascii_lowercase();
    normalized == "ar" || normalized == "ur"
}

pub fn current_language_is_rtl() -> bool {
    is_rtl_language(current_effective_language().as_str())
}

fn lookup_po_translation(message: &str) -> Option<String> {
    let lang = current_effective_language();
    let catalogs = PO_TRANSLATIONS.get_or_init(load_po_translations);
    catalogs
        .get(&lang)
        .and_then(|catalog| catalog.get(message))
        .filter(|translated| !translated.is_empty())
        .cloned()
}

fn current_effective_language() -> String {
    let lock = EFFECTIVE_LANGUAGE.get_or_init(|| RwLock::new("en".to_string()));
    match lock.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn set_effective_language(language: String) {
    let lock = EFFECTIVE_LANGUAGE.get_or_init(|| RwLock::new("en".to_string()));
    let mut guard = match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = language;
}

fn load_po_translations() -> HashMap<String, HashMap<String, String>> {
    let mut catalogs = HashMap::new();
    let po_dir = po_dir();
    if !po_dir.exists() {
        return catalogs;
    }

    let Ok(entries) = fs::read_dir(po_dir) else {
        return catalogs;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("po") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if let Ok(content) = fs::read_to_string(&path) {
            catalogs.insert(stem.to_string(), parse_po_catalog(&content));
        }
    }

    catalogs
}

fn parse_po_catalog(content: &str) -> HashMap<String, String> {
    #[derive(Clone, Copy)]
    enum ParseMode {
        MsgId,
        MsgStr,
    }

    let mut translations = HashMap::new();
    let mut current_msgid = String::new();
    let mut current_msgstr = String::new();
    let mut mode: Option<ParseMode> = None;

    let flush_entry =
        |translations: &mut HashMap<String, String>, msgid: &mut String, msgstr: &mut String| {
            if !msgid.is_empty() && !msgstr.is_empty() {
                translations
                    .entry(msgid.clone())
                    .or_insert_with(|| msgstr.clone());
            }
            msgid.clear();
            msgstr.clear();
        };

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush_entry(&mut translations, &mut current_msgid, &mut current_msgstr);
            mode = None;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        if line.starts_with("msgid ") {
            flush_entry(&mut translations, &mut current_msgid, &mut current_msgstr);
            current_msgid = parse_po_quoted(line);
            current_msgstr.clear();
            mode = Some(ParseMode::MsgId);
            continue;
        }

        if line.starts_with("msgstr ") {
            current_msgstr = parse_po_quoted(line);
            mode = Some(ParseMode::MsgStr);
            continue;
        }

        if line.starts_with('"') {
            match mode {
                Some(ParseMode::MsgId) => current_msgid.push_str(parse_po_quoted(line).as_str()),
                Some(ParseMode::MsgStr) => current_msgstr.push_str(parse_po_quoted(line).as_str()),
                None => {}
            }
        }
    }

    flush_entry(&mut translations, &mut current_msgid, &mut current_msgstr);
    translations
}

fn parse_po_quoted(line: &str) -> String {
    let Some(quote_start) = line.find('"') else {
        return String::new();
    };
    let quoted = &line[quote_start..];
    serde_json::from_str::<String>(quoted).unwrap_or_default()
}

fn resolve_system_language() -> Option<String> {
    let candidates = [
        std::env::var("LC_ALL").ok(),
        std::env::var("LC_MESSAGES").ok(),
        std::env::var("LANG").ok(),
    ];

    candidates
        .into_iter()
        .flatten()
        .find_map(|value| normalize_system_locale(&value))
}

fn normalize_system_locale(raw_locale: &str) -> Option<String> {
    let normalized = raw_locale
        .split('.')
        .next()
        .unwrap_or(raw_locale)
        .replace('-', "_");
    let lowered = normalized.to_lowercase();

    if lowered.starts_with("en") {
        return Some("en".to_string());
    }
    if lowered.starts_with("zh") {
        if lowered.contains("hans")
            || lowered.contains("cn")
            || lowered.contains("sg")
            || lowered.contains("my")
        {
            return Some("zh_Hans".to_string());
        }
        return None;
    }
    if lowered.starts_with("de") {
        return Some("de".to_string());
    }
    if lowered.starts_with("nl") {
        return Some("nl".to_string());
    }
    if lowered.starts_with("hi") {
        return Some("hi".to_string());
    }
    if lowered.starts_with("es") {
        return Some("es".to_string());
    }
    if lowered.starts_with("fr") {
        return Some("fr".to_string());
    }
    if lowered.starts_with("ar") {
        return Some("ar".to_string());
    }
    if lowered.starts_with("bn") {
        return Some("bn".to_string());
    }
    if lowered.starts_with("pt") {
        return Some("pt_BR".to_string());
    }
    if lowered.starts_with("ru") {
        return Some("ru".to_string());
    }
    if lowered.starts_with("ur") {
        return Some("ur".to_string());
    }

    None
}

fn locale_dir() -> String {
    let dev_locale_dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "po", "locale"]
        .iter()
        .collect();
    resolve_locale_dir(
        std::env::var("ACTIONEER_LOCALE_DIR").ok(),
        std::env::var("SNAP").ok(),
        std::env::var("FLATPAK_ID").is_ok(),
        dev_locale_dir.exists(),
        &dev_locale_dir,
    )
}

fn po_dir() -> PathBuf {
    if let Ok(path) = std::env::var("ACTIONEER_PO_DIR") {
        return PathBuf::from(path);
    }

    let dev_po_dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "po"].iter().collect();
    resolve_po_dir(
        std::env::var("SNAP").ok(),
        std::env::var("FLATPAK_ID").is_ok(),
        dev_po_dir.exists(),
        &dev_po_dir,
    )
}

fn resolve_po_dir(
    snap_root: Option<String>,
    is_flatpak: bool,
    dev_po_dir_exists: bool,
    dev_po_dir: &Path,
) -> PathBuf {
    if let Some(snap_root) = snap_root {
        let snap_po_dir = Path::new(&snap_root).join("usr/share/actioneer/po");
        if snap_po_dir.exists() {
            return snap_po_dir;
        }
    }

    if is_flatpak {
        let flatpak_po_dir = PathBuf::from("/app/share/actioneer/po");
        if flatpak_po_dir.exists() {
            return flatpak_po_dir;
        }
    }

    if dev_po_dir_exists {
        return dev_po_dir.to_path_buf();
    }

    dev_po_dir.to_path_buf()
}

fn resolve_locale_dir(
    actioneer_locale_dir: Option<String>,
    snap_root: Option<String>,
    is_flatpak: bool,
    dev_locale_dir_exists: bool,
    dev_locale_dir: &Path,
) -> String {
    if let Some(path) = actioneer_locale_dir {
        return path;
    }

    if let Some(snap_root) = snap_root {
        return format!("{snap_root}/usr/share/locale");
    }

    if is_flatpak {
        return "/app/share/locale".to_string();
    }

    if dev_locale_dir_exists {
        return dev_locale_dir.to_string_lossy().to_string();
    }

    "/usr/share/locale".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_language_preference, current_effective_language, current_language_is_rtl, init,
        is_rtl_language, normalize_system_locale, parse_locale_string, parse_po_catalog,
        resolve_initial_language, resolve_language_preference, resolve_locale_dir, resolve_po_dir,
        set_effective_language, tr,
    };
    use crate::preferences::LanguagePreference;
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
}
