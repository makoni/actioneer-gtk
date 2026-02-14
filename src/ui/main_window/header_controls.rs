use crate::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;

#[derive(Clone)]
pub struct HeaderControls {
    header_bar: adw::HeaderBar,
    refresh_button: gtk::Button,
    rate_limit_label: gtk::Label,
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
        header_bar.pack_end(&rate_limit_box);

        Self {
            header_bar,
            refresh_button,
            rate_limit_label,
        }
    }

    pub fn header_bar(&self) -> adw::HeaderBar {
        self.header_bar.clone()
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
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn header_controls_create_expected_widgets() {
        let Some(_guard) = gtk_test_guard("header_controls_create_expected_widgets") else {
            return;
        };
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
    }
}
