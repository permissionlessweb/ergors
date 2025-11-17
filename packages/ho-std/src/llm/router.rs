use crate::llm::{HoError, HoResult};
use crate::traits::{ApiKeyMethod, LlmProviderTrait};
use crate::types::ergors::orch::v1::*;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Refactored LLM Router with dynamic provider management
/// Uses trait-based providers defined via llm_entity! macro
pub struct LlmRouter {
    /// HTTP client for API requests
    client: Client,
    /// Registry of available providers
    providers: HashMap<String, Box<dyn LlmProviderTrait>>,
}

impl LlmRouter {
    /// Create new LLM router with automatic provider registration
    pub async fn new(config: &LlmRouterConfig) -> HoResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| HoError::Cfg(format!("Failed to create HTTP client: {}", e)))?;

        // Determine home path from config
        let home_path = std::path::PathBuf::from(&config.api_keys_file)
            .parent()
            .ok_or_else(|| HoError::Cfg("Invalid API keys file path".to_string()))?
            .to_path_buf();

        // Initialize router with empty providers
        let mut router = Self {
            client,

            providers: HashMap::new(),
        };

        // // Register all providers from macro-generated descriptors
        // router.register_all_providers(key_accessor).await?;

        info!(
            "LLM Router initialized with {} providers",
            router.providers.len()
        );

        Ok(router)
    }

    /// Register all providers discovered via llm_entity! macro
    async fn register_all_providers(&mut self) -> HoResult<()> {
        Ok(())
    }

    /// Process a prompt request using the appropriate provider
    /// This is the single unified entrypoint for all LLM inference
    pub async fn handle_request(
        &self,
        request: &PromptRequest,
        model: &str,
    ) -> HoResult<PromptResponse> {
        // Find provider that supports this model
        let provider = self.find_provider_for_model(model).ok_or_else(|| {
            HoError::Llm(format!(
                "No provider found for model: {}, available models: {:#?}",
                model,
                self.get_providers()
            ))
        })?;

        debug!(
            "Routing request for model {} to provider {}",
            model,
            provider.name()
        );

        // Call the provider
        provider.call(&self.client, request).await
    }

    /// Find a provider that supports the given model
    fn find_provider_for_model(&self, model: &str) -> Option<&Box<dyn LlmProviderTrait>> {
        for (name, provider) in &self.providers {
            if provider.supports_model(model) {
                return Some(provider);
            }
        }
        None
    }

    /// Route request to specific provider by name
    pub async fn route_to_provider(
        &self,
        provider_name: &str,
        request: &PromptRequest,
    ) -> HoResult<PromptResponse> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| HoError::Llm(format!("Provider not found: {}", provider_name)))?;

        provider.call(&self.client, request).await
    }

    /// Get all available models across all configured providers
    pub fn get_available_models(&self) -> Vec<String> {
        let mut models = Vec::new();

        for provider in self.providers.values() {
            models.extend(provider.supported_models().iter().map(|m| m.to_string()));
        }

        models
    }

    /// Get all configured providers
    pub fn get_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Get provider by name
    pub fn get_provider(&self, name: &str) -> Option<&Box<dyn LlmProviderTrait>> {
        self.providers.get(name)
    }

    /// Check if a specific provider is available
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// Get models for a specific provider
    pub fn get_provider_models(&self, provider_name: &str) -> Option<Vec<String>> {
        self.providers
            .get(provider_name)
            .map(|p| p.supported_models().iter().map(|m| m.to_string()).collect())
    }
}

// Implement Clone manually since Box<dyn LlmProvider> doesn't implement Clone
impl Clone for LlmRouter {
    fn clone(&self) -> Self {
        // Note: This is a simplified clone that creates a new router with same config
        // The providers will be re-registered on creation
        // For true deep clone, we'd need to make providers cloneable
        panic!("LlmRouter::clone() should use LlmRouter::new() instead");
    }
}
