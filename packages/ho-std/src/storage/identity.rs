//! Dedicated storage layer for encrypted node identity management.
//!
//! This module provides secure storage and retrieval of encrypted node identity keys.
//! It uses password-based encryption (Argon2 + ChaCha20Poly1305) to protect private keys
//! at rest, while keeping public keys and metadata accessible without decryption.
//!
//! # Security Model
//!
//! - Private keys are never stored in plaintext
//! - Password-based key derivation using Argon2id (RFC 9106)
//! - ChaCha20Poly1305 authenticated encryption
//! - Salt is stored separately for key derivation
//! - Metadata (public key, user, host, etc.) remains accessible without password

use crate::custody::encrypted::{decrypt, encrypt};
use crate::error::{HoError, HoResult};
use crate::keys::commonware::{NodePrivKey, NodePubkey};
use crate::types::ergors::storage::v1::{
    EncryptedNodeIdentity, NodeIdentityCustodyConfig, NodeIdentityMetadata,
};
use camino::{Utf8Path, Utf8PathBuf};
use rand_core::OsRng;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default encryption method identifier
const ENCRYPTION_METHOD_V1: &str = "argon2id-chacha20poly1305-v1";

/// Default identity storage filename
const DEFAULT_IDENTITY_FILENAME: &str = "node_identity.enc";

/// Storage layer for encrypted node identities
pub struct IdentityStorage {
    /// Path to the identity storage file
    identity_path: Utf8PathBuf,
    /// Cached decrypted private key (cleared on lock)
    cached_key: Arc<RwLock<Option<CachedKey>>>,
    /// Cache TTL in seconds
    cache_ttl_secs: u64,
}

/// Cached decrypted key with expiry
struct CachedKey {
    key: NodePrivKey,
    cached_at: std::time::Instant,
}

impl IdentityStorage {
    /// Create a new identity storage at the specified path
    pub fn new(identity_path: impl AsRef<Utf8Path>) -> Self {
        Self {
            identity_path: identity_path.as_ref().to_owned(),
            cached_key: Arc::new(RwLock::new(None)),
            cache_ttl_secs: 300, // 5 minutes default
        }
    }

    /// Create identity storage with custom cache TTL
    pub fn with_cache_ttl(identity_path: impl AsRef<Utf8Path>, cache_ttl_secs: u64) -> Self {
        Self {
            identity_path: identity_path.as_ref().to_owned(),
            cached_key: Arc::new(RwLock::new(None)),
            cache_ttl_secs,
        }
    }

    /// Create identity storage from config
    pub fn from_config(config: &NodeIdentityCustodyConfig, default_dir: &Utf8Path) -> Self {
        let identity_path = if config.identity_path.is_empty() {
            default_dir.join(DEFAULT_IDENTITY_FILENAME)
        } else {
            Utf8PathBuf::from(&config.identity_path)
        };

        Self {
            identity_path,
            cached_key: Arc::new(RwLock::new(None)),
            cache_ttl_secs: config.cache_ttl_secs,
        }
    }

    /// Get the identity storage path
    pub fn identity_path(&self) -> &Utf8Path {
        &self.identity_path
    }

    /// Check if an encrypted identity exists at the storage path
    pub fn exists(&self) -> bool {
        self.identity_path.exists()
    }

    /// Create and store a new encrypted identity
    ///
    /// # Arguments
    /// * `password` - Password for encrypting the private key
    /// * `metadata` - Optional node metadata
    ///
    /// # Returns
    /// The encrypted identity record (also persisted to disk)
    pub fn create_identity(
        &self,
        password: &str,
        _metadata: Option<NodeIdentityMetadata>,
    ) -> HoResult<EncryptedNodeIdentity> {
        // Generate new keypair
        let private_key = NodePrivKey::new(&mut OsRng);
        let _public_key = private_key.id();

        self.store_identity(&private_key, password)
    }

    /// Store an existing private key as an encrypted identity
    ///
    /// # Arguments
    /// * `private_key` - The private key to encrypt and store
    /// * `password` - Password for encryption
    /// * `metadata` - Optional node metadata
    pub fn store_identity(
        &self,
        private_key: &NodePrivKey,
        password: &str,
    ) -> HoResult<EncryptedNodeIdentity> {
        let public_key = private_key.id();
        let private_key_bytes = private_key.clone().into_bytes();

        // Encrypt the private key using password-based encryption
        let encrypted_private_key = encrypt(
            &mut OsRng,
            password
                .try_into()
                .map_err(|e: anyhow::Error| HoError::Cfg(format!("Invalid password: {}", e)))?,
            &private_key_bytes,
        );

        let encrypted_identity = EncryptedNodeIdentity {
            public_key: public_key.0.to_vec(),
            encrypted_private_key,
            encrypted_at: None,
            encryption_method: ENCRYPTION_METHOD_V1.to_string(),
            // Salt is embedded in the encrypted blob for our current encryption scheme
            kdf_salt: vec![],
            kdf_params: r#"{"memory_cost":2097152,"time_cost":1,"parallelism":4}"#.to_string(),
            version: 1,
            metadata: None,
        };

        // Ensure parent directory exists
        if let Some(parent) = self.identity_path.parent() {
            fs::create_dir_all(parent.as_std_path())?;
        }

        // Write to disk
        let json = serde_json::to_string_pretty(&encrypted_identity)?;
        fs::write(self.identity_path.as_std_path(), json)?;

        info!(
            "🔐 Created encrypted identity at: {} (pubkey: {})",
            self.identity_path,
            hex::encode(&encrypted_identity.public_key[..8])
        );

        Ok(encrypted_identity)
    }

