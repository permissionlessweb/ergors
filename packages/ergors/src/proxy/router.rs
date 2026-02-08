// Proxy router for configurable provider routing.
//
// Routes requests to different upstream providers based on:
// - Model name patterns (e.g., "claude-*" -> Anthropic, "gpt-*" -> OpenAI)
// - Explicit configuration overrides
// - Generic provider configuration with extensible API key management
//
// Cosmic Orchestrator Module - Python-to-Rust Migration
//
// This module implements the complete AgentOrchestrator functionality from orchestrator.py,
// incorporating cosmic/geometric principles and fractal recursion for AI agent orchestration.

use anyhow::{anyhow, Result};
use async_stream::stream;
use axum::{
    extract::State,
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use bytes::Bytes;
use commonware_cryptography::{blake3, Hasher};
use ho_std::constants::{ANTHROPIC_BASE_URL, OPENAI_BASE_URL};
use ho_std::error::{error_json, error_json_detailed};
use ho_std::traits::ApiKeyMethod;
use ho_std::types::ergors::orch::v1::*;
use ho_std::types::ergors::orch::v1::{
    InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig,
};
use reqwest::Client;
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::proxy::{
    error::OpenResponsesError,
    open_responses::{
        filter_tools, parse_open_responses_request, prompt_response_to_open_responses,
    },
    streaming::OpenResponsesStreamTransformer,
    upstream::{forward_to_anthropic, forward_to_openai},
};
use crate::ErgorsAppState;

pub async fn handle_prompt(
    State(state): State<ErgorsAppState>,
    Json(r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let pr = match serde_json::from_value::<PromptRequest>(r.clone()) {
        Ok(r) => r,
        Err(e) => {
            return Json(error_json(
                &format!(
                    "Invalid request format: {}. Expected format: {:#?}",
                    e,
                    PromptRequest::default()
                ),
                "INVALID_REQUEST",
            ));
        }
    };

    if pr.messages.is_empty() {
        return Json(error_json(
            "Prompt messages cannot be empty",
            "INVALID_PROMPT",
        ));
    }

    // Check if Open Responses format is requested
    let use_open_responses = pr
        .response_format
        .as_deref()
        .map(|f| f == "open_responses")
        .unwrap_or(false);

    match state.r.handle_request(&pr, &pr.model).await {
        Ok(llm_response) => {
            if use_open_responses {
                // Return in Open Responses format
                let response_id = format!("resp_{}", Uuid::new_v4().simple());
                let json = prompt_response_to_open_responses(&llm_response, &response_id);
                Json(json)
            } else {
                // Return in original ERGORS format
                let response = PromptResponse {
                    id: Uuid::new_v4().into(),
                    prompt: blake3::Blake3::hash(serde_json::to_string(&pr).unwrap().as_bytes())
                        .to_string(),
                    response: llm_response.response,
                    model: pr.model.to_string(),
                    timestamp: Some(pbjson_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    tokens_used: llm_response.tokens_used,
                    provider: "default".to_string(),
                    cost: 0.0,
                    latency_ms: 0,
                    status: None,
                    output: vec![],
                    response_metadata: None,
                };

                // Store to Cnidarium with original request context
                if let Err(e) = state.s.put_prompt_w_ctx(&response, Some(&pr)).await {
                    error!("Failed to store prompt to storage: {}", e);
                }

                Json(serde_json::to_value(response).unwrap())
            }
        }
        Err(e) => {
            if use_open_responses {
                // Return Open Responses error format
                Json(
                    OpenResponsesError::ModelError {
                        message: format!("LLM processing failed: {}", e),
                    }
                    .to_json(),
                )
            } else {
                // Log error with full chain if detail enabled
                let error_chain = e.error_chain();
                error!(
                    error_type = e.to_string(),
                    error = %e,
                    error_chain = ?error_chain,
                    root_cause = ?error_chain.last(),
                    "LLM processing failed"
                );
                // Use detailed error response which respects RUST_LOG_DETAIL env
                Json(error_json_detailed(&e))
            }
        }
    }
}

/// Handle Open Responses API requests (/v1/responses).
/// Supports both streaming and non-streaming modes with standardized events.
pub async fn handle_open_responses(
    State(state): State<ErgorsAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Parse the request body
    let req_json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return OpenResponsesError::InvalidRequest {
                param: "body".to_string(),
                message: format!("Invalid JSON: {}", e),
            }
            .into_response();
        }
    };

    // Convert to PromptRequest
    let mut prompt_req = match parse_open_responses_request(&req_json) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    // Handle previous_response_id - load conversation context
    if let Some(ref prev_id) = prompt_req.previous_response_id {
        let prev_messages = state
            .s
            .get_open_response_context(prev_id)
            .await
            .unwrap_or_default();
        if !prev_messages.is_empty() {
            debug!(
                "Loaded {} previous messages from response {}",
                prev_messages.len(),
                prev_id
            );
            let mut combined = prev_messages;
            combined.append(&mut prompt_req.messages);
            prompt_req.messages = combined;
        }
    }

    // Apply allowed_tools filtering
    if !prompt_req.allowed_tools.is_empty() {
        prompt_req.tools = filter_tools(&prompt_req.tools, &prompt_req.allowed_tools);
    }

    // Generate response ID
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = prompt_req.model.clone();

    if prompt_req.stream {
        // Streaming path: forward to upstream provider and transform events
        handle_open_responses_streaming(state, headers, prompt_req, response_id, model).await
    } else {
        // Non-streaming path: use LlmRouter
        handle_open_responses_non_streaming(state, prompt_req, response_id).await
    }
}

/// Non-streaming Open Responses handler
async fn handle_open_responses_non_streaming(
    state: ErgorsAppState,
    prompt_req: PromptRequest,
    response_id: String,
) -> Response {
    match state.r.handle_request(&prompt_req, &prompt_req.model).await {
        Ok(llm_response) => {
            // Store response for future previous_response_id lookups
            if let Err(e) = state
                .s
                .put_open_response(&response_id, &prompt_req, &llm_response.response)
                .await
            {
                error!("Failed to store open response session: {}", e);
            }

            let json = prompt_response_to_open_responses(&llm_response, &response_id);
            Json(json).into_response()
        }
        Err(e) => OpenResponsesError::ModelError {
            message: format!("LLM processing failed: {}", e),
        }
        .into_response(),
    }
}

/// Streaming Open Responses handler - forwards to upstream and transforms events
async fn handle_open_responses_streaming(
    _state: ErgorsAppState,
    headers: HeaderMap,
    prompt_req: PromptRequest,
    response_id: String,
    model: String,
) -> Response {
    // Determine provider type from model name
    let is_anthropic = is_anthropic_model(&model);

    // Extract API key from headers
    let api_key = if is_anthropic {
        headers
            .get("x-api-key")
            .or_else(|| headers.get("authorization"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.strip_prefix("Bearer ").unwrap_or(s).to_string())
    } else {
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    };

    let api_key = match api_key {
        Some(k) => k,
        None => {
            return OpenResponsesError::InvalidRequest {
                param: "authorization".to_string(),
                message: "API key required for streaming. Provide via Authorization header."
                    .to_string(),
            }
            .into_response();
        }
    };

    // Build upstream request body in native provider format
    let upstream_body = if is_anthropic {
        build_anthropic_request(&prompt_req)
    } else {
        build_openai_request(&prompt_req)
    };

    let upstream_bytes = Bytes::from(serde_json::to_vec(&upstream_body).unwrap_or_default());

    // Create HTTP client
    let client = reqwest::Client::new();

    // Forward to upstream
    let upstream_response = if is_anthropic {
        forward_to_anthropic(&client, upstream_bytes, &api_key, None, None).await
    } else {
        forward_to_openai(&client, upstream_bytes, &api_key, None).await
    };

    let response = match upstream_response {
        Ok(r) => r,
        Err(e) => {
            return OpenResponsesError::ServerError {
                message: format!("Upstream request failed: {}", e),
            }
            .into_response();
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return OpenResponsesError::ModelError {
            message: format!("Upstream error ({}): {}", status, body_text),
        }
        .into_response();
    }

    // Create streaming transformer and emit Open Responses events
    let mut transformer = OpenResponsesStreamTransformer::new(response_id, model.clone());

    let sse_stream = stream! {
        let body_text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading upstream stream: {}", e);
                yield Ok::<Event, Infallible>(Event::default()
                    .event("response.failed")
                    .data(serde_json::to_string(&OpenResponsesError::ServerError {
                        message: format!("Stream read error: {}", e),
                    }.to_json()).unwrap_or_default()));
                return;
            }
        };

        // Parse SSE events
        let mut buffer = body_text;
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            let mut event_type = String::new();
            let mut data = String::new();

            for line in event_data.lines() {
                if let Some(et) = line.strip_prefix("event: ") {
                    event_type = et.to_string();
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data = d.to_string();
                }
            }

            if data.is_empty() {
                continue;
            }

            // Transform based on provider type
            let events = if is_anthropic {
                transformer.transform_anthropic_event(&event_type, &data)
            } else {
                transformer.transform_openai_chunk(&data)
            };

            for evt in events {
                yield Ok::<Event, Infallible>(evt);
            }
        }

        // Emit terminal [DONE] event
        yield Ok::<Event, Infallible>(OpenResponsesStreamTransformer::done_event());
    };

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Determine if a model is an Anthropic model based on name
fn is_anthropic_model(model: &str) -> bool {
    model.starts_with("claude")
        || model.contains("anthropic")
        || model.contains("haiku")
        || model.contains("sonnet")
        || model.contains("opus")
}

/// Build an Anthropic Messages API request from a PromptRequest
fn build_anthropic_request(req: &PromptRequest) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "max_tokens": req.llm_config.as_ref().map(|c| c.max_tokens).unwrap_or(4096),
        "stream": true,
    });

    if !req.system.is_empty() {
        body["system"] = serde_json::json!(req.system);
    }

    if !req.tools.is_empty() {
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .filter_map(|t| {
                t.function.as_ref().map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "description": f.description,
                        "input_schema": {"type": "object"},
                    })
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    body
}

