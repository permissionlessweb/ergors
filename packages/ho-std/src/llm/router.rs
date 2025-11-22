use crate::llm::{HoError, HoResult, StateReadExt};
use crate::traits::LlmProviderTrait;
use crate::types::ergors::orch::v1::*;
use cnidarium::StateRead;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Refactored LLM Router with dynamic provider management
/// Uses trait-based ps defined via llm_entity! macro
/// Providers are stored and retrieved from cnidarium verifiable storage
pub struct LlmRouter {
    /// HTTP c for API requests
    c: Client,
    /// Registered ps mapped by name
    ps: HashMap<String, Arc<dyn LlmProviderTrait>>,
}

impl LlmRouter {
    /// Create new LLM r with automatic provider registration from storage
    ///
    /// # Arguments
    /// * `s` - StateRead implementation to read provider configs from storage
    /// * `cfg` - LLM r configuration
    pub async fn new<S: StateRead>(s: &S, cfg: &LlmRouterConfig) -> HoResult<Self> {
        let mut r = Self {
            c: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .build()
                .map_err(|e| HoError::Cfg(format!("Failed to create HTTP c: {}", e)))?,
            ps: HashMap::new(),
        };

        r.register_all_providers(s, cfg.entities.clone()).await?;

        Ok(r)
    }

    /// Process a prompt req using the appropriate provider
    /// This is the single unified entrypoint for all LLM inference
    pub async fn handle_request(&self, req: &PromptRequest, m: &str) -> HoResult<PromptResponse> {
        // Find provider that supports this m
        let provider = self.find_provider_for_model(m).ok_or_else(|| {
            let ap: Vec<String> = self.ps.keys().map(|k| k.clone()).collect();
            HoError::Llm(format!(
                "No {} provider found, available provider: {:?}",
                m, ap
            ))
        })?;

        debug!("Routing req for m {} to provider {}", m, provider.name());

        // Call the provider
        provider.call(&self.c, req).await
    }

    /// Register all ps configured in storage
    async fn register_all_providers<S: StateRead>(
        &mut self,
        s: &S,
        entities: Vec<LlmEntity>,
    ) -> HoResult<()> {
        // Get all configured ps from storage
        let ents = s.get_llm_providers().await?;
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
                for m in &entity.models {
                    if !OpenAiProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "anthropic" => {
                let mut p = AnthropicProvider::new(None);
                for m in &entity.models {
                    if !AnthropicProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "grok" => {
                let mut p = GrokProvider::new(None);
                for m in &entity.models {
                    if !GrokProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "akashml" | "akash" => {
                let mut p = AkashProvider::new(None);
                for m in &entity.models {
                    if !AkashProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "kimi" | "kimi_research" => {
                let mut p = KimiProvider::new(None);
                for m in &entity.models {
                    if !KimiProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "qwen" => {
                let mut p = QwenProvider::new(None);
                for m in &entity.models {
                    if !QwenProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "venice" => {
                let mut p = VeniceProvider::new(None);
                for m in &entity.models {
                    if !VeniceProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            unknown => {
                return Err(HoError::Cfg(format!(
                    "Unknown provider type: {}. Available ps: openai, anthropic, grok, akashml, kimi, qwen, venice",
                    unknown
                )));
            }
        };

        debug!("Registered LLM provider: {}", entity.name);
        self.ps.insert(entity.name.clone(), provider);

        Ok(())
    }

    /// Get all registered ps
    pub fn get_providers(&self) -> Vec<&Arc<dyn LlmProviderTrait>> {
        self.ps.values().collect()
    }

    /// Find provider that supports the given m
    fn find_provider_for_model(&self, m: &str) -> Option<&Arc<dyn LlmProviderTrait>> {
        self.ps.values().find(|provider| provider.supports_model(m))
    }

    /// Get a specific provider by name
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LlmProviderTrait>> {
        self.ps.get(name)
    }

    /// Get the number of registered ps
    pub fn provider_count(&self) -> usize {
        self.ps.len()
    }
}

/// Storage integration methods for LLM r configuration
impl LlmRouter {
    /// Initialize storage with default r configuration
    ///
    /// This writes the default r c to storage if none exists
    pub async fn init_storage<S: StateRead + crate::traits::StateWrite>(
        s: &mut S,
        c: &LlmRouterConfig,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;
        if s.get_cfg().await?.is_some() {
            info!("LLM r c already exists in storage");

            return Ok(());
        }
        s.put_llm_router_config(c);
        s.put_llm_providers(&c.entities);
        Ok(())
    }

    /// Update r configuration in storage
    pub async fn update_storage_config<S: StateRead + crate::traits::StateWrite>(
        s: &mut S,
        c: &LlmRouterConfig,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;
        s.put_llm_router_config(c);
        s.put_llm_providers(&c.entities);
        Ok(())
    }

    /// Add a provider to storage
    pub async fn add_provider_to_storage<S: StateRead + crate::traits::StateWrite>(
        s: &mut S,
        provider: &LlmEntity,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;
        s.put_llm_provider(provider);
        Ok(())
    }

    /// Remove a provider from storage
    pub async fn remove_provider_from_storage<S: crate::traits::StateWrite>(
        s: &mut S,
        provider_name: &str,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;

        s.delete_llm_provider(provider_name);

        info!("Removed provider {} from storage", provider_name);

        Ok(())
    }

    /// Load r configuration from storage
    pub async fn load_config_from_storage<S: StateRead>(
        s: &S,
    ) -> HoResult<Option<LlmRouterConfig>> {
        s.get_cfg().await
    }
}
