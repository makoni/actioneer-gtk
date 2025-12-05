use gtk4::{self as gtk, prelude::*};

#[derive(Clone)]
pub struct FilterChips {
    pub success: gtk::ToggleButton,
    pub failed: gtk::ToggleButton,
    pub running: gtk::ToggleButton,
}

#[derive(Clone)]
pub struct FilterControls {
    container: gtk::Box,
    pub chips: FilterChips,
}

impl FilterControls {
    pub fn new() -> Self {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        container.add_css_class("linked");
        container.set_valign(gtk::Align::Center);

        let success_chip = create_status_toggle("emblem-ok-symbolic", "Show successful runs");
        let failed_chip = create_status_toggle("dialog-error-symbolic", "Show failed runs");
        let running_chip =
            create_status_toggle("media-playback-start-symbolic", "Show running/queued runs");

        container.append(&success_chip);
        container.append(&failed_chip);
        container.append(&running_chip);

        let chips = FilterChips {
            success: success_chip,
            failed: failed_chip,
            running: running_chip,
        };

        Self { container, chips }
    }

    pub fn widget(&self) -> gtk::Box {
        self.container.clone()
    }
}

fn create_status_toggle(icon_name: &str, tooltip: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();

    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("compact");
    button.set_focus_on_click(true);
    button.set_size_request(32, 32);
    button.set_active(true);
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires GTK display"]
    fn filter_controls_build_expected_chips() {
        gtk::init().expect("GTK init failed");
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
    }
}
