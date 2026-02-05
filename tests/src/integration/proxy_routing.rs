//! Proxy routing integration tests
//!
//! Tests the LLM proxy routing layer: DeploymentProviderCache operations,
//! router priority ordering (deployment > configured provider), OpenAI
//! compatibility layer, token usage extraction, model listing, and the
//! ProxyRouter glob matching / default routing / custom base URLs.
//!
//! These tests use in-memory mocks and data structures only -- no HTTP
//! servers, no port listeners, no live infrastructure.

use ho_std::llm::DeploymentProviderCache;
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashServiceEndpoint, AkashWorkflowStatus,
    PromptMessage, PromptRequest, TokenUsage,
};

use ergors::proxy::router::{
    ProxyRouter, ProxyRouterConfig, ProviderType, DEFAULT_ANTHROPIC_URL, DEFAULT_OPENAI_URL,
};

use std::collections::HashMap;

// ============================================================================
// Test 1: DeploymentProviderCache Operations
// ============================================================================

/// Verify a completed, labeled deployment with endpoints is cached on add.
#[tokio::test]
async fn test_cache_add_completed_labeled_deployment() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "session-1",
        "qwen-inference",
        "Qwen/Qwen3-235B-A22B-FP8",
        "https://provider.akash:30123",
    );

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 1);
}

/// Verify O(1) lookup by label returns the correct endpoint.
#[tokio::test]
async fn test_cache_lookup_returns_correct_endpoint() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "session-2",
        "my-llama",
        "meta-llama/Llama-3-70B",
        "https://provider.akash:31000",
    );
    cache.add_deployment(&workflow).await.unwrap();

    let endpoint = cache.get("my-llama").await;
    assert!(endpoint.is_some(), "Cache lookup by label must succeed");
    let ep = endpoint.unwrap();
    assert_eq!(ep.session_id, "session-2");
    assert_eq!(ep.label, "my-llama");
    assert_eq!(ep.model_name(), "meta-llama/Llama-3-70B");
    assert_eq!(ep.base_url().unwrap(), "https://provider.akash:31000");
}

/// Verify that removing a deployment makes it unreachable.
#[tokio::test]
async fn test_cache_remove_makes_deployment_unreachable() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "remove-me",
        "disposable-model",
        "model-name",
        "https://provider:8080",
    );
    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 1);

    cache.remove_deployment("disposable-model").await.unwrap();
    assert_eq!(cache.count().await, 0);
    assert!(cache.get("disposable-model").await.is_none());
}

/// Verify list_models returns all cached deployment labels.
#[tokio::test]
async fn test_cache_list_models_returns_all_labels() {
    let cache = DeploymentProviderCache::new();

    cache
        .add_deployment(&make_completed_workflow(
            "s1", "alpha", "model-a", "https://a:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s2", "beta", "model-b", "https://b:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s3", "gamma", "model-c", "https://c:8080",
        ))
        .await
        .unwrap();

    let models = cache.list_models().await;
    assert_eq!(models.len(), 3);
    assert!(models.contains(&"alpha".to_string()));
    assert!(models.contains(&"beta".to_string()));
    assert!(models.contains(&"gamma".to_string()));
}

/// Verify that clear() empties the entire cache.
#[tokio::test]
async fn test_cache_clear_removes_all_entries() {
    let cache = DeploymentProviderCache::new();

    cache
        .add_deployment(&make_completed_workflow(
            "s1", "a", "m-a", "https://a:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s2", "b", "m-b", "https://b:8080",
        ))
        .await
        .unwrap();
    assert_eq!(cache.count().await, 2);

    cache.clear().await;
    assert_eq!(cache.count().await, 0);
    assert!(cache.list_models().await.is_empty());
}

/// Verify label collision replaces the old entry (last-write-wins).
#[tokio::test]
async fn test_cache_label_collision_replaces_entry() {
    let cache = DeploymentProviderCache::new();

    let wf1 = make_completed_workflow(
        "session-old",
        "shared-label",
        "model-old",
        "https://old:8080",
    );
    let wf2 = make_completed_workflow(
        "session-new",
        "shared-label",
        "model-new",
        "https://new:8080",
    );

    cache.add_deployment(&wf1).await.unwrap();
    cache.add_deployment(&wf2).await.unwrap();
    assert_eq!(cache.count().await, 1);

    let ep = cache.get("shared-label").await.unwrap();
    assert_eq!(ep.session_id, "session-new");
    assert_eq!(ep.model_name(), "model-new");
}

