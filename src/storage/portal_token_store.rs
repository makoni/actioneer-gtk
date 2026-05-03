use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use getrandom::fill as getrandom_fill;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::secret_portal;

const STORAGE_DIR: &str = "actioneer";
const PORTAL_DIR: &str = "secret-portal";
const TOKEN_FILENAME: &str = "github_token.portal";
const FILE_MAGIC: &[u8; 4] = b"ACTN";
const FILE_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

pub struct PortalTokenStore {
    key: [u8; 32],
    cipher_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum PortalStoreError {
    #[error("portal secret retrieval failed: {0}")]
    Secret(#[from] secret_portal::PortalSecretError),
    #[error("configuration directory unavailable for portal storage")]
    MissingConfigDir,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encryption error")]
    Encryption,
    #[error("decryption error")]
    Decryption,
    #[error("stored token is missing")]
    TokenMissing,
}

impl PortalTokenStore {
    pub fn new() -> Result<Self, PortalStoreError> {
        let storage_dir = portal_storage_dir()?;
        let cipher_path = storage_dir.join(TOKEN_FILENAME);
        let portal_secret = secret_portal::retrieve_secret(None)?;
        let key = derive_key(&portal_secret.secret);

        Ok(Self { key, cipher_path })
    }

    pub fn has_token(&self) -> bool {
        self.cipher_path.exists()
    }

    pub fn save_token(&self, token: &str) -> Result<(), PortalStoreError> {
        ensure_parent(&self.cipher_path)?;

        let mut nonce = [0u8; NONCE_LEN];
        getrandom_fill(&mut nonce).map_err(|_| PortalStoreError::Encryption)?;

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), token.as_bytes())
            .map_err(|_| PortalStoreError::Encryption)?;

        let mut payload = Vec::with_capacity(
            FILE_MAGIC.len() + 1 + NONCE_LEN + std::mem::size_of::<u32>() + ciphertext.len(),
        );
        payload.extend_from_slice(FILE_MAGIC);
        payload.push(FILE_VERSION);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
        payload.extend_from_slice(&ciphertext);

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.cipher_path)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.cipher_path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn get_token(&self) -> Result<String, PortalStoreError> {
        if !self.cipher_path.exists() {
            return Err(PortalStoreError::TokenMissing);
        }

        let mut data = Vec::new();
        let mut file = OpenOptions::new().read(true).open(&self.cipher_path)?;
        file.read_to_end(&mut data)?;

        if data.len() < FILE_MAGIC.len() + 1 + NONCE_LEN + std::mem::size_of::<u32>() {
            return Err(PortalStoreError::Decryption);
        }

        if &data[..FILE_MAGIC.len()] != FILE_MAGIC {
            return Err(PortalStoreError::Decryption);
        }

        if data[FILE_MAGIC.len()] != FILE_VERSION {
            return Err(PortalStoreError::Decryption);
        }

        let nonce_start = FILE_MAGIC.len() + 1;
        let nonce_end = nonce_start + NONCE_LEN;
        let len_start = nonce_end;
        let len_end = len_start + std::mem::size_of::<u32>();

        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[len_start..len_end]);
        let ciphertext_len = u32::from_be_bytes(len_bytes) as usize;

        if data.len() < len_end + ciphertext_len {
            return Err(PortalStoreError::Decryption);
        }

        let ciphertext = &data[len_end..len_end + ciphertext_len];

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&data[nonce_start..nonce_end]), ciphertext)
            .map_err(|_| PortalStoreError::Decryption)?;

        let token = String::from_utf8(plaintext).map_err(|_| PortalStoreError::Decryption)?;
        Ok(token)
    }

    pub fn delete_token(&self) -> Result<(), PortalStoreError> {
        if self.cipher_path.exists() {
            fs::remove_file(&self.cipher_path)?;
        }
        Ok(())
    }
}

fn portal_storage_dir() -> Result<PathBuf, PortalStoreError> {
    let mut dir = dirs::config_dir().ok_or(PortalStoreError::MissingConfigDir)?;
    dir.push(STORAGE_DIR);
    dir.push(PORTAL_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn derive_key(secret: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    let digest = Sha256::digest(secret);
    key.copy_from_slice(&digest);
    key
}

fn ensure_parent(path: &Path) -> Result<(), PortalStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
impl PortalTokenStore {
    pub fn with_secret_for_tests(
        secret: &[u8],
        cipher_path: PathBuf,
    ) -> Result<Self, PortalStoreError> {
        if let Some(parent) = cipher_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let key = derive_key(secret);
        Ok(Self { key, cipher_path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_and_decrypts_tokens() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cipher_path = temp_dir.path().join("token.bin");
        let secret = vec![42u8; 32];
        let store = PortalTokenStore::with_secret_for_tests(&secret, cipher_path)
            .expect("failed to create test store");

        store.save_token("example-token").unwrap();
        let token = store.get_token().unwrap();
        assert_eq!(token, "example-token");
        assert!(store.has_token());
        store.delete_token().unwrap();
        assert!(!store.has_token());
    }
}
