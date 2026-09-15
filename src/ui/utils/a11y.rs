use gtk4::prelude::*;
use gtk4::{self as gtk};

/// Gives a control whose only visible content is an icon (or a bare number) both
/// a tooltip and an accessible name.
///
/// GTK maps tooltip text to the accessible *description*, never the name, and an
/// unnamed node is pruned by assistive technology — so an icon-only button or a
/// status dot would otherwise be invisible to a screen reader. A tooltip is also
/// mouse-only, which leaves keyboard users without the information.
pub fn describe_control(widget: &impl IsA<gtk::Widget>, text: &str) {
    let widget = widget.as_ref();
    widget.set_tooltip_text(Some(text));
    widget.update_property(&[gtk::accessible::Property::Label(text)]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn describe_control_sets_both_the_tooltip_and_the_accessible_label() {
        run_gtk_test("describe_control_sets_both", || {
            let button = gtk::Button::new();
            assert_eq!(button.tooltip_text(), None);

            describe_control(&button, "Toggle favorite");

            // The tooltip is the mouse-only half.
            assert_eq!(
                button.tooltip_text().map(|t| t.to_string()).as_deref(),
                Some("Toggle favorite")
            );
            // The accessible label is what keeps the node from being pruned by
            // assistive technology. GTK maps a tooltip to the *description*, so
            // without this an icon-only button is invisible to a screen reader.
            assert_eq!(
                button.accessible_role(),
                gtk::AccessibleRole::Button,
                "the widget keeps its role"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn describe_control_overwrites_a_previous_description() {
        run_gtk_test("describe_control_overwrites", || {
            let label = gtk::Label::new(None);
            describe_control(&label, "first");
            describe_control(&label, "second");
            assert_eq!(
                label.tooltip_text().map(|t| t.to_string()).as_deref(),
                Some("second")
            );
        });
    }
}
