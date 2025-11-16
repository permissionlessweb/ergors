use tonic::async_trait;

use crate::llm::HoResult;

/// Core trait for accessing API keys with support for multiple backends. Currently fetches api keys encrypted in storage
#[async_trait]
pub trait ApiKeyMethod: Send + Sync {
    /// Get API key for a specific provider
    async fn get_key(&self, provider: &str) -> HoResult<Option<String>>;

    /// Set/update API key for a provider (if supported by the backend)
    async fn set_key(&mut self, provider: &str, key: String) -> HoResult<()>;

    /// Check if a key exists for a provider
    async fn has_key(&self, provider: &str) -> bool {
        self.get_key(provider).await.ok().flatten().is_some()
    }

    /// Get all available providers with configured keys
    async fn available_providers(&self) -> Vec<String>;
}
