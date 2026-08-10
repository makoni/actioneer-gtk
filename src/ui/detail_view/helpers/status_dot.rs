use gtk4::prelude::*;
use gtk4::{self as gtk};

/// Sizes used by the workflows redesign for the round status indicators.
pub(crate) const WORKFLOW_DOT_SIZE: i32 = 26;
pub(crate) const RUN_DOT_SIZE: i32 = 18;
pub(crate) const JOB_DOT_SIZE: i32 = 16;
pub(crate) const STEP_DOT_SIZE: i32 = 13;

/// Builds the round, tinted status indicator used for workflows, runs, jobs and
/// steps. The indicator is a fixed-size circle whose background/foreground come
/// from the existing status classes (`success`, `error`, `warning`, `accent`,
/// `dim-label`) — see `status-dot` rules in `style.rs`.
pub(crate) fn build_status_dot(icon_name: &str, status_class: &str, size: i32) -> gtk::Box {
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("status-dot");
    dot.set_size_request(size, size);
    dot.set_valign(gtk::Align::Center);
    dot.set_halign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(icon_pixel_size(size));
    dot.append(&icon);

    set_status_dot_state(&dot, icon_name, status_class);
    dot
}

/// Updates an existing status dot in place (icon + tint class).
pub(crate) fn set_status_dot_state(dot: &gtk::Box, icon_name: &str, status_class: &str) {
    if let Some(icon) = dot
        .first_child()
        .and_then(|child| child.downcast::<gtk::Image>().ok())
    {
        icon.set_icon_name(Some(icon_name));
    }

    for class in ["success", "error", "warning", "accent", "dim-label", "idle"] {
        dot.remove_css_class(class);
    }
    let class = if status_class.is_empty() {
        "idle"
    } else {
        status_class
    };
    dot.add_css_class(class);
}

fn icon_pixel_size(dot_size: i32) -> i32 {
    // Glyphs should be roughly half the dot size, clamped to a sane range.
    (dot_size / 2).clamp(8, 14)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn status_dot_applies_class_and_icon() {
        let Some(_guard) = gtk_test_guard("status_dot_applies_class_and_icon") else {
            return;
        };

        let dot = build_status_dot("object-select-symbolic", "success", WORKFLOW_DOT_SIZE);

        assert!(dot.has_css_class("status-dot"));
        assert!(dot.has_css_class("success"));
        let (width, height) = dot.size_request();
        assert_eq!((width, height), (WORKFLOW_DOT_SIZE, WORKFLOW_DOT_SIZE));

        let icon = dot
            .first_child()
            .and_then(|child| child.downcast::<gtk::Image>().ok())
            .expect("status dot should contain an icon");
        assert_eq!(icon.icon_name().as_deref(), Some("object-select-symbolic"));

        set_status_dot_state(&dot, "dialog-error-symbolic", "error");
        assert!(!dot.has_css_class("success"));
        assert!(dot.has_css_class("error"));
        assert_eq!(icon.icon_name().as_deref(), Some("dialog-error-symbolic"));

        // Empty status falls back to the neutral "idle" tint.
        set_status_dot_state(&dot, "media-playback-start-symbolic", "");
        assert!(dot.has_css_class("idle"));
    }
}
