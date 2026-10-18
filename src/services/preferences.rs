// The two value enums live in the kernel: `i18n` needs `LanguagePreference`,
// and the kernel is the layer that may not depend on a service. Re-exported so
// every existing `preferences::LanguagePreference` path still resolves.
pub use crate::kernel::i18n::{LanguagePreference, ThemePreference};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    /// Auto-refresh interval in seconds (0 = disabled)
    pub refresh_interval: u64,

    /// Last selected repository ID
    pub last_selected_repo_id: Option<i64>,

    /// Window width
    pub window_width: i32,

    /// Window height  
    pub window_height: i32,

    /// Show notifications
    pub enable_notifications: bool,

    /// App color theme preference
    #[serde(default)]
    pub theme_preference: ThemePreference,

    /// Follow the active Omarchy theme's colors when running on Omarchy
    #[serde(default = "default_follow_omarchy")]
    pub follow_omarchy: bool,

    /// App language preference
    #[serde(default)]
    pub language_preference: LanguagePreference,

    /// Saved run filter preferences for workflow panes
    #[serde(default)]
    pub run_filters: RunFilterPreferences,
}

/// Existing config files predate the flag; they should follow Omarchy by
/// default, exactly as a fresh install does.
fn default_follow_omarchy() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunFilterPreferences {
    pub show_success: bool,
    pub show_failed: bool,
    pub show_running: bool,
    pub default_branch_only: bool,
}

impl Default for RunFilterPreferences {
    fn default() -> Self {
        Self {
            show_success: true,
            show_failed: true,
            show_running: true,
            default_branch_only: false,
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            refresh_interval: 10, // Default 10 seconds
            last_selected_repo_id: None,
            window_width: 1000,
            window_height: 700,
            enable_notifications: true,
            theme_preference: ThemePreference::System,
            follow_omarchy: true,
            language_preference: LanguagePreference::System,
            run_filters: RunFilterPreferences::default(),
        }
    }
}

#[derive(Clone)]
pub struct PreferencesManager {
    prefs: Arc<RwLock<Preferences>>,
    config_path: PathBuf,
    updates: watch::Sender<Preferences>,
}

impl PreferencesManager {
    pub fn new() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("actioneer");

        fs::create_dir_all(&config_dir)?;
        Self::with_dir(&config_dir)
    }

    /// Reads and writes preferences inside an explicit directory.
    ///
    /// Tests point this at a `tempfile` directory; `new()` is the production
    /// entry point and picks the XDG config location.
    pub fn with_dir(config_dir: &std::path::Path) -> anyhow::Result<Self> {
        let config_path = config_dir.join("preferences.json");
        let prefs = if config_path.exists() {
            let data = fs::read_to_string(&config_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Preferences::default()
        };

        let (updates, _) = watch::channel(prefs.clone());

        Ok(Self {
            prefs: Arc::new(RwLock::new(prefs)),
            config_path,
            updates,
        })
    }

    pub async fn get(&self) -> Preferences {
        self.prefs.read().await.clone()
    }

    pub fn get_blocking(&self) -> Preferences {
        self.prefs.blocking_read().clone()
    }

    /// The latest preferences without blocking — safe on the GTK main
    /// thread, where [`Self::get_blocking`] could stall behind a writer.
    /// Falls back to the watch channel when a writer holds the lock.
    pub fn current(&self) -> Preferences {
        self.prefs
            .try_read()
            .map(|prefs| prefs.clone())
            .unwrap_or_else(|_| self.updates.borrow().clone())
    }

    pub async fn update<F>(&self, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut Preferences),
    {
        let current = {
            let mut prefs = self.prefs.write().await;
            f(&mut prefs);
            prefs.clone()
        };
        self.save(&current).await?;
        // `send_replace` stores the value even before anything subscribed;
        // `send` drops it when there are no receivers, which would leave the
        // watch-based fallback in `current()` stale.
        self.updates.send_replace(current);
        Ok(())
    }

    pub async fn set_refresh_interval(&self, seconds: u64) -> anyhow::Result<()> {
        self.update(|p| p.refresh_interval = seconds).await
    }

    pub async fn set_last_selected_repo(&self, repo_id: Option<i64>) -> anyhow::Result<()> {
        self.update(|p| p.last_selected_repo_id = repo_id).await
    }

    pub async fn set_window_size(&self, width: i32, height: i32) -> anyhow::Result<()> {
        self.update(|p| {
            p.window_width = width;
            p.window_height = height;
        })
        .await
    }

    pub async fn set_notifications_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        self.update(|p| p.enable_notifications = enabled).await
    }

    pub async fn set_theme_preference(&self, preference: ThemePreference) -> anyhow::Result<()> {
        self.update(|p| p.theme_preference = preference).await
    }

    pub async fn set_follow_omarchy(&self, enabled: bool) -> anyhow::Result<()> {
        self.update(|p| p.follow_omarchy = enabled).await
    }

    pub async fn set_language_preference(
        &self,
        preference: LanguagePreference,
    ) -> anyhow::Result<()> {
        self.update(|p| p.language_preference = preference).await
    }

    pub async fn set_run_filters(&self, filters: RunFilterPreferences) -> anyhow::Result<()> {
        self.update(|p| p.run_filters = filters).await
    }

    pub fn subscribe(&self) -> watch::Receiver<Preferences> {
        self.updates.subscribe()
    }

    async fn save(&self, prefs: &Preferences) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(prefs)?;
        tokio::fs::write(&self.config_path, data).await?;
        Ok(())
    }
}