    /// Load the encrypted identity from disk (without decryption)
    ///
    /// Returns the encrypted identity which includes the public key
    /// and metadata, but the private key remains encrypted.
    pub fn load_encrypted(&self) -> HoResult<EncryptedNodeIdentity> {
        if !self.exists() {
            return Err(HoError::Cfg(format!(
                "No encrypted identity found at: {}",
                self.identity_path
            )));
        }

        let json = fs::read_to_string(self.identity_path.as_std_path())?;
        let encrypted: EncryptedNodeIdentity = serde_json::from_str(&json)?;

        debug!(
            "📂 Loaded encrypted identity from: {} (pubkey: {})",
            self.identity_path,
            hex::encode(&encrypted.public_key[..8.min(encrypted.public_key.len())])
        );

        Ok(encrypted)
    }

    /// Get the public key without decryption
    pub fn get_public_key(&self) -> HoResult<NodePubkey> {
        let encrypted = self.load_encrypted()?;
        NodePubkey::from_bytes(&encrypted.public_key)
            .ok_or_else(|| HoError::Cfg("Invalid public key in encrypted identity".to_string()))
    }

    /// Decrypt and retrieve the private key
    ///
    /// This operation checks the cache first. If the key is cached and not expired,
    /// it returns the cached key. Otherwise, it decrypts the key from disk.
    pub async fn get_private_key(&self, password: &str) -> HoResult<NodePrivKey> {
        // Check cache first
        {
            let cache = self.cached_key.read().await;
            if let Some(ref cached) = *cache {
                if self.cache_ttl_secs == 0
                    || cached.cached_at.elapsed().as_secs() < self.cache_ttl_secs
                {
                    debug!("🔓 Returning cached private key");
                    return Ok(cached.key.clone());
                }
            }
        }

        // Cache miss or expired - decrypt from disk
        let encrypted = self.load_encrypted()?;
        let private_key = self.decrypt_private_key(&encrypted, password)?;

        // Update cache
        {
            let mut cache = self.cached_key.write().await;
            *cache = Some(CachedKey {
                key: private_key.clone(),
                cached_at: std::time::Instant::now(),
            });
        }

        info!("🔓 Decrypted and cached private key");
        Ok(private_key)
    }

    /// Decrypt the private key from an encrypted identity record
    fn decrypt_private_key(
        &self,
        encrypted: &EncryptedNodeIdentity,
        password: &str,
    ) -> HoResult<NodePrivKey> {
        let decrypted_bytes = decrypt(
            password
                .try_into()
                .map_err(|e: anyhow::Error| HoError::Cfg(format!("Invalid password: {}", e)))?,
            &encrypted.encrypted_private_key,
        )
        .map_err(|e| HoError::Cfg(format!("Failed to decrypt private key: {}", e)))?;

        NodePrivKey::from_bytes(&decrypted_bytes).ok_or_else(|| {
            HoError::Cfg("Decrypted bytes do not form a valid private key".to_string())
        })
    }

    /// Check if the private key is currently cached (unlocked)
    pub async fn is_unlocked(&self) -> bool {
        let cache = self.cached_key.read().await;
        if let Some(ref cached) = *cache {
            self.cache_ttl_secs == 0 || cached.cached_at.elapsed().as_secs() < self.cache_ttl_secs
        } else {
            false
        }
    }

    /// Lock the identity storage, clearing any cached key material
    pub async fn lock(&self) {
        let mut cache = self.cached_key.write().await;
        *cache = None;
        info!("🔒 Locked identity storage, cleared cached key");
    }

