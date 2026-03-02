use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    #[default]
    System,
    En,
    De,
    Nl,
    ZhHans,
    Hi,
    Es,
    Fr,
    Ar,
    Bn,
    PtBr,
    Ru,
    Ur,
}

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

    /// App language preference
    #[serde(default)]
    pub language_preference: LanguagePreference,

    /// Saved run filter preferences for workflow panes
    #[serde(default)]
    pub run_filters: RunFilterPreferences,
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

    pub async fn update<F>(&self, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut Preferences),
    {
        let mut prefs = self.prefs.write().await;
        f(&mut prefs);
        let current = prefs.clone();
        self.save(&current)?;
        let _ = self.updates.send(current);
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

    fn save(&self, prefs: &Preferences) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(prefs)?;
        fs::write(&self.config_path, data)?;
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
        assert_eq!(prefs.language_preference, LanguagePreference::System);
        assert!(prefs.run_filters.show_success);
        assert!(prefs.run_filters.show_failed);
        assert!(prefs.run_filters.show_running);
        assert!(!prefs.run_filters.default_branch_only);
    }

    #[tokio::test]
    async fn test_preferences_update() {
        let manager = PreferencesManager::new().unwrap();

        manager.set_refresh_interval(30).await.unwrap();

        let prefs = manager.get().await;
        assert_eq!(prefs.refresh_interval, 30);
    }
}
