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

pub fn init() {
    let _ = setlocale(LocaleCategory::LcAll, "");
    let locale_dir = locale_dir();
    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let initial_language = std::env::var("ACTIONEER_EFFECTIVE_LANG")
        .ok()
        .unwrap_or_else(|| resolve_language_preference(LanguagePreference::System));
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

    // SAFETY: process-local environment variables for current process only.
    unsafe {
        std::env::set_var("LANGUAGE", &effective_language);
        std::env::set_var("ACTIONEER_EFFECTIVE_LANG", &effective_language);
    }

    set_effective_language(effective_language);
    let _ = setlocale(LocaleCategory::LcAll, "");
    changed
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
    EFFECTIVE_LANGUAGE
        .get_or_init(|| RwLock::new("en".to_string()))
        .read()
        .expect("effective language lock poisoned")
        .clone()
}

fn set_effective_language(language: String) {
    let lock = EFFECTIVE_LANGUAGE.get_or_init(|| RwLock::new("en".to_string()));
    let mut guard = lock.write().expect("effective language lock poisoned");
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
                translations.insert(msgid.clone(), msgstr.clone());
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
    [env!("CARGO_MANIFEST_DIR"), "po"].iter().collect()
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
        is_rtl_language, normalize_system_locale, parse_po_catalog, resolve_language_preference,
        resolve_locale_dir, set_effective_language, tr,
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
    fn normalizes_supported_locales() {
        assert_eq!(
            normalize_system_locale("ru_RU.UTF-8"),
            Some("ru".to_string())
        );
        assert_eq!(
            normalize_system_locale("pt_PT.UTF-8"),
            Some("pt_BR".to_string())
        );
        assert_eq!(
            normalize_system_locale("zh_CN.UTF-8"),
            Some("zh_Hans".to_string())
        );
        assert_eq!(normalize_system_locale("de_DE.UTF-8"), None);
    }

    #[test]
    fn explicit_language_preference_is_preserved() {
        assert_eq!(
            resolve_language_preference(LanguagePreference::Fr),
            "fr".to_string()
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
        init();
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
        init();
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
}
