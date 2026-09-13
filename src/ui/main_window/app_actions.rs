//! Registering the window's and the application's actions, menus and accels.
//!
//! Split out of `main_window.rs`. Wiring GActions is its own concern and it was
//! a quarter of the file; `window_actions.rs` next door holds the handlers
//! these actions invoke.

use super::*;

impl MainWindow {
    pub(super) fn setup_header_menu(&self, header: &adw::HeaderBar) {
        self.ensure_window_actions();

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(tr("Application menu"))
            .build();
        menu_button.add_css_class("flat");

        let menu = Menu::new();
        let preferences_label = tr("Preferences");
        let shortcuts_label = tr("Keyboard Shortcuts");
        let help_label = tr("Help");
        let sign_out_label = tr("Sign out");
        let report_issue_label = tr("Report Issue");
        let about_label = tr("About Actioneer");
        let donate_label = tr("Donate");
        let debug_label = tr("Debug");
        let quit_label = tr("Quit");
        menu.append(Some(preferences_label.as_str()), Some("app.preferences"));
        menu.append(Some(shortcuts_label.as_str()), Some("app.shortcuts"));
        menu.append(Some(help_label.as_str()), Some("app.help"));
        if cfg!(debug_assertions) {
            let debug_menu = Menu::new();
            let test_notification_label = tr("Send test notification");
            let trigger_test_crash_label = tr("Trigger test crash");
            debug_menu.append(
                Some(test_notification_label.as_str()),
                Some("win.send_test_notification"),
            );
            debug_menu.append(
                Some(trigger_test_crash_label.as_str()),
                Some("win.trigger_test_crash"),
            );
            menu.append_submenu(Some(debug_label.as_str()), &debug_menu);
        }
        menu.append(Some(sign_out_label.as_str()), Some("win.sign_out"));
        menu.append(Some(report_issue_label.as_str()), Some("app.report_issue"));
        menu.append(Some(about_label.as_str()), Some("app.about"));
        menu.append(Some(donate_label.as_str()), Some("app.donate"));
        menu.append(Some(quit_label.as_str()), Some("app.quit"));

        menu_button.set_menu_model(Some(&menu));
        header.pack_end(&menu_button);
    }

    pub(super) fn ensure_window_actions(&self) {
        let window = self.window.clone();

        if window.lookup_action("open_preferences").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("open_preferences", None);
            action.connect_activate(move |_, _| {
                this.open_preferences_window();
            });
            window.add_action(&action);
        }

        if window.lookup_action("sign_out").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("sign_out", None);
            action.connect_activate(move |_, _| {
                this.show_sign_out_dialog();
            });
            window.add_action(&action);
        }

        if cfg!(debug_assertions) && window.lookup_action("send_test_notification").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("send_test_notification", None);
            action.connect_activate(move |_, _| {
                this.dispatch_test_notification();
            });
            window.add_action(&action);
        }

        if cfg!(debug_assertions) && window.lookup_action("trigger_test_crash").is_none() {
            let this = self.clone();
            let action = gio::SimpleAction::new("trigger_test_crash", None);
            action.connect_activate(move |_, _| {
                this.trigger_test_crash();
            });
            window.add_action(&action);
        }
    }

    pub(super) fn ensure_app_actions(&self, app: &adw::Application) {
        let replace_action = |name: &str, action: &gio::SimpleAction| {
            if app.lookup_action(name).is_some() {
                app.remove_action(name);
            }
            app.add_action(action);
        };

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("preferences", None);
            action.connect_activate(move |_, _| {
                this.open_preferences_window();
            });
            replace_action("preferences", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("about", None);
            action.connect_activate(move |_, _| {
                this.open_about_window();
            });
            replace_action("about", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("shortcuts", None);
            action.connect_activate(move |_, _| {
                this.open_shortcuts_window();
            });
            replace_action("shortcuts", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("help", None);
            action.connect_activate(move |_, _| {
                this.open_help_window();
            });
            replace_action("help", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("report_issue", None);
            action.connect_activate(move |_, _| {
                this.open_report_issue();
            });
            replace_action("report_issue", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("donate", None);
            action.connect_activate(move |_, _| {
                this.open_donation_url();
            });
            replace_action("donate", &action);
        }

        {
            let this = self.clone();
            let action = gio::SimpleAction::new("refresh", None);
            action.connect_activate(move |_, _| {
                if this.client.lock().is_some() {
                    this.load_repositories();
                } else {
                    warn!("Cannot refresh: GitHub client not initialized");
                }
            });
            replace_action("refresh", &action);
        }

        if app.lookup_action("quit").is_none() {
            let app_clone = app.clone();
            let action = gio::SimpleAction::new("quit", None);
            action.connect_activate(move |_, _| {
                app_clone.quit();
            });
            app.add_action(&action);
        }

        let this = self.clone();
        let action = gio::SimpleAction::new("reload-ui", None);
        action.connect_activate(move |_, _| {
            this.reload_window_for_language_change();
        });
        replace_action("reload-ui", &action);
    }

    pub(super) fn ensure_app_accels(&self, app: &adw::Application) {
        app.set_accels_for_action("app.refresh", &["F5"]);
        app.set_accels_for_action("app.quit", &["<Primary>q"]);
        app.set_accels_for_action("app.preferences", &["<Primary>comma"]);
        app.set_accels_for_action("app.shortcuts", &["<Primary>question", "<Primary>slash"]);
        app.set_accels_for_action("app.help", &["F1"]);
    }

    pub(super) fn ensure_app_focus_action(app: &adw::Application, window: &adw::ApplicationWindow) {
        if app.lookup_action("focus-main-window").is_some() {
            return;
        }

        let window_weak = window.downgrade();
        let action = gio::SimpleAction::new("focus-main-window", None);
        action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.present();
            }
        });

        app.add_action(&action);
    }
}
