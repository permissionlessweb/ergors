//! ## Storage Key Structure::LlmConfig
//!  StateRead and StateWrite extension traits for LLM router storage
//!
//! These traits extend the base cnidarium StateRead/StateWrite traits to provide
//! domain-specific storage operations for LLM providers and router configurations.
//! All data is stored in the verifiable merkle tree for network consensus.
//!
//!
//! | Key Pattern | Description | Example | Value Type |
//! |------------|-------------|---------|------------|
//! | `llm/router_config` | Global router configuration | `llm/router_config` | `LlmRouterConfig` (JSON) |
//! | `llm/provider/{name}` | Individual provider config | `llm/provider/openai` | `LlmEntity` (JSON) |
//! | `llm/models/{model}` | Model→Provider mapping | `llm/models/gpt-4` | Provider name (UTF-8 string) |
//!
//! ### Key Design Rationale

use crate::error::{HoError, HoResult};

use crate::orchestrate::{LlmEntity, LlmRouterConfig};
use crate::traits::StateWrite;
use async_trait::async_trait;
use cnidarium::StateRead;
use futures::StreamExt;

pub mod state_key {
    /// State key for LLM router configuration
    pub fn router_config() -> &'static str {
        "llm/router_config"
    }

    /// State key prefix for LLM provider configurations
    pub fn provider_prefix() -> &'static str {
        "llm/provider/"
    }

    /// State key for a specific LLM provider
    pub fn provider(name: &str) -> String {
        format!("llm/provider/{}", name)
    }

    /// State key prefix for LLM models index
    pub fn models_prefix() -> &'static str {
        "llm/models/"
    }

    /// State key for model to provider mapping
    pub fn model_provider(model: &str) -> String {
        format!("llm/models/{}", model)
    }

    /// State key for encrypted API key store
    pub fn encrypted_api_keys() -> &'static str {
        "custody/api_keys_store"
    }
}

/// Extension trait for reading LLM configurations from verifiable storage
#[async_trait]
pub trait StateReadExt: StateRead {
    /// Get LLM router configuration from storage
    async fn get_cfg(&self) -> HoResult<Option<LlmRouterConfig>> {
        let key = state_key::router_config();
        match self.get_raw(key).await {
            Ok(Some(data)) => Ok(Some(serde_json::from_slice(&data).map_err(|e| {
                HoError::DeSerialization(format!("Failed to deserialize router config: {}", e))
            })?)),
            Ok(None) => Ok(None),
            Err(e) => Err(HoError::Storage(format!("router config err: {}", e))),
        }
    }

