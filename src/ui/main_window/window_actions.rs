use super::{DONATION_URL, HOMEPAGE_URL, ISSUE_URL, MainWindow};
use crate::crash_report;
use crate::i18n::tr;
use crate::ui::preferences_window::PreferencesWindow;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::fs;
use tracing::{error, info, warn};

impl MainWindow {
    /// Builds a simple informational alert with a single dismiss button.
    fn info_alert(message: &str) -> adw::AlertDialog {
        let dialog = adw::AlertDialog::new(None, Some(message));
        dialog.add_response("ok", tr("OK").as_str());
        dialog.set_default_response(Some("ok"));
        dialog
    }

    pub(super) fn open_preferences_window(&self) {
        if let Some(manager) = &self.preferences_manager {
            let parent = self.window.clone();
            let window = PreferencesWindow::new(&parent, manager.clone());
            window.present();
        } else {
            warn!("Preferences unavailable; preferences manager failed to initialize");
            let unavailable_message = tr("Preferences are currently unavailable.");
            let dialog = Self::info_alert(unavailable_message.as_str());
            dialog.present(Some(&self.window));
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
        let issue_url = crash_report::build_issue_url_for_pending_report()
            .map(|url| url.unwrap_or_else(|| ISSUE_URL.to_string()))
            .unwrap_or_else(|err| {
                warn!("Failed to prefill crash issue URL: {}", err);
                ISSUE_URL.to_string()
            });
        if let Err(err) = open::that(issue_url) {
            error!("Failed to open issue tracker URL: {}", err);
            let open_issue_error = tr("Failed to open issue tracker in the browser.");
            let dialog = Self::info_alert(open_issue_error.as_str());
            dialog.present(Some(&self.window));
        }
    }

    pub(super) fn show_pending_crash_report_dialog(&self) {
        let pending = match crash_report::read_pending_report() {
            Ok(Some(report)) => report,
            Ok(None) => return,
            Err(err) => {
                error!("Failed to read pending crash report: {}", err);
                return;
            }
        };

        let report_text = match fs::read_to_string(&pending.report_path) {
            Ok(text) => text,
            Err(err) => {
                warn!("Failed to read crash report content: {}", err);
                format!(
                    "{}\n{}",
                    tr("Crash report file:"),
                    pending.report_path.as_str()
                )
            }
        };

        let popover = gtk::Popover::builder()
            .autohide(false)
            .has_arrow(true)
            .build();
        popover.set_parent(&self.header_bar);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_size_request(640, 420);

        let message_label = gtk::Label::new(Some(
            tr("Actioneer detected a crash report from the previous run. You can report it on GitHub and attach the crash file.")
                .as_str(),
        ));
        message_label.set_wrap(true);
        message_label.set_xalign(0.0);
        content.append(&message_label);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_height(260)
            .build();
        let text_view = gtk::TextView::new();
        text_view.set_editable(false);
        text_view.set_monospace(true);
        text_view.set_direction(gtk::TextDirection::Ltr);
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.buffer().set_text(report_text.as_str());
        scrolled.set_child(Some(&text_view));
        content.append(&scrolled);

        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let copy_button = gtk::Button::with_label(tr("Copy diagnostics").as_str());
        let issue_button = gtk::Button::with_label(tr("Report Issue").as_str());
        let close_button = gtk::Button::with_label(tr("Close").as_str());
        close_button.add_css_class("suggested-action");

        let report_text_for_copy = report_text.clone();
        copy_button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(report_text_for_copy.as_str());
            }
        });

        let this = self.clone();
        issue_button.connect_clicked(move |_| {
            this.open_report_issue();
        });

        let popover_for_close = popover.clone();
        close_button.connect_clicked(move |_| {
            if let Err(err) = crash_report::clear_pending_report() {
                warn!("Failed to clear pending crash report marker: {}", err);
            }
            popover_for_close.popdown();
        });

        button_row.append(&copy_button);
        button_row.append(&issue_button);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        button_row.append(&spacer);
        button_row.append(&close_button);
        content.append(&button_row);

        popover.set_child(Some(&content));
        popover.popup();
    }

    pub(super) fn open_donation_url(&self) {
        if let Err(err) = open::that(DONATION_URL) {
            error!("Failed to open donation URL: {}", err);
            let open_donation_error = tr("Failed to open donation page in the browser.");
            let dialog = Self::info_alert(open_donation_error.as_str());
            dialog.present(Some(&self.window));
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
                let dialog = Self::info_alert(notifications_unavailable.as_str());
                dialog.present(Some(&self.window));
            }
        }
    }

    pub(super) fn trigger_test_crash(&self) {
        let _ = self;
        panic!("Intentional debug crash triggered from app menu");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;
    use gtk4::glib;

    #[test]
    #[ignore = "requires GTK display"]
    fn shortcuts_window_builds_and_closes() {
        run_gtk_test("shortcuts_window_builds_and_closes", || {
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
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn info_alert_has_single_ok_response() {
        run_gtk_test("info_alert_has_single_ok_response", || {
            let dialog = MainWindow::info_alert("Something went wrong");

            assert!(dialog.has_response("ok"));
            assert_eq!(dialog.default_response().as_deref(), Some("ok"));
        });
    }
}