impl Default for PreferencesManager {
    fn default() -> Self {
        Self::new().expect("Failed to create PreferencesManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_preferences_default() {
        let prefs = Preferences::default();
        assert_eq!(prefs.refresh_interval, 10); // Default 10 seconds
        assert_eq!(prefs.window_width, 1000);
        assert!(prefs.enable_notifications);
        assert_eq!(prefs.theme_preference, ThemePreference::System);
        assert!(prefs.follow_omarchy);
        assert_eq!(prefs.language_preference, LanguagePreference::System);
        assert!(prefs.run_filters.show_success);
        assert!(prefs.run_filters.show_failed);
        assert!(prefs.run_filters.show_running);
        assert!(!prefs.run_filters.default_branch_only);
    }

    #[test]
    fn follow_omarchy_defaults_to_true_when_the_field_is_missing() {
        let mut value = serde_json::to_value(Preferences::default()).expect("serialize");
        value
            .as_object_mut()
            .expect("object")
            .remove("follow_omarchy");
        let prefs: Preferences = serde_json::from_value(value).expect("deserialize");
        assert!(prefs.follow_omarchy, "old configs must follow Omarchy");

        let off = Preferences {
            follow_omarchy: false,
            ..Preferences::default()
        };
        assert_eq!(
            serde_json::to_value(off).expect("serialize")["follow_omarchy"],
            serde_json::json!(false)
        );
    }

    #[tokio::test]
    async fn current_reads_the_latest_preferences() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manager = PreferencesManager::with_dir(dir.path()).expect("manager");
        assert!(manager.current().follow_omarchy);
        manager.set_follow_omarchy(false).await.expect("set");
        assert!(!manager.current().follow_omarchy);
    }

    #[tokio::test]
    async fn test_preferences_update() {
        // A directory of its own: `PreferencesManager::new()` is the
        // developer's real preferences, which this test used to rewrite.
        let dir = tempfile::tempdir().unwrap();
        let manager = PreferencesManager::with_dir(dir.path()).unwrap();

        manager.set_refresh_interval(30).await.unwrap();

        let prefs = manager.get().await;
        assert_eq!(prefs.refresh_interval, 30);
    }

    #[test]
    fn language_preference_serde_round_trip() {
        let it: LanguagePreference = serde_json::from_str("\"it\"").unwrap();
        assert_eq!(it, LanguagePreference::It);
        let ja: LanguagePreference = serde_json::from_str("\"ja\"").unwrap();
        assert_eq!(ja, LanguagePreference::Ja);
        assert_eq!(
            serde_json::to_string(&LanguagePreference::It).unwrap(),
            "\"it\""
        );
        assert_eq!(
            serde_json::to_string(&LanguagePreference::Ja).unwrap(),
            "\"ja\""
        );
    }
}
