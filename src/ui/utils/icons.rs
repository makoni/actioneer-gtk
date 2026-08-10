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
