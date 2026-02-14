use crate::i18n::{apply_language_preference, tr};
use crate::preferences::{LanguagePreference, Preferences, PreferencesManager, ThemePreference};
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
            .title(tr("Preferences"))
            .transient_for(parent)
            .modal(true)
            .default_width(420)
            .default_height(420)
            .build();

        let general_page = adw::PreferencesPage::new();

        let refresh_group = adw::PreferencesGroup::builder()
            .title(tr("Refresh"))
            .build();

        let two_seconds = tr("2 seconds");
        let five_seconds = tr("5 seconds");
        let ten_seconds = tr("10 seconds");
        let thirty_seconds = tr("30 seconds");
        let refresh_options = gtk::StringList::new(&[
            two_seconds.as_str(),
            five_seconds.as_str(),
            ten_seconds.as_str(),
            thirty_seconds.as_str(),
        ]);

        let refresh_row = adw::ComboRow::builder()
            .title(tr("Auto-refresh interval"))
            .subtitle(tr("How often to check for workflow run updates"))
            .model(&refresh_options)
            .build();
        refresh_group.add(&refresh_row);

        let notifications_group = adw::PreferencesGroup::builder()
            .title(tr("Notifications"))
            .build();

        let notify_row = adw::ActionRow::builder()
            .title(tr("Desktop Notifications"))
            .subtitle(tr("Show a notification when a workflow run finishes"))
            .build();
        let notify_switch = gtk::Switch::new();
        notify_switch.set_hexpand(false);
        notify_switch.set_vexpand(false);
        notify_switch.set_halign(gtk::Align::End);
        notify_switch.set_valign(gtk::Align::Center);
        notify_row.add_suffix(&notify_switch);
        notify_row.set_activatable_widget(Some(&notify_switch));
        notifications_group.add(&notify_row);

        let appearance_group = adw::PreferencesGroup::builder()
            .title(tr("Appearance"))
            .build();
        let system_theme = tr("System");
        let light_theme = tr("Light");
        let dark_theme = tr("Dark");
        let theme_options = gtk::StringList::new(&[
            system_theme.as_str(),
            light_theme.as_str(),
            dark_theme.as_str(),
        ]);
        let theme_row = adw::ComboRow::builder()
            .title(tr("Theme"))
            .subtitle(tr("Select application theme"))
            .model(&theme_options)
            .build();
        appearance_group.add(&theme_row);

        let language_group = adw::PreferencesGroup::builder()
            .title(tr("Language"))
            .build();
        let system_language = tr("System language (fallback: English)");
        let language_options = gtk::StringList::new(&[
            system_language.as_str(),
            "English",
            "简体中文",
            "हिन्दी",
            "Español",
            "Français",
            "العربية",
            "বাংলা",
            "Português (Brasil)",
            "Русский",
            "اردو",
        ]);
        let language_row = adw::ComboRow::builder()
            .title(tr("Application language"))
            .subtitle(tr("Switch language instantly"))
            .model(&language_options)
            .build();
        language_group.add(&language_row);

        general_page.add(&refresh_group);
        general_page.add(&notifications_group);
        general_page.add(&appearance_group);
        general_page.add(&language_group);
        window.add(&general_page);

        let refresh_row_clone = refresh_row.clone();
        let notify_clone = notify_switch.clone();
        let theme_row_clone = theme_row.clone();
        let language_row_clone = language_row.clone();
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
            let refresh_index = match prefs.refresh_interval {
                2 => 0,
                5 => 1,
                10 => 2,
                30 => 3,
                _ => 2,
            };
            refresh_row_clone.set_selected(refresh_index);
            notify_clone.set_active(prefs.enable_notifications);
            theme_row_clone.set_selected(theme_to_index(prefs.theme_preference));
            language_row_clone.set_selected(language_to_index(prefs.language_preference));
            glib::ControlFlow::Continue
        });

        let manager_for_combo = manager.clone();
        refresh_row.connect_selected_notify(move |combo| {
            let interval = match combo.selected() {
                0 => 2,
                1 => 5,
                2 => 10,
                3 => 30,
                _ => 10,
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

        let manager_for_theme = manager.clone();
        theme_row.connect_selected_notify(move |combo| {
            let theme_preference = index_to_theme(combo.selected());
            apply_theme(theme_preference);

            let manager = manager_for_theme.clone();
            runtime_handle().spawn(async move {
                if let Err(err) = manager.set_theme_preference(theme_preference).await {
                    warn!("Failed to update theme preference: {}", err);
                }
            });
        });

        let manager_for_language = manager.clone();
        let window_for_language = window.clone();
        let parent_for_language = parent.clone();
        language_row.connect_selected_notify(move |combo| {
            let language_preference = index_to_language(combo.selected());
            let changed = apply_language_preference(language_preference);

            let manager = manager_for_language.clone();
            runtime_handle().spawn(async move {
                if let Err(err) = manager.set_language_preference(language_preference).await {
                    warn!("Failed to update language preference: {}", err);
                }
            });

            if changed && let Some(app) = parent_for_language.application() {
                window_for_language.close();
                app.activate_action("reload-ui", None);
            }
        });

        Self { window, manager }
    }

    pub fn present(&self) {
        let _ = &self.manager;
        self.window.present();
    }
}

fn apply_theme(theme: ThemePreference) {
    let style_manager = adw::StyleManager::default();
    match theme {
        ThemePreference::System => style_manager.set_color_scheme(adw::ColorScheme::Default),
        ThemePreference::Light => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
        ThemePreference::Dark => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
    }
}

fn theme_to_index(theme: ThemePreference) -> u32 {
    match theme {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    }
}

fn index_to_theme(index: u32) -> ThemePreference {
    match index {
        1 => ThemePreference::Light,
        2 => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

fn language_to_index(language: LanguagePreference) -> u32 {
    match language {
        LanguagePreference::System => 0,
        LanguagePreference::En => 1,
        LanguagePreference::ZhHans => 2,
        LanguagePreference::Hi => 3,
        LanguagePreference::Es => 4,
        LanguagePreference::Fr => 5,
        LanguagePreference::Ar => 6,
        LanguagePreference::Bn => 7,
        LanguagePreference::PtBr => 8,
        LanguagePreference::Ru => 9,
        LanguagePreference::Ur => 10,
    }
}

fn index_to_language(index: u32) -> LanguagePreference {
    match index {
        1 => LanguagePreference::En,
        2 => LanguagePreference::ZhHans,
        3 => LanguagePreference::Hi,
        4 => LanguagePreference::Es,
        5 => LanguagePreference::Fr,
        6 => LanguagePreference::Ar,
        7 => LanguagePreference::Bn,
        8 => LanguagePreference::PtBr,
        9 => LanguagePreference::Ru,
        10 => LanguagePreference::Ur,
        _ => LanguagePreference::System,
    }
}
