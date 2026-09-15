//! The composition root's payload: every service the UI needs, built once.
//!
//! Before this existed, `MainWindow::new` constructed its own
//! `FavoritesManager`, `PreferencesManager`, `DataCache` and
//! `NotificationManager`, which is why the window could not be built in a test.
//! Construction now happens here and the window receives the result.
//!
//! `Clone` matters: `MainWindow` rebuilds itself on a language change
//! (`reload_window_for_language_change`), and that second construction needs
//! services too. Every field is an `Arc` or an `Option<Arc>`, so a clone shares
//! rather than copies.
//!
//! The gateway is a **slot**, not a value. The token is unknown at composition
//! time: the app starts signed out, and the gateway appears only when the user
//! signs in or enters demo mode.

use crate::domain::models::{RateLimitInfo, Repo};
use crate::services::cache::{CachePersistenceConfig, DataCache};
use crate::services::favorites::FavoritesManager;
use crate::services::gateway::GitHubGateway;
use crate::services::notifications::NotificationManager;
use crate::services::preferences::PreferencesManager;
use gtk4::gio;
use gtk4::prelude::IsA;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppServices {
    /// Empty until the user signs in or enters demo mode.
    pub gateway: Arc<Mutex<Option<GitHubGateway>>>,
    pub favorites: Option<Arc<FavoritesManager>>,
    pub preferences: Option<Arc<PreferencesManager>>,
    pub cache: Arc<DataCache>,
    pub notifications: Option<NotificationManager>,
}

impl AppServices {
    /// Builds the real services. The only place that does.
    ///
    /// Takes `IsA<gio::Application>` rather than `adw::Application` so this
    /// module does not name libadwaita — `NotificationManager` only needs the
    /// application to resolve its id.
    pub fn build<A: IsA<gio::Application>>(app: &A) -> Self {
        let cache = Arc::new(
            CachePersistenceConfig::for_app(crate::kernel::app::APP_ID)
                .map_or_else(DataCache::new, DataCache::with_persistence),
        );
        if cache.has_persistence() {
            let cache_clone = cache.clone();
            crate::runtime::handle().spawn(async move {
                if cache_clone.hydrate_from_disk().await {
                    info!("Loaded cache snapshot from disk");
                }
            });
        }

        let favorites = match FavoritesManager::new() {
            Ok(manager) => Some(Arc::new(manager)),
            Err(err) => {
                warn!("Failed to initialize FavoritesManager: {}", err);
                None
            }
        };
        let preferences = match PreferencesManager::new() {
            Ok(manager) => Some(Arc::new(manager)),
            Err(err) => {
                warn!("Failed to initialize PreferencesManager: {}", err);
                None
            }
        };

        Self {
            gateway: Arc::new(Mutex::new(None)),
            favorites,
            preferences,
            cache,
            notifications: Some(NotificationManager::for_application(app)),
        }
    }

    /// Services backed by temporary files and demo data, for tests.
    ///
    /// Unconditionally `pub` rather than `#[cfg(test)]`: integration tests link
    /// the *non-test* library and cannot see `#[cfg(test)]` items.
    ///
    /// The caller owns the directory — usually a `tempfile::TempDir` — which is
    /// what keeps `tempfile` a dev-dependency instead of dragging it into the
    /// shipped library.
    #[doc(hidden)]
    pub fn test_fakes(dir: &std::path::Path) -> Self {
        Self {
            gateway: Arc::new(Mutex::new(Some(GitHubGateway::demo()))),
            favorites: FavoritesManager::with_path(dir.join("favorites.json"))
                .ok()
                .map(Arc::new),
            preferences: PreferencesManager::with_dir(dir).ok().map(Arc::new),
            cache: Arc::new(DataCache::new()),
            // `for_application` would need a running GApplication and a main
            // context; `new` falls back to a no-op dispatcher, which is what a
            // test wants.
            notifications: Some(NotificationManager::new(crate::kernel::app::APP_ID)),
        }
    }

    // ---- slot transitions -------------------------------------------------
    //
    // These change *which data source is installed* and nothing else. The UI
    // reaction — repainting the sidebar, stopping timers, swapping stacks —
    // stays in `MainWindow`, because it is UI.

    /// Installs a demo gateway and returns the data to seed the UI with.
    pub fn enter_demo(&self) -> (Vec<Repo>, Option<RateLimitInfo>) {
        let gateway = GitHubGateway::demo();
        let seed = gateway.demo_seed().expect("a demo gateway always seeds");
        *self.gateway.lock() = Some(gateway);
        seed
    }

    /// Installs a live gateway for `token`. Returns false if it could not be
    /// built, leaving the slot untouched.
    pub fn authenticate(&self, token: String) -> bool {
        match GitHubGateway::live(Some(token)) {
            Ok(gateway) => {
                *self.gateway.lock() = Some(gateway);
                true
            }
            Err(err) => {
                warn!("Failed to initialize GitHub gateway: {}", err);
                false
            }
        }
    }

    /// Empties the slot.
    pub fn sign_out(&self) {
        *self.gateway.lock() = None;
    }
}
