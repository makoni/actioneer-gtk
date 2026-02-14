use crate::APP_ICON_NAME;
use crate::i18n::tr;
use gtk4 as gtk;
use gtk4::prelude::*;

pub struct WelcomeScreen {
    widget: gtk::Box,
    signin_button: gtk::Button,
    demo_button: gtk::Button,
    quit_button: gtk::Button,
}

impl WelcomeScreen {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_valign(gtk::Align::Center);
        widget.set_halign(gtk::Align::Center);
        widget.set_vexpand(true);
        widget.set_hexpand(true);

        // App icon placeholder (you can replace with actual icon later)
        let icon_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        icon_box.set_halign(gtk::Align::Center);
        icon_box.set_margin_bottom(24);

        let icon = gtk::Image::from_icon_name(APP_ICON_NAME);
        icon.set_pixel_size(128);
        icon_box.append(&icon);
        widget.append(&icon_box);

        // Welcome title
        let title = gtk::Label::new(Some(tr("Welcome to Actioneer").as_str()));
        title.add_css_class("title-1");
        title.set_margin_bottom(12);
        widget.append(&title);

        // Subtitle
        let subtitle = gtk::Label::new(Some(
            tr("Manage your GitHub Actions workflows with ease").as_str(),
        ));
        subtitle.add_css_class("dim-label");
        subtitle.set_margin_bottom(36);
        widget.append(&subtitle);

        // Features list
        let features_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        features_box.set_halign(gtk::Align::Center);
        features_box.set_margin_bottom(48);

        Self::add_feature(
            &features_box,
            "media-playback-start-symbolic",
            tr("Trigger workflows instantly").as_str(),
            "success",
        );
        Self::add_feature(
            &features_box,
            "view-reveal-symbolic",
            tr("Monitor runs in real-time").as_str(),
            "accent",
        );
        Self::add_feature(
            &features_box,
            "folder-documents-symbolic",
            tr("View detailed logs").as_str(),
            "warning",
        );

        widget.append(&features_box);

        // Buttons
        let buttons_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        buttons_box.set_halign(gtk::Align::Center);
        buttons_box.set_width_request(300);

        let signin_button = gtk::Button::new();
        signin_button.add_css_class("suggested-action");
        signin_button.add_css_class("pill");
        signin_button.set_widget_name("welcome-signin-button");

        let signin_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        signin_content.set_halign(gtk::Align::Center);
        signin_content.set_valign(gtk::Align::Center);

        let signin_icon = gtk::Image::from_icon_name("avatar-default-symbolic");
        signin_icon.set_pixel_size(20);
        signin_content.append(&signin_icon);

        let signin_label = gtk::Label::new(Some(tr("Sign in with GitHub").as_str()));
        signin_label.set_halign(gtk::Align::Center);
        signin_label.add_css_class("heading");
        signin_content.append(&signin_label);

        signin_button.set_child(Some(&signin_content));
        buttons_box.append(&signin_button);

        let demo_button = gtk::Button::with_label(tr("Try Demo Mode").as_str());
        demo_button.add_css_class("pill");
        demo_button.set_widget_name("welcome-demo-button");
        demo_button.set_tooltip_text(Some(tr("Explore Actioneer with sample data").as_str()));
        if !cfg!(debug_assertions) {
            demo_button.set_visible(false);
        }
        buttons_box.append(&demo_button);

        let quit_button = gtk::Button::new();
        quit_button.add_css_class("pill");
        quit_button.add_css_class("flat");
        quit_button.set_widget_name("welcome-quit-button");

        let quit_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        quit_content.set_halign(gtk::Align::Center);
        quit_content.set_valign(gtk::Align::Center);

        let quit_icon = gtk::Image::from_icon_name("application-exit-symbolic");
        quit_icon.set_pixel_size(18);
        quit_content.append(&quit_icon);

        let quit_label = gtk::Label::new(Some(tr("Quit").as_str()));
        quit_label.set_halign(gtk::Align::Center);
        quit_content.append(&quit_label);

        quit_button.set_child(Some(&quit_content));
        buttons_box.append(&quit_button);

        widget.append(&buttons_box);

        Self {
            widget,
            signin_button,
            demo_button,
            quit_button,
        }
    }

    fn add_feature(container: &gtk::Box, icon_name: &str, text: &str, css_class: &str) {
        let feature_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        feature_box.set_halign(gtk::Align::Start);

        let icon = gtk::Image::from_icon_name(icon_name);
        icon.add_css_class(css_class);
        icon.set_pixel_size(24);
        feature_box.append(&icon);

        let label = gtk::Label::new(Some(text));
        label.set_halign(gtk::Align::Start);
        feature_box.append(&label);

        container.append(&feature_box);
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    pub fn connect_signin<F: Fn() + 'static>(&self, callback: F) {
        self.signin_button.connect_clicked(move |_| callback());
    }

    pub fn connect_demo<F: Fn() + 'static>(&self, callback: F) {
        self.demo_button.connect_clicked(move |_| callback());
    }

    pub fn connect_quit<F: Fn() + 'static>(&self, callback: F) {
        self.quit_button.connect_clicked(move |_| callback());
    }
}
