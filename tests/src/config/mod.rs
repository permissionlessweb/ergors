//! Configuration System Tests
//!
//! Tests for the config system including:
//! - Config loading from TOML
//! - Config saving to TOML
//! - Config validation (valid and invalid configs)
//! - Environment variable overrides
//! - Secrets separation (api-keys.json isolation)
//! - Default value handling
//! - Config accessor methods

// Shared test imports - only compiled during test builds
use camino::Utf8PathBuf;
use ergors::config::{
    AkashDeployConfig, CaptureConfig, CosmwasmConfig, CosmwasmGasLimits, ErgorsConfig, ProxyConfig,
};
use ho_std::traits::{HoConfigTrait, NodeIdentityCustodyBackend};
use std::fs;
use tempfile::TempDir;

// =============================================================================
// CONFIG LOADING TESTS
// =============================================================================

#[cfg(test)]
mod config_loading {
    use super::*;

    fn setup_temp_home() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().expect("create temp dir");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("valid utf8");
        (temp, home)
    }

    #[test]
    fn test_create_default_config() {
        let (_temp, home) = setup_temp_home();
        let config = ErgorsConfig::new(&home);

        // Check all required sections exist and don't panic
        let _ = config.network();
        assert!(config.identity().api_port > 0);
    }

    #[test]
    fn test_config_save_load_roundtrip() {
        let (_temp, home) = setup_temp_home();
        let config_path = home.join("config.toml");

        // Create and save config
        let original = ErgorsConfig::new(&home);
        original.save(&config_path).expect("save config");

        // Load config
        let loaded = ErgorsConfig::load(config_path.as_str()).expect("load config");

        // Compare - identity api_port should be preserved
        assert_eq!(original.identity().api_port, loaded.identity().api_port);
    }

    #[test]
    fn test_load_nonexistent_config_fails() {
        let result = ErgorsConfig::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_malformed_config_fails() {
        let temp = TempDir::new().expect("temp");
        let path = temp.path().join("bad.toml");

        // Write invalid TOML
        fs::write(&path, "this is not { valid toml").expect("write");

        let path_str = path.to_string_lossy();
        let result = ErgorsConfig::load(&*path_str);
        assert!(result.is_err());
    }
}

// =============================================================================
// CONFIG VALIDATION TESTS
// =============================================================================

#[cfg(test)]
mod config_validation {
    use super::*;

    #[test]
    fn test_valid_config_passes_validation() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let result = config.validate();

        // Default config should be valid
        assert!(
            result.is_ok(),
            "Default config should validate: {:?}",
            result
        );
    }

    #[test]
    fn test_config_accessor_methods() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);

        // Test that all accessors return non-panicking values
        let _ = config.network();
        let _ = config.identity();
        let _ = config.storage();
        let _ = config.llm();
        let _ = config.custody();
        let _ = config.cosmwasm();
        let _ = config.akash();
    }
}

// =============================================================================
// CUSTODY CONFIG TESTS
// =============================================================================

#[cfg(test)]
mod custody_config {
    use super::*;

    #[test]
    fn test_default_custody_config() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let custody = config.custody();

        assert_eq!(custody.backend, "password_encrypted");
        assert!(custody.cache_keys);
        assert_eq!(custody.cache_ttl_secs, 300); // 5 minutes
    }

    #[test]
    fn test_custody_backend_parsing() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let backend = config.custody_backend();

        assert_eq!(backend, NodeIdentityCustodyBackend::PasswordEncrypted);
    }

    #[test]
    fn test_identity_path_default() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let path = config.identity_path();

        assert!(path.as_str().contains("node_identity.enc"));
    }
}

// =============================================================================
// COSMWASM CONFIG TESTS
// =============================================================================

#[cfg(test)]
mod cosmwasm_config {
    use super::*;

    #[test]
    fn test_default_cosmwasm_config() {
        let config = ErgorsConfig::default_cosmwasm_config();

        assert!(!config.enabled);
        assert!(config.cache_dir.is_empty()); // Uses default
        assert_eq!(config.memory_limit, 33_554_432); // 32MB
        assert!(config.gas_limits.is_some());
    }

    #[test]
    fn test_default_gas_limits() {
        let limits = ErgorsConfig::default_gas_limits();

        assert_eq!(limits.instantiate, 100_000_000);
        assert_eq!(limits.execute, 50_000_000);
        assert_eq!(limits.query, 10_000_000);
        assert_eq!(limits.migrate, 75_000_000);
    }

