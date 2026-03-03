use super::{DONATION_URL, HOMEPAGE_URL, ISSUE_URL, MainWindow};
use crate::i18n::tr;
use crate::ui::preferences_window::PreferencesWindow;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use libadwaita::prelude::*;
use tracing::{error, info, warn};

impl MainWindow {
    pub(super) fn open_preferences_window(&self) {
        if let Some(manager) = &self.preferences_manager {
            let parent = self.window.clone();
            let window = PreferencesWindow::new(&parent, manager.clone());
            window.present();
        } else {
            warn!("Preferences unavailable; preferences manager failed to initialize");
            let unavailable_message = tr("Preferences are currently unavailable.");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                unavailable_message.as_str(),
            );
            dialog.connect_response(|dialog, _| dialog.close());
            dialog.present();
        }
    }

    pub(super) fn open_about_window(&self) {
        let about = adw::AboutWindow::builder()
            .transient_for(&self.window)
            .application_name("Actioneer")
            .application_icon(crate::APP_ICON_NAME)
            .developer_name("Sergey Armodin")
            .version(env!("CARGO_PKG_VERSION"))
            .website(HOMEPAGE_URL)
            .issue_url(ISSUE_URL)
            .license_type(gtk::License::MitX11)
            .build();
        about.present();
    }

    pub(super) fn open_shortcuts_window(&self) {
        let window = Self::build_shortcuts_window(&self.window);
        window.present();
    }

    fn build_shortcuts_window(parent: &adw::ApplicationWindow) -> adw::Window {
        let window = adw::Window::builder()
            .title(tr("Keyboard Shortcuts"))
            .transient_for(parent)
            .modal(true)
            .default_width(460)
            .default_height(340)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        content.append(&Self::shortcut_item(
            tr("Refresh repositories").as_str(),
            "F5",
        ));
        content.append(&Self::shortcut_item(
            tr("Open preferences").as_str(),
            "Ctrl+,",
        ));
        content.append(&Self::shortcut_item(
            tr("Show keyboard shortcuts").as_str(),
            "Ctrl+?",
        ));
        content.append(&Self::shortcut_item(tr("Open help").as_str(), "F1"));
        content.append(&Self::shortcut_item(
            tr("Quit application").as_str(),
            "Ctrl+Q",
        ));

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        content.append(&spacer);

        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let button_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        button_spacer.set_hexpand(true);
        button_row.append(&button_spacer);

        let close_button = gtk::Button::with_label(tr("Close").as_str());
        close_button.add_css_class("suggested-action");
        let window_for_close = window.clone();
        close_button.connect_clicked(move |_| {
            window_for_close.close();
        });
        button_row.append(&close_button);
        content.append(&button_row);

        window.set_content(Some(&content));
        window
    }

    fn shortcut_item(title: &str, accelerator: &str) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.set_hexpand(true);

        let accelerator_label = gtk::Label::new(Some(accelerator));
        accelerator_label.add_css_class("dim-label");
        accelerator_label.set_xalign(1.0);

        row.append(&title_label);
        row.append(&accelerator_label);
        row
    }

    pub(super) fn open_help_window(&self) {
        let window = adw::Window::builder()
            .title(tr("Actioneer Help"))
            .transient_for(&self.window)
            .modal(true)
            .default_width(620)
            .default_height(520)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let help_text = tr("Actioneer Help\n\n\
Getting started\n\
1. Sign in with your GitHub account on the welcome screen.\n\
2. Pick a repository in the left sidebar.\n\
3. Expand a workflow to inspect recent runs.\n\n\
Useful actions\n\
- Refresh repository/workflow status with F5.\n\
- Trigger manual workflow runs from the play button.\n\
- Open Preferences with Ctrl+, to adjust refresh/notifications.\n\
- Open Keyboard Shortcuts with Ctrl+? for quick references.\n\n\
Troubleshooting\n\
- If no repos appear, verify your token and network access.\n\
- For expired logs, retry from the latest run or trigger a new run.\n\
- Use Report Issue from the menu to send diagnostics and steps.");
        let label = gtk::Label::new(Some(help_text.as_str()));
        label.set_wrap(true);
        label.set_selectable(false);
        label.set_xalign(0.0);
        content.append(&label);

        let issue_hint_text = format!(
            "{}\n{}",
            tr("Need more help? Use “Report Issue” in the app menu or visit:"),
            ISSUE_URL
        );
        let issue_hint = gtk::Label::new(Some(issue_hint_text.as_str()));
        issue_hint.set_wrap(true);
        issue_hint.set_xalign(0.0);
        issue_hint.add_css_class("dim-label");
        content.append(&issue_hint);

        scrolled.set_child(Some(&content));

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_vexpand(true);
        container.set_hexpand(true);
        container.append(&scrolled);

        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        button_row.set_margin_top(12);
        button_row.set_margin_bottom(18);
        button_row.set_margin_start(18);
        button_row.set_margin_end(18);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        button_row.append(&spacer);

        let close_button = gtk::Button::with_label(tr("Close").as_str());
        close_button.add_css_class("suggested-action");
        let window_for_close = window.clone();
        close_button.connect_clicked(move |_| {
            window_for_close.close();
        });
        button_row.append(&close_button);

        container.append(&button_row);
        window.set_content(Some(&container));
        window.present();
    }

    pub(super) fn open_report_issue(&self) {
        if let Err(err) = open::that(ISSUE_URL) {
            error!("Failed to open issue tracker URL: {}", err);
            let open_issue_error = tr("Failed to open issue tracker in the browser.");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Error,
                gtk::ButtonsType::Ok,
                open_issue_error.as_str(),
            );
            dialog.connect_response(|dialog, _| dialog.close());
            dialog.present();
        }
    }

    pub(super) fn open_donation_url(&self) {
        if let Err(err) = open::that(DONATION_URL) {
            error!("Failed to open donation URL: {}", err);
            let open_donation_error = tr("Failed to open donation page in the browser.");
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Error,
                gtk::ButtonsType::Ok,
                open_donation_error.as_str(),
            );
            dialog.connect_response(|dialog, _| dialog.close());
            dialog.present();
        }
    }

    pub(super) fn dispatch_test_notification(&self) {
        info!("Debug: dispatching test notification action");
        match self.notification_manager.clone() {
            Some(manager) => {
                crate::runtime_handle().spawn(async move {
                    match manager
                        .notify_message(
                            tr("Actioneer notification test").as_str(),
                            tr("If you can read this, GNOME notifications are working.").as_str(),
                        )
                        .await
                    {
                        Ok(()) => info!("Debug: test notification dispatched"),
                        Err(err) => warn!("Failed to dispatch test notification: {}", err),
                    }
                });
            }
            None => {
                warn!("Notifications unavailable; could not send test notification");
                let notifications_unavailable = tr("Notifications are currently unavailable.");
                let dialog = gtk::MessageDialog::new(
                    Some(&self.window),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::Ok,
                    notifications_unavailable.as_str(),
                );
                dialog.connect_response(|dialog, _| dialog.close());
                dialog.present();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;
    use gtk4::glib;

    #[test]
    #[ignore = "requires GTK display"]
    fn shortcuts_window_builds_and_closes() {
        let Some(_guard) = gtk_test_guard("shortcuts_window_builds_and_closes") else {
            return;
        };

        let app = adw::Application::builder()
            .application_id("me.spaceinbox.actioneer.tests.shortcuts")
            .build();
        let parent = adw::ApplicationWindow::new(&app);

        let window = MainWindow::build_shortcuts_window(&parent);
        window.present();
        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }

        window.close();
        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }

        assert!(!window.is_visible());
    }
}
