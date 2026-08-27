use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FavoritesData {
    repo_ids: HashSet<i64>,
}

#[derive(Clone)]
pub struct FavoritesManager {
    data: Arc<RwLock<FavoritesData>>,
    config_path: PathBuf,
    updates: watch::Sender<HashSet<i64>>,
}

impl FavoritesManager {
    pub fn new() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("actioneer");

        fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("favorites.json");
        let data = if config_path.exists() {
            let json = fs::read_to_string(&config_path)?;
            serde_json::from_str(&json).unwrap_or_default()
        } else {
            FavoritesData::default()
        };

        let (updates, _) = watch::channel(data.repo_ids.clone());

        Ok(Self {
            data: Arc::new(RwLock::new(data)),
            config_path,
            updates,
        })
    }

    pub async fn is_favorite(&self, repo_id: i64) -> bool {
        self.data.read().await.repo_ids.contains(&repo_id)
    }

    pub async fn toggle_favorite(&self, repo_id: i64) -> anyhow::Result<bool> {
        let mut data = self.data.write().await;
        let is_now_favorite = if data.repo_ids.contains(&repo_id) {
            data.repo_ids.remove(&repo_id);
            false
        } else {
            data.repo_ids.insert(repo_id);
            true
        };
        self.save(&data)?;
        self.emit_update(&data.repo_ids);
        Ok(is_now_favorite)
    }

    pub async fn add_favorite(&self, repo_id: i64) -> anyhow::Result<bool> {
        let mut data = self.data.write().await;
        data.repo_ids.insert(repo_id);
        self.save(&data)?;
        self.emit_update(&data.repo_ids);
        // The insert above makes the resulting state `true` by construction — this
        // literal is the slot a server-confirmed state fills once the manager is
        // remote.
        Ok(true)
    }

    pub async fn remove_favorite(&self, repo_id: i64) -> anyhow::Result<bool> {
        let mut data = self.data.write().await;
        data.repo_ids.remove(&repo_id);
        self.save(&data)?;
        self.emit_update(&data.repo_ids);
        // The remove above makes the resulting state `false` by construction — this
        // literal is the slot a server-confirmed state fills once the manager is
        // remote.
        Ok(false)
    }

    pub async fn get_all(&self) -> HashSet<i64> {
        self.data.read().await.repo_ids.clone()
    }

    pub async fn clear_all(&self) -> anyhow::Result<()> {
        let mut data = self.data.write().await;
        data.repo_ids.clear();
        self.save(&data)?;
        self.emit_update(&data.repo_ids);
        Ok(())
    }

    pub fn subscribe(&self) -> watch::Receiver<HashSet<i64>> {
        self.updates.subscribe()
    }

    fn emit_update(&self, repo_ids: &HashSet<i64>) {
        self.updates.send_replace(repo_ids.clone());
    }

    fn save(&self, data: &FavoritesData) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        fs::write(&self.config_path, json)?;
        Ok(())
    }
}

impl Default for FavoritesManager {
    fn default() -> Self {
        Self::new().expect("Failed to create FavoritesManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_favorites_toggle() {
        let manager = FavoritesManager::new().unwrap();
        let repo_id = 12345;

        // Initially not a favorite
        assert!(!manager.is_favorite(repo_id).await);

        // Add to favorites
        let is_fav = manager.toggle_favorite(repo_id).await.unwrap();
        assert!(is_fav);
        assert!(manager.is_favorite(repo_id).await);

        // Remove from favorites
        let is_fav = manager.toggle_favorite(repo_id).await.unwrap();
        assert!(!is_fav);
        assert!(!manager.is_favorite(repo_id).await);
    }

    #[tokio::test]
    async fn test_favorites_get_all() {
        let manager = FavoritesManager::new().unwrap();

        // Ensure a clean slate in case prior runs left persisted favorites
        manager.clear_all().await.unwrap();

        manager.add_favorite(1).await.unwrap();
        manager.add_favorite(2).await.unwrap();
        manager.add_favorite(3).await.unwrap();

        let all = manager.get_all().await;
        assert_eq!(all.len(), 3);
        assert!(all.contains(&1));
        assert!(all.contains(&2));
        assert!(all.contains(&3));
    }
}