/// Build an OpenAI Chat Completions request from a PromptRequest
fn build_openai_request(req: &PromptRequest) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    if !req.system.is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": req.system,
        }));
    }

    for m in &req.messages {
        messages.push(serde_json::json!({
            "role": m.role,
            "content": m.content,
        }));
    }

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
    });

    if let Some(cfg) = &req.llm_config {
        if cfg.max_tokens > 0 {
            body["max_tokens"] = serde_json::json!(cfg.max_tokens);
        }
    }

    if !req.tools.is_empty() {
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .filter_map(|t| {
                t.function.as_ref().map(|f| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": f.name,
                            "description": f.description,
                            "parameters": {"type": "object"},
                        }
                    })
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    body
}

pub async fn handle_fractal_hoe_creation(// State(_state): State<ErgorsAppState>,
    // Json(request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    info!("🌀 Creating fractal hoe");
    //TODO: boostrap new node via desired method
    // Create persistent SSH connection manager
    // let mut ssh_manager = SSHConnectionManager::new(target_node.to_string());
    info!("🔌 Step 1: Establishing persistent SSH connection");
    // match ssh_manager.connect().await {}
    info!("🛠️  Step 2: Installing development environment on target node");
    // ssh_manager.install_dev_environment_via_ssh(&mut ssh_manager).await
    info!("📊  Step 3: Closing SSH connection before returning");
    // Close SSH connection before returning
    // let _ = ssh_manager.close().await;
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}

