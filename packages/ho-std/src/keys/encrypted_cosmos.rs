//! Encrypted Cosmos Key Storage
//!
//! Provides secure storage for cosmos-sdk compatible keys using the same
//! encryption pattern as API keys (Argon2id + ChaCha20Poly1305).

use crate::keys::cosmos::{CosmosAccountInfo, CosmosKeyPair, CosmosMnemonic};
use crate::types::ergors::orch::v1::{CosmosAccount, CosmosKeyStore, EncryptedCosmosMnemonic};
use anyhow::{anyhow, Context, Result};
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

/// Default HD path for cosmos-sdk
const DEFAULT_COSMOS_HD_PATH: &str = "m/44'/118'/0'/0/0";

/// Manager for encrypted cosmos key storage
pub struct EncryptedCosmosKeyManager {
    /// Derived encryption key (from password)
    derived_key: Option<[u8; KEY_SIZE]>,
    /// Salt used for key derivation
    salt: [u8; SALT_SIZE],
    /// Cached decrypted mnemonics (key_name -> mnemonic phrase)
    mnemonic_cache: HashMap<String, String>,
}

impl EncryptedCosmosKeyManager {
    /// Create a new manager with a fresh salt
    pub fn new() -> Self {
        let mut salt = [0u8; SALT_SIZE];
        rand::thread_rng().fill_bytes(&mut salt);
        Self {
            derived_key: None,
            salt,
            mnemonic_cache: HashMap::new(),
        }
    }

    /// Create a manager from an existing store (loads salt from first key)
    pub fn from_store(store: &CosmosKeyStore) -> Self {
        let mut salt = [0u8; SALT_SIZE];
        if let Some(first_key) = store.keys.first() {
            if first_key.kdf_salt.len() >= SALT_SIZE {
                salt.copy_from_slice(&first_key.kdf_salt[..SALT_SIZE]);
            }
        }
        Self {
            derived_key: None,
            salt,
            mnemonic_cache: HashMap::new(),
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
            .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

        self.derived_key = Some(key);
        Ok(())
    }

    /// Lock the manager (clear derived key and cache)
    pub fn lock(&mut self) {
        if let Some(mut key) = self.derived_key.take() {
            key.iter_mut().for_each(|b| *b = 0);
        }
        // Clear cached mnemonics
        for (_, mut phrase) in self.mnemonic_cache.drain() {
            // Zero out string memory (best effort)
            unsafe {
                let bytes = phrase.as_bytes_mut();
                bytes.iter_mut().for_each(|b| *b = 0);
            }
        }
    }

    /// Check if manager is unlocked
    pub fn is_unlocked(&self) -> bool {
        self.derived_key.is_some()
    }

    /// Generate a new cosmos key and encrypt it
    pub fn generate_key(
        &mut self,
        key_name: &str,
        chain_id: &str,
        address_prefix: &str,
    ) -> Result<(EncryptedCosmosMnemonic, CosmosAccountInfo)> {
        self.generate_key_with_label(key_name, chain_id, address_prefix, "", false)
    }

    /// Generate a new cosmos key with label and default designation
    pub fn generate_key_with_label(
        &mut self,
        key_name: &str,
        chain_id: &str,
        address_prefix: &str,
        label: &str,
        is_default: bool,
    ) -> Result<(EncryptedCosmosMnemonic, CosmosAccountInfo)> {
        let _key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow!("Manager is locked"))?;

        // Generate new mnemonic
        let mnemonic = CosmosMnemonic::generate()?;
        let keypair = mnemonic.derive_keypair(0)?;
        let account_info = CosmosAccountInfo::from_keypair(&keypair, key_name, address_prefix)?;

        // Encrypt the mnemonic
        let mut encrypted = self.encrypt_mnemonic(key_name, mnemonic.phrase(), chain_id, address_prefix)?;
        encrypted.label = label.to_string();
        encrypted.is_default = is_default;

        // Cache the mnemonic
        self.mnemonic_cache
            .insert(key_name.to_string(), mnemonic.phrase().to_string());

        Ok((encrypted, account_info))
    }

