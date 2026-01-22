//! Encrypted API Key Storage and Management
//!
//! Provides secure storage of LLM provider API keys using password-based encryption.
//! Keys are encrypted using Argon2id for key derivation and ChaCha20Poly1305 for encryption,
//! matching the custody system's security model.

use crate::types::ergors::storage::v1::{EncryptedApiKey, EncryptedApiKeyStore};
use anyhow::{Context, Result};
use argon2::Argon2;
use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use pbjson_types::Timestamp;
use prost::Message;
use rand::RngCore;
use std::collections::HashMap;
use std::time::SystemTime;

/// Encryption method identifier
const ENCRYPTION_METHOD: &str = "argon2id-chacha20poly1305-v1";

/// Key size for ChaCha20Poly1305 (256 bits)
const KEY_SIZE: usize = 32;

/// Nonce size for ChaCha20Poly1305 (96 bits)
const NONCE_SIZE: usize = 12;

/// Salt size for Argon2id
const SALT_SIZE: usize = 32;

/// Current store version
const STORE_VERSION: u32 = 1;

/// Manager for encrypted API key storage
pub struct EncryptedApiKeyManager {
    /// Derived encryption key (from password)
    derived_key: Option<[u8; KEY_SIZE]>,
    /// Salt used for key derivation
    salt: [u8; SALT_SIZE],
    /// Cached decrypted keys (provider -> api_key)
    cache: HashMap<String, String>,
}

impl EncryptedApiKeyManager {
    /// Create a new manager with a fresh salt
    pub fn new() -> Self {
        let mut salt = [0u8; SALT_SIZE];
        rand::thread_rng().fill_bytes(&mut salt);
        Self {
            derived_key: None,
            salt,
            cache: HashMap::new(),
        }
    }

    /// Create a manager from an existing store (loads salt from store)
    pub fn from_store(store: &EncryptedApiKeyStore) -> Self {
        let mut salt = [0u8; SALT_SIZE];
        if store.kdf_salt.len() >= SALT_SIZE {
            salt.copy_from_slice(&store.kdf_salt[..SALT_SIZE]);
        }
        Self {
            derived_key: None,
            salt,
            cache: HashMap::new(),
        }
    }

    /// Derive encryption key from password
    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let mut key = [0u8; KEY_SIZE];

        // Use lighter params for faster unlocking while still secure
        #[cfg(test)]
        let params = argon2::Params::new(1 << 10, 1, 1, Some(KEY_SIZE))
            .expect("the parameters should be valid");

        #[cfg(not(test))]
        let params = argon2::Params::new(1 << 16, 2, 2, Some(KEY_SIZE))
            .expect("the parameters should be valid");

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        argon2
            .hash_password_into(password.as_bytes(), &self.salt, &mut key)
            .map_err(|e| anyhow::anyhow!("Key derivation failed: {}", e))?;

        self.derived_key = Some(key);
        Ok(())
    }

    /// Lock the manager (clear derived key and cache)
    pub fn lock(&mut self) {
        if let Some(mut key) = self.derived_key.take() {
            // Zero out the key memory
            key.iter_mut().for_each(|b| *b = 0);
        }
        // Clear cached keys
        for (_, mut value) in self.cache.drain() {
            // Zero out string memory (best effort)
            unsafe {
                let bytes = value.as_bytes_mut();
                bytes.iter_mut().for_each(|b| *b = 0);
            }
        }
    }

    /// Check if manager is unlocked
    pub fn is_unlocked(&self) -> bool {
        self.derived_key.is_some()
    }

    /// Encrypt an API key for a provider
    pub fn encrypt_key(&self, provider: &str, api_key: &str) -> Result<EncryptedApiKey> {
        let key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Manager is locked"))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));

        let encrypted = cipher
            .encrypt(nonce, api_key.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Create timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp = Some(Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });

        Ok(EncryptedApiKey {
            provider_name: provider.to_string(),
            encrypted_key: encrypted,
            encrypted_at: timestamp,
            encryption_method: ENCRYPTION_METHOD.to_string(),
            nonce: nonce_bytes.to_vec(),
        })
    }

    /// Decrypt an API key
    pub fn decrypt_key(&mut self, encrypted: &EncryptedApiKey) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.cache.get(&encrypted.provider_name) {
            return Ok(cached.clone());
        }

        let key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Manager is locked"))?;

        // Validate nonce size
        if encrypted.nonce.len() != NONCE_SIZE {
            return Err(anyhow::anyhow!("Invalid nonce size"));
        }

        let nonce = Nonce::from_slice(&encrypted.nonce);

        // Decrypt
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));

        let decrypted = cipher
            .decrypt(nonce, encrypted.encrypted_key.as_ref())
            .map_err(|_| anyhow::anyhow!("Decryption failed - wrong password or corrupted data"))?;

        let api_key = String::from_utf8(decrypted)
            .context("Decrypted data is not valid UTF-8")?;

        // Cache the result
        self.cache.insert(encrypted.provider_name.clone(), api_key.clone());

        Ok(api_key)
    }

    /// Create an encrypted store from a map of provider -> api_key
    pub fn create_store(&self, api_keys: &HashMap<String, String>) -> Result<EncryptedApiKeyStore> {
        let mut encrypted_keys = Vec::new();

        for (provider, api_key) in api_keys {
            if !api_key.is_empty() {
                let encrypted = self.encrypt_key(provider, api_key)?;
                encrypted_keys.push(encrypted);
            }
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp = Some(Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });

        // KDF params (for documentation/debugging)
        #[cfg(test)]
        let kdf_params = r#"{"memory_cost":1024,"time_cost":1,"parallelism":1}"#;
        #[cfg(not(test))]
        let kdf_params = r#"{"memory_cost":65536,"time_cost":2,"parallelism":2}"#;

        Ok(EncryptedApiKeyStore {
            version: STORE_VERSION,
            keys: encrypted_keys,
            created_at: timestamp,
            updated_at: timestamp,
            kdf_salt: self.salt.to_vec(),
            kdf_params: kdf_params.to_string(),
        })
    }

    /// Load and decrypt all keys from a store
    pub fn load_store(&mut self, store: &EncryptedApiKeyStore) -> Result<HashMap<String, String>> {
        let mut result = HashMap::new();

        for encrypted in &store.keys {
            match self.decrypt_key(encrypted) {
                Ok(api_key) => {
                    result.insert(encrypted.provider_name.clone(), api_key);
                }
                Err(e) => {
                    tracing::warn!("Failed to decrypt key for {}: {}", encrypted.provider_name, e);
                }
            }
        }

        Ok(result)
    }

    /// Get the salt (for storing with the encrypted data)
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    /// Add or update a single key in an existing store
    pub fn add_key_to_store(
        &self,
        store: &mut EncryptedApiKeyStore,
        provider: &str,
        api_key: &str,
    ) -> Result<()> {
        // Remove existing key for this provider
        store.keys.retain(|k| k.provider_name != provider);

        // Add new encrypted key
        if !api_key.is_empty() {
            let encrypted = self.encrypt_key(provider, api_key)?;
            store.keys.push(encrypted);
        }

        // Update timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        store.updated_at = Some(Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });

        Ok(())
    }

    /// Serialize store to bytes (for Cnidarium storage)
    pub fn serialize_store(store: &EncryptedApiKeyStore) -> Vec<u8> {
        store.encode_to_vec()
    }

    /// Deserialize store from bytes
    pub fn deserialize_store(bytes: &[u8]) -> Result<EncryptedApiKeyStore> {
        EncryptedApiKeyStore::decode(bytes).context("Failed to deserialize encrypted API key store")
    }
}

