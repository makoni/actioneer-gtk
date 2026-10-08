use crate::kernel::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;

/// The two header bars of the split view: the sidebar's carries the refresh
/// button and the menu, the content's the rate-limit readout.
#[derive(Clone)]
pub struct HeaderControls {
    header_bar: adw::HeaderBar,
    content_header_bar: adw::HeaderBar,
    refresh_button: gtk::Button,
    rate_limit_label: gtk::Label,
    rate_limit_box: gtk::Box,
}

impl HeaderControls {
    pub fn new() -> Self {
        let header_bar = adw::HeaderBar::new();

        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some(tr("Refresh repositories").as_str()));
        header_bar.pack_start(&refresh_button);

        let rate_limit_label = gtk::Label::new(Some(tr("Rate limit: –").as_str()));
        rate_limit_label.add_css_class("dim-label");
        rate_limit_label.add_css_class("caption");
        rate_limit_label.set_halign(gtk::Align::End);

        let rate_limit_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        rate_limit_box.add_css_class("linked");
        rate_limit_box.append(&rate_limit_label);

        // The repository name is already the first line of the detail pane, so
        // the content header shows no title of its own — only the back button
        // when the split view is collapsed, and the rate limit.
        let content_header_bar = adw::HeaderBar::new();
        content_header_bar.set_show_title(false);
        content_header_bar.pack_end(&rate_limit_box);

        Self {
            header_bar,
            content_header_bar,
            refresh_button,
            rate_limit_label,
            rate_limit_box,
        }
    }

    pub fn header_bar(&self) -> adw::HeaderBar {
        self.header_bar.clone()
    }

    pub fn content_header_bar(&self) -> adw::HeaderBar {
        self.content_header_bar.clone()
    }

    /// The rate-limit readout and its container; a breakpoint hides the
    /// container on narrow windows, where it would crowd out the back button.
    pub fn rate_limit_box(&self) -> gtk::Box {
        self.rate_limit_box.clone()
    }

    pub fn refresh_button(&self) -> gtk::Button {
        self.refresh_button.clone()
    }

    pub fn rate_limit_label(&self) -> gtk::Label {
        self.rate_limit_label.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn header_controls_create_expected_widgets() {
        run_gtk_test("header_controls_create_expected_widgets", || {
            let controls = HeaderControls::new();
            let expected_tooltip = tr("Refresh repositories");
            assert_eq!(
                controls
                    .refresh_button()
                    .tooltip_text()
                    .as_ref()
                    .map(|s| s.as_str()),
                Some(expected_tooltip.as_str())
            );
            assert_eq!(controls.rate_limit_label().text(), tr("Rate limit: –"));
        });
    }
}