/// Verify that a deployment without a label is silently skipped.
#[tokio::test]
async fn test_cache_skips_unlabeled_deployment() {
    let cache = DeploymentProviderCache::new();

    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "unlabeled-session".to_string();
    workflow.label = String::new(); // no label
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "svc".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 0);
}

/// Verify that a non-completed deployment is silently skipped.
#[tokio::test]
async fn test_cache_skips_non_completed_deployment() {
    let cache = DeploymentProviderCache::new();

    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "pending-session".to_string();
    workflow.label = "pending-model".to_string();
    workflow.status = AkashWorkflowStatus::Running as i32; // not completed
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "svc".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 0);
}

/// Verify that a completed deployment with no endpoints is skipped.
#[tokio::test]
async fn test_cache_skips_deployment_without_endpoints() {
    let cache = DeploymentProviderCache::new();

    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "no-ep-session".to_string();
    workflow.label = "no-endpoint-model".to_string();
    workflow.status = AkashWorkflowStatus::Completed as i32;
    // No endpoints added

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 0);
}

/// Verify model_name() falls back to label when model_name is empty.
#[tokio::test]
async fn test_cache_model_name_fallback_to_label() {
    let cache = DeploymentProviderCache::new();

    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "fallback-session".to_string();
    workflow.label = "my-legacy-deploy".to_string();
    workflow.model_name = String::new(); // empty
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.account_address = "akash1test".to_string();
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "inference".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    let ep = cache.get("my-legacy-deploy").await.unwrap();
    assert_eq!(ep.model_name(), "my-legacy-deploy");
}

/// Verify model_name() returns the explicit model name when set.
#[tokio::test]
async fn test_cache_model_name_uses_explicit_when_set() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "explicit-model-session",
        "my-label",
        "Qwen/Qwen3-235B-A22B-FP8",
        "https://provider:8080",
    );

    cache.add_deployment(&workflow).await.unwrap();
    let ep = cache.get("my-label").await.unwrap();
    assert_eq!(ep.model_name(), "Qwen/Qwen3-235B-A22B-FP8");
    // Label is preserved separately
    assert_eq!(ep.label, "my-label");
}

// ============================================================================
// Test 2: Router Priority Ordering
// ============================================================================

/// Verify that a deployment in the cache is found before any API provider.
/// This tests the DeploymentProviderCache side of priority routing.
#[tokio::test]
async fn test_router_priority_deployment_over_provider() {
    let cache = DeploymentProviderCache::new();

    // Add a deployment for "my-model"
    let workflow = make_completed_workflow(
        "priority-session",
        "my-model",
        "Qwen/Qwen3-235B-A22B-FP8",
        "https://deployment.akash:8443",
    );
    cache.add_deployment(&workflow).await.unwrap();

    // Deployment cache should resolve "my-model"
    let endpoint = cache.get("my-model").await;
    assert!(
        endpoint.is_some(),
        "Deployment cache should resolve model before falling back to API provider"
    );
    assert_eq!(endpoint.unwrap().session_id, "priority-session");
}

/// Verify that after removing a deployment, the cache no longer resolves it.
/// In a real router, this would cause fallback to configured providers.
#[tokio::test]
async fn test_router_fallback_after_deployment_removal() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "temp-session",
        "temp-model",
        "model-name",
        "https://provider:8080",
    );
    cache.add_deployment(&workflow).await.unwrap();
    assert!(cache.get("temp-model").await.is_some());

    // Remove deployment
    cache.remove_deployment("temp-model").await.unwrap();

    // Now the cache returns None, which would cause LlmRouter to fall back
    assert!(
        cache.get("temp-model").await.is_none(),
        "After removal, cache must not resolve the model"
    );
}

// ============================================================================
// Test 3: OpenAI Compatibility Layer
// ============================================================================

/// Verify that PromptRequest can be constructed with messages matching
/// the OpenAI chat completion format (system, user, assistant roles).
#[test]
fn test_prompt_request_openai_compatible_message_roles() {
    let request = PromptRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            PromptMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
                ..Default::default()
            },
            PromptMessage {
                role: "user".to_string(),
                content: "Hello, how are you?".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role, "system");
    assert_eq!(request.messages[1].role, "user");
    assert_eq!(request.model, "gpt-4");
}

