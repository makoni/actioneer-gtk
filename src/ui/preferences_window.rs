use crate::kernel::i18n::{apply_language_preference, tr};
use crate::runtime::channel::MainContextChannelExt;
use crate::runtime::handle;
use crate::services::preferences::{
    LanguagePreference, Preferences, PreferencesManager, ThemePreference,
};
use gtk4::glib::Propagation;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::sync::Arc;
use tracing::warn;

pub struct PreferencesWindow {
    dialog: adw::PreferencesDialog,
    parent: adw::ApplicationWindow,
    manager: Arc<PreferencesManager>,
}

impl PreferencesWindow {
    pub fn new(parent: &adw::ApplicationWindow, manager: Arc<PreferencesManager>) -> Self {
        let dialog = adw::PreferencesDialog::builder()
            .title(tr("Preferences"))
            .content_width(420)
            .content_height(420)
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
            "Deutsch",
            "Nederlands",
            "简体中文",
            "हिन्दी",
            "Español",
            "Français",
            "العربية",
            "বাংলা",
            "Português (Brasil)",
            "Русский",
            "اردو",
            "Italiano",
            "日本語",
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
        dialog.add(&general_page);

        // The subscription outlives the dialog — the manager lives as long as
        // the app — so it must not keep the rows alive. Hold them weakly and
        // detach once the dialog is gone.
        let refresh_row_weak = refresh_row.downgrade();
        let notify_weak = notify_switch.downgrade();
        let theme_row_weak = theme_row.downgrade();
        let language_row_weak = language_row.downgrade();
        let (sender, receiver) =
            glib::MainContext::default().channel::<Preferences>(glib::Priority::default());

        handle().spawn({
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
            let (Some(refresh_row), Some(notify_switch), Some(theme_row), Some(language_row)) = (
                refresh_row_weak.upgrade(),
                notify_weak.upgrade(),
                theme_row_weak.upgrade(),
                language_row_weak.upgrade(),
            ) else {
                return glib::ControlFlow::Break;
            };
            let refresh_index = match prefs.refresh_interval {
                2 => 0,
                5 => 1,
                10 => 2,
                30 => 3,
                _ => 2,
            };
            refresh_row.set_selected(refresh_index);
            notify_switch.set_active(prefs.enable_notifications);
            theme_row.set_selected(theme_to_index(prefs.theme_preference));
            language_row.set_selected(language_to_index(prefs.language_preference));
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
            handle().spawn(async move {
                if let Err(err) = manager.set_refresh_interval(interval).await {
                    warn!("Failed to save refresh interval: {}", err);
                }
            });
        });

        let manager_for_notify = manager.clone();
        notify_switch.connect_state_set(move |_, state| {
            let manager = manager_for_notify.clone();
            handle().spawn(async move {
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
            handle().spawn(async move {
                if let Err(err) = manager.set_theme_preference(theme_preference).await {
                    warn!("Failed to update theme preference: {}", err);
                }
            });
        });

        let manager_for_language = manager.clone();
        // The dialog is hosted inside the parent window, so both are ancestors
        // of this row: hold them weakly or the handler pins the whole tree.
        let dialog_for_language = dialog.downgrade();
        let parent_for_language = parent.downgrade();
        language_row.connect_selected_notify(move |combo| {
            let language_preference = index_to_language(combo.selected());
            let changed = apply_language_preference(language_preference);

            let manager = manager_for_language.clone();
            handle().spawn(async move {
                if let Err(err) = manager.set_language_preference(language_preference).await {
                    warn!("Failed to update language preference: {}", err);
                }
            });

            let app = parent_for_language
                .upgrade()
                .and_then(|parent| parent.application());
            if changed && let Some(app) = app {
                if let Some(dialog) = dialog_for_language.upgrade() {
                    dialog.close();
                }
                app.activate_action("reload-ui", None);
            }
        });

        Self {
            dialog,
            parent: parent.clone(),
            manager,
        }
    }

    pub fn present(&self) {
        let _ = &self.manager;
        self.dialog.present(Some(&self.parent));
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
        LanguagePreference::De => 2,
        LanguagePreference::Nl => 3,
        LanguagePreference::ZhHans => 4,
        LanguagePreference::Hi => 5,
        LanguagePreference::Es => 6,
        LanguagePreference::Fr => 7,
        LanguagePreference::Ar => 8,
        LanguagePreference::Bn => 9,
        LanguagePreference::PtBr => 10,
        LanguagePreference::Ru => 11,
        LanguagePreference::Ur => 12,
        LanguagePreference::It => 13,
        LanguagePreference::Ja => 14,
    }
}

fn index_to_language(index: u32) -> LanguagePreference {
    match index {
        1 => LanguagePreference::En,
        2 => LanguagePreference::De,
        3 => LanguagePreference::Nl,
        4 => LanguagePreference::ZhHans,
        5 => LanguagePreference::Hi,
        6 => LanguagePreference::Es,
        7 => LanguagePreference::Fr,
        8 => LanguagePreference::Ar,
        9 => LanguagePreference::Bn,
        10 => LanguagePreference::PtBr,
        11 => LanguagePreference::Ru,
        12 => LanguagePreference::Ur,
        13 => LanguagePreference::It,
        14 => LanguagePreference::Ja,
        _ => LanguagePreference::System,
    }
}

#[cfg(test)]
mod tests {
    use super::{PreferencesWindow, index_to_language, language_to_index};
    use crate::services::preferences::{LanguagePreference, PreferencesManager};
    use crate::ui::test_helpers::{collect_widget_weaks, run_gtk_test};
    use gtk4::prelude::*;
    use gtk4::{self as gtk, glib};
    use libadwaita as adw;
    use libadwaita::prelude::*;
    use std::sync::Arc;

    #[test]
    #[ignore = "requires GTK display"]
    fn dialog_is_released_once_closed() {
        run_gtk_test("dialog_is_released_once_closed", || {
            crate::runtime::init_test_runtime();
            // The dialog lives inside the parent window, so a handler holding
            // either one strongly — or the preferences subscription holding
            // the rows — would keep the dialog's widgets alive after it closes,
            // once per opening.
            let dir = tempfile::tempdir().expect("temp dir");
            let manager = Arc::new(PreferencesManager::with_dir(dir.path()).expect("manager"));
            let parent = adw::ApplicationWindow::builder().build();
            let mut weaks: Vec<(String, glib::WeakRef<gtk::Widget>)> = Vec::new();

            let weak = {
                let prefs = PreferencesWindow::new(&parent, manager.clone());
                prefs.present();
                let dialog = prefs.dialog.clone();
                collect_widget_weaks(&dialog.clone().upcast::<gtk::Widget>(), &mut weaks);
                dialog.force_close();
                dialog.downgrade()
            };

            let context = glib::MainContext::default();
            for _ in 0..50 {
                while context.pending() {
                    let _ = context.iteration(false);
                }
                if weak.upgrade().is_none() && weaks.iter().all(|(_, w)| w.upgrade().is_none()) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            assert!(
                weak.upgrade().is_none(),
                "preferences dialog outlived its last strong reference"
            );
            let survivors: Vec<&str> = weaks
                .iter()
                .filter(|(_, weak)| weak.upgrade().is_some())
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(
                survivors.is_empty(),
                "widgets under the preferences dialog survived it: {survivors:?}"
            );
            parent.destroy();
        });
    }

    #[test]
    fn language_index_mapping_handles_de_and_nl() {
        assert_eq!(language_to_index(LanguagePreference::De), 2);
        assert_eq!(language_to_index(LanguagePreference::Nl), 3);
        assert_eq!(index_to_language(2), LanguagePreference::De);
        assert_eq!(index_to_language(3), LanguagePreference::Nl);
    }

    #[test]
    fn language_index_mapping_handles_it_and_ja() {
        assert_eq!(language_to_index(LanguagePreference::It), 13);
        assert_eq!(language_to_index(LanguagePreference::Ja), 14);
        assert_eq!(index_to_language(13), LanguagePreference::It);
        assert_eq!(index_to_language(14), LanguagePreference::Ja);
    }
}