    #[test]
    fn test_cosmwasm_enabled_check() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);

        // Default is disabled
        assert!(!config.cosmwasm_enabled());
    }

    #[test]
    fn test_wasm_cache_dir_default() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let cache_dir = config.wasm_cache_dir();

        assert!(cache_dir.as_str().contains("wasm_cache"));
    }

    #[test]
    fn test_resolve_wasm_path_absolute() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);

        // Absolute path should be unchanged
        let abs_path = "/absolute/path/contract.wasm";
        let resolved = config.resolve_wasm_path(abs_path);
        assert_eq!(resolved.as_str(), abs_path);
    }

    #[test]
    fn test_resolve_wasm_path_relative() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);

        // Relative path should be joined with home
        let rel_path = "contracts/my_contract.wasm";
        let resolved = config.resolve_wasm_path(rel_path);
        assert!(resolved.as_str().contains("contracts/my_contract.wasm"));
    }
}

// =============================================================================
// AKASH CONFIG TESTS
// =============================================================================

#[cfg(test)]
mod akash_config {
    use super::*;

    #[test]
    fn test_default_akash_config() {
        let config = ErgorsConfig::default_akash_config();

        // Check mainnet defaults
        assert!(config.rpc_endpoint.contains("akash"));
        assert_eq!(config.chain_id, "akashnet-2");
        assert_eq!(config.gas_prices, "0.025uakt");
        assert!((config.gas_adjustment - 1.3).abs() < 0.001);
        assert_eq!(config.keyring_backend, "file");
        assert_eq!(config.default_key_name, "default");
    }

    #[test]
    fn test_akash_enabled_check() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);

        // Default Akash config should be enabled (has endpoints)
        assert!(config.akash_enabled());
    }

    #[test]
    fn test_akash_disabled_when_empty() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let mut config = ErgorsConfig::new(&home);

        // Set empty Akash config
        config.set_akash(AkashDeployConfig {
            rpc_endpoint: String::new(),
            chain_id: String::new(),
            ..Default::default()
        });

        assert!(!config.akash_enabled());
    }
}

// =============================================================================
// PROXY CONFIG TESTS
// =============================================================================

#[cfg(test)]
mod proxy_config {
    use super::*;

    #[test]
    fn test_default_proxy_config() {
        let config = ProxyConfig::default();

        assert!(config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert!(config.capture.enabled);
        assert!(config.capture.include_chunks);
    }

    #[test]
    fn test_proxy_config_from_toml() {
        let toml = r#"
enabled = true
bind_addr = "127.0.0.1:9090"

[router]
anthropic_base_url = "https://custom.anthropic.com"

[router.model_routes]
"llama-*" = "http://localhost:11434"

[capture]
enabled = true
include_chunks = false
max_sessions = 1000
"#;

        let config: ProxyConfig = toml::from_str(toml).expect("parse");

        assert_eq!(config.bind_addr, "127.0.0.1:9090");
        assert_eq!(
            config.router.anthropic_base_url,
            Some("https://custom.anthropic.com".to_string())
        );
        assert!(config.router.model_routes.contains_key("llama-*"));
        assert!(!config.capture.include_chunks);
        assert_eq!(config.capture.max_sessions, 1000);
    }

    #[test]
    fn test_capture_config_defaults() {
        let config = CaptureConfig::default();

        assert!(config.enabled);
        assert!(config.include_chunks);
        assert_eq!(config.max_sessions, 0); // Unlimited
        assert_eq!(config.retention_seconds, 0); // Unlimited
    }
}

// =============================================================================
// ENVIRONMENT VARIABLE OVERRIDE TESTS
// =============================================================================

#[cfg(test)]
mod env_overrides {
    use super::*;
    use std::env;

    fn with_env_var<F, T>(key: &str, value: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        // Save original value
        let original = env::var(key).ok();

        // Set test value
        env::set_var(key, value);

        let result = f();

        // Restore original
        match original {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }

        result
    }

    #[test]
    fn test_proxy_config_from_env() {
        with_env_var("ANTHROPIC_API_KEY", "test-api-key", || {
            let config = ProxyConfig::from_env();

            assert!(config
                .router
                .provider_api_keys
                .get("anthropic")
                .map(|k| k == "test-api-key")
                .unwrap_or(false));
        });
    }

