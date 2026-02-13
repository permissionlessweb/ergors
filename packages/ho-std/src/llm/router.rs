use crate::llm::response_id::{RequestClassification, RequestContext, ResponseId};
use crate::llm::{DeploymentProviderCache, HoError, HoResult, StateReadExt};
use crate::traits::LlmProviderTrait;
use crate::types::ergors::orch::v1::{
    content_block, response_output_item, ContentBlock, LlmEntity, LlmRouterConfig,
    MessageItemContent, PromptRequest, PromptResponse, ResponseMetadata, ResponseOutputItem,
};
use cnidarium::StateRead;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Refactored LLM Router with dynamic provider management
/// Uses trait-based ps defined via llm_entity! macro
/// Providers are stored and retrieved from cnidarium verifiable storage
pub struct LlmRouter {
    /// HTTP c for API requests
    c: Client,
    /// Registered ps mapped by name (RwLock for runtime registration)
    ps: RwLock<HashMap<String, Arc<dyn LlmProviderTrait>>>,
    /// Provider name → upstream model name (for model substitution in call_provider_by_name)
    default_models: RwLock<HashMap<String, String>>,
    /// In-memory cache of active Akash deployments for inference
    deployment_cache: Arc<DeploymentProviderCache>,
}

impl LlmRouter {
    /// Create new LLM r with automatic provider registration from storage
    ///
    /// # Arguments
    /// * `s` - StateRead implementation to read provider configs from storage
    /// * `cfg` - LLM r configuration
    pub async fn new<S: StateRead>(s: &S, cfg: &LlmRouterConfig) -> HoResult<Self> {
        let deployment_cache = Arc::new(DeploymentProviderCache::new());

        let mut r = Self {
            c: Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .build()
                .map_err(|e| HoError::Cfg(format!("Failed to create HTTP c: {}", e)))?,
            ps: RwLock::new(HashMap::new()),
            default_models: RwLock::new(HashMap::new()),
            deployment_cache,
        };

        r.register_all_providers_init(s, cfg.entities.clone()).await?;

        // TODO: Initial cache refresh from storage
        // r.deployment_cache.refresh(s).await?;

        Ok(r)
    }

    /// Get a reference to the deployment cache.
    /// Used to add/remove deployments when they complete/fail.
    pub fn deployment_cache(&self) -> Arc<DeploymentProviderCache> {
        Arc::clone(&self.deployment_cache)
    }

    /// Process a prompt req using the appropriate provider
    /// This is the single unified entrypoint for all LLM inference
    ///
    /// Routing priority:
    /// 1. Check active Akash deployments by label (O(1) cache lookup)
    /// 2. Fall back to configured providers (OpenAI, Anthropic, etc.)
    pub async fn handle_request(&self, req: &PromptRequest, m: &str) -> HoResult<PromptResponse> {
        // PRIORITY 1: Check if this model name matches an active deployment label
        if let Some(deployment) = self.deployment_cache.get(m).await {
            debug!(
                "Routing request for model '{}' to Akash deployment: {}",
                m, deployment.session_id
            );
            return self.route_to_deployment(req, &deployment).await;
        }

        // PRIORITY 2: Check configured providers
        let ps = self.ps.read().await;
        let provider = ps.values().find(|p| p.supports_model(m)).cloned().ok_or_else(|| {
            let ap: Vec<String> = ps.keys().cloned().collect();
            HoError::Llm(format!(
                "No provider found for model '{}'. Available providers: {:?}",
                m, ap
            ))
        })?;
        drop(ps);

        debug!("Routing req for m {} to provider {}", m, provider.name());

        // Call the provider
        provider.call(&self.c, req).await
    }

    /// Route a request to an Akash deployment endpoint.
    ///
    /// Constructs an OpenAI-compatible request to the deployment's external URI.
    /// Auth headers are stripped as per design (Option C: deployment-specific auth).
    async fn route_to_deployment(
        &self,
        req: &PromptRequest,
        deployment: &crate::llm::DeploymentEndpoint,
    ) -> HoResult<PromptResponse> {
        self.route_to_deployment_with_context(req, deployment, None)
            .await
    }

