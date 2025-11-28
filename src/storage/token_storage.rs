use super::{
    portal_token_store::{PortalStoreError, PortalTokenStore},
    secret_portal::{self, PortalPreference},
};
use keyring::Entry;
use thiserror::Error;
use tracing::{debug, info, warn};

const SERVICE_NAME: &str = "me.spaceinbox.actioneer";
const TOKEN_KEY: &str = "github_token";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Keyring error: {0}")]
    KeyringError(#[from] keyring::Error),

    #[error("Token not found")]
    TokenNotFound,

    #[error("Secure keyring storage is not available")]
    KeyringUnavailable,

    #[error("Secret portal storage error: {0}")]
    SecretPortal(PortalStoreError),
}

pub struct TokenStorage {
    entry: Entry,
    backend: Backend,
}

enum Backend {
    Portal(PortalTokenStore),
    Keyring,
}

impl TokenStorage {
    pub fn new() -> Result<Self, StorageError> {
        info!(
            "Creating TokenStorage with service: {}, key: {}",
            SERVICE_NAME, TOKEN_KEY
        );
        let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)?;
        let preference = secret_portal::portal_preference();

        if let Some(backend) = try_portal_backend(&entry, preference) {
            return Ok(Self { entry, backend });
        }

        info!("Falling back to system keyring storage");
        let backend = Backend::Keyring;
        ensure_keyring_ready(&entry)?;
        info!("✅ Keyring is available and working");
        info!("TokenStorage initialized (keyring backend)");

        Ok(Self { entry, backend })
    }

    /// Save the token to secure storage
    pub fn save_token(&self, token: &str) -> Result<(), StorageError> {
        match &self.backend {
            Backend::Portal(store) => {
                info!("Saving token to portal-backed storage");
                store.save_token(token).map_err(map_portal_err)?;
            }
            Backend::Keyring => {
                info!(
                    "Saving token to keyring storage (service: {}, key: {})",
                    SERVICE_NAME, TOKEN_KEY
                );
                self.entry.set_password(token)?;
                info!("Token saved to keyring - verifying...");
                if let Ok(retrieved) = self.entry.get_password() {
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
            Backend::Portal(store) => {
                info!("Retrieving token from portal-backed storage");
                store.get_token().map_err(map_portal_err)
            }
            Backend::Keyring => {
                info!(
                    "Retrieving token from keyring storage (service: {}, key: {})",
                    SERVICE_NAME, TOKEN_KEY
                );
                match self.entry.get_password() {
                    Ok(token) => {
                        info!(
                            "✅ Token retrieved from keyring successfully (length: {} chars)",
                            token.len()
                        );
                        Ok(token)
                    }
                    Err(keyring::Error::NoEntry) => {
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
            Backend::Portal(store) => {
                store.delete_token().map_err(map_portal_err)?;
                // Best effort cleanup in case a legacy keyring entry still exists
                match self.entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => (),
                    Err(err) => warn!("Failed to clean legacy keyring entry: {err}"),
                }
                Ok(())
            }
            Backend::Keyring => match self.entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {
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
            Backend::Portal(store) => store.has_token(),
            Backend::Keyring => self.entry.get_password().is_ok(),
        }
    }
}

impl Default for TokenStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create TokenStorage")
    }
}

fn try_portal_backend(entry: &Entry, preference: PortalPreference) -> Option<Backend> {
    if !preference.use_portal() {
        debug!("Secret portal disabled (reason: {})", preference.describe());
        return None;
    }

    match secret_portal::secret_portal_available() {
        Ok(true) => match PortalTokenStore::new() {
            Ok(store) => {
                info!(
                    "Using secret portal storage (reason: {})",
                    preference.describe()
                );
                migrate_keyring_token(entry, &store);
                Some(Backend::Portal(store))
            }
            Err(err) => {
                warn!("Secret portal initialization failed: {err}");
                None
            }
        },
        Ok(false) => {
            warn!("Secret portal requested but interface not advertised by the session");
            None
        }
        Err(err) => {
            warn!("Secret portal detection failed: {err}");
            None
        }
    }
}

fn migrate_keyring_token(entry: &Entry, store: &PortalTokenStore) {
    if store.has_token() {
        debug!("Portal storage already contains a token; skipping migration");
        return;
    }

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
        Err(keyring::Error::NoEntry) => {
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
        Err(keyring::Error::NoEntry) => false,
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
