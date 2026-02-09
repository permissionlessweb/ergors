//! Encrypted API Key → Custody Accessor → ProxyRouter Integration Tests
//!
//! Tests the full lifecycle:
//! 1. Encrypt API keys with EncryptedApiKeyManager
//! 2. Serialize/deserialize the encrypted store (simulating persistence)
//! 3. Decrypt into the in-memory cache
//! 4. Access keys via the ApiKeyMethod trait (hot path)
//! 5. Wire into ProxyRouter and verify routing resolves custody-backed keys

use ho_std::llm::EncryptedApiKeyManager;
use ho_std::traits::ApiKeyMethod;

use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// ENCRYPTED KEY → CACHE → ACCESSOR LIFECYCLE
// =============================================================================

#[cfg(test)]
mod encrypted_key_lifecycle {
    use super::*;

    /// Full roundtrip: encrypt keys → serialize store → new manager from store
    /// → unlock → load_store → verify cache is populated → access via ApiKeyMethod
    #[tokio::test]
    async fn test_encrypt_serialize_decrypt_accessor_roundtrip() {
        let password = "integration_test_pw";

        // --- Encrypt phase (simulates sentinel bootstrap) ---
        let mut writer = EncryptedApiKeyManager::new();
        writer.unlock(password).unwrap();

        let mut keys = HashMap::new();
        keys.insert("anthropic".to_string(), "sk-ant-test-key-9999".to_string());
        keys.insert("openai".to_string(), "sk-oai-test-key-8888".to_string());
        keys.insert("grok".to_string(), "xai-grok-key-7777".to_string());

        let store = writer.create_store(&keys).unwrap();
        assert_eq!(store.keys.len(), 3);

        // --- Serialize phase (simulates Cnidarium persistence) ---
        let bytes = EncryptedApiKeyManager::serialize_store(&store);
        assert!(!bytes.is_empty());

        let loaded_store = EncryptedApiKeyManager::deserialize_store(&bytes).unwrap();
        assert_eq!(loaded_store.keys.len(), 3);

        // --- Decrypt phase (simulates server startup load_and_store) ---
        let mut reader = EncryptedApiKeyManager::from_store(&loaded_store);
        reader.unlock(password).unwrap();

        let decrypted = reader.load_store(&loaded_store).unwrap();
        assert_eq!(decrypted.len(), 3);
        assert_eq!(decrypted.get("anthropic").unwrap(), "sk-ant-test-key-9999");
        assert_eq!(decrypted.get("openai").unwrap(), "sk-oai-test-key-8888");
        assert_eq!(decrypted.get("grok").unwrap(), "xai-grok-key-7777");

        // --- Hot path: cache is populated, accessor reads from memory ---
        let ant = reader.get_key("anthropic").await.unwrap();
        assert_eq!(ant.as_deref(), Some("sk-ant-test-key-9999"));

        let oai = reader.get_key("openai").await.unwrap();
        assert_eq!(oai.as_deref(), Some("sk-oai-test-key-8888"));

        let grok = reader.get_key("grok").await.unwrap();
        assert_eq!(grok.as_deref(), Some("xai-grok-key-7777"));

        // Unknown provider returns None
        let missing = reader.get_key("unknown-provider").await.unwrap();
        assert!(missing.is_none());

        // available_providers lists all cached
        let mut providers = reader.available_providers().await;
        providers.sort();
        assert_eq!(providers, vec!["anthropic", "grok", "openai"]);
    }

    /// Verify wrong password fails decryption but doesn't panic or corrupt state
    #[tokio::test]
    async fn test_wrong_password_yields_empty_cache() {
        let correct_pw = "correct_password";
        let wrong_pw = "wrong_password";

        let mut writer = EncryptedApiKeyManager::new();
        writer.unlock(correct_pw).unwrap();

        let keys = HashMap::from([
            ("anthropic".to_string(), "sk-secret".to_string()),
        ]);
        let store = writer.create_store(&keys).unwrap();

        // Deserialize and unlock with wrong password
        let bytes = EncryptedApiKeyManager::serialize_store(&store);
        let loaded = EncryptedApiKeyManager::deserialize_store(&bytes).unwrap();

        let mut reader = EncryptedApiKeyManager::from_store(&loaded);
        reader.unlock(wrong_pw).unwrap(); // unlock succeeds (derives a key, just the wrong one)

        // load_store logs warnings but returns partial results (empty in this case)
        let decrypted = reader.load_store(&loaded).unwrap();
        assert!(decrypted.is_empty(), "wrong password should yield no decrypted keys");

        // Cache should be empty — no keys accessible
        let key = reader.get_key("anthropic").await.unwrap();
        assert!(key.is_none(), "wrong password cache must not expose keys");
    }

