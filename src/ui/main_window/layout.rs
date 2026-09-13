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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn the_sidebar_clamp_is_narrow_and_holds_its_child() {
        run_gtk_test("sidebar_clamp", || {
            let child = gtk::Label::new(Some("child"));
            let clamp = create_sidebar_clamp(&child);

            assert_eq!(clamp.maximum_size(), 420);
            assert!(!clamp.hexpands(), "the sidebar must not take slack width");
            assert!(clamp.vexpands());
            assert!(clamp.has_css_class("sidebar-surface"));
            assert_eq!(
                clamp.child().map(|c| c.type_()),
                Some(child.type_()),
                "the child is installed"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_detail_clamp_is_wide_and_takes_the_slack() {
        run_gtk_test("detail_clamp", || {
            let child = gtk::Label::new(Some("child"));
            let clamp = create_detail_clamp(&child);

            assert_eq!(clamp.maximum_size(), 800);
            assert!(clamp.hexpands(), "the detail pane absorbs the extra width");
            assert!(clamp.child().is_some());
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_two_clamps_differ_in_width_budget() {
        run_gtk_test("clamps_differ", || {
            let a = gtk::Label::new(None);
            let b = gtk::Label::new(None);
            assert!(
                create_detail_clamp(&a).maximum_size() > create_sidebar_clamp(&b).maximum_size()
            );
        });
    }
}
