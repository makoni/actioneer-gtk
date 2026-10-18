use super::{DONATION_URL, HOMEPAGE_URL, ISSUE_URL, MainWindow};
use crate::kernel::i18n::tr;
use crate::services::crash_report;
use crate::ui::preferences_window::{PREFERENCES_DIALOG_NAME, PreferencesWindow};
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::fs;
use tracing::{error, info, warn};

/// Widget names of the menu's dialogs; see `dialog_is_on_top`.
const ABOUT_DIALOG_NAME: &str = "about-dialog";
const SHORTCUTS_DIALOG_NAME: &str = "shortcuts-dialog";
const HELP_DIALOG_NAME: &str = "help-dialog";

impl MainWindow {
    /// Builds a simple informational alert with a single dismiss button.
    fn info_alert(message: &str) -> adw::AlertDialog {
        let dialog = adw::AlertDialog::new(None, Some(message));
        dialog.add_response("ok", tr("OK").as_str());
        dialog.set_default_response(Some("ok"));
        dialog
    }

    /// Whether the dialog named `name` is the one on top in the window.
    ///
    /// The menu's dialogs are hosted inside the window, so their accelerators
    /// (Ctrl+,, Ctrl+?, F1) still reach it while one is open, and pressing the
    /// same one again would stack a second copy. Only that is refused: another
    /// dialog may still open over, say, the crash report shown at startup.
    fn dialog_is_on_top(&self, name: &str) -> bool {
        self.window
            .visible_dialog()
            .is_some_and(|dialog| dialog.widget_name() == name)
    }

    pub(super) fn open_preferences_window(&self) {
        if self.dialog_is_on_top(PREFERENCES_DIALOG_NAME) {
            return;
        }
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
        if self.dialog_is_on_top(ABOUT_DIALOG_NAME) {
            return;
        }
        let about = adw::AboutDialog::builder()
            .name(ABOUT_DIALOG_NAME)
            .application_name("Actioneer")
            .application_icon(crate::kernel::app::APP_ICON_NAME)
            .developer_name("Sergey Armodin")
            .version(env!("CARGO_PKG_VERSION"))
            .website(HOMEPAGE_URL)
            .issue_url(ISSUE_URL)
            .license_type(gtk::License::MitX11)
            .build();
        about.present(Some(&self.window));
    }

    pub(super) fn open_shortcuts_window(&self) {
        if self.dialog_is_on_top(SHORTCUTS_DIALOG_NAME) {
            return;
        }
        Self::build_shortcuts_dialog().present(Some(&self.window));
    }