    /// Get a specific LLM provider configuration by name
    async fn get_llm_provider(&self, name: &str) -> HoResult<Option<LlmEntity>> {
        let key = state_key::provider(name);
        match self.get_raw(&key).await {
            Ok(Some(data)) => {
                let provider: LlmEntity = serde_json::from_slice(&data).map_err(|e| {
                    HoError::DeSerialization(format!(
                        "Failed to deserialize provider {}: {}",
                        name, e
                    ))
                })?;
                Ok(Some(provider))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to read provider {}: {}",
                name, e
            ))),
        }
    }

    /// Query all configured LLM providers from storage
    ///
    /// Streams all provider configurations stored under the `llm/provider/` prefix
    /// and deserializes them into a vector of LlmEntity objects.
    async fn get_llm_providers(&self) -> HoResult<Vec<LlmEntity>> {
        use futures::pin_mut;

        let prefix = state_key::provider_prefix();
        let stream = self.prefix_raw(prefix);
        let mut providers = Vec::new();

        // Pin the stream for async iteration
        pin_mut!(stream);

        // Stream through all entries with the provider prefix
        while let Some(result) = stream.next().await {
            match result {
                Ok((_key, value)) => {
                    // Deserialize the provider entity
                    match serde_json::from_slice::<LlmEntity>(&value) {
                        Ok(provider) => {
                            providers.push(provider);
                        }
                        Err(e) => {
                            // Log warning but continue processing other providers
                            tracing::warn!("Failed to deserialize provider: {}", e);
                        }
                    }
                }
                Err(e) => {
                    // Log error but continue processing
                    tracing::error!("Error reading provider from storage stream: {}", e);
                }
            }
        }

        Ok(providers)
    }

    /// Get the provider name for a specific model
    async fn get_model_provider(&self, model: &str) -> HoResult<Option<String>> {
        let key = state_key::model_provider(model);
        match self.get_raw(&key).await {
            Ok(Some(data)) => {
                let provider_name = String::from_utf8(data).map_err(|e| {
                    HoError::DeSerialization(format!("Invalid UTF-8 in provider name: {}", e))
                })?;
                Ok(Some(provider_name))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to read model provider mapping: {}",
                e
            ))),
        }
    }

    /// Get encrypted API key store from storage
    async fn get_encrypted_api_key_store(
        &self,
    ) -> HoResult<Option<crate::types::ergors::storage::v1::EncryptedApiKeyStore>> {
        use crate::types::ergors::storage::v1::EncryptedApiKeyStore;
        use prost::Message;

        let key = state_key::encrypted_api_keys();
        match self.get_raw(key).await {
            Ok(Some(data)) => {
                let store = EncryptedApiKeyStore::decode(data.as_slice()).map_err(|e| {
                    HoError::DeSerialization(format!("Failed to decode encrypted API key store: {}", e))
                })?;
                Ok(Some(store))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to read encrypted API key store: {}",
                e
            ))),
        }
    }

    /// Load and decrypt all API keys from storage using the custody password
    ///
    /// Returns a HashMap of provider_name -> decrypted_api_key
    async fn load_and_decrypt_api_keys(
        &self,
        password: &str,
    ) -> HoResult<std::collections::HashMap<String, String>> {
        use crate::llm::EncryptedApiKeyManager;
        use std::collections::HashMap;

        // Get the encrypted store
        let store = match self.get_encrypted_api_key_store().await? {
            Some(s) => s,
            None => return Ok(HashMap::new()),
        };

        // Create manager and unlock with password
        let mut manager = EncryptedApiKeyManager::from_store(&store);
        manager.unlock(password).map_err(|e| {
            HoError::Crypto(format!("Failed to unlock API key store: {}", e))
        })?;

        // Decrypt all keys
        manager.load_store(&store).map_err(|e| {
            HoError::Crypto(format!("Failed to decrypt API keys: {}", e))
        })
    }

    /// Get encrypted API key for a provider (legacy - for backwards compatibility)
    #[deprecated(note = "Use get_encrypted_api_key_store instead")]
    async fn get_encrypted_api_key(&self, provider: &str) -> HoResult<Option<Vec<u8>>> {
        let key = format!("custody/api_keys/{}", provider);
        match self.get_raw(&key).await {
            Ok(data) => Ok(data),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to read encrypted API key for {}: {}",
                provider, e
            ))),
        }
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

/// Extension trait for writing LLM configurations to verifiable storage
#[async_trait]
pub trait StateWriteExt: StateWrite {
    /// Store LLM router configuration
    fn put_llm_router_config(&mut self, config: &LlmRouterConfig) {
        let key = state_key::router_config().to_string();
        let data = serde_json::to_vec(config).expect("Failed to serialize router config");
        self.put_raw(key, data);
    }

    /// Store a LLM provider configuration
    fn put_llm_provider(&mut self, provider: &LlmEntity) {
        let key = state_key::provider(&provider.name);
        let data = serde_json::to_vec(provider).expect("Failed to serialize provider");
        self.put_raw(key, data);

        // Create model -> provider index entries for fast lookups
        for model in &provider.models {
            let model_key = state_key::model_provider(model);
            self.put_raw(model_key, provider.name.as_bytes().to_vec());
        }
    }

    /// Store multiple providers atomically (for initialization)
    fn put_llm_providers(&mut self, providers: &[LlmEntity]) {
        for provider in providers {
            self.put_llm_provider(provider);
        }
    }

    /// Delete a LLM provider configuration
    fn delete_llm_provider(&mut self, name: &str) {
        let key = state_key::provider(name);
        self.delete(key);
    }

    /// Delete router configuration
    fn delete_llm_router_config(&mut self) {
        let key = state_key::router_config().to_string();
        self.delete(key);
    }

    /// Store encrypted API key store
    fn put_encrypted_api_key_store(
        &mut self,
        store: &crate::types::ergors::storage::v1::EncryptedApiKeyStore,
    ) {
        use prost::Message;

        let key = state_key::encrypted_api_keys().to_string();
        let data = store.encode_to_vec();
        self.put_raw(key, data);
    }

    /// Store encrypted API key for a provider (legacy - for backwards compatibility)
    #[deprecated(note = "Use put_encrypted_api_key_store instead")]
    fn put_encrypted_api_key(&mut self, provider: &str, encrypted_data: Vec<u8>) {
        let key = format!("custody/api_keys/{}", provider);
        self.put_raw(key, encrypted_data);
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}