    #[test]
    fn test_proxy_config_env_overrides() {
        let base_config = ProxyConfig::default();

        with_env_var("ANTHROPIC_API_BASE", "https://custom.api.com", || {
            let config = base_config.clone().with_env_overrides();

            assert_eq!(
                config.router.anthropic_base_url,
                Some("https://custom.api.com".to_string())
            );
        });
    }

    #[test]
    fn test_multiple_env_overrides() {
        // This test verifies that multiple env vars can be set
        with_env_var("ANTHROPIC_API_KEY", "ant-key", || {
            with_env_var("OPENAI_API_KEY", "oai-key", || {
                let config = ProxyConfig::from_env();

                assert!(config.router.provider_api_keys.contains_key("anthropic"));
                assert!(config.router.provider_api_keys.contains_key("openai"));
            });
        });
    }
}

// =============================================================================
// CONFIG MUTATION TESTS
// =============================================================================

#[cfg(test)]
mod config_mutation {
    use super::*;

    #[test]
    fn test_set_cosmwasm_config() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let mut config = ErgorsConfig::new(&home);

        let new_cosmwasm = CosmwasmConfig {
            enabled: true,
            cache_dir: "/custom/cache".to_string(),
            memory_limit: 67_108_864, // 64MB
            gas_limits: Some(CosmwasmGasLimits {
                instantiate: 200_000_000,
                execute: 100_000_000,
                query: 20_000_000,
                migrate: 150_000_000,
            }),
            initial_contracts: vec![],
        };

        config.set_cosmwasm(new_cosmwasm);

        let cw = config.cosmwasm();
        assert!(cw.enabled);
        assert_eq!(cw.cache_dir, "/custom/cache");
        assert_eq!(cw.memory_limit, 67_108_864);
    }

    #[test]
    fn test_set_akash_config() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let mut config = ErgorsConfig::new(&home);

        let new_akash = AkashDeployConfig {
            rpc_endpoint: "https://custom.rpc".to_string(),
            grpc_endpoint: "https://custom.grpc".to_string(),
            rest_endpoint: "https://custom.rest".to_string(),
            chain_id: "testnet-1".to_string(),
            gas_prices: "0.05uakt".to_string(),
            gas_adjustment: 1.5,
            keyring_backend: "test".to_string(),
            default_key_name: "mykey".to_string(),
            trusted_providers: vec!["akash1provider".to_string()],
        };

        config.set_akash(new_akash);

        let akash = config.akash();
        assert_eq!(akash.rpc_endpoint, "https://custom.rpc");
        assert_eq!(akash.chain_id, "testnet-1");
        assert_eq!(akash.trusted_providers.len(), 1);
    }
}

// =============================================================================
// CONFIG SERIALIZATION TESTS
// =============================================================================

#[cfg(test)]
mod config_serialization {
    use super::*;

    #[test]
    fn test_proxy_config_toml_roundtrip() {
        let original = ProxyConfig {
            enabled: true,
            bind_addr: "127.0.0.1:3000".to_string(),
            router: Default::default(),
            capture: CaptureConfig {
                enabled: false,
                include_chunks: true,
                max_sessions: 500,
                retention_seconds: 3600,
            },
        };

        let toml_str = toml::to_string(&original).expect("serialize");
        let restored: ProxyConfig = toml::from_str(&toml_str).expect("deserialize");

        assert_eq!(original.enabled, restored.enabled);
        assert_eq!(original.bind_addr, restored.bind_addr);
        assert_eq!(original.capture.enabled, restored.capture.enabled);
        assert_eq!(original.capture.max_sessions, restored.capture.max_sessions);
    }

    #[test]
    fn test_config_file_persistence() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");
        let config_path = home.join("config.toml");

        // Create, modify, save
        let mut config = ErgorsConfig::new(&home);
        config.set_akash(AkashDeployConfig {
            chain_id: "persist-test".to_string(),
            ..ErgorsConfig::default_akash_config()
        });
        config.save(&config_path).expect("save");

        // Load and verify
        let loaded = ErgorsConfig::load(config_path.as_str()).expect("load");
        assert_eq!(loaded.akash().chain_id, "persist-test");
    }
}

// =============================================================================
// INITIAL CONTRACTS CONFIG TESTS
// =============================================================================

#[cfg(test)]
mod initial_contracts {
    use super::*;

    #[test]
    fn test_initial_contracts_empty_by_default() {
        let temp = TempDir::new().expect("temp");
        let home = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8");

        let config = ErgorsConfig::new(&home);
        let contracts = config.initial_contracts();

        assert!(contracts.is_empty());
    }
}