impl Default for EncryptedApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EncryptedApiKeyManager {
    fn drop(&mut self) {
        self.lock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("test_password_123").unwrap();

        let api_key = "sk-test-anthropic-key-12345";
        let encrypted = manager.encrypt_key("anthropic", api_key).unwrap();

        assert_eq!(encrypted.provider_name, "anthropic");
        assert!(!encrypted.encrypted_key.is_empty());
        assert_eq!(encrypted.encryption_method, ENCRYPTION_METHOD);

        let decrypted = manager.decrypt_key(&encrypted).unwrap();
        assert_eq!(decrypted, api_key);
    }

    #[test]
    fn test_store_roundtrip() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("test_password_123").unwrap();

        let mut api_keys = HashMap::new();
        api_keys.insert("anthropic".to_string(), "sk-ant-123".to_string());
        api_keys.insert("openai".to_string(), "sk-openai-456".to_string());

        let store = manager.create_store(&api_keys).unwrap();
        assert_eq!(store.keys.len(), 2);

        // Serialize and deserialize
        let bytes = EncryptedApiKeyManager::serialize_store(&store);
        let loaded_store = EncryptedApiKeyManager::deserialize_store(&bytes).unwrap();

        // Create new manager with same salt and password
        let mut manager2 = EncryptedApiKeyManager::from_store(&loaded_store);
        manager2.unlock("test_password_123").unwrap();

        let loaded_keys = manager2.load_store(&loaded_store).unwrap();
        assert_eq!(loaded_keys.get("anthropic"), Some(&"sk-ant-123".to_string()));
        assert_eq!(loaded_keys.get("openai"), Some(&"sk-openai-456".to_string()));
    }

    #[test]
    fn test_wrong_password_fails() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("correct_password").unwrap();

        let encrypted = manager.encrypt_key("test", "secret_key").unwrap();
        let store = manager.create_store(&HashMap::from([("test".to_string(), "secret_key".to_string())])).unwrap();

        // Try with wrong password
        let mut manager2 = EncryptedApiKeyManager::from_store(&store);
        manager2.unlock("wrong_password").unwrap();

        let result = manager2.decrypt_key(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_works() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("password").unwrap();

        let encrypted = manager.encrypt_key("test", "my_key").unwrap();

        // First call populates cache
        let result1 = manager.decrypt_key(&encrypted).unwrap();

        // Second call should use cache
        let result2 = manager.decrypt_key(&encrypted).unwrap();

        assert_eq!(result1, result2);
        assert_eq!(result1, "my_key");
    }

    #[test]
    fn test_lock_clears_cache_and_key() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("password").unwrap();
        assert!(manager.is_unlocked());

        let encrypted = manager.encrypt_key("test", "key").unwrap();
        manager.decrypt_key(&encrypted).unwrap();

        manager.lock();
        assert!(!manager.is_unlocked());
        assert!(manager.cache.is_empty());
    }
}
