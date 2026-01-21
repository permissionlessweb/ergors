//! Proxy router for configurable provider routing.
//!
//! Routes requests to different upstream providers based on:
//! - Model name patterns (e.g., "claude-*" -> Anthropic, "gpt-*" -> OpenAI)
//! - Explicit configuration overrides
//! - API key management per upstream

use anyhow::Result;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Default Anthropic API base URL
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
/// Default OpenAI API base URL
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com";

/// Provider type for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Anthropic,
    OpenAI,
    Custom,
}

/// Route target containing upstream URL and optional API key
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub base_url: String,
    pub api_key: Option<String>,
    pub provider_type: ProviderType,
}

/// Proxy router configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProxyRouterConfig {
    /// Override base URL for Anthropic API requests
    #[serde(default)]
    pub anthropic_base_url: Option<String>,

    /// Override base URL for OpenAI API requests
    #[serde(default)]
    pub openai_base_url: Option<String>,

    /// Model-specific routing rules (glob patterns supported)
    /// e.g., "claude-*" -> "https://api.anthropic.com"
    /// e.g., "llama-*" -> "http://localhost:11434"
    #[serde(default)]
    pub model_routes: HashMap<String, String>,

    /// API key overrides per upstream URL
    #[serde(default)]
    pub api_keys: HashMap<String, String>,

    /// Default API keys by provider
    #[serde(default)]
    pub provider_api_keys: HashMap<String, String>,
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

    /// Get the route target for an Anthropic-format request
    pub fn route_anthropic(&self, model: &str) -> RouteTarget {
        // Check model-specific routes first
        if let Some(route) = self.match_model_route(model) {
            return route;
        }

        // Use configured or default Anthropic URL
        let base_url = self
            .config
            .anthropic_base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_URL.to_string());

        let api_key = self
            .config
            .api_keys
            .get(&base_url)
            .cloned()
            .or_else(|| self.config.provider_api_keys.get("anthropic").cloned());

        RouteTarget {
            base_url,
            api_key,
            provider_type: ProviderType::Anthropic,
        }
    }

    /// Get the route target for an OpenAI-format request
    pub fn route_openai(&self, model: &str) -> RouteTarget {
        // Check model-specific routes first
        if let Some(route) = self.match_model_route(model) {
            return route;
        }

        // Use configured or default OpenAI URL
        let base_url = self
            .config
            .openai_base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_OPENAI_URL.to_string());

        let api_key = self
            .config
            .api_keys
            .get(&base_url)
            .cloned()
            .or_else(|| self.config.provider_api_keys.get("openai").cloned());

        RouteTarget {
            base_url,
            api_key,
            provider_type: ProviderType::OpenAI,
        }
    }

    /// Match a model name against configured routes
    fn match_model_route(&self, model: &str) -> Option<RouteTarget> {
        for (pattern, url) in &self.config.model_routes {
            if glob_match(pattern, model) {
                debug!("Model '{}' matched route pattern '{}' -> {}", model, pattern, url);

                let api_key = self.config.api_keys.get(url).cloned();
                let provider_type = infer_provider_type(url);

                return Some(RouteTarget {
                    base_url: url.clone(),
                    api_key,
                    provider_type,
                });
            }
        }
        None
    }

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

/// Simple glob matching for model patterns
/// Supports:
/// - "*" matches any sequence of characters
/// - "?" matches any single character
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Match zero or more characters
                if pattern_chars.peek().is_none() {
                    return true; // Trailing * matches everything
                }
                // Try matching rest of pattern at each position
                let rest_pattern: String = pattern_chars.collect();
                let mut remaining = String::new();
                while text_chars.peek().is_some() {
                    if glob_match(&rest_pattern, &remaining) {
                        return true;
                    }
                    remaining.push(text_chars.next().unwrap());
                }
                // Try matching at end
                return glob_match(&rest_pattern, &remaining);
            }
            '?' => {
                // Match any single character
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match literal character (case-insensitive)
                match text_chars.next() {
                    Some(t) if t.to_lowercase().next() == c.to_lowercase().next() => {}
                    _ => return false,
                }
            }
        }
    }

    // Pattern consumed, text should also be consumed
    text_chars.peek().is_none()
}

/// Infer provider type from URL
fn infer_provider_type(url: &str) -> ProviderType {
    let url_lower = url.to_lowercase();
    if url_lower.contains("anthropic") {
        ProviderType::Anthropic
    } else if url_lower.contains("openai") {
        ProviderType::OpenAI
    } else {
        ProviderType::Custom
    }
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

        // Single character wildcard
        assert!(glob_match("gpt-?", "gpt-4"));
        assert!(!glob_match("gpt-?", "gpt-4-turbo"));

        // Exact match
        assert!(glob_match("gpt-4", "gpt-4"));
        assert!(!glob_match("gpt-4", "gpt-4-turbo"));

        // Case insensitive
        assert!(glob_match("Claude-*", "claude-3-opus"));
    }

    #[test]
    fn test_default_routing() {
        let router = ProxyRouter::default_router();

        let anthropic_target = router.route_anthropic("claude-3-opus");
        assert_eq!(anthropic_target.base_url, DEFAULT_ANTHROPIC_URL);
        assert_eq!(anthropic_target.provider_type, ProviderType::Anthropic);

        let openai_target = router.route_openai("gpt-4");
        assert_eq!(openai_target.base_url, DEFAULT_OPENAI_URL);
        assert_eq!(openai_target.provider_type, ProviderType::OpenAI);
    }

    #[test]
    fn test_model_routing() {
        let mut model_routes = HashMap::new();
        model_routes.insert("llama-*".to_string(), "http://localhost:11434".to_string());
        model_routes.insert("mistral-*".to_string(), "http://localhost:11434".to_string());

        let config = ProxyRouterConfig {
            model_routes,
            ..Default::default()
        };

        let router = ProxyRouter::new(config);

        // Should route to local Ollama
        let llama_target = router.route_openai("llama-3.1-70b");
        assert_eq!(llama_target.base_url, "http://localhost:11434");

        // Should route to default OpenAI
        let gpt_target = router.route_openai("gpt-4");
        assert_eq!(gpt_target.base_url, DEFAULT_OPENAI_URL);
    }

    #[test]
    fn test_custom_base_url() {
        let config = ProxyRouterConfig {
            anthropic_base_url: Some("http://localhost:8080".to_string()),
            ..Default::default()
        };

        let router = ProxyRouter::new(config);
        let target = router.route_anthropic("claude-3-opus");
        assert_eq!(target.base_url, "http://localhost:8080");
    }
}