    /// Import an existing mnemonic and encrypt it
    pub fn import_mnemonic(
        &mut self,
        key_name: &str,
        phrase: &str,
        chain_id: &str,
        address_prefix: &str,
    ) -> Result<(EncryptedCosmosMnemonic, CosmosAccountInfo)> {
        self.import_mnemonic_with_label(key_name, phrase, chain_id, address_prefix, "", false)
    }

    /// Import an existing mnemonic with a label and default designation
    pub fn import_mnemonic_with_label(
        &mut self,
        key_name: &str,
        phrase: &str,
        chain_id: &str,
        address_prefix: &str,
        label: &str,
        is_default: bool,
    ) -> Result<(EncryptedCosmosMnemonic, CosmosAccountInfo)> {
        let _key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow!("Manager is locked"))?;

        // Validate mnemonic
        let mnemonic = CosmosMnemonic::from_phrase(phrase)?;
        let keypair = mnemonic.derive_keypair(0)?;
        let account_info = CosmosAccountInfo::from_keypair(&keypair, key_name, address_prefix)?;

        // Encrypt the mnemonic
        let mut encrypted = self.encrypt_mnemonic(key_name, phrase, chain_id, address_prefix)?;
        encrypted.label = label.to_string();
        encrypted.is_default = is_default;

        // Cache the mnemonic
        self.mnemonic_cache
            .insert(key_name.to_string(), phrase.to_string());