    /// Route a request to an Akash deployment endpoint with request context.
    ///
    /// This allows tracking session IDs, conversation chaining, and latency.
    pub async fn route_to_deployment_with_context(
        &self,
        req: &PromptRequest,
        deployment: &crate::llm::DeploymentEndpoint,
        context: Option<RequestContext>,
    ) -> HoResult<PromptResponse> {
        // Start timing for latency tracking
        let start_time = Instant::now();

        // Get base URL from deployment endpoint
        let base_url = deployment.base_url().ok_or_else(|| {
            HoError::Llm(format!(
                "Deployment '{}' has no primary endpoint",
                deployment.label
            ))
        })?;

        debug!(
            "Forwarding to deployment endpoint: {} (DSEQ: {})",
            base_url, deployment.dseq
        );

        // Determine classification and endpoint path from context or request type
        let (classification, endpoint_path) = if let Some(ref ctx) = context {
            (ctx.classification, ctx.endpoint_path.as_str())
        } else {
            // Infer from request content
            let classification = if req.messages.is_empty() {
                RequestClassification::Embedding
            } else {
                RequestClassification::Chat
            };
            let path = match classification {
                RequestClassification::Embedding => "/v1/embeddings",
                _ => "/v1/chat/completions",
            };
            (classification, path)
        };

        let full_url = format!("{}{}", base_url, endpoint_path);

        debug!(
            "Deployment request URL: {} (classification: {})",
            full_url,
            classification.as_str()
        );

        // Extract temperature and max_tokens from llm_config
        let temperature = req
            .llm_config
            .as_ref()
            .map(|cfg| cfg.temperature as f64)
            .unwrap_or(0.7f64);
        let max_tokens = req
            .llm_config
            .as_ref()
            .map(|cfg| cfg.max_tokens as i64)
            .unwrap_or(0);

        // Convert PromptRequest to OpenAI-compatible JSON
        let mut openai_request = serde_json::json!({
            "model": deployment.model_name(), // Use actual model name (falls back to label)
            "messages": req.messages,
            "temperature": temperature,
            "stream": false, // TODO: Support streaming
        });

        // Only include max_tokens when explicitly set (> 0).
        // Protobuf u32 default is 0, meaning "not set" — omit to let server use its default.
        if max_tokens > 0 {
            openai_request["max_tokens"] = serde_json::json!(max_tokens);
        }

        // Make the request (auth headers stripped)
        let response = self
            .c
            .post(&full_url)
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| HoError::Llm(format!("Deployment request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(HoError::Llm(format!(
                "Deployment returned error {}: {}",
                status, body
            )));
        }

        // Calculate latency before parsing response
        let latency_ms = start_time.elapsed().as_millis() as u64;

        // Parse OpenAI response format
        let openai_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| HoError::Llm(format!("Failed to parse deployment response: {}", e)))?;

        // Extract content from OpenAI response format
        let content = openai_response
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        // Extract token usage from OpenAI response
        let tokens_used = openai_response.get("usage").map(|usage| {
            let prompt = usage
                .get("prompt_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32;
            let completion = usage
                .get("completion_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32;
            let total = usage
                .get("total_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32;

            crate::types::ergors::orch::v1::TokenUsage {
                prompt,
                completion,
                total,
            }
        });

        // Extract provider-specific response ID if present
        let provider_response_id = openai_response
            .get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string());

        // Generate response ID
        let previous_response_id = context
            .as_ref()
            .and_then(|ctx| ctx.previous_response_id);
        let sequence = context.as_ref().map(|ctx| ctx.sequence).unwrap_or(0);
        let mut response_id = ResponseId::new(classification.as_str(), previous_response_id, sequence);
        if let Some(provider_id) = provider_response_id {
            response_id = response_id.with_provider_id(provider_id);
        }

        // Calculate cost estimate based on tokens (simple pricing model)
        // TODO: Make this configurable per deployment
        let cost = tokens_used
            .as_ref()
            .map(|t| {
                // Estimate: $0.001 per 1K tokens for deployment (much cheaper than API)
                (t.total as f64) * 0.000001
            })
            .unwrap_or(0.0);

        // Build Open Responses output items using correct types
        let message_content = MessageItemContent {
            role: "assistant".to_string(),
            content: vec![ContentBlock {
                r#type: "text".to_string(),
                block: Some(content_block::Block::Text(content.clone())),
            }],
        };

        let output = vec![ResponseOutputItem {
            id: response_id.to_open_responses_format(),
            r#type: "message".to_string(),
            status: "completed".to_string(),
            content: Some(response_output_item::Content::Message(message_content)),
        }];

        // Build response metadata
        let now_timestamp = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        };
        let response_metadata = Some(ResponseMetadata {
            created: Some(now_timestamp),
            completed: Some(now_timestamp),
            previous_response_id: previous_response_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        });

        // Build PromptResponse with full tracking
        Ok(PromptResponse {
            id: response_id.to_bytes(),
            provider: format!("akash-deployment:{}", deployment.session_id),
            model: deployment.label.clone(),
            prompt: String::new(), // Original prompt not needed in response
            response: vec![content],
            timestamp: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                nanos: 0,
            }),
            tokens_used,
            cost,
            latency_ms,
            status: Some("completed".to_string()),
            output,
            response_metadata,
        })
    }

    /// Register all ps configured in storage (init-time only)
    async fn register_all_providers_init<S: StateRead>(
        &mut self,
        s: &S,
        entities: Vec<LlmEntity>,
    ) -> HoResult<()> {
        // Get all configured ps from storage
        let ents = s.get_llm_providers().await?;
        info!("Found {} LLM entities in storage", ents.len());
        let mut ps = self.ps.write().await;
        let mut dm = self.default_models.write().await;
        for entity in entities {
            let provider = Self::build_provider_from_entity(&entity)?;
            debug!("Registered LLM provider: {}", entity.name);
            if !entity.default_model.is_empty() {
                dm.insert(entity.name.clone(), entity.default_model.clone());
            }
            ps.insert(entity.name.clone(), provider);
        }

        Ok(())
    }

    /// Register a provider at runtime from an LlmEntity.
    /// Safe to call after initialization — uses internal RwLock.
    pub async fn register_provider(&self, entity: &LlmEntity) -> HoResult<()> {
        let provider = Self::build_provider_from_entity(entity)?;
        let mut ps = self.ps.write().await;
        info!("Runtime-registered LLM provider: {}", entity.name);
        ps.insert(entity.name.clone(), provider);
        drop(ps);
        if !entity.default_model.is_empty() {
            let mut dm = self.default_models.write().await;
            dm.insert(entity.name.clone(), entity.default_model.clone());
        }
        Ok(())
    }

    /// Build a provider trait object from an LlmEntity configuration
    fn build_provider_from_entity(entity: &LlmEntity) -> HoResult<Arc<dyn LlmProviderTrait>> {
        use crate::llm::*;

        let provider: Arc<dyn LlmProviderTrait> = match entity.name.to_lowercase().as_str() {
            "openai" => {
                let mut p = OpenAiProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !OpenAiProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "anthropic" => {
                let mut p = AnthropicProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !AnthropicProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "grok" => {
                let mut p = GrokProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !GrokProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "akashml" | "akash" => {
                let mut p = AkashProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !AkashProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "kimi" | "kimi_research" => {
                let mut p = KimiProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !KimiProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "qwen" => {
                let mut p = QwenProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !QwenProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "venice" => {
                let mut p = VeniceProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !VeniceProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            "ollama" => {
                let mut p = OllamaProvider::new(None).with_base_url(entity.base_url.clone());
                for m in &entity.models {
                    if !OllamaProvider::MODELS.contains(&m.as_str()) {
                        p.add_supported_model(m.clone());
                    }
                }
                Arc::new(p)
            }
            // Generic/custom provider - supports any OpenAI-compatible API
            // Default providers above are preferred for convenience, but any provider name is allowed
            custom_name => {
                info!(
                    "Registering custom provider '{}' (OpenAI-compatible) at {}",
                    custom_name, entity.base_url
                );
                // Use OpenAI provider as the base for custom providers (most APIs are OpenAI-compatible)
                // Set empty string api_key for keyless operation (skips Authorization header)
                let mut p = OpenAiProvider::new(Some(String::new())).with_base_url(entity.base_url.clone());
                // Add all configured models as supported
                for m in &entity.models {
                    p.add_supported_model(m.clone());
                }
                Arc::new(p)
            }
        };

        Ok(provider)
    }

    /// Call a specific provider by name (bypasses model-pattern matching).
    /// Used by role-based routing where the provider is already known.
    pub async fn call_provider_by_name(&self, name: &str, req: &PromptRequest) -> HoResult<PromptResponse> {
        let ps = self.ps.read().await;
        let provider = ps.get(name).cloned().ok_or_else(|| {
            let available: Vec<String> = ps.keys().cloned().collect();
            HoError::Llm(format!(
                "Provider '{}' not found in LLM router. Registered providers: {:?}. \
                 Was this provider added via 'provider add' with a base_url?",
                name, available
            ))
        })?;
        drop(ps);

        // Substitute model name if a default_model override is registered
        let model_override = self.default_models.read().await.get(name).cloned();
        if let Some(model) = model_override {
            let mut req = req.clone();
            req.model = model;
            return provider.call(&self.c, &req).await;
        }
        provider.call(&self.c, req).await
    }

    /// Get all registered ps
    pub async fn get_providers(&self) -> Vec<Arc<dyn LlmProviderTrait>> {
        self.ps.read().await.values().cloned().collect()
    }

    /// Get a specific provider by name
    pub async fn get_provider(&self, name: &str) -> Option<Arc<dyn LlmProviderTrait>> {
        self.ps.read().await.get(name).cloned()
    }

    /// Get the default model name for a provider (if registered via model_map)
    pub async fn get_default_model(&self, name: &str) -> Option<String> {
        self.default_models.read().await.get(name).cloned()
    }

    /// Get all registered provider names
    pub async fn get_provider_names(&self) -> Vec<String> {
        self.ps.read().await.keys().cloned().collect()
    }

    /// Get the number of registered ps
    pub async fn provider_count(&self) -> usize {
        self.ps.read().await.len()
    }
}

/// Storage integration methods for LLM r configuration
impl LlmRouter {
    /// Initialize storage with default r configuration
    ///
    /// This writes the default r c to storage if none exists
    pub async fn init_storage<S: StateRead + cnidarium::StateWrite>(
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
    pub async fn update_storage_config<S: StateRead + cnidarium::StateWrite>(
        s: &mut S,
        c: &LlmRouterConfig,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;
        s.put_llm_router_config(c);
        s.put_llm_providers(&c.entities);
        Ok(())
    }

    /// Add a provider to storage
    pub async fn add_provider_to_storage<S: StateRead + cnidarium::StateWrite>(
        s: &mut S,
        provider: &LlmEntity,
    ) -> HoResult<()> {
        use crate::llm::StateWriteExt;
        s.put_llm_provider(provider);
        Ok(())
    }

    /// Remove a provider from storage
    pub async fn remove_provider_from_storage<S: cnidarium::StateWrite>(
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
