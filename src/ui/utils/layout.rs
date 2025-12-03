use gtk4::{self as gtk, prelude::*};
use libadwaita as adw;

/// Wraps a widget in a sidebar-friendly ClampScrollable so width/scroll behavior stays consistent.
pub fn create_sidebar_clamp<W: IsA<gtk::Widget>>(child: &W) -> adw::ClampScrollable {
    let clamp = adw::ClampScrollable::new();
    clamp.set_maximum_size(420);
    clamp.set_hexpand(false);
    clamp.set_vexpand(true);
    clamp.set_child(Some(child));
    clamp
}

/// Wraps a widget in the standard detail-pane ClampScrollable configuration.
pub fn create_detail_clamp<W: IsA<gtk::Widget>>(child: &W) -> adw::ClampScrollable {
    let clamp = adw::ClampScrollable::new();
    clamp.set_maximum_size(800);
    clamp.set_hexpand(true);
    clamp.set_vexpand(true);
    clamp.set_child(Some(child));
    clamp
}