    /// Verify a password is correct without caching the key
    pub fn verify_password(&self, password: &str) -> HoResult<bool> {
        let encrypted = self.load_encrypted()?;
        match self.decrypt_private_key(&encrypted, password) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Re-encrypt the identity with a new password
    ///
    /// Requires the current password to decrypt the key first.
    pub async fn change_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> HoResult<()> {
        // Decrypt with current password
        let private_key = self.get_private_key(current_password).await?;

        // Load current metadata
        let _encrypted = self.load_encrypted()?;

        // Re-encrypt with new password
        self.store_identity(&private_key, new_password)?;

        // Clear cache (force re-authentication)
        self.lock().await;

        info!("🔄 Changed identity encryption password");
        Ok(())
    }

    /// Export the private key bytes (for operations that need raw bytes)
    pub async fn get_key_bytes(&self, password: &str) -> HoResult<[u8; 32]> {
        let private_key = self.get_private_key(password).await?;
        Ok(private_key.into_bytes())
    }

    /// Update metadata without re-encrypting the key
    pub async fn update_metadata(&self, metadata: NodeIdentityMetadata) -> HoResult<()> {
        let mut encrypted = self.load_encrypted()?;
        encrypted.metadata = Some(metadata);

        let json = serde_json::to_string_pretty(&encrypted)?;
        fs::write(self.identity_path.as_std_path(), json)?;

        debug!("📝 Updated identity metadata");
        Ok(())
    }

    /// Delete the encrypted identity file
    pub fn delete(&self) -> HoResult<()> {
        if self.exists() {
            fs::remove_file(self.identity_path.as_std_path())?;
            info!("🗑️ Deleted encrypted identity at: {}", self.identity_path);
        }
        Ok(())
    }
}

/// Builder for creating encrypted node identities with fluent API
pub struct EncryptedIdentityBuilder {
    metadata: NodeIdentityMetadata,
}

impl Default for EncryptedIdentityBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptedIdentityBuilder {
    pub fn new() -> Self {
        Self {
            metadata: NodeIdentityMetadata::default(),
        }
    }

    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.metadata.user = user.into();
        self
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.metadata.host = host.into();
        self
    }

    pub fn p2p_port(mut self, port: u32) -> Self {
        self.metadata.p2p_port = port;
        self
    }

    pub fn api_port(mut self, port: u32) -> Self {
        self.metadata.api_port = port;
        self
    }

    pub fn ssh_port(mut self, port: u32) -> Self {
        self.metadata.ssh_port = port;
        self
    }

    pub fn node_type(mut self, node_type: impl Into<String>) -> Self {
        self.metadata.node_type = node_type.into();
        self
    }

    pub fn os(mut self, os: impl Into<String>) -> Self {
        self.metadata.os = os.into();
        self
    }

    pub fn build(self) -> NodeIdentityMetadata {
        let mut metadata = self.metadata;
        metadata.created_at = Some(pbjson_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        });
        metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_temp_storage() -> (TempDir, IdentityStorage) {
        let temp_dir = TempDir::new().unwrap();
        let identity_path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("test_identity.enc")).unwrap();
        let storage = IdentityStorage::new(&identity_path);
        (temp_dir, storage)
    }

    #[test]
    fn test_create_and_load_identity() {
        let (_temp_dir, storage) = setup_temp_storage();
        let password = "test_password_123";

        // Create identity
        let metadata = EncryptedIdentityBuilder::new()
            .user("testuser")
            .host("localhost")
            .p2p_port(26969)
            .api_port(8080)
            .build();

        let encrypted = storage.create_identity(password, Some(metadata)).unwrap();
        assert!(!encrypted.public_key.is_empty());
        assert!(!encrypted.encrypted_private_key.is_empty());
        assert_eq!(encrypted.encryption_method, ENCRYPTION_METHOD_V1);

        // Load without decryption
        let loaded = storage.load_encrypted().unwrap();
        assert_eq!(loaded.public_key, encrypted.public_key);

        // Verify public key can be retrieved
        let pubkey = storage.get_public_key().unwrap();
        assert_eq!(pubkey.0.to_vec(), encrypted.public_key);
    }

    #[tokio::test]
    async fn test_decrypt_private_key() {
        let (_temp_dir, storage) = setup_temp_storage();
        let password = "secure_password";

        storage.create_identity(password, None).unwrap();

        // Decrypt with correct password
        let private_key = storage.get_private_key(password).await.unwrap();
        assert!(!private_key.clone().into_bytes().is_empty());

        // Verify it's cached
        assert!(storage.is_unlocked().await);

        // Lock and verify
        storage.lock().await;
        assert!(!storage.is_unlocked().await);
    }

    #[tokio::test]
    async fn test_wrong_password() {
        let (_temp_dir, storage) = setup_temp_storage();
        let password = "correct_password";

        storage.create_identity(password, None).unwrap();

        // Try with wrong password
        let result = storage.get_private_key("wrong_password").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_password() {
        let (_temp_dir, storage) = setup_temp_storage();
        let password = "verify_me";

        storage.create_identity(password, None).unwrap();

        assert!(storage.verify_password(password).unwrap());
        assert!(!storage.verify_password("not_the_password").unwrap());
    }

    #[tokio::test]
    async fn test_change_password() {
        let (_temp_dir, storage) = setup_temp_storage();
        let old_password = "old_password";
        let new_password = "new_password";

        storage.create_identity(old_password, None).unwrap();
        let original_pubkey = storage.get_public_key().unwrap();

        // Change password
        storage
            .change_password(old_password, new_password)
            .await
            .unwrap();

        // Old password should fail
        assert!(!storage.verify_password(old_password).unwrap());

        // New password should work
        assert!(storage.verify_password(new_password).unwrap());

        // Public key should be unchanged
        let new_pubkey = storage.get_public_key().unwrap();
        assert_eq!(original_pubkey.0.to_vec(), new_pubkey.0.to_vec());
    }
}
