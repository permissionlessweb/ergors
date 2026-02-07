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
    AkashDeploymentWorkflow, AkashServiceEndpoint, AkashWorkflowStatus, PromptMessage,
    PromptRequest, TokenUsage,
};

#[allow(deprecated)]
use ergors::proxy::router::{ProviderType, ProxyRouter};
use ergors::proxy::{InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig};

use ho_std::constants::{ANTHROPIC_BASE_URL, OPENAI_BASE_URL};
use std::collections::HashMap;

const DEFAULT_ANTHROPIC_URL: &str = ANTHROPIC_BASE_URL;
const DEFAULT_OPENAI_URL: &str = OPENAI_BASE_URL;

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
            "s1",
            "alpha",
            "model-a",
            "https://a:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s2",
            "beta",
            "model-b",
            "https://b:8080",
        ))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow(
            "s3",
            "gamma",
            "model-c",
            "https://c:8080",
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
        .add_deployment(&make_completed_workflow("s1", "a", "m-a", "https://a:8080"))
        .await
        .unwrap();
    cache
        .add_deployment(&make_completed_workflow("s2", "b", "m-b", "https://b:8080"))
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
    let provider_models = vec!["gpt-4".to_string(), "claude-3-opus".to_string()];

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
// Test 6: ProxyRouter Glob Matching & Provider Routing
// ============================================================================

/// Verify that default ProxyRouter routes Anthropic to DEFAULT_ANTHROPIC_URL.
#[tokio::test]
async fn test_proxy_router_default_anthropic_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_anthropic("claude-3-opus").await.unwrap();

    assert_eq!(target.base_url, DEFAULT_ANTHROPIC_URL);
    assert_eq!(
        target.provider_type,
        InferenceProviderType::Anthropic as i32
    );
}

/// Verify that default ProxyRouter routes OpenAI to DEFAULT_OPENAI_URL.
#[tokio::test]
async fn test_proxy_router_default_openai_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_openai("gpt-4").await.unwrap();

    assert_eq!(target.base_url, DEFAULT_OPENAI_URL);
    assert_eq!(target.provider_type, InferenceProviderType::Openai as i32);
}

/// Verify that default ProxyRouter routes Ollama to localhost:11434.
#[tokio::test]
async fn test_proxy_router_default_ollama_routing() {
    let router = ProxyRouter::default_router();
    let target = router.route_ollama("llama3.1").await.unwrap();

    assert_eq!(target.base_url, "http://localhost:11434");
    assert_eq!(target.provider_type, InferenceProviderType::Ollama as i32);
    assert!(target.api_key.is_none());
}

/// Verify that model-specific glob routes override default routing.
#[tokio::test]
async fn test_proxy_router_glob_model_route_overrides() {
    let config = make_router_config_with_routes(
        vec![("llama-*", "local-ollama"), ("mistral-*", "local-ollama")],
        vec![(
            "local-ollama",
            "http://localhost:11434",
            InferenceProviderType::Ollama,
        )],
    );
    let router = ProxyRouter::new(config, None);

    let llama_target = router.route_openai("llama-3.1-70b").await.unwrap();
    assert_eq!(llama_target.base_url, "http://localhost:11434");

    let mistral_target = router.route_openai("mistral-large-2").await.unwrap();
    assert_eq!(mistral_target.base_url, "http://localhost:11434");

    let gpt_target = router.route_openai("gpt-4").await.unwrap();
    assert_eq!(gpt_target.base_url, DEFAULT_OPENAI_URL);
}

