use gettextrs::{
    LocaleCategory, bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain,
};
use std::path::{Path, PathBuf};

pub const GETTEXT_PACKAGE: &str = "actioneer";

pub fn init() {
    let _ = setlocale(LocaleCategory::LcAll, "");
    let locale_dir = locale_dir();
    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);
}

pub fn tr(message: &str) -> String {
    gettext(message)
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
    use super::resolve_locale_dir;
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
}