/// Convenience alias for use in tests and external code.
pub type ProviderType = InferenceProviderType;

/// Route target containing upstream URL and optional API key
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub base_url: String,
    pub api_key: Option<String>,
    pub provider_type: i32, // Use i32 for proto enum
}

/// Proxy router for request routing
#[derive(Clone)]
pub struct ProxyRouter {
    config: ProxyRouterConfig,
    client: Client,
    key_accessor: Option<Arc<tokio::sync::RwLock<dyn ApiKeyMethod>>>,
}

impl std::fmt::Debug for ProxyRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyRouter")
            .field("config", &self.config)
            .field("key_accessor", &self.key_accessor.is_some())
            .finish()
    }
}

impl ProxyRouter {
    /// Create a new proxy router with the given configuration and optional key accessor
    pub fn new(config: ProxyRouterConfig, key_accessor: Option<Arc<tokio::sync::RwLock<dyn ApiKeyMethod>>>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            key_accessor,
        }
    }

    /// Create a proxy router with default configuration
    pub fn default_router() -> Self {
        Self::new(ProxyRouterConfig::default(), None)
    }

    /// Get a reference to the key accessor (for live updates from gRPC handlers)
    pub fn key_accessor(&self) -> Option<&Arc<tokio::sync::RwLock<dyn ApiKeyMethod>>> {
        self.key_accessor.as_ref()
    }

    // ============= Generic Provider Access =============

    /// Get provider configuration by ID
    pub fn get_provider(&self, provider_id: &str) -> Option<&InferenceProviderConfig> {
        self.config.providers.get(provider_id)
    }

    /// Get enabled provider configuration by ID
    pub fn get_enabled_provider(&self, provider_id: &str) -> Option<&InferenceProviderConfig> {
        self.get_provider(provider_id).filter(|p| p.enabled)
    }

    /// Get route target from provider configuration.
    /// Resolves API key via custody-backed accessor or env var fallback.
    async fn provider_to_route_target(
        &self,
        provider: &InferenceProviderConfig,
    ) -> Result<RouteTarget> {
        if !provider.enabled {
            return Err(anyhow!("Provider '{}' is disabled", provider.provider_id));
        }

        // Resolve API key: custody:// via accessor, env:// via env var
        let api_key = if let Some(custody_id) = provider.api_key_ref.strip_prefix("custody://") {
            // Custody-backed key resolution
            if let Some(accessor) = &self.key_accessor {
                accessor.read().await.get_key(custody_id).await.ok().flatten()
            } else {
                warn!(
                    "Provider '{}' references custody://{} but no key accessor configured",
                    provider.provider_id, custody_id
                );
                None
            }
        } else if let Some(env_ref) = provider.api_key_ref.strip_prefix("env://") {
            // Legacy env var fallback
            std::env::var(env_ref).ok()
        } else if !provider.api_key_ref.is_empty() {
            // Bare provider_id — try accessor lookup by provider_id
            if let Some(accessor) = &self.key_accessor {
                accessor
                    .read().await
                    .get_key(&provider.provider_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else if let Some(accessor) = &self.key_accessor {
            // No api_key_ref at all — still try accessor by provider_id
            accessor
                .read().await
                .get_key(&provider.provider_id)
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        Ok(RouteTarget {
            base_url: provider.base_url.clone(),
            api_key,
            provider_type: provider.provider_type,
        })
    }

    // ============= Routing Methods =============

    /// Get the route target for an Anthropic-format request
    pub async fn route_anthropic(&self, model: &str) -> Result<RouteTarget> {
        if let Ok(route) = self.match_model_route(model).await {
            return Ok(route);
        }

        if let Some(provider) = self.get_enabled_provider("anthropic") {
            return self.provider_to_route_target(provider).await;
        }

        // Default fallback
        Ok(RouteTarget {
            base_url: ANTHROPIC_BASE_URL.to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Anthropic as i32,
        })
    }

    /// Get the route target for an OpenAI-format request
    pub async fn route_openai(&self, model: &str) -> Result<RouteTarget> {
        if let Ok(route) = self.match_model_route(model).await {
            return Ok(route);
        }

        if let Some(provider) = self.get_enabled_provider("openai") {
            return self.provider_to_route_target(provider).await;
        }

        // Default fallback
        Ok(RouteTarget {
            base_url: OPENAI_BASE_URL.to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Openai as i32,
        })
    }

    /// Get the route target for an Ollama-format request
    pub async fn route_ollama(&self, model: &str) -> Result<RouteTarget> {
        if let Ok(route) = self.match_model_route(model).await {
            return Ok(route);
        }

        if let Some(provider) = self.get_enabled_provider("ollama") {
            return self.provider_to_route_target(provider).await;
        }

        // Default fallback
        Ok(RouteTarget {
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Ollama as i32,
        })
    }

    /// Match a model name against configured routes
    async fn match_model_route(&self, model: &str) -> Result<RouteTarget> {
        for (pattern, provider_id) in &self.config.model_routes {
            if glob_match(pattern, model) {
                debug!(
                    "Model '{}' matched route pattern '{}' -> provider '{}'",
                    model, pattern, provider_id
                );

                if let Some(provider) = self.get_enabled_provider(provider_id) {
                    return self.provider_to_route_target(provider).await;
                } else {
                    warn!(
                        "Model route points to unknown/disabled provider: {}",
                        provider_id
                    );
                }
            }
        }
        Err(anyhow!("No matching route for model: {}", model))
    }

    // ============= Forwarding Methods =============

    /// Forward request to Anthropic (or configured upstream)
    pub async fn forward_anthropic(
        &self,
        body: Bytes,
        api_key: &str,
        model: &str,
        anthropic_version: Option<&str>,
        anthropic_beta: Option<&str>,
    ) -> Result<reqwest::Response> {
        let target = self.route_anthropic(model).await?;
        let effective_key = target.api_key.as_deref().unwrap_or(api_key);
        let url = format!("{}/v1/messages", target.base_url);

        debug!("Routing Anthropic request for model '{}' to {}", model, url);

        let mut request = self
            .client
            .post(&url)
            .header("x-api-key", effective_key)
            .header(
                "anthropic-version",
                anthropic_version.unwrap_or("2023-06-01"),
            )
            .header("content-type", "application/json")
            .body(body);

        if let Some(beta) = anthropic_beta {
            request = request.header("anthropic-beta", beta);
        }

        let response = request.send().await?;
        Ok(response)
    }

    /// Forward request to OpenAI (or configured upstream)
    pub async fn forward_openai(
        &self,
        body: Bytes,
        api_key: &str,
        model: &str,
        organization: Option<&str>,
    ) -> Result<reqwest::Response> {
        let target = self.route_openai(model).await?;
        let effective_key = target.api_key.as_deref().unwrap_or(api_key);
        let url = format!("{}/v1/chat/completions", target.base_url);

        debug!("Routing OpenAI request for model '{}' to {}", model, url);

        let mut request = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", effective_key))
            .header("content-type", "application/json")
            .body(body);

        if let Some(org) = organization {
            request = request.header("openai-organization", org);
        }

        let response = request.send().await?;
        Ok(response)
    }

    /// Forward request to Ollama (or configured upstream)
    pub async fn forward_ollama(
        &self,
        body: Bytes,
        model: &str,
        endpoint: &str, // e.g., "/api/generate", "/api/chat"
    ) -> Result<reqwest::Response> {
        let target = self.route_ollama(model).await?;
        let url = format!("{}{}", target.base_url, endpoint);

        debug!("Routing Ollama request for model '{}' to {}", model, url);

        let mut request = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .body(body);

        if let Some(api_key) = target.api_key {
            request = request.header("authorization", format!("Bearer {}", api_key));
        }

        let response = request.send().await?;
        Ok(response)
    }

    /// Get the HTTP client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Update configuration
    pub fn update_config(&mut self, config: ProxyRouterConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &ProxyRouterConfig {
        &self.config
    }
}

/// Simple glob pattern matching (supports * wildcard)
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return pattern == text;
    }

    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.is_empty() {
        return true;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            // First part must match at the beginning
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Last part must match at the end
            if !text[pos..].ends_with(part) {
                return false;
            }
        } else {
            // Middle parts must exist in order
            if let Some(idx) = text[pos..].find(part) {
                pos += idx + part.len();
            } else {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("claude-*", "claude-3-opus"));
        assert!(glob_match("gpt-*", "gpt-4"));
        assert!(glob_match("gpt-*-turbo", "gpt-4-turbo"));

        assert!(!glob_match("claude-*", "gpt-4"));
        assert!(!glob_match("gpt-*", "claude-3"));

        assert!(glob_match("gpt-4", "gpt-4"));
        assert!(!glob_match("gpt-4", "gpt-4-turbo"));

        assert!(glob_match("*", "anything"));
    }

    #[tokio::test]
    async fn test_default_routing() {
        let router = ProxyRouter::default_router();

        let anthropic_target = router.route_anthropic("claude-3-opus").await.unwrap();
        assert_eq!(anthropic_target.base_url, ANTHROPIC_BASE_URL);
        assert_eq!(
            anthropic_target.provider_type,
            InferenceProviderType::Anthropic as i32
        );

        let openai_target = router.route_openai("gpt-4").await.unwrap();
        assert_eq!(openai_target.base_url, OPENAI_BASE_URL);
        assert_eq!(
            openai_target.provider_type,
            InferenceProviderType::Openai as i32
        );
    }
}
