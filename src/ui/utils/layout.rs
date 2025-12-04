use gtk4::{self as gtk, prelude::*};
use libadwaita as adw;

/// Wraps a widget in a sidebar-friendly clamp so width/scroll behavior stays consistent.
pub fn create_sidebar_clamp<W: IsA<gtk::Widget>>(child: &W) -> adw::Clamp {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(420);
    clamp.set_hexpand(false);
    clamp.set_vexpand(true);
    clamp.set_margin_top(12);
    clamp.set_margin_bottom(12);
    clamp.set_margin_start(12);
    clamp.set_margin_end(6);
    clamp.add_css_class("card");
    clamp.add_css_class("background");
    clamp.add_css_class("sidebar-surface");
    clamp.set_child(Some(child));
    clamp
}

/// Wraps a widget in the standard detail-pane clamp configuration.
pub fn create_detail_clamp<W: IsA<gtk::Widget>>(child: &W) -> adw::Clamp {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(800);
    clamp.set_hexpand(true);
    clamp.set_vexpand(true);
    clamp.set_margin_top(12);
    clamp.set_margin_bottom(24);
    clamp.set_margin_start(12);
    clamp.set_margin_end(12);
    clamp.add_css_class("card");
    clamp.add_css_class("background");
    clamp.add_css_class("detail-surface");
    clamp.set_child(Some(child));
    clamp
}
