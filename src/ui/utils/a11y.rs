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
