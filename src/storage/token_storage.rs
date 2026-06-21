use super::{
    portal_token_store::{PortalStoreError, PortalTokenStore},
    secret_portal::{self, PortalPreference},
};
use keyring::v1::Entry;
use keyring_core::Error as KeyringCoreError;
use std::sync::OnceLock;
use thiserror::Error;
use tracing::{debug, info, warn};

const SERVICE_NAME: &str = "me.spaceinbox.actioneer";
const TOKEN_KEY: &str = "github_token";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Keyring error: {0}")]
    KeyringError(#[from] KeyringCoreError),

    #[error("Token not found")]
    TokenNotFound,

    #[error("Secure keyring storage is not available")]
    KeyringUnavailable,

    #[error("Secret portal storage error: {0}")]
    SecretPortal(PortalStoreError),
}

pub struct TokenStorage {
    backend: Backend,
}

enum Backend {
    Portal {
        store: PortalTokenStore,
        legacy_entry: Option<Entry>,
    },
    Keyring {
        entry: Entry,
    },
}

static KEYRING_STORE_CONFIGURED: OnceLock<()> = OnceLock::new();

impl TokenStorage {
    pub fn new() -> Result<Self, StorageError> {
        info!(
            "Creating TokenStorage with service: {}, key: {}",
            SERVICE_NAME, TOKEN_KEY
        );
        let preference = secret_portal::portal_preference();

        if let Some(backend) = try_portal_backend(preference) {
            return Ok(Self { backend });
        }

        info!("Falling back to system keyring storage");
        let entry = keyring_entry()?;
        let backend = Backend::Keyring { entry };
        let entry = match &backend {
            Backend::Keyring { entry } => entry,
            Backend::Portal { .. } => unreachable!(),
        };
        ensure_keyring_ready(entry)?;
        info!("✅ Keyring is available and working");
        info!("TokenStorage initialized (keyring backend)");

        Ok(Self { backend })
    }

    /// Save the token to secure storage
    pub fn save_token(&self, token: &str) -> Result<(), StorageError> {
        match &self.backend {
            Backend::Portal { store, .. } => {
                info!("Saving token to portal-backed storage");
                store.save_token(token).map_err(map_portal_err)?;
            }
            Backend::Keyring { entry } => {
                info!(
                    "Saving token to keyring storage (service: {}, key: {})",
                    SERVICE_NAME, TOKEN_KEY
                );
                entry.set_password(token)?;
                info!("Token saved to keyring - verifying...");
                if let Ok(retrieved) = entry.get_password() {
                    if retrieved == token {
                        info!("✅ Token verified in keyring - save successful");
                    } else {
                        warn!("⚠️  Token mismatch after save!");
                    }
                }
            }
        }

        Ok(())
    }

    /// Retrieve the token from secure storage
    pub fn get_token(&self) -> Result<String, StorageError> {
        match &self.backend {
            Backend::Portal { store, .. } => {
                info!("Retrieving token from portal-backed storage");
                store.get_token().map_err(map_portal_err)
            }
            Backend::Keyring { entry } => {
                info!(
                    "Retrieving token from keyring storage (service: {}, key: {})",
                    SERVICE_NAME, TOKEN_KEY
                );
                match entry.get_password() {
                    Ok(token) => {
                        info!(
                            "✅ Token retrieved from keyring successfully (length: {} chars)",
                            token.len()
                        );
                        Ok(token)
                    }
                    Err(KeyringCoreError::NoEntry) => {
                        info!("❌ No token found in keyring storage");
                        Err(StorageError::TokenNotFound)
                    }
                    Err(e) => {
                        info!("❌ Keyring error: {:?}", e);
                        Err(StorageError::KeyringError(e))
                    }
                }
            }
        }
    }

    /// Delete the token from secure storage
    pub fn delete_token(&self) -> Result<(), StorageError> {
        info!("Deleting token from secure storage");
        match &self.backend {
            Backend::Portal {
                store,
                legacy_entry,
            } => {
                store.delete_token().map_err(map_portal_err)?;
                // Best effort cleanup in case a legacy keyring entry still exists
                if let Some(entry) = legacy_entry.as_ref() {
                    match entry.delete_credential() {
                        Ok(()) | Err(KeyringCoreError::NoEntry) => (),
                        Err(err) => warn!("Failed to clean legacy keyring entry: {err}"),
                    }
                }
                Ok(())
            }
            Backend::Keyring { entry } => match entry.delete_credential() {
                Ok(()) | Err(KeyringCoreError::NoEntry) => {
                    debug!("Keyring token deleted or already absent");
                    Ok(())
                }
                Err(e) => Err(StorageError::KeyringError(e)),
            },
        }
    }

    /// Check if a token exists
    pub fn has_token(&self) -> bool {
        match &self.backend {
            Backend::Portal { store, .. } => store.has_token(),
            Backend::Keyring { entry } => entry.get_password().is_ok(),
        }
    }
}

impl Default for TokenStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create TokenStorage")
    }
}