/// Verify that custom base URL overrides the default for a provider.
#[tokio::test]
async fn test_proxy_router_custom_base_url() {
    let config = make_router_config_with_providers(vec![
        (
            "anthropic",
            "http://localhost:9090",
            InferenceProviderType::Anthropic,
        ),
        (
            "openai",
            "http://localhost:9091",
            InferenceProviderType::Openai,
        ),
        (
            "ollama",
            "http://remote-ollama:11434",
            InferenceProviderType::Ollama,
        ),
    ]);
    let router = ProxyRouter::new(config, None);

    assert_eq!(
        router.route_anthropic("claude-3-opus").await.unwrap().base_url,
        "http://localhost:9090"
    );
    assert_eq!(
        router.route_openai("gpt-4").await.unwrap().base_url,
        "http://localhost:9091"
    );
    assert_eq!(
        router.route_ollama("llama3").await.unwrap().base_url,
        "http://remote-ollama:11434"
    );
}

/// Verify that API keys are resolved via custody-backed accessor.
#[tokio::test]
async fn test_proxy_router_api_key_via_accessor() {
    use ho_std::traits::ApiKeyMethod;
    use std::sync::Arc;

    let accessor = Arc::new(MockKeyAccessor(HashMap::from([
        ("custom".to_string(), "sk-custom-key-123".to_string()),
    ])));

    let mut providers = HashMap::new();
    providers.insert(
        "custom".to_string(),
        InferenceProviderConfig {
            provider_id: "custom".to_string(),
            base_url: "http://custom-provider:8080".to_string(),
            enabled: true,
            provider_type: InferenceProviderType::Custom as i32,
            ..Default::default()
        },
    );

    let mut model_routes = HashMap::new();
    model_routes.insert("custom-*".to_string(), "custom".to_string());

    let config = ProxyRouterConfig {
        model_routes,
        providers,
        ..Default::default()
    };
    let router = ProxyRouter::new(config, Some(accessor));

    let target = router.route_openai("custom-model-v1").await.unwrap();
    assert_eq!(target.base_url, "http://custom-provider:8080");
    assert_eq!(target.api_key.as_deref(), Some("sk-custom-key-123"));
}

/// Verify that custody-backed keys are resolved for provider fallback routing.
#[tokio::test]
async fn test_proxy_router_provider_api_key_via_accessor() {
    use std::sync::Arc;

    let accessor = Arc::new(MockKeyAccessor(HashMap::from([
        ("anthropic".to_string(), "sk-ant-fallback".to_string()),
        ("openai".to_string(), "sk-oai-fallback".to_string()),
    ])));

    let config = make_router_config_with_providers(vec![
        (
            "anthropic",
            "https://api.anthropic.com",
            InferenceProviderType::Anthropic,
        ),
        (
            "openai",
            "https://api.openai.com",
            InferenceProviderType::Openai,
        ),
    ]);
    let router = ProxyRouter::new(config, Some(accessor));

    let anthropic_target = router.route_anthropic("claude-3-opus").await.unwrap();
    assert_eq!(anthropic_target.api_key.as_deref(), Some("sk-ant-fallback"));

    let openai_target = router.route_openai("gpt-4").await.unwrap();
    assert_eq!(openai_target.api_key.as_deref(), Some("sk-oai-fallback"));
}

/// Verify that ProxyRouterConfig default has no overrides.
#[test]
fn test_proxy_router_config_default_is_empty() {
    let config = ProxyRouterConfig::default();

    assert!(config.model_routes.is_empty());
    assert!(config.providers.is_empty());
}

/// Verify that update_config replaces the router configuration.
#[tokio::test]
async fn test_proxy_router_update_config() {
    let mut router = ProxyRouter::default_router();

    assert_eq!(
        router.route_anthropic("claude").await.unwrap().base_url,
        DEFAULT_ANTHROPIC_URL
    );

    let new_config = make_router_config_with_providers(vec![(
        "anthropic",
        "http://new-proxy:8080",
        InferenceProviderType::Anthropic,
    )]);
    router.update_config(new_config);

    assert_eq!(
        router.route_anthropic("claude").await.unwrap().base_url,
        "http://new-proxy:8080"
    );
}