/// Verify that PromptRequest default produces an empty request.
#[test]
fn test_prompt_request_default_is_empty() {
    let request = PromptRequest::default();
    assert!(request.messages.is_empty());
    assert!(request.model.is_empty());
    assert!(request.context.is_none());
    assert!(request.llm_config.is_none());
}

/// Verify that PromptMessage default role and content are empty.
#[test]
fn test_prompt_message_default_values() {
    let msg = PromptMessage::default();
    assert_eq!(msg.role, "");
    assert_eq!(msg.content, "");
    assert!(msg.tool_calls.is_empty());
    assert!(msg.tool_result.is_none());
}

// ============================================================================
// Test 4: Token Usage Extraction
// ============================================================================

/// Verify TokenUsage struct holds prompt, completion, and total fields.
#[test]
fn test_token_usage_fields() {
    let usage = TokenUsage {
        prompt: 150,
        completion: 350,
        total: 500,
    };

    assert_eq!(usage.prompt, 150);
    assert_eq!(usage.completion, 350);
    assert_eq!(usage.total, 500);
}

/// Verify TokenUsage default is all zeros.
#[test]
fn test_token_usage_default_is_zero() {
    let usage = TokenUsage::default();
    assert_eq!(usage.prompt, 0);
    assert_eq!(usage.completion, 0);
    assert_eq!(usage.total, 0);
}

/// Verify that OpenAI JSON token usage can be extracted using serde_json.
/// This mirrors the extraction logic in LlmRouter::route_to_deployment_with_context.
#[test]
fn test_token_usage_extraction_from_openai_json() {
    let openai_response = serde_json::json!({
        "id": "chatcmpl-test123",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 25,
            "completion_tokens": 10,
            "total_tokens": 35
        }
    });

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

        TokenUsage {
            prompt,
            completion,
            total,
        }
    });

    assert!(tokens_used.is_some());
    let usage = tokens_used.unwrap();
    assert_eq!(usage.prompt, 25);
    assert_eq!(usage.completion, 10);
    assert_eq!(usage.total, 35);
}

/// Verify extraction returns None when usage field is absent.
#[test]
fn test_token_usage_extraction_absent_usage_field() {
    let openai_response = serde_json::json!({
        "id": "chatcmpl-no-usage",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "No usage info"
            }
        }]
    });

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

        TokenUsage {
            prompt,
            completion,
            total,
        }
    });

    assert!(tokens_used.is_none());
}

/// Verify content extraction from OpenAI response format.
#[test]
fn test_content_extraction_from_openai_response() {
    let openai_response = serde_json::json!({
        "id": "chatcmpl-content-test",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "The answer is 42."
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 6,
            "total_tokens": 16
        }
    });

    let content = openai_response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    assert_eq!(content, "The answer is 42.");
}

// ============================================================================
// Test 5: /v1/models Endpoint Data
// ============================================================================

/// Verify that deployments and configured providers can both contribute
/// to a combined model list (simulating /v1/models endpoint).
#[tokio::test]
async fn test_models_endpoint_combines_deployments_and_providers() {
    let cache = DeploymentProviderCache::new();

    // Add two deployments
    cache
        .add_deployment(&make_completed_workflow(
            "s1",
            "qwen-inference",
            "Qwen/Qwen3-235B-A22B-FP8",
            "https://a:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s2",
            "llama-inference",
            "meta-llama/Llama-3-70B",
            "https://b:8080",
        ))
        .await
        .unwrap();

    let deployment_models = cache.list_models().await;

    // Simulated configured provider models
    let provider_models = vec![
        "gpt-4".to_string(),
        "claude-3-opus".to_string(),
    ];

    // Combine: deployments + providers
    let mut all_models: Vec<String> = deployment_models;
    all_models.extend(provider_models);

    assert!(all_models.len() >= 4);
    assert!(all_models.contains(&"qwen-inference".to_string()));
    assert!(all_models.contains(&"llama-inference".to_string()));
    assert!(all_models.contains(&"gpt-4".to_string()));
    assert!(all_models.contains(&"claude-3-opus".to_string()));
}

