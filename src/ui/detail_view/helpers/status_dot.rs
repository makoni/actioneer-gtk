use gtk4::prelude::*;
use gtk4::{self as gtk};

/// Sizes used by the workflows redesign for the round status indicators.
pub(crate) const WORKFLOW_DOT_SIZE: i32 = 26;
pub(crate) const RUN_DOT_SIZE: i32 = 18;
pub(crate) const JOB_DOT_SIZE: i32 = 16;
/// Even, like the others: see `icon_pixel_size` for why the parity matters.
pub(crate) const STEP_DOT_SIZE: i32 = 14;

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
    // Screen readers skip an unnamed generic node; the role plus the label set
    // by `describe_control` make the status readable without a mouse.
    dot.set_accessible_role(gtk::AccessibleRole::Img);
    // With a single child, `homogeneous` hands it the box's full width, which is
    // what lets the glyph centre itself. Without it GtkBox only allocates the
    // icon its natural width and packs it against the leading edge, leaving the
    // glyph visibly off-centre inside the circle. (Expand flags would work too,
    // but they propagate to the dot and stretch it inside the row.)
    dot.set_homogeneous(true);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(icon_pixel_size(size));
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
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

/// Glyph size for a dot: roughly half the circle, clamped to a sane range, and
/// always leaving an *even* margin around itself.
///
/// GTK centres a child by integer division, so when `dot_size - pixel_size` is
/// odd the glyph lands half a pixel left of and above the true centre — never
/// right or below. At 1x that shows up as a blurred, slightly off edge; at 2x
/// the half becomes a whole physical pixel, measurable as a 2px difference
/// between the left and right margins. Matching the parity removes it outright.
fn icon_pixel_size(dot_size: i32) -> i32 {
    const MIN: i32 = 8;
    const MAX: i32 = 14;

    let target = (dot_size / 2).clamp(MIN, MAX);
    if (dot_size - target) % 2 == 0 {
        target
    } else if target > MIN {
        target - 1
    } else {
        target + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    fn every_dot_size_centres_its_glyph_exactly() {
        // An odd (dot - glyph) margin cannot be split evenly, and GTK rounds it
        // down, so the glyph sits half a pixel left of and above centre — one
        // whole physical pixel at 2x, always in the same direction. Measured on
        // screen before this rule existed: run and step dots were off by 2px
        // between their left and right margins at 2x, job dots (16 - 8) by none.
        for size in [WORKFLOW_DOT_SIZE, RUN_DOT_SIZE, JOB_DOT_SIZE, STEP_DOT_SIZE] {
            let glyph = icon_pixel_size(size);
            assert_eq!(
                (size - glyph) % 2,
                0,
                "dot {size} with a {glyph}px glyph leaves an odd margin, so the \
                 glyph cannot sit on the centre"
            );
            assert!(
                (8..=14).contains(&glyph),
                "glyph {glyph} for dot {size} left the legible range"
            );
            assert!(
                glyph * 2 <= size + size / 3,
                "glyph {glyph} is too large for a {size}px dot"
            );
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn status_dot_applies_class_and_icon() {
        run_gtk_test("status_dot_applies_class_and_icon", || {
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

            // The glyph must be centred inside the fixed-size circle, not packed
            // against its leading edge — and the dot itself must not expand, or it
            // would push the row's title sideways.
            assert!(dot.is_homogeneous());
            assert_eq!(icon.halign(), gtk::Align::Center);
            assert_eq!(icon.valign(), gtk::Align::Center);
            assert!(!dot.hexpands());

            set_status_dot_state(&dot, "dialog-error-symbolic", "error");
            assert!(!dot.has_css_class("success"));
            assert!(dot.has_css_class("error"));
            assert_eq!(icon.icon_name().as_deref(), Some("dialog-error-symbolic"));

            // The dot must be reachable by assistive tech: an unnamed generic node
            // is pruned, which would leave a screen reader with no status at all.
            assert_eq!(dot.accessible_role(), gtk::AccessibleRole::Img);

            // Empty status falls back to the neutral "idle" tint.
            set_status_dot_state(&dot, "media-playback-start-symbolic", "");
            assert!(dot.has_css_class("idle"));
        });
    }
}
