use gtk4::{self as gtk};

/// Preferred heart icon for favorites, with a star fallback for icon themes
/// (e.g. newer Adwaita) that dropped `emblem-favorite-symbolic`.
pub fn favorite_icon_name() -> &'static str {
    let has_heart = gtk::gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display))
        .is_some_and(|theme| theme.has_icon("emblem-favorite-symbolic"));

    if has_heart {
        "emblem-favorite-symbolic"
    } else {
        "starred-symbolic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn favorite_icon_is_one_of_the_two_supported_names() {
        run_gtk_test("favorite_icon_name_is_supported", || {
            // Which one depends on the icon theme the display offers; newer
            // Adwaita dropped the heart, which is why the fallback exists.
            let name = favorite_icon_name();
            assert!(
                matches!(name, "emblem-favorite-symbolic" | "starred-symbolic"),
                "unexpected favorite icon name: {name}"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_chosen_favorite_icon_exists_in_the_theme() {
        run_gtk_test("favorite_icon_exists", || {
            let display = gtk::gdk::Display::default().expect("a display under test");
            let theme = gtk::IconTheme::for_display(&display);
            assert!(
                theme.has_icon(favorite_icon_name()),
                "the fallback must resolve in the active theme"
            );
        });
    }
}