    /// The app's accelerators as a list. `adw::ShortcutsDialog` is the native
    /// widget for this, but it arrived in libadwaita 1.8, past this crate's
    /// floor. An `adw::Dialog` becomes a bottom sheet on a phone, and the rows'
    /// titles wrap, so long translations fit.
    fn build_shortcuts_dialog() -> adw::Dialog {
        let group = adw::PreferencesGroup::new();
        for (title, accelerator) in [
            (tr("Refresh repositories"), "F5"),
            (tr("Open preferences"), "Ctrl+,"),
            (tr("Show keyboard shortcuts"), "Ctrl+?"),
            (tr("Open help"), "F1"),
            (tr("Quit application"), "Ctrl+Q"),
        ] {
            group.add(&Self::shortcut_row(&title, accelerator));
        }
        let page = adw::PreferencesPage::new();
        page.add(&group);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&page));

        adw::Dialog::builder()
            .name(SHORTCUTS_DIALOG_NAME)
            .title(tr("Keyboard Shortcuts"))
            .content_width(420)
            .child(&toolbar)
            .build()
    }

    fn shortcut_row(title: &str, accelerator: &str) -> adw::ActionRow {
        let row = adw::ActionRow::builder().title(title).build();
        let accelerator_label = gtk::Label::new(Some(accelerator));
        accelerator_label.add_css_class("dim-label");
        // Key names read left to right in every language.
        accelerator_label.set_direction(gtk::TextDirection::Ltr);
        row.add_suffix(&accelerator_label);
        row
    }

    pub(super) fn open_help_window(&self) {
        if self.dialog_is_on_top(HELP_DIALOG_NAME) {
            return;
        }
        Self::build_help_dialog().present(Some(&self.window));
    }

    fn build_help_dialog() -> adw::Dialog {
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
        label.set_selectable(false);
        crate::ui::utils::leading_wrapping(&label);
        content.append(&label);

        let issue_hint_text = format!(
            "{}\n{}",
            tr("Need more help? Use “Report Issue” in the app menu or visit:"),
            ISSUE_URL
        );
        let issue_hint = gtk::Label::new(Some(issue_hint_text.as_str()));
        crate::ui::utils::leading_wrapping(&issue_hint);
        // The issue tracker's URL has no spaces to break at.
        issue_hint.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        issue_hint.add_css_class("dim-label");
        content.append(&issue_hint);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .child(&content)
            .build();

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scrolled));

        adw::Dialog::builder()
            .name(HELP_DIALOG_NAME)
            .title(tr("Actioneer Help"))
            .content_width(560)
            .content_height(560)
            .child(&toolbar)
            .build()
    }

    pub(super) fn open_report_issue(&self) {
        Self::report_issue(&self.window);
    }

    /// Opens the issue tracker, prefilled from a pending crash report if there
    /// is one; `window` parents the error alert if the browser cannot open.
    fn report_issue(window: &adw::ApplicationWindow) {
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
            dialog.present(Some(window));
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

        self.build_crash_report_dialog(report_text)
            .present(Some(&self.window));
    }

    /// Offers last run's crash report for copying or filing.
    ///
    /// Only the Close button marks the report as seen, as it did when this
    /// was a popover: a dialog can also be dismissed by Escape, or on a phone
    /// by swiping the sheet away, which is too easy to do by accident to cost
    /// the report — dismissed that way it is offered again next launch.
    pub(super) fn build_crash_report_dialog(&self, report_text: String) -> adw::Dialog {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(6);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);

        let message_label = gtk::Label::new(Some(
            tr("Actioneer detected a crash report from the previous run. You can report it on GitHub and attach the crash file.")
                .as_str(),
        ));
        crate::ui::utils::leading_wrapping(&message_label);
        content.append(&message_label);

        let text_view = gtk::TextView::new();
        text_view.set_editable(false);
        text_view.set_monospace(true);
        text_view.set_direction(gtk::TextDirection::Ltr);
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.buffer().set_text(report_text.as_str());
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .min_content_height(80)
            .child(&text_view)
            .build();
        scrolled.add_css_class("card");
        content.append(&scrolled);

        let copy_button = gtk::Button::with_label(tr("Copy diagnostics").as_str());
        let issue_button = gtk::Button::with_label(tr("Report Issue").as_str());
        let close_button = gtk::Button::with_label(tr("Close").as_str());
        close_button.add_css_class("suggested-action");
        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        button_row.set_halign(gtk::Align::End);
        button_row.append(&copy_button);
        button_row.append(&issue_button);
        button_row.append(&close_button);
        content.append(&button_row);

        copy_button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(report_text.as_str());
            }
        });
        // Weak: the dialog is hosted inside this window.
        let window = self.window.downgrade();
        issue_button.connect_clicked(move |_| {
            if let Some(window) = window.upgrade() {
                Self::report_issue(&window);
            }
        });

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&content));

        let dialog = adw::Dialog::builder()
            .title("Actioneer")
            .content_width(640)
            .content_height(480)
            .child(&toolbar)
            .build();
        crate::ui::utils::set_adaptive_dialog_min_size(&dialog);
        // Three buttons side by side outgrow a phone in the longer languages.
        let narrow = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 480sp")
                .expect("a valid breakpoint condition"),
        );
        narrow.add_setter(
            &button_row,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        narrow.add_setter(&button_row, "halign", Some(&gtk::Align::Fill.to_value()));
        dialog.add_breakpoint(narrow);
        // Weak: the button sits inside the dialog.
        let dialog_for_close = dialog.downgrade();
        close_button.connect_clicked(move |_| {
            if let Err(err) = crash_report::clear_pending_report() {
                warn!("Failed to clear pending crash report marker: {}", err);
            }
            if let Some(dialog) = dialog_for_close.upgrade() {
                dialog.close();
            }
        });
        dialog
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
                crate::runtime::handle().spawn(async move {
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
    fn shortcuts_dialog_builds_and_closes() {
        run_gtk_test("shortcuts_dialog_builds_and_closes", || {
            let app = adw::Application::builder()
                .application_id("me.spaceinbox.actioneer.tests.shortcuts")
                .build();
            let parent = adw::ApplicationWindow::new(&app);

            let dialog = MainWindow::build_shortcuts_dialog();
            dialog.present(Some(&parent));
            while glib::MainContext::default().pending() {
                let _ = glib::MainContext::default().iteration(false);
            }
            assert_eq!(parent.visible_dialog().as_ref(), Some(&dialog));

            dialog.force_close();
            while glib::MainContext::default().pending() {
                let _ = glib::MainContext::default().iteration(false);
            }

            assert!(parent.visible_dialog().is_none());
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn shortcuts_and_help_dialogs_are_released_once_closed() {
        run_gtk_test("shortcuts_and_help_released", || {
            let app = adw::Application::builder()
                .application_id("me.spaceinbox.actioneer.tests.dialogs")
                .build();
            let parent = adw::ApplicationWindow::new(&app);
            crate::ui::test_helpers::assert_dialog_released(
                &parent,
                MainWindow::build_shortcuts_dialog,
                "shortcuts dialog",
            );
            crate::ui::test_helpers::assert_dialog_released(
                &parent,
                MainWindow::build_help_dialog,
                "help dialog",
            );
            parent.destroy();
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