        Ok((encrypted, account_info))
    }

    /// Encrypt a mnemonic phrase
    fn encrypt_mnemonic(
        &self,
        key_name: &str,
        phrase: &str,
        chain_id: &str,
        address_prefix: &str,
    ) -> Result<EncryptedCosmosMnemonic> {
        let key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow!("Manager is locked"))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let encrypted = cipher
            .encrypt(nonce, phrase.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        // Create timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp = Some(Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });

        // KDF params
        #[cfg(test)]
        let kdf_params = r#"{"memory_cost":1024,"time_cost":1,"parallelism":1}"#;
        #[cfg(not(test))]
        let kdf_params = r#"{"memory_cost":65536,"time_cost":2,"parallelism":2}"#;

        Ok(EncryptedCosmosMnemonic {
            key_name: key_name.to_string(),
            encrypted_mnemonic: encrypted,
            nonce: nonce_bytes.to_vec(),
            kdf_salt: self.salt.to_vec(),
            kdf_params: kdf_params.to_string(),
            encryption_method: ENCRYPTION_METHOD.to_string(),
            chain_id: chain_id.to_string(),
            address_prefix: address_prefix.to_string(),
            default_hd_path: DEFAULT_COSMOS_HD_PATH.to_string(),
            created_at: timestamp,
            last_used_at: timestamp,
            version: STORE_VERSION,
            label: String::new(),
            is_default: false,
        })
    }

    /// Decrypt a cosmos mnemonic
    pub fn decrypt_mnemonic(&mut self, encrypted: &EncryptedCosmosMnemonic) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.mnemonic_cache.get(&encrypted.key_name) {
            return Ok(cached.clone());
        }

        let key = self
            .derived_key
            .as_ref()
            .ok_or_else(|| anyhow!("Manager is locked"))?;

        // Validate nonce size
        if encrypted.nonce.len() != NONCE_SIZE {
            return Err(anyhow!("Invalid nonce size"));
        }

        let nonce = Nonce::from_slice(&encrypted.nonce);

        // Decrypt
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let decrypted = cipher
            .decrypt(nonce, encrypted.encrypted_mnemonic.as_ref())
            .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;

        let phrase =
            String::from_utf8(decrypted).context("Decrypted data is not valid UTF-8")?;

        // Cache the result
        self.mnemonic_cache
            .insert(encrypted.key_name.clone(), phrase.clone());

        Ok(phrase)
    }

    /// Get a keypair from an encrypted mnemonic at a given account index
    pub fn get_keypair(
        &mut self,
        encrypted: &EncryptedCosmosMnemonic,
        account_index: u32,
    ) -> Result<CosmosKeyPair> {
        let phrase = self.decrypt_mnemonic(encrypted)?;
        let mnemonic = CosmosMnemonic::from_phrase(&phrase)?;
        mnemonic.derive_keypair(account_index)
    }

    /// Get a keypair from an encrypted mnemonic with custom coin type
    ///
    /// This allows deriving addresses for different cosmos chains:
    /// - 118: Cosmos/Akash (default)
    /// - 330: Terra
    /// - 60: Ethereum (for EVM chains)
    /// - 529: Secret Network
    pub fn get_keypair_with_coin_type(
        &mut self,
        encrypted: &EncryptedCosmosMnemonic,
        account_index: u32,
        coin_type: u32,
    ) -> Result<CosmosKeyPair> {
        let phrase = self.decrypt_mnemonic(encrypted)?;
        let mnemonic = CosmosMnemonic::from_phrase(&phrase)?;
        mnemonic.derive_keypair_with_coin_type(account_index, coin_type)
    }

    /// Create an empty key store
    pub fn create_empty_store() -> CosmosKeyStore {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        CosmosKeyStore {
            version: STORE_VERSION,
            keys: vec![],
            derived_accounts: vec![],
            updated_at: Some(Timestamp {
                seconds: now.as_secs() as i64,
                nanos: now.subsec_nanos() as i32,
            }),
            default_key_name: String::new(),
        }
    }

    /// Add a key to an existing store
    pub fn add_key_to_store(
        &self,
        store: &mut CosmosKeyStore,
        encrypted_key: EncryptedCosmosMnemonic,
        account_info: CosmosAccountInfo,
    ) {
        let is_default = encrypted_key.is_default;
        let key_name = encrypted_key.key_name.clone();

        // If this key is marked as default, unmark all others
        if is_default {
            for k in store.keys.iter_mut() {
                k.is_default = false;
            }
            store.default_key_name = key_name.clone();
        }

        // Remove existing key with same name
        store.keys.retain(|k| k.key_name != key_name);
        store.keys.push(encrypted_key);

        // Add derived account info
        store
            .derived_accounts
            .retain(|a| a.key_name != account_info.key_name);
        store.derived_accounts.push(CosmosAccount {
            key_name: account_info.key_name,
            hd_path: account_info.hd_path,
            address: account_info.address,
            public_key: account_info.public_key,
            account_index: account_info.account_index,
        });

        // Update timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        store.updated_at = Some(Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });
    }

    /// Set a key as the default in the store, unmarking any previous default
    pub fn set_default_key(store: &mut CosmosKeyStore, key_name: &str) -> Result<()> {
        let key_exists = store.keys.iter().any(|k| k.key_name == key_name);
        if !key_exists {
            return Err(anyhow!("Key '{}' not found in store", key_name));
        }

        for k in store.keys.iter_mut() {
            k.is_default = k.key_name == key_name;
        }
        store.default_key_name = key_name.to_string();
        Ok(())
    }

    /// Get the default key name from the store
    pub fn get_default_key_name(store: &CosmosKeyStore) -> Option<&str> {
        if !store.default_key_name.is_empty() {
            return Some(&store.default_key_name);
        }
        // Fallback: check is_default flag on individual keys
        store.keys.iter()
            .find(|k| k.is_default)
            .map(|k| k.key_name.as_str())
    }

    /// Get the default key from the store
    pub fn get_default_key(store: &CosmosKeyStore) -> Option<&EncryptedCosmosMnemonic> {
        if !store.default_key_name.is_empty() {
            return store.keys.iter().find(|k| k.key_name == store.default_key_name);
        }
        store.keys.iter().find(|k| k.is_default)
    }

    /// Check if an address already exists in the store (duplicate detection)
    pub fn address_exists(store: &CosmosKeyStore, address: &str) -> bool {
        store.derived_accounts.iter().any(|a| a.address == address)
    }

    /// Delete a key from the store by name
    pub fn delete_key(store: &mut CosmosKeyStore, key_name: &str) -> Result<()> {
        let key_exists = store.keys.iter().any(|k| k.key_name == key_name);
        if !key_exists {
            return Err(anyhow!("Key '{}' not found in store", key_name));
        }

        store.keys.retain(|k| k.key_name != key_name);
        store.derived_accounts.retain(|a| a.key_name != key_name);

        // Clear default if we just deleted the default key
        if store.default_key_name == key_name {
            store.default_key_name = String::new();
        }

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
    pub fn serialize_store(store: &CosmosKeyStore) -> Vec<u8> {
        store.encode_to_vec()
    }

    /// Deserialize store from bytes
    pub fn deserialize_store(bytes: &[u8]) -> Result<CosmosKeyStore> {
        CosmosKeyStore::decode(bytes).context("Failed to deserialize cosmos key store")
    }

    /// List all key names in a store
    pub fn list_keys(store: &CosmosKeyStore) -> Vec<&str> {
        store.keys.iter().map(|k| k.key_name.as_str()).collect()
    }

    /// Get a key by name from a store
    pub fn get_key_by_name<'a>(
        store: &'a CosmosKeyStore,
        key_name: &str,
    ) -> Option<&'a EncryptedCosmosMnemonic> {
        store.keys.iter().find(|k| k.key_name == key_name)
    }
}

