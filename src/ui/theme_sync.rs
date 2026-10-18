//! Keeps the app's appearance in sync with preferences and the desktop.
//!
//! Two inputs decide how the app looks: the user's System/Light/Dark
//! preference, and — when "Follow Omarchy theme" is on and the desktop is
//! Omarchy — the active theme palette. This module owns both halves: it
//! applies the effective appearance and watches the Omarchy state directory
//! so a theme switch reaches the app without a restart.

use crate::services::omarchy_theme::{self, OmarchyTheme, PaletteMode};
use crate::services::preferences::{Preferences, PreferencesManager, ThemePreference};
use crate::ui::style;
use gtk4::gio::prelude::{FileExt, FileMonitorExt};
use gtk4::{gio, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Coalesces the burst of file events one `omarchy theme set` produces —
/// staging directory created, theme replaced, name rewritten — into a single
/// re-read.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(75);

thread_local! {
    /// Kept for the app's lifetime: dropping a monitor stops the live updates.
    static STATE_MONITOR: RefCell<Option<gio::FileMonitor>> = const { RefCell::new(None) };
    /// Re-armed after every refresh: a theme switch replaces `current/theme`
    /// (`rm -rf theme && mv next-theme theme`), which leaves a monitor on the
    /// old inode watching a deleted directory.
    static THEME_MONITOR: RefCell<Option<gio::FileMonitor>> = const { RefCell::new(None) };
    /// The pending debounce source, replaced on every new event.
    static PENDING_REFRESH: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

/// Applies the effective appearance: the Omarchy palette and its square
/// corners when following is enabled and a theme is available, the plain
/// preference otherwise. Re-applying what is already applied re-parses no
/// stylesheet (see `style`).
pub fn apply_preferences(prefs: &Preferences) {
    let theme = if prefs.follow_omarchy {
        omarchy_theme::load()
    } else {
        None
    };
    apply_appearance(prefs, theme.as_ref());
}

fn apply_appearance(prefs: &Preferences, theme: Option<&OmarchyTheme>) {
    match theme {
        Some(theme) => {
            style::apply_omarchy_palette(theme);
            style::apply_square_corners();
            adw::StyleManager::default().set_color_scheme(scheme_for_mode(theme.mode));
            info!(theme = %theme.name, "Following the Omarchy theme");
        }
        None => {
            style::clear_omarchy_palette();
            style::clear_square_corners();
            adw::StyleManager::default()
                .set_color_scheme(scheme_for_preference(prefs.theme_preference));
        }
    }
}

/// Watches the Omarchy state directory and re-applies the appearance when the
/// desktop theme changes. A no-op on machines without Omarchy state.
pub fn start_watching(manager: Arc<PreferencesManager>) {
    STATE_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let Some(state_dir) = omarchy_theme::current_state_dir() else {
            return;
        };
        if let Some(monitor) = watch_dir(&state_dir, manager.clone()) {
            slot.borrow_mut().replace(monitor);
        }
    });
    arm_theme_monitor(&manager);
}

/// (Re)points the palette-directory monitor at the current theme directory.
fn arm_theme_monitor(manager: &Arc<PreferencesManager>) {
    THEME_MONITOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        // Dropping the old monitor cancels it.
        slot.take();
        let Some(theme_dir) = omarchy_theme::current_theme_dir() else {
            return;
        };
        if let Some(monitor) = watch_dir(&theme_dir, manager.clone()) {
            slot.replace(monitor);
        }
    });
}

fn watch_dir(dir: &Path, manager: Arc<PreferencesManager>) -> Option<gio::FileMonitor> {
    let monitor = gio::File::for_path(dir)
        .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        .ok()?;
    monitor.connect_changed(move |_, _, _, _| schedule_refresh(manager.clone()));
    Some(monitor)
}

fn schedule_refresh(manager: Arc<PreferencesManager>) {
    PENDING_REFRESH.with(|pending| {
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let source = glib::timeout_add_local_once(REFRESH_DEBOUNCE, move || {
            PENDING_REFRESH.with(|pending| {
                pending.borrow_mut().take();
            });
            let prefs = manager.current();
            let theme = if prefs.follow_omarchy {
                omarchy_theme::load()
            } else {
                None
            };
            let state_present = omarchy_theme::current_state_dir().is_some();
            match refresh_action(prefs.follow_omarchy, theme.is_some(), state_present) {
                Refresh::Apply => apply_appearance(&prefs, theme.as_ref()),
                Refresh::KeepCurrent => {
                    info!("The Omarchy theme is being replaced; keeping the current appearance")
                }
            }
            arm_theme_monitor(&manager);
        });
        pending.replace(Some(source));
    });
}

/// What a refresh triggered by a change under the Omarchy state does.
#[derive(Debug, PartialEq, Eq)]
enum Refresh {
    Apply,
    /// The theme is missing while the state that names it is still there:
    /// `omarchy theme set` is between `rm -rf theme` and `mv next-theme
    /// theme`. Dropping the palette now would flash stock Adwaita — rounded,
    /// perhaps light — until the next event; the move fires one, so wait.
    KeepCurrent,
}

fn refresh_action(follow: bool, theme_loaded: bool, state_present: bool) -> Refresh {
    if follow && !theme_loaded && state_present {
        Refresh::KeepCurrent
    } else {
        Refresh::Apply
    }
}

fn scheme_for_mode(mode: PaletteMode) -> adw::ColorScheme {
    match mode {
        PaletteMode::Dark => adw::ColorScheme::ForceDark,
        PaletteMode::Light => adw::ColorScheme::ForceLight,
    }
}

fn scheme_for_preference(preference: ThemePreference) -> adw::ColorScheme {
    match preference {
        ThemePreference::System => adw::ColorScheme::Default,
        ThemePreference::Light => adw::ColorScheme::ForceLight,
        ThemePreference::Dark => adw::ColorScheme::ForceDark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_mid_theme_switch_keeps_the_current_appearance() {
        assert_eq!(refresh_action(true, false, true), Refresh::KeepCurrent);
    }

    #[test]
    fn a_refresh_applies_once_the_theme_is_back_or_the_state_is_gone() {
        assert_eq!(refresh_action(true, true, true), Refresh::Apply);
        // No Omarchy state at all: the desktop stopped being Omarchy.
        assert_eq!(refresh_action(true, false, false), Refresh::Apply);
    }

    #[test]
    fn a_refresh_applies_when_not_following() {
        assert_eq!(refresh_action(false, false, true), Refresh::Apply);
        assert_eq!(refresh_action(false, false, false), Refresh::Apply);
    }

    #[test]
    fn a_palette_mode_forces_the_matching_scheme() {
        assert_eq!(
            scheme_for_mode(PaletteMode::Dark),
            adw::ColorScheme::ForceDark
        );
        assert_eq!(
            scheme_for_mode(PaletteMode::Light),
            adw::ColorScheme::ForceLight
        );
    }

    #[test]
    fn the_preference_maps_to_its_scheme() {
        assert_eq!(
            scheme_for_preference(ThemePreference::System),
            adw::ColorScheme::Default
        );
        assert_eq!(
            scheme_for_preference(ThemePreference::Light),
            adw::ColorScheme::ForceLight
        );
        assert_eq!(
            scheme_for_preference(ThemePreference::Dark),
            adw::ColorScheme::ForceDark
        );
    }
}
