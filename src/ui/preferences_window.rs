use crate::preferences::{Preferences, PreferencesManager};
use crate::runtime_handle;
use crate::ui::utils::MainContextChannelExt;
use gtk4::glib::Propagation;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::sync::Arc;
use tracing::warn;

pub struct PreferencesWindow {
    window: adw::PreferencesWindow,
    manager: Arc<PreferencesManager>,
}

impl PreferencesWindow {
    pub fn new(parent: &adw::ApplicationWindow, manager: Arc<PreferencesManager>) -> Self {
        let window = adw::PreferencesWindow::builder()
            .title("Preferences")
            .transient_for(parent)
            .modal(true)
            .default_width(420)
            .default_height(360)
            .build();

        let general_page = adw::PreferencesPage::new();

        let refresh_group = adw::PreferencesGroup::builder().title("Refresh").build();

        // Create string list for combo row options
        let string_list =
            gtk::StringList::new(&["2 seconds", "5 seconds", "10 seconds", "30 seconds"]);

        let refresh_row = adw::ComboRow::builder()
            .title("Auto-refresh interval")
            .subtitle("How often to check for workflow run updates")
            .model(&string_list)
            .build();

        refresh_group.add(&refresh_row);

        let notifications_group = adw::PreferencesGroup::builder()
            .title("Notifications")
            .build();

        let notify_row = adw::ActionRow::builder()
            .title("Desktop Notifications")
            .subtitle("Show a notification when a workflow run finishes")
            .build();
        let notify_switch = gtk::Switch::new();
        notify_switch.set_hexpand(false);
        notify_switch.set_vexpand(false);
        notify_switch.set_halign(gtk::Align::End);
        notify_switch.set_valign(gtk::Align::Center);
        notify_row.add_suffix(&notify_switch);
        notify_row.set_activatable_widget(Some(&notify_switch));
        notifications_group.add(&notify_row);

        general_page.add(&refresh_group);
        general_page.add(&notifications_group);
        window.add(&general_page);

        let combo_clone = refresh_row.clone();
        let notify_clone = notify_switch.clone();
        let (sender, receiver) =
            glib::MainContext::default().channel::<Preferences>(glib::Priority::default());

        runtime_handle().spawn({
            let manager = manager.clone();
            async move {
                let mut updates = manager.subscribe();
                if sender.send(updates.borrow().clone()).is_err() {
                    return;
                }

                while updates.changed().await.is_ok() {
                    if sender.send(updates.borrow().clone()).is_err() {
                        break;
                    }
                }
            }
        });

        receiver.attach(None, move |prefs| {
            // Map interval to combo row index
            let index = match prefs.refresh_interval {
                2 => 0,
                5 => 1,
                10 => 2,
                30 => 3,
                _ => 2, // Default to 10 seconds
            };
            combo_clone.set_selected(index);
            notify_clone.set_active(prefs.enable_notifications);
            glib::ControlFlow::Continue
        });

        let manager_for_combo = manager.clone();
        refresh_row.connect_selected_notify(move |combo| {
            let selected = combo.selected();
            // Map index to interval in seconds
            let interval = match selected {
                0 => 2,
                1 => 5,
                2 => 10,
                3 => 30,
                _ => 10, // Default fallback
            };
            let manager = manager_for_combo.clone();
            runtime_handle().spawn(async move {
                if let Err(err) = manager.set_refresh_interval(interval).await {
                    warn!("Failed to save refresh interval: {}", err);
                }
            });
        });

        let manager_for_notify = manager.clone();
        notify_switch.connect_state_set(move |_, state| {
            let manager = manager_for_notify.clone();
            runtime_handle().spawn(async move {
                if let Err(err) = manager.set_notifications_enabled(state).await {
                    warn!("Failed to update notifications preference: {}", err);
                }
            });
            Propagation::Proceed
        });

        Self { window, manager }
    }

    pub fn present(&self) {
        // Hold a reference to the manager so subscriptions stay alive while the window is visible.
        let _ = &self.manager;
        self.window.present();
    }
}