/// Verify that InferenceProviderType enum contains expected variants.
#[test]
fn test_provider_type_variants() {
    assert_eq!(ProviderType::Anthropic, ProviderType::Anthropic);
    assert_eq!(ProviderType::Openai, ProviderType::Openai);
    assert_eq!(ProviderType::Ollama, ProviderType::Ollama);
    assert_eq!(ProviderType::Custom, ProviderType::Custom);
    assert_ne!(ProviderType::Anthropic, ProviderType::Openai);
}

/// Verify that glob patterns with * match multiple characters but not mismatches.
#[tokio::test]
async fn test_proxy_router_wildcard_glob() {
    let config = make_router_config_with_routes(
        vec![("gpt-4*", "openai-custom")],
        vec![(
            "openai-custom",
            "http://gpt-custom:8080",
            InferenceProviderType::Openai,
        )],
    );
    let router = ProxyRouter::new(config, None);

    let target = router.route_openai("gpt-4").await.unwrap();
    assert_eq!(target.base_url, "http://gpt-custom:8080");

    let target_turbo = router.route_openai("gpt-4-turbo").await.unwrap();
    assert_eq!(target_turbo.base_url, "http://gpt-custom:8080");

    let target_claude = router.route_openai("claude-3").await.unwrap();
    assert_eq!(target_claude.base_url, DEFAULT_OPENAI_URL);
}

/// Verify case-sensitive glob matching (patterns are matched as-is).
#[tokio::test]
async fn test_proxy_router_case_sensitive_glob() {
    let config = make_router_config_with_routes(
        vec![("Claude-*", "anthropic-custom")],
        vec![(
            "anthropic-custom",
            "http://case-test:8080",
            InferenceProviderType::Anthropic,
        )],
    );
    let router = ProxyRouter::new(config, None);

    let target = router.route_anthropic("Claude-3-opus").await.unwrap();
    assert_eq!(target.base_url, "http://case-test:8080");
}

// ============================================================================
// Mock Key Accessor for tests
// ============================================================================

/// Simple in-memory ApiKeyMethod implementation for testing
struct MockKeyAccessor(HashMap<String, String>);

#[async_trait::async_trait]
impl ho_std::traits::ApiKeyMethod for MockKeyAccessor {
    async fn get_key(&self, provider: &str) -> ho_std::error::HoResult<Option<String>> {
        Ok(self.0.get(provider).cloned())
    }

    async fn set_key(&mut self, provider: &str, key: String) -> ho_std::error::HoResult<()> {
        self.0.insert(provider.to_string(), key);
        Ok(())
    }

    async fn available_providers(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
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

/// Build a ProxyRouterConfig with providers (no API keys).
fn make_router_config_with_providers(
    providers_list: Vec<(&str, &str, InferenceProviderType)>,
) -> ProxyRouterConfig {
    let mut providers = HashMap::new();
    for (id, base_url, ptype) in providers_list {
        providers.insert(
            id.to_string(),
            InferenceProviderConfig {
                provider_id: id.to_string(),
                base_url: base_url.to_string(),
                enabled: true,
                provider_type: ptype as i32,
                ..Default::default()
            },
        );
    }
    ProxyRouterConfig {
        providers,
        ..Default::default()
    }
}

/// Build a ProxyRouterConfig with model routes and backing providers.
fn make_router_config_with_routes(
    routes: Vec<(&str, &str)>,
    providers_list: Vec<(&str, &str, InferenceProviderType)>,
) -> ProxyRouterConfig {
    let mut model_routes = HashMap::new();
    for (pattern, provider_id) in routes {
        model_routes.insert(pattern.to_string(), provider_id.to_string());
    }
    let mut providers = HashMap::new();
    for (id, base_url, ptype) in providers_list {
        providers.insert(
            id.to_string(),
            InferenceProviderConfig {
                provider_id: id.to_string(),
                base_url: base_url.to_string(),
                enabled: true,
                provider_type: ptype as i32,
                ..Default::default()
            },
        );
    }
    ProxyRouterConfig {
        model_routes,
        providers,
        ..Default::default()
    }
}