fn try_portal_backend(preference: PortalPreference) -> Option<Backend> {
    if !preference.use_portal() {
        debug!("Secret portal disabled (reason: {})", preference.describe());
        return None;
    }

    match secret_portal::secret_portal_available() {
        Ok(true) => debug!("Secret portal advertised by the session"),
        Ok(false) => warn!(
            "Secret portal requested but interface not advertised by the session (attempting anyway)"
        ),
        Err(err) => warn!("Secret portal detection failed (attempting anyway): {err}"),
    }

    match PortalTokenStore::new() {
        Ok(store) => {
            info!(
                "Using secret portal storage (reason: {})",
                preference.describe()
            );
            let legacy_entry = keyring_entry_for_migration();
            migrate_keyring_token(legacy_entry.as_ref(), &store);
            Some(Backend::Portal {
                store,
                legacy_entry,
            })
        }
        Err(err) => {
            warn!("Secret portal initialization failed: {err}");
            None
        }
    }
}

fn keyring_entry() -> Result<Entry, StorageError> {
    configure_keyring_store()?;
    Entry::new(SERVICE_NAME, TOKEN_KEY).map_err(StorageError::KeyringError)
}

fn keyring_entry_for_migration() -> Option<Entry> {
    match keyring_entry() {
        Ok(entry) => Some(entry),
        Err(StorageError::KeyringUnavailable) => {
            debug!("Keyring backend unavailable during portal migration; skipping legacy access");
            None
        }
        Err(err) => {
            warn!("Failed to prepare keyring migration entry: {err}");
            None
        }
    }
}

fn configure_keyring_store() -> Result<(), StorageError> {
    if KEYRING_STORE_CONFIGURED.get().is_some() {
        return Ok(());
    }

    // keyring 4.1 dropped the crate-root `use_native_store`; the `v1` API now
    // installs the platform's native store (secret-service on Linux) on the
    // first `Entry::new` via an internal `call_once`. That setup silently
    // no-ops when no session secret service is reachable, so we trigger it and
    // then confirm a default store was actually registered.
    let _ = Entry::new(SERVICE_NAME, TOKEN_KEY);
    if keyring_core::get_default_store().is_none() {
        warn!("Failed to initialize system keyring backend: no default store available");
        return Err(StorageError::KeyringUnavailable);
    }

    let _ = KEYRING_STORE_CONFIGURED.set(());
    Ok(())
}

fn migrate_keyring_token(entry: Option<&Entry>, store: &PortalTokenStore) {
    if store.has_token() {
        debug!("Portal storage already contains a token; skipping migration");
        return;
    }

    let Some(entry) = entry else {
        return;
    };

    match entry.get_password() {
        Ok(token) => {
            info!("Migrating existing keyring token into portal storage");
            if let Err(err) = store.save_token(&token) {
                warn!("Failed to migrate token into portal storage: {err}");
                return;
            }
            if let Err(err) = entry.delete_credential() {
                warn!("Failed to delete migrated keyring credential: {err}");
            }
        }
        Err(KeyringCoreError::NoEntry) => {
            debug!("No keyring token present to migrate");
        }
        Err(err) => {
            warn!("Failed to read keyring during portal migration: {err}");
        }
    }
}

fn ensure_keyring_ready(entry: &Entry) -> Result<(), StorageError> {
    if has_existing_token(entry) {
        info!("✅ Found existing token in keyring, skipping test");
        return Ok(());
    }

    if !test_keyring(entry) {
        warn!("❌ Keyring test failed; secure storage unavailable");
        return Err(StorageError::KeyringUnavailable);
    }

    Ok(())
}

fn has_existing_token(entry: &Entry) -> bool {
    match entry.get_password() {
        Ok(_) => true,
        Err(KeyringCoreError::NoEntry) => false,
        Err(err) => {
            warn!("Failed to check existing token: {err}");
            false
        }
    }
}

fn test_keyring(_entry: &Entry) -> bool {
    let test_key = "github_token_test";
    let test_entry = match Entry::new(SERVICE_NAME, test_key) {
        Ok(entry) => entry,
        Err(err) => {
            warn!("Failed to prepare keyring test entry: {err}");
            return false;
        }
    };

    let test_value = format!(
        "test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    info!("No existing token, testing keyring with separate test key");
    let write_result = test_entry.set_password(&test_value);
    if write_result.is_err() {
        warn!("Test write failed: {:?}", write_result.err());
        return false;
    }

    info!("Test write successful, attempting read...");
    let matches = match test_entry.get_password() {
        Ok(retrieved) => {
            let matches = retrieved == test_value;
            let _ = test_entry.delete_credential();
            matches
        }
        Err(err) => {
            warn!("Test read failed: {err}");
            false
        }
    };

    if !matches {
        warn!("Test value mismatch when verifying keyring");
    }

    matches
}

fn map_portal_err(err: PortalStoreError) -> StorageError {
    match err {
        PortalStoreError::TokenMissing => StorageError::TokenNotFound,
        other => StorageError::SecretPortal(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_storage_lifecycle() {
        let storage = match TokenStorage::new() {
            Ok(storage) => storage,
            Err(StorageError::KeyringUnavailable) => {
                eprintln!("Skipping token storage lifecycle test: keyring unavailable");
                return;
            }
            Err(err) => panic!("Failed to create TokenStorage: {}", err),
        };

        let _ = storage.delete_token();
        assert!(!storage.has_token());

        let test_token = "ghp_test_token_123";
        storage.save_token(test_token).unwrap();
        assert!(storage.has_token());

        let retrieved = storage.get_token().unwrap();
        assert_eq!(retrieved, test_token);

        storage.delete_token().unwrap();
        assert!(!storage.has_token());
    }
}
