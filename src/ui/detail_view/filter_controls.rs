use crate::i18n::tr;
use gtk4::{self as gtk, prelude::*};

#[derive(Clone)]
pub struct FilterChips {
    pub success: gtk::ToggleButton,
    pub failed: gtk::ToggleButton,
    pub running: gtk::ToggleButton,
    success_count: gtk::Label,
    failed_count: gtk::Label,
    running_count: gtk::Label,
}

impl FilterChips {
    /// Updates the per-status counters shown inside the segmented control.
    pub fn set_counts(&self, success: usize, running: usize, failed: usize) {
        self.success_count.set_text(&success.to_string());
        self.running_count.set_text(&running.to_string());
        self.failed_count.set_text(&failed.to_string());
    }
}

#[derive(Clone)]
pub struct FilterControls {
    container: gtk::Box,
    pub chips: FilterChips,
}

impl FilterControls {
    pub fn new() -> Self {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        container.add_css_class("linked");
        container.add_css_class("segmented-status-filter");
        container.set_valign(gtk::Align::Center);

        let (success_chip, success_count) = create_status_segment(
            "object-select-symbolic",
            "seg-success",
            tr("Show successful runs").as_str(),
        );
        let (running_chip, running_count) = create_status_segment(
            "media-playback-start-symbolic",
            "seg-running",
            tr("Show running/queued runs").as_str(),
        );
        let (failed_chip, failed_count) = create_status_segment(
            "dialog-error-symbolic",
            "seg-failed",
            tr("Show failed runs").as_str(),
        );

        // Design order: success, running, failure.
        container.append(&success_chip);
        container.append(&running_chip);
        container.append(&failed_chip);

        let chips = FilterChips {
            success: success_chip,
            failed: failed_chip,
            running: running_chip,
            success_count,
            failed_count,
            running_count,
        };

        Self { container, chips }
    }

    pub fn widget(&self) -> gtk::Box {
        self.container.clone()
    }
}

fn create_status_segment(
    icon_name: &str,
    segment_class: &str,
    tooltip: &str,
) -> (gtk::ToggleButton, gtk::Label) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(13);
    content.append(&icon);

    let count = gtk::Label::new(Some("0"));
    count.add_css_class("segment-count");
    count.add_css_class("caption");
    content.append(&count);

    let button = gtk::ToggleButton::builder()
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .child(&content)
        .build();

    button.add_css_class("flat");
    button.add_css_class("filter-segment");
    button.add_css_class(segment_class);
    button.set_active(true);
    (button, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn filter_controls_build_expected_chips() {
        let Some(_guard) = gtk_test_guard("filter_controls_build_expected_chips") else {
            return;
        };
        let controls = FilterControls::new();
        let toolbar = controls.widget();

        let mut count = 0;
        let mut child = toolbar.first_child();
        while let Some(widget) = child {
            count += 1;
            child = widget.next_sibling();
        }
        assert_eq!(count, 3);

        assert!(controls.chips.success.is_active());
        assert!(controls.chips.failed.is_active());
        assert!(controls.chips.running.is_active());

        assert!(controls.chips.success.has_css_class("seg-success"));
        assert!(controls.chips.running.has_css_class("seg-running"));
        assert!(controls.chips.failed.has_css_class("seg-failed"));
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn filter_chips_update_counts() {
        let Some(_guard) = gtk_test_guard("filter_chips_update_counts") else {
            return;
        };
        let controls = FilterControls::new();
        controls.chips.set_counts(7, 1, 2);

        assert_eq!(controls.chips.success_count.text().as_str(), "7");
        assert_eq!(controls.chips.running_count.text().as_str(), "1");
        assert_eq!(controls.chips.failed_count.text().as_str(), "2");
    }
}