impl Default for EncryptedCosmosKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EncryptedCosmosKeyManager {
    fn drop(&mut self) {
        self.lock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::cosmos::AKASH_PREFIX;

    #[test]
    fn test_generate_and_encrypt_key() {
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test_password").unwrap();

        let (encrypted, account_info) =
            manager.generate_key("test_key", "akashnet-2", AKASH_PREFIX).unwrap();

        assert_eq!(encrypted.key_name, "test_key");
        assert_eq!(encrypted.chain_id, "akashnet-2");
        assert!(account_info.address.starts_with("akash1"));
    }

    #[test]
    fn test_import_mnemonic() {
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test_password").unwrap();

        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

        let (encrypted, account_info) = manager
            .import_mnemonic("imported_key", phrase, "akashnet-2", AKASH_PREFIX)
            .unwrap();

        assert_eq!(encrypted.key_name, "imported_key");
        assert!(account_info.address.starts_with("akash1"));
    }

    #[test]
    fn test_decrypt_mnemonic_roundtrip() {
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test_password").unwrap();

        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

        let (encrypted, _) = manager
            .import_mnemonic("test_key", phrase, "akashnet-2", AKASH_PREFIX)
            .unwrap();

        // Clear cache to test actual decryption
        manager.mnemonic_cache.clear();

        let decrypted = manager.decrypt_mnemonic(&encrypted).unwrap();
        assert_eq!(decrypted, phrase);
    }

    #[test]
    fn test_store_operations() {
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test_password").unwrap();

        let mut store = EncryptedCosmosKeyManager::create_empty_store();

        let (encrypted, account_info) =
            manager.generate_key("key1", "akashnet-2", AKASH_PREFIX).unwrap();
        manager.add_key_to_store(&mut store, encrypted, account_info);

        assert_eq!(store.keys.len(), 1);
        assert_eq!(store.derived_accounts.len(), 1);

        let keys = EncryptedCosmosKeyManager::list_keys(&store);
        assert_eq!(keys, vec!["key1"]);
    }

    #[test]
    fn test_serialize_deserialize_store() {
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test_password").unwrap();

        let mut store = EncryptedCosmosKeyManager::create_empty_store();
        let (encrypted, account_info) =
            manager.generate_key("key1", "akashnet-2", AKASH_PREFIX).unwrap();
        manager.add_key_to_store(&mut store, encrypted, account_info);

        let bytes = EncryptedCosmosKeyManager::serialize_store(&store);
        let loaded_store = EncryptedCosmosKeyManager::deserialize_store(&bytes).unwrap();

        assert_eq!(loaded_store.keys.len(), 1);
        assert_eq!(loaded_store.keys[0].key_name, "key1");
    }
}