    /// Verify set_key allows runtime cache updates without re-encryption
    #[tokio::test]
    async fn test_runtime_set_key_updates_cache() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("pw").unwrap();

        // Initially empty
        assert!(manager.get_key("anthropic").await.unwrap().is_none());

        // Set at runtime
        manager.set_key("anthropic", "sk-runtime-injected".into()).await.unwrap();

        // Now accessible
        let key = manager.get_key("anthropic").await.unwrap();
        assert_eq!(key.as_deref(), Some("sk-runtime-injected"));

        // Overwrite
        manager.set_key("anthropic", "sk-updated".into()).await.unwrap();
        let updated = manager.get_key("anthropic").await.unwrap();
        assert_eq!(updated.as_deref(), Some("sk-updated"));
    }

    /// Verify lock() wipes the cache — keys are no longer accessible
    #[tokio::test]
    async fn test_lock_clears_accessor_cache() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("pw").unwrap();

        let store = manager
            .create_store(&HashMap::from([
                ("anthropic".to_string(), "sk-ant".to_string()),
            ]))
            .unwrap();
        manager.load_store(&store).unwrap();

        // Key is accessible
        assert_eq!(
            manager.get_key("anthropic").await.unwrap().as_deref(),
            Some("sk-ant")
        );

        // Lock clears everything
        manager.lock();
        assert!(manager.get_key("anthropic").await.unwrap().is_none());
        assert!(manager.available_providers().await.is_empty());
    }
}

// =============================================================================
// PROXY ROUTER WITH CUSTODY ACCESSOR
// =============================================================================

#[cfg(test)]
mod router_with_custody_accessor {
    use super::*;
    use ergors::proxy::router::ProxyRouter;
    use ergors::proxy::{InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig};

    /// Build a ProxyRouter backed by an EncryptedApiKeyManager with pre-loaded keys,
    /// then verify route resolution populates api_key from the custody cache.
    #[tokio::test]
    async fn test_router_resolves_keys_from_custody_accessor() {
        // Setup: encrypt and load keys into manager
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("test_pw").unwrap();

        let keys = HashMap::from([
            ("anthropic".to_string(), "sk-ant-custody-001".to_string()),
            ("openai".to_string(), "sk-oai-custody-002".to_string()),
            ("custom-llm".to_string(), "sk-custom-003".to_string()),
        ]);
        let store = manager.create_store(&keys).unwrap();
        manager.load_store(&store).unwrap();

        // Wrap as Arc<RwLock<dyn ApiKeyMethod>>
        let accessor: Arc<tokio::sync::RwLock<dyn ApiKeyMethod>> = Arc::new(tokio::sync::RwLock::new(manager));

        // Build ProxyRouterConfig with providers referencing custody
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            InferenceProviderConfig {
                provider_id: "anthropic".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                api_key_ref: "custody://anthropic".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Anthropic as i32,
                ..Default::default()
            },
        );
        providers.insert(
            "openai".to_string(),
            InferenceProviderConfig {
                provider_id: "openai".to_string(),
                base_url: "https://api.openai.com".to_string(),
                api_key_ref: "custody://openai".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Openai as i32,
                ..Default::default()
            },
        );
        providers.insert(
            "custom-llm".to_string(),
            InferenceProviderConfig {
                provider_id: "custom-llm".to_string(),
                base_url: "http://custom:8080".to_string(),
                api_key_ref: "custody://custom-llm".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Custom as i32,
                ..Default::default()
            },
        );

        let mut model_routes = HashMap::new();
        model_routes.insert("custom-*".to_string(), "custom-llm".to_string());

        let config = ProxyRouterConfig {
            providers,
            model_routes,
            ..Default::default()
        };
        let router = ProxyRouter::new(config, Some(accessor));

        // Route Anthropic — should resolve custody key
        let ant_target = router.route_anthropic("claude-3-opus").await.unwrap();
        assert_eq!(ant_target.base_url, "https://api.anthropic.com");
        assert_eq!(ant_target.api_key.as_deref(), Some("sk-ant-custody-001"));

        // Route OpenAI — should resolve custody key
        let oai_target = router.route_openai("gpt-4").await.unwrap();
        assert_eq!(oai_target.base_url, "https://api.openai.com");
        assert_eq!(oai_target.api_key.as_deref(), Some("sk-oai-custody-002"));

