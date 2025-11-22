//! StorageBackend trait implementation for ErgorsStorage
//!
//! This bridges the ho-std StorageBackend trait with the concrete ErgorsStorage implementation

use crate::{
    llm::key_accessor::StorageBackend, types::ergors::storage::v1::EncryptedApiKey, HoResult,
};

use crate::storage::ErgorsStorage;

impl StorageBackend for ErgorsStorage {
    async fn get_encrypted_api_key(
        &self,
        provider_name: &str,
    ) -> HoResult<Option<EncryptedApiKey>> {
        self.get_encrypted_api_key(provider_name).await
    }

    async fn put_encrypted_api_key(
        &self,
        provider_name: &str,
        encrypted_key: &EncryptedApiKey,
    ) -> HoResult<()> {
        self.put_encrypted_api_key(provider_name, encrypted_key)
            .await
    }

    async fn delete_encrypted_api_key(&self, provider_name: &str) -> HoResult<()> {
        self.delete_encrypted_api_key(provider_name).await
    }

    async fn list_api_key_providers(&self) -> HoResult<Vec<String>> {
        self.list_api_key_providers().await
    }
}