/// Verify that deployment endpoint metadata includes the fields needed
/// for the /v1/models response (session_id, owner, dseq).
#[tokio::test]
async fn test_deployment_endpoint_has_model_metadata() {
    let cache = DeploymentProviderCache::new();

    let workflow = make_completed_workflow(
        "meta-session",
        "meta-model",
        "Qwen/Qwen3-235B-A22B-FP8",
        "https://provider:8443",
    );
    cache.add_deployment(&workflow).await.unwrap();

    let ep = cache.get("meta-model").await.unwrap();
    assert_eq!(ep.session_id, "meta-session");
    assert_eq!(ep.owner, "akash1testaccount");
    assert!(!ep.endpoints.is_empty());
    assert!(ep.primary_endpoint().is_some());
}

// ============================================================================
// Test 6: ProxyRouter Glob Matching
// ============================================================================

/// Verify that default ProxyRouter routes Anthropic to DEFAULT_ANTHROPIC_URL.
#[test]
fn test_proxy_router_default_anthropic_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_anthropic("claude-3-opus");

    assert_eq!(target.base_url, DEFAULT_ANTHROPIC_URL);
    assert_eq!(target.provider_type, ProviderType::Anthropic);
}

/// Verify that default ProxyRouter routes OpenAI to DEFAULT_OPENAI_URL.
#[test]
fn test_proxy_router_default_openai_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_openai("gpt-4");

    assert_eq!(target.base_url, DEFAULT_OPENAI_URL);
    assert_eq!(target.provider_type, ProviderType::OpenAI);
}

/// Verify that default ProxyRouter routes Ollama to localhost:11434.
#[test]
fn test_proxy_router_default_ollama_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_ollama("llama3.1");

    assert_eq!(target.base_url, "http://localhost:11434");
    assert_eq!(target.provider_type, ProviderType::Ollama);
    assert!(target.api_key.is_none());
}