        // Route via glob match — custom-model-v1 -> custom-llm provider
        let custom_target = router.route_openai("custom-model-v1").await.unwrap();
        assert_eq!(custom_target.base_url, "http://custom:8080");
        assert_eq!(custom_target.api_key.as_deref(), Some("sk-custom-003"));
    }

    /// Verify that a provider without a matching custody key resolves api_key=None
    #[tokio::test]
    async fn test_router_missing_custody_key_returns_none() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("pw").unwrap();
        // Only load anthropic — openai key is NOT in custody
        let store = manager
            .create_store(&HashMap::from([
                ("anthropic".to_string(), "sk-ant-only".to_string()),
            ]))
            .unwrap();
        manager.load_store(&store).unwrap();

        let accessor: Arc<tokio::sync::RwLock<dyn ApiKeyMethod>> = Arc::new(tokio::sync::RwLock::new(manager));

        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            InferenceProviderConfig {
                provider_id: "anthropic".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                api_key_ref: "custody://anthropic".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Anthropic as i32,
                ..Default::default()
            },
        );
        providers.insert(
            "openai".to_string(),
            InferenceProviderConfig {
                provider_id: "openai".to_string(),
                base_url: "https://api.openai.com".to_string(),
                api_key_ref: "custody://openai".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Openai as i32,
                ..Default::default()
            },
        );

        let config = ProxyRouterConfig {
            providers,
            ..Default::default()
        };
        let router = ProxyRouter::new(config, Some(accessor));

        // Anthropic has a key
        let ant = router.route_anthropic("claude-3").await.unwrap();
        assert_eq!(ant.api_key.as_deref(), Some("sk-ant-only"));

        // OpenAI does NOT have a key in custody — returns None
        let oai = router.route_openai("gpt-4").await.unwrap();
        assert!(oai.api_key.is_none(), "missing custody key should yield None");
    }

    /// Verify that a router with no accessor still routes correctly (api_key=None)
    #[tokio::test]
    async fn test_router_without_accessor_routes_without_keys() {
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            InferenceProviderConfig {
                provider_id: "anthropic".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                api_key_ref: "custody://anthropic".to_string(),
                enabled: true,
                provider_type: InferenceProviderType::Anthropic as i32,
                ..Default::default()
            },
        );

        let config = ProxyRouterConfig {
            providers,
            ..Default::default()
        };
        // No accessor — custody:// reference exists but can't be resolved
        let router = ProxyRouter::new(config, None);

        let target = router.route_anthropic("claude-3").await.unwrap();
        assert_eq!(target.base_url, "https://api.anthropic.com");
        assert!(target.api_key.is_none(), "no accessor = no key resolution");
    }

    /// Verify that bare api_key_ref (no prefix) falls back to accessor lookup by provider_id
    #[tokio::test]
    async fn test_router_bare_api_key_ref_falls_back_to_accessor() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("pw").unwrap();
        let store = manager
            .create_store(&HashMap::from([
                ("my-provider".to_string(), "sk-bare-key".to_string()),
            ]))
            .unwrap();
        manager.load_store(&store).unwrap();
        let accessor: Arc<tokio::sync::RwLock<dyn ApiKeyMethod>> = Arc::new(tokio::sync::RwLock::new(manager));

        let mut providers = HashMap::new();
        providers.insert(
            "my-provider".to_string(),
            InferenceProviderConfig {
                provider_id: "my-provider".to_string(),
                base_url: "http://my-provider:8080".to_string(),
                api_key_ref: "some-opaque-ref".to_string(), // no custody:// or env:// prefix
                enabled: true,
                provider_type: InferenceProviderType::Custom as i32,
                ..Default::default()
            },
        );

        let mut model_routes = HashMap::new();
        model_routes.insert("my-*".to_string(), "my-provider".to_string());

        let config = ProxyRouterConfig {
            providers,
            model_routes,
            ..Default::default()
        };
        let router = ProxyRouter::new(config, Some(accessor));

        let target = router.route_openai("my-model").await.unwrap();
        assert_eq!(target.api_key.as_deref(), Some("sk-bare-key"));
    }

    /// Verify that empty api_key_ref still tries accessor by provider_id
    #[tokio::test]
    async fn test_router_empty_api_key_ref_tries_accessor() {
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock("pw").unwrap();
        let store = manager
            .create_store(&HashMap::from([
                ("ollama".to_string(), "sk-ollama-token".to_string()),
            ]))
            .unwrap();
        manager.load_store(&store).unwrap();
        let accessor: Arc<tokio::sync::RwLock<dyn ApiKeyMethod>> = Arc::new(tokio::sync::RwLock::new(manager));

        let mut providers = HashMap::new();
        providers.insert(
            "ollama".to_string(),
            InferenceProviderConfig {
                provider_id: "ollama".to_string(),
                base_url: "http://localhost:11434".to_string(),
                api_key_ref: String::new(), // empty — no explicit reference
                enabled: true,
                provider_type: InferenceProviderType::Ollama as i32,
                ..Default::default()
            },
        );

        let config = ProxyRouterConfig {
            providers,
            ..Default::default()
        };
        let router = ProxyRouter::new(config, Some(accessor));

        let target = router.route_ollama("llama3").await.unwrap();
        assert_eq!(target.api_key.as_deref(), Some("sk-ollama-token"));
    }
}
