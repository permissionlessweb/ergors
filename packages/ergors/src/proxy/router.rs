//! Proxy router for configurable provider routing.
//!
//! Routes requests to different upstream providers based on:
//! - Model name patterns (e.g., "claude-*" -> Anthropic, "gpt-*" -> OpenAI)
//! - Explicit configuration overrides
//! - Generic provider configuration with extensible API key management

use anyhow::{anyhow, Result};
use bytes::Bytes;
use ho_std::constants::{ANTHROPIC_BASE_URL, OPENAI_BASE_URL};
use ho_std::types::ergors::orch::v1::{
    InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig,
};

/// Convenience alias for use in tests and external code.
pub type ProviderType = InferenceProviderType;
use reqwest::Client;
use tracing::{debug, warn};

/// Route target containing upstream URL and optional API key
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub base_url: String,
    pub api_key: Option<String>,
    pub provider_type: i32, // Use i32 for proto enum
}

/// Proxy router for request routing
#[derive(Debug, Clone)]
pub struct ProxyRouter {
    config: ProxyRouterConfig,
    client: Client,
}

impl ProxyRouter {
    /// Create a new proxy router with the given configuration
    pub fn new(config: ProxyRouterConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Create a proxy router with default configuration
    pub fn default_router() -> Self {
        Self::new(ProxyRouterConfig::default())
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

    /// Get route target from provider configuration
    fn provider_to_route_target(&self, provider: &InferenceProviderConfig) -> Result<RouteTarget> {
        if !provider.enabled {
            return Err(anyhow!("Provider '{}' is disabled", provider.provider_id));
        }

        // Resolve API key (support both direct key and key references)
        let api_key = if !provider.api_key.is_empty() {
            Some(provider.api_key.clone())
        } else if !provider.api_key_ref.is_empty() {
            // TODO: Implement custody key resolution
            // For now, treat api_key_ref as env var reference
            if let Some(env_ref) = provider.api_key_ref.strip_prefix("env://") {
                std::env::var(env_ref).ok()
            } else {
                warn!(
                    "API key reference not yet supported: {}",
                    provider.api_key_ref
                );
                None
            }
        } else {
            None
        };

        Ok(RouteTarget {
            base_url: provider.base_url.clone(),
            api_key,
            provider_type: provider.provider_type,
        })
    }

    // ============= Legacy API (for backward compatibility) =============

    /// Get the route target for an Anthropic-format request
    pub fn route_anthropic(&self, model: &str) -> RouteTarget {
        // Check model-specific routes first (pattern -> provider_id)
        if let Ok(route) = self.match_model_route(model) {
            return route;
        }

        // Fallback: look for "anthropic" provider
        if let Some(provider) = self.get_enabled_provider("anthropic") {
            if let Ok(target) = self.provider_to_route_target(provider) {
                return target;
            }
        }

        // Final fallback: use default Anthropic URL
        warn!("No anthropic provider configured, using default URL");
        RouteTarget {
            base_url: ANTHROPIC_BASE_URL.to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Anthropic as i32,
        }
    }

    /// Get the route target for an OpenAI-format request
    pub fn route_openai(&self, model: &str) -> RouteTarget {
        // Check model-specific routes first (pattern -> provider_id)
        if let Ok(route) = self.match_model_route(model) {
            return route;
        }

        // Fallback: look for "openai" provider
        if let Some(provider) = self.get_enabled_provider("openai") {
            if let Ok(target) = self.provider_to_route_target(provider) {
                return target;
            }
        }

        // Final fallback: use default OpenAI URL
        warn!("No openai provider configured, using default URL");
        RouteTarget {
            base_url: OPENAI_BASE_URL.to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Openai as i32,
        }
    }

    /// Get the route target for an Ollama-format request
    pub fn route_ollama(&self, model: &str) -> RouteTarget {
        // Check model-specific routes first (pattern -> provider_id)
        if let Ok(route) = self.match_model_route(model) {
            return route;
        }

        // Fallback: look for "ollama" provider
        if let Some(provider) = self.get_enabled_provider("ollama") {
            if let Ok(target) = self.provider_to_route_target(provider) {
                return target;
            }
        }

        // Check deprecated ollama_base_url field for backward compatibility
        #[allow(deprecated)]
        if !self.config.ollama_base_url.is_empty() {
            warn!("Using deprecated ollama_base_url field, please migrate to providers map");
            return RouteTarget {
                base_url: self.config.ollama_base_url.clone(),
                api_key: None,
                provider_type: InferenceProviderType::Ollama as i32,
            };
        }

        // Final fallback: use localhost
        warn!("No ollama provider configured, using localhost:11434");
        RouteTarget {
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            provider_type: InferenceProviderType::Ollama as i32,
        }
    }

    /// Match a model name against configured routes
    /// Returns RouteTarget by looking up provider from model_routes map
    fn match_model_route(&self, model: &str) -> Result<RouteTarget> {
        for (pattern, provider_id) in &self.config.model_routes {
            if glob_match(pattern, model) {
                debug!(
                    "Model '{}' matched route pattern '{}' -> provider '{}'",
                    model, pattern, provider_id
                );

                // Look up provider configuration
                if let Some(provider) = self.get_enabled_provider(provider_id) {
                    return self.provider_to_route_target(provider);
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
        let target = self.route_anthropic(model);
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
        let target = self.route_openai(model);
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
        let target = self.route_ollama(model);
        let url = format!("{}{}", target.base_url, endpoint);

        debug!("Routing Ollama request for model '{}' to {}", model, url);

        let mut request = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .body(body);

        // Add API key header if configured (some Ollama deployments require it)
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
        // Basic wildcard
        assert!(glob_match("claude-*", "claude-3-opus"));
        assert!(glob_match("gpt-*", "gpt-4"));
        assert!(glob_match("gpt-*-turbo", "gpt-4-turbo"));

        // No match
        assert!(!glob_match("claude-*", "gpt-4"));
        assert!(!glob_match("gpt-*", "claude-3"));

        // Exact match
        assert!(glob_match("gpt-4", "gpt-4"));
        assert!(!glob_match("gpt-4", "gpt-4-turbo"));

        // Universal wildcard
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn test_default_routing() {
        let router = ProxyRouter::default_router();

        let anthropic_target = router.route_anthropic("claude-3-opus");
        assert_eq!(anthropic_target.base_url, ANTHROPIC_BASE_URL);
        assert_eq!(
            anthropic_target.provider_type,
            InferenceProviderType::Anthropic as i32
        );

        let openai_target = router.route_openai("gpt-4");
        assert_eq!(openai_target.base_url, OPENAI_BASE_URL);
        assert_eq!(
            openai_target.provider_type,
            InferenceProviderType::Openai as i32
        );
    }
}