/// Verify that model-specific glob routes override default routing.
#[test]
fn test_proxy_router_glob_model_route_overrides() {
    let mut model_routes = HashMap::new();
    model_routes.insert("llama-*".to_string(), "http://localhost:11434".to_string());
    model_routes.insert(
        "mistral-*".to_string(),
        "http://localhost:11434".to_string(),
    );

    let config = ProxyRouterConfig {
        model_routes,
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    // llama-* should route to local Ollama
    let llama_target = router.route_openai("llama-3.1-70b");
    assert_eq!(llama_target.base_url, "http://localhost:11434");

    // mistral-* should also route to local Ollama
    let mistral_target = router.route_openai("mistral-large-2");
    assert_eq!(mistral_target.base_url, "http://localhost:11434");

    // gpt-4 should NOT match any glob, falls back to default OpenAI
    let gpt_target = router.route_openai("gpt-4");
    assert_eq!(gpt_target.base_url, DEFAULT_OPENAI_URL);
}

/// Verify that custom base URL overrides the default for a provider.
#[test]
fn test_proxy_router_custom_base_url() {
    let config = ProxyRouterConfig {
        anthropic_base_url: Some("http://localhost:9090".to_string()),
        openai_base_url: Some("http://localhost:9091".to_string()),
        ollama_base_url: Some("http://remote-ollama:11434".to_string()),
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    assert_eq!(
        router.route_anthropic("claude-3-opus").base_url,
        "http://localhost:9090"
    );
    assert_eq!(
        router.route_openai("gpt-4").base_url,
        "http://localhost:9091"
    );
    assert_eq!(
        router.route_ollama("llama3").base_url,
        "http://remote-ollama:11434"
    );
}

/// Verify that API key overrides are returned for matching base URLs.
#[test]
fn test_proxy_router_api_key_override_by_url() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "http://custom-provider:8080".to_string(),
        "sk-custom-key-123".to_string(),
    );

    let mut model_routes = HashMap::new();
    model_routes.insert(
        "custom-*".to_string(),
        "http://custom-provider:8080".to_string(),
    );

    let config = ProxyRouterConfig {
        model_routes,
        api_keys,
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    let target = router.route_openai("custom-model-v1");
    assert_eq!(target.base_url, "http://custom-provider:8080");
    assert_eq!(target.api_key.as_deref(), Some("sk-custom-key-123"));
}

/// Verify that provider_api_keys are used as fallback for provider-level keys.
#[test]
fn test_proxy_router_provider_api_key_fallback() {
    let mut provider_api_keys = HashMap::new();
    provider_api_keys.insert("anthropic".to_string(), "sk-ant-fallback".to_string());
    provider_api_keys.insert("openai".to_string(), "sk-oai-fallback".to_string());

    let config = ProxyRouterConfig {
        provider_api_keys,
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    let anthropic_target = router.route_anthropic("claude-3-opus");
    assert_eq!(anthropic_target.api_key.as_deref(), Some("sk-ant-fallback"));

    let openai_target = router.route_openai("gpt-4");
    assert_eq!(openai_target.api_key.as_deref(), Some("sk-oai-fallback"));
}

/// Verify that ProxyRouterConfig default has no overrides.
#[test]
fn test_proxy_router_config_default_is_empty() {
    let config = ProxyRouterConfig::default();

    assert!(config.anthropic_base_url.is_none());
    assert!(config.openai_base_url.is_none());
    assert!(config.ollama_base_url.is_none());
    assert!(config.model_routes.is_empty());
    assert!(config.api_keys.is_empty());
    assert!(config.provider_api_keys.is_empty());
}

/// Verify that update_config replaces the router configuration.
#[test]
fn test_proxy_router_update_config() {
    let mut router = ProxyRouter::default_router();

    // Default should route to standard URLs
    assert_eq!(
        router.route_anthropic("claude").base_url,
        DEFAULT_ANTHROPIC_URL
    );

    // Update config with custom base URL
    let new_config = ProxyRouterConfig {
        anthropic_base_url: Some("http://new-proxy:8080".to_string()),
        ..Default::default()
    };
    router.update_config(new_config);

    // Now should route to new URL
    assert_eq!(
        router.route_anthropic("claude").base_url,
        "http://new-proxy:8080"
    );
}

/// Verify that ProviderType enum contains expected variants.
#[test]
fn test_provider_type_variants() {
    assert_eq!(ProviderType::Anthropic, ProviderType::Anthropic);
    assert_eq!(ProviderType::OpenAI, ProviderType::OpenAI);
    assert_eq!(ProviderType::Ollama, ProviderType::Ollama);
    assert_eq!(ProviderType::Custom, ProviderType::Custom);
    assert_ne!(ProviderType::Anthropic, ProviderType::OpenAI);
}

/// Verify that glob patterns with ? match single characters.
#[test]
fn test_proxy_router_single_char_glob() {
    let mut model_routes = HashMap::new();
    model_routes.insert("gpt-?".to_string(), "http://gpt-single:8080".to_string());

    let config = ProxyRouterConfig {
        model_routes,
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    // gpt-4 should match gpt-?
    let target = router.route_openai("gpt-4");
    assert_eq!(target.base_url, "http://gpt-single:8080");

    // gpt-4-turbo should NOT match gpt-? (too many chars)
    let target_multi = router.route_openai("gpt-4-turbo");
    assert_eq!(target_multi.base_url, DEFAULT_OPENAI_URL);
}

/// Verify case-insensitive glob matching.
#[test]
fn test_proxy_router_case_insensitive_glob() {
    let mut model_routes = HashMap::new();
    model_routes.insert(
        "Claude-*".to_string(),
        "http://case-test:8080".to_string(),
    );

    let config = ProxyRouterConfig {
        model_routes,
        ..Default::default()
    };
    let router = ProxyRouter::new(config);

    // Lowercase input should match uppercase pattern
    let target = router.route_anthropic("claude-3-opus");
    assert_eq!(target.base_url, "http://case-test:8080");
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a fully-formed completed workflow suitable for cache insertion.
fn make_completed_workflow(
    session_id: &str,
    label: &str,
    model_name: &str,
    endpoint_uri: &str,
) -> AkashDeploymentWorkflow {
    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = session_id.to_string();
    workflow.label = label.to_string();
    workflow.model_name = model_name.to_string();
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.account_address = "akash1testaccount".to_string();
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "inference".to_string(),
        external_uri: endpoint_uri.to_string(),
        internal_port: 8000,
        external_port: 8443,
        protocol: "TCP".to_string(),
        model_name: model_name.to_string(),
    });
    workflow
}
