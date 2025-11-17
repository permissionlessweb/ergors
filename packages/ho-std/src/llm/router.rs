use crate::llm::{HoError, HoResult, StateReadExt};
use crate::traits::LlmProviderTrait;
use crate::types::ergors::orch::v1::*;
use async_trait::async_trait;
use cnidarium::StateRead;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Refactored LLM Router with dynamic provider management
/// Uses trait-based providers defined via llm_entity! macro
/// Providers are stored and retrieved from cnidarium verifiable storage
pub struct LlmRouter {
    /// HTTP client for API requests
    client: Client,
    /// Registered providers mapped by name
    providers: HashMap<String, Arc<dyn LlmProviderTrait>>,
}

impl LlmRouter {
    /// Create new LLM router with automatic provider registration from storage
    ///
    /// # Arguments
    /// * `state` - StateRead implementation to read provider configs from storage
    /// * `cfg` - LLM router configuration
    pub async fn new<S: StateRead>(state: &S, cfg: &LlmRouterConfig) -> HoResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_seconds))
            .build()
            .map_err(|e| HoError::Cfg(format!("Failed to create HTTP client: {}", e)))?;

        // Initialize router with empty providers
        let mut router = Self {
            client,
            providers: HashMap::new(),
        };

        router
            .register_all_providers(state, cfg.entities.clone())
            .await?;
        info!("LlmRouter running {} providers", router.providers.len());

        Ok(router)
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
            let available_providers: Vec<String> =
                self.providers.keys().map(|k| k.clone()).collect();
            HoError::Llm(format!(
                "No provider found for model: {}, available providers: {:?}",
                model, available_providers
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

    /// Register all providers configured in storage
    ///
    /// Reads LlmEntity configurations from storage and instantiates the corresponding
    /// provider implementations (OpenAI, Anthropic, Grok, etc.)
    async fn register_all_providers<S: StateRead>(
        &mut self,
        state: &S,
        entities: Vec<LlmEntity>,
    ) -> HoResult<()> {
        // Get all configured providers from storage
        let ents = state.get_llm_providers().await?;
        info!("Found {} LLM entities in storage", ents.len());

        for entity in entities {
            self.register_provider_from_entity(&entity)?;
        }

        Ok(())
    }

    /// Register a single provider from an LlmEntity configuration
    ///
    /// This method maps the entity name to the corresponding provider implementation
    /// defined via the llm_entity! macro
    fn register_provider_from_entity(&mut self, entity: &LlmEntity) -> HoResult<()> {
        use crate::llm::*;

        let provider: Arc<dyn LlmProviderTrait> = match entity.name.to_lowercase().as_str() {
            "openai" => {
                let mut p = OpenAiProvider::new(None);
                for model in &entity.models {
                    if !OpenAiProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "anthropic" => {
                let mut p = AnthropicProvider::new(None);
                for model in &entity.models {
                    if !AnthropicProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "grok" => {
                let mut p = GrokProvider::new(None);
                for model in &entity.models {
                    if !GrokProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "akashml" | "akash" => {
                let mut p = AkashProvider::new(None);
                for model in &entity.models {
                    if !AkashProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "kimi" | "kimi_research" => {
                let mut p = KimiProvider::new(None);
                for model in &entity.models {
                    if !KimiProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "qwen" => {
                let mut p = QwenProvider::new(None);
                for model in &entity.models {
                    if !QwenProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            "venice" => {
                let mut p = VeniceProvider::new(None);
                for model in &entity.models {
                    if !VeniceProvider::MODELS.contains(&model.as_str()) {
                        p.add_supported_model(model.clone());
                    }
                }
                Arc::new(p)
            }
            unknown => {
                return Err(HoError::Cfg(format!(
                    "Unknown provider type: {}. Available providers: openai, anthropic, grok, akashml, kimi, qwen, venice",
                    unknown
                )));
            }
        };

        debug!("Registered LLM provider: {}", entity.name);
        self.providers.insert(entity.name.clone(), provider);

        Ok(())
    }

    /// Get all registered providers
    pub fn get_providers(&self) -> Vec<&Arc<dyn LlmProviderTrait>> {
        self.providers.values().collect()
    }

    /// Find provider that supports the given model
    fn find_provider_for_model(&self, model: &str) -> Option<&Arc<dyn LlmProviderTrait>> {
        self.providers
            .values()
            .find(|provider| provider.supports_model(model))
    }

    /// Get a specific provider by name
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LlmProviderTrait>> {
        self.providers.get(name)
    }

    /// Get the number of registered providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

/// Storage integration methods for LLM router configuration
impl LlmRouter {
    /// Initialize storage with default router configuration
    ///
    /// This writes the default router config to storage if none exists
    pub async fn init_storage<S: StateRead + crate::traits::StateWrite>(
        state: &mut S,
        config: &LlmRouterConfig,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;

        // Check if config already exists
        if state.get_cfg().await?.is_some() {
            info!("LLM router config already exists in storage");
            return Ok(());
        }

        // Store the router configuration
        state.put_llm_router_config(config);

        // Store all provider entities
        state.put_llm_providers(&config.entities);

        info!(
            "Initialized LLM router storage with {} providers",
            config.entities.len()
        );

        Ok(())
    }

    /// Update router configuration in storage
    pub async fn update_storage_config<S: StateRead + crate::traits::StateWrite>(
        state: &mut S,
        config: &LlmRouterConfig,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;

        state.put_llm_router_config(config);
        state.put_llm_providers(&config.entities);

        info!("Updated LLM router configuration in storage");

        Ok(())
    }

    /// Add a provider to storage
    pub async fn add_provider_to_storage<S: StateRead + crate::traits::StateWrite>(
        state: &mut S,
        provider: &LlmEntity,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;

        state.put_llm_provider(provider);

        info!("Added provider {} to storage", provider.name);

        Ok(())
    }

    /// Remove a provider from storage
    pub async fn remove_provider_from_storage<S: crate::traits::StateWrite>(
        state: &mut S,
        provider_name: &str,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;

        state.delete_llm_provider(provider_name);

        info!("Removed provider {} from storage", provider_name);

        Ok(())
    }

    /// Load router configuration from storage
    pub async fn load_config_from_storage<S: StateRead>(
        state: &S,
    ) -> HoResult<Option<LlmRouterConfig>> {
        state.get_cfg().await
    }
}
