use crate::define_wrapper;
use crate::traits::Wrap;

use camino::{Utf8Path, Utf8PathBuf};
use ho_std::custody::PasswordEncryptedCustody;
use ho_std::llm::{HoError, HoResult};
use ho_std::traits::{NodeIdentityCustody, NodeIdentityCustodyBackend};
pub use ho_std::types::ergors::orch::v1::{
    AkashDeployConfig, ContractConfig, ContractDeployment, ContractMigration, CosmwasmConfig,
    CosmwasmGasLimits,
};
use ho_std::types::ergors::{network::v1::*, orch::v1::*, storage::v1::*};

use ho_std::traits::file_ops::ConfigLoaderTrait;
use ho_std::traits::{HoConfigTrait, LLMRouterConfigTrait, NetworkConfigTrait, NodeIdentityTrait};
use ho_std::utils::DefaultFileOps;
// Define all wrapper types using the macro
define_wrapper!(ErgorsConfig, HoConfig);
define_wrapper!(CwHoLlmRouterConfig, LlmRouterConfig);

// Network trait implementations for proto types
impl HoConfigTrait for ErgorsConfig {
    type Identity = NodeIdentity;
    type StorageConfig = StorageConfig;
    type LLMConfig = CwHoLlmRouterConfig;
    type HoConfigResult = HoResult<()>;

    fn new(home: &Utf8Path) -> Self {
        Self(HoConfig {
            network: Some(NetworkConfig::new()),
            identity: Some(NodeIdentity::new()),
            storage: Some(StorageConfig::new(home)),
            llm: Some(LlmRouterConfig::new(home)),
            home: home.as_str().into(),
            custody: None,  // Custody config is managed separately
            cosmwasm: None, // CosmWasm config is optional
            akash: Some(Self::default_akash_config()), // Akash mainnet defaults
        })
    }

    fn network(&self) -> &NetworkConfig {
        self.network.as_ref().expect("network config should exist")
    }

    fn identity(&self) -> &Self::Identity {
        self.identity
            .as_ref()
            .expect("ego is useful in moderation (cannot access node identity")
    }

    fn storage(&self) -> &Self::StorageConfig {
        self.storage
            .as_ref()
            .expect("memories seed ego (cannot find storage config)")
    }

    fn llm(&self) -> &Self::LLMConfig {
        CwHoLlmRouterConfig::wrap_ref(
            self.llm
                .as_ref()
                .expect("ego is useful in moderation (cannot access llmConfig)"),
        )
    }

    fn validate(&self) -> Self::HoConfigResult {
        self.network().validate()?;
        self.llm().validate()?;
        // self.storage.validate
        // self.identity.validate
        Ok(())
    }

    fn set_network_config(&mut self, config: NetworkConfig) {
        self.0.network = Some(config)
    }

    fn set_identity(&mut self, identity: Self::Identity) {
        self.0.identity = Some(identity);
    }

    fn set_storage_config(&mut self, config: Self::StorageConfig) {
        self.0.storage = Some(config)
    }

    fn set_llm_config(&mut self, config: Self::LLMConfig) {
        self.0.llm = Some(config.unwrap());
    }

    fn file_path(&self) -> &str {
        todo!()
    }

    fn from_file(path: &str) -> HoResult<Self>
    where
        Self: Sized,
    {
        DefaultFileOps::from_toml_file(path)
    }

    fn load<P: AsRef<std::path::Path> + std::fmt::Display>(path: P) -> HoResult<Self>
    where
        Self: Sized,
    {
        let contents = std::fs::read_to_string(&path).map_err(|e| {
            HoError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "ho config file not found: {}. hint: run 'init' to create new config",
                    path
                ),
            ))
        })?;
        let mut config: Self = toml::from_str(&contents)?;
        // Apply defaults for missing optional configs (handles migration of old configs)
        config.apply_defaults();
        Ok(config)
    }

    fn save<P: AsRef<std::path::Path>>(&self, path: P) -> HoResult<()> {
        let contents = toml::to_string_pretty(&self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

// Custody configuration helpers
impl ErgorsConfig {
    /// Get custody configuration (returns default as custody is not stored in HoConfig)
    pub fn custody(&self) -> NodeIdentityCustodyConfig {
        // Note: custody config is not part of HoConfig proto, use default
        Self::default_custody_config()
    }

    /// Set custody configuration (no-op as custody is not stored in HoConfig)
    pub fn set_custody(&mut self, _config: NodeIdentityCustodyConfig) {
        // Note: custody config is not part of HoConfig proto
        // Custody configuration should be managed separately if needed
        tracing::warn!("set_custody called but custody is not stored in HoConfig");
    }

    /// Get the custody backend type from config
    pub fn custody_backend(&self) -> NodeIdentityCustodyBackend {
        let custody = self.custody();
        match custody.backend.as_str() {
            "plaintext" => NodeIdentityCustodyBackend::Plaintext,
            "password_encrypted" | "" => NodeIdentityCustodyBackend::PasswordEncrypted,
            "node_key_encrypted" => NodeIdentityCustodyBackend::NodeKeyEncrypted,
            "threshold" => NodeIdentityCustodyBackend::Threshold,
            endpoint if endpoint.starts_with("remote:") => {
                NodeIdentityCustodyBackend::RemoteCustody(endpoint[7..].to_string())
            }
            _ => NodeIdentityCustodyBackend::PasswordEncrypted, // Default
        }
    }

    /// Get the encrypted identity file path from config
    pub fn identity_path(&self) -> Utf8PathBuf {
        let custody = self.custody();
        if custody.identity_path.is_empty() {
            Utf8PathBuf::from(&self.0.home).join("node_identity.enc")
        } else {
            Utf8PathBuf::from(&custody.identity_path)
        }
    }

    /// Create a password-encrypted custody backend from this config
    ///
    /// Returns a custody backend configured according to the config settings.
    /// The backend must be unlocked with a password before use.
    pub fn create_password_custody(&self) -> PasswordEncryptedCustody {
        let custody_config = self.custody();
        let identity_path = self.identity_path();

        PasswordEncryptedCustody::with_cache_ttl(identity_path, custody_config.cache_ttl_secs)
    }

    /// Create default custody config for new installations
    pub fn default_custody_config() -> NodeIdentityCustodyConfig {
        NodeIdentityCustodyConfig {
            backend: "password_encrypted".to_string(),
            cache_keys: true,
            cache_ttl_secs: 300,          // 5 minutes
            identity_path: String::new(), // Will use default
            remote_endpoint: String::new(),
        }
    }

    /// Get CosmWasm configuration (returns default if not configured)
    pub fn cosmwasm(&self) -> CosmwasmConfig {
        self.0
            .cosmwasm
            .clone()
            .unwrap_or_else(Self::default_cosmwasm_config)
    }

    /// Set CosmWasm configuration
    pub fn set_cosmwasm(&mut self, config: CosmwasmConfig) {
        self.0.cosmwasm = Some(config);
    }

    /// Get the WASM cache directory
    pub fn wasm_cache_dir(&self) -> Utf8PathBuf {
        let cosmwasm = self.cosmwasm();
        if cosmwasm.cache_dir.is_empty() {
            Utf8PathBuf::from(&self.0.home)
                .join("data")
                .join("wasm_cache")
        } else {
            Utf8PathBuf::from(&cosmwasm.cache_dir)
        }
    }

    /// Check if CosmWasm is enabled
    pub fn cosmwasm_enabled(&self) -> bool {
        self.0.cosmwasm.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    /// Get initial contracts to deploy
    pub fn initial_contracts(&self) -> Vec<ContractDeployment> {
        self.0
            .cosmwasm
            .as_ref()
            .map(|c| c.initial_contracts.clone())
            .unwrap_or_default()
    }

    /// Create default CosmWasm config
    pub fn default_cosmwasm_config() -> CosmwasmConfig {
        CosmwasmConfig {
            enabled: false,
            cache_dir: String::new(), // Will use default
            memory_limit: 33_554_432, // 32MB
            gas_limits: Some(Self::default_gas_limits()),
            initial_contracts: vec![],
        }
    }

    /// Create default gas limits
    pub fn default_gas_limits() -> CosmwasmGasLimits {
        CosmwasmGasLimits {
            instantiate: 100_000_000,
            execute: 50_000_000,
            query: 10_000_000,
            migrate: 75_000_000,
        }
    }

    /// Resolve WASM path (handles relative paths)
    pub fn resolve_wasm_path(&self, wasm_path: &str) -> Utf8PathBuf {
        let path = Utf8PathBuf::from(wasm_path);
        if path.is_absolute() {
            path
        } else {
            Utf8PathBuf::from(&self.0.home).join(wasm_path)
        }
    }

    /// Get Akash deploy configuration (returns default if not configured)
    pub fn akash(&self) -> AkashDeployConfig {
        self.0
            .akash
            .clone()
            .unwrap_or_else(Self::default_akash_config)
    }

    /// Set Akash deploy configuration
    pub fn set_akash(&mut self, config: AkashDeployConfig) {
        self.0.akash = Some(config);
    }

    /// Check if Akash deployment is configured
    pub fn akash_enabled(&self) -> bool {
        self.0
            .akash
            .as_ref()
            .map(|c| !c.rpc_endpoints.is_empty() && !c.chain_id.is_empty())
            .unwrap_or(false)
    }

    /// Create default Akash deploy config for mainnet
    pub fn default_akash_config() -> AkashDeployConfig {
        AkashDeployConfig {
            // New multi-endpoint support with failover
            rpc_endpoints: vec![
                "https://rpc-akash.ecostake.com:443".to_string(),
                "https://akash-rpc.polkachu.com:443".to_string(),
            ],
            grpc_endpoints: vec![
                "https://akash.grpc.kleomedes.network:443".to_string(),
                "https://akash-grpc.publicnode.com:443".to_string(),
                "https://akash-grpc.polkachu.com:443".to_string(),
            ],
            rest_endpoints: vec![
                "https://rest-akash.ecostake.com".to_string(),
                "https://akash-api.polkachu.com".to_string(),
            ],

            // Retry configuration
            max_retries_per_endpoint: 2,
            max_total_retries: 6,
            connection_timeout_seconds: 10,

            chain_id: "akashnet-2".to_string(),
            gas_prices: "0.025uakt".to_string(),
            gas_adjustment: 1.3,
            keyring_backend: "file".to_string(),
            default_key_name: "default".to_string(),
            trusted_providers: vec![],
        }
    }

    /// Apply defaults for missing optional configs
    ///
    /// This handles migration of old config files that don't have
    /// newer optional sections like [akash].
    pub fn apply_defaults(&mut self) {
        // Ensure Akash config exists with mainnet defaults
        if self.0.akash.is_none() {
            self.0.akash = Some(Self::default_akash_config());
        }
    }
}

impl LLMRouterConfigTrait for CwHoLlmRouterConfig {
    fn default_provider(&self) -> &str {
        todo!()
    }

    fn timeout_seconds(&self) -> u32 {
        todo!()
    }

    fn retry_attempts(&self) -> u32 {
        todo!()
    }

    fn remove_provider(&mut self, _name: &str) {
        todo!()
    }

    fn set_default_provider(&mut self, _name: String) {
        todo!()
    }

    fn set_timeout(&mut self, _timeout: u32) {
        todo!()
    }

    fn set_retry_attempts(&mut self, _attempts: u32) {
        todo!()
    }
    fn validate(&self) -> HoResult<()> {
        // validate each llm provider has keys defined in .env file
        for _llm in &self.0.entities {}
        Ok(())
    }
}

// ==============================================
// Proxy Configuration
// ==============================================

use ho_std::types::ergors::orch::v1::{
    InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig,
};
use serde::{Deserialize, Serialize};

/// Configuration for the LLM proxy service
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyConfig {
    /// Enable proxy endpoints
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Bind address for proxy (default: "0.0.0.0:8080")
    #[serde(default = "default_proxy_addr")]
    pub bind_addr: String,

    /// Router configuration for upstream providers
    #[serde(default)]
    pub router: ProxyRouterConfig,

    /// Capture settings
    #[serde(default)]
    pub capture: CaptureConfig,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_addr: default_proxy_addr(),
            router: ProxyRouterConfig::default(),
            capture: CaptureConfig::default(),
        }
    }
}

/// Configuration for request/response capture
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaptureConfig {
    /// Enable capture of requests and responses
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Include streaming chunks in capture
    #[serde(default = "default_true")]
    pub include_chunks: bool,

    /// Maximum number of sessions to retain (0 = unlimited)
    #[serde(default)]
    pub max_sessions: usize,

    /// Retention period in seconds (0 = unlimited)
    #[serde(default)]
    pub retention_seconds: u64,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            include_chunks: true,
            max_sessions: 0,
            retention_seconds: 0,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_proxy_addr() -> String {
    "0.0.0.0:8080".to_string()
}

impl ProxyConfig {
    /// Create a new ProxyConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load proxy configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();
        //TODO: have default method for checking custody for antrhopic api_key_ref
        // Create Anthropic provider from env if specified
        if let Ok(url) = std::env::var("ANTHROPIC_API_BASE") {
            let provider = InferenceProviderConfig {
                provider_id: "anthropic".to_string(),
                base_url: url,
                api_key_ref: String::new(),
                provider_type: InferenceProviderType::Anthropic as i32,
                enabled: true,
                display_name: "Anthropic".to_string(),
                description: "Anthropic Claude API".to_string(),
                metadata: std::collections::HashMap::new(),
                max_concurrent_requests: 0,
                timeout_seconds: 0,
                created_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                updated_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
            };
            config
                .router
                .providers
                .insert("anthropic".to_string(), provider);
        } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            // Just key, use default URL — custody resolves by provider_id
            let provider = InferenceProviderConfig {
                provider_id: "anthropic".to_string(),
                base_url: "https://api.anthropic.com".to_string(),
                api_key_ref: "custody://anthropic".to_string(),
                provider_type: InferenceProviderType::Anthropic as i32,
                enabled: true,
                display_name: "Anthropic".to_string(),
                description: "Anthropic Claude API".to_string(),
                metadata: std::collections::HashMap::new(),
                max_concurrent_requests: 0,
                timeout_seconds: 0,
                created_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                updated_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
            };
            config
                .router
                .providers
                .insert("anthropic".to_string(), provider);
        }

        // Create OpenAI provider from env if specified
        if let Ok(url) = std::env::var("OPENAI_API_BASE") {
            let provider = InferenceProviderConfig {
                provider_id: "openai".to_string(),
                base_url: url,
                api_key_ref: String::new(),
                provider_type: InferenceProviderType::Openai as i32,
                enabled: true,
                display_name: "OpenAI".to_string(),
                description: "OpenAI API".to_string(),
                metadata: std::collections::HashMap::new(),
                max_concurrent_requests: 0,
                timeout_seconds: 0,
                created_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                updated_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
            };
            config
                .router
                .providers
                .insert("openai".to_string(), provider);
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            // Just key, use default URL — custody resolves by provider_id
            let provider = InferenceProviderConfig {
                provider_id: "openai".to_string(),
                base_url: ho_std::constants::OPENAI_BASE_URL.to_string(),
                api_key_ref: "custody://openai".to_string(),
                provider_type: InferenceProviderType::Openai as i32,
                enabled: true,
                display_name: "OpenAI".to_string(),
                description: "OpenAI API".to_string(),
                metadata: std::collections::HashMap::new(),
                max_concurrent_requests: 0,
                timeout_seconds: 0,
                created_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                updated_at: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                }),
            };
            config
                .router
                .providers
                .insert("openai".to_string(), provider);
        }

        config
    }

    /// Merge environment variables into existing config
    pub fn with_env_overrides(mut self) -> Self {
        // Environment variables take precedence - update or create provider configs

        // Anthropic provider
        if let Ok(url) = std::env::var("ANTHROPIC_API_BASE") {
            let provider = self
                .router
                .providers
                .entry("anthropic".to_string())
                .or_insert_with(|| InferenceProviderConfig {
                    provider_id: "anthropic".to_string(),
                    base_url: String::new(),
                    api_key_ref: String::new(),
                    provider_type: InferenceProviderType::Anthropic as i32,
                    enabled: true,
                    display_name: "Anthropic".to_string(),
                    description: "Anthropic Claude API".to_string(),
                    metadata: std::collections::HashMap::new(),
                    max_concurrent_requests: 0,
                    timeout_seconds: 0,
                    created_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    updated_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                });
            provider.base_url = url;
        } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            self.router
                .providers
                .entry("anthropic".to_string())
                .or_insert_with(|| InferenceProviderConfig {
                    provider_id: "anthropic".to_string(),
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key_ref: "custody://anthropic".to_string(),
                    provider_type: InferenceProviderType::Anthropic as i32,
                    enabled: true,
                    display_name: "Anthropic".to_string(),
                    description: "Anthropic Claude API".to_string(),
                    metadata: std::collections::HashMap::new(),
                    max_concurrent_requests: 0,
                    timeout_seconds: 0,
                    created_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    updated_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                });
        }

        // OpenAI provider
        if let Ok(url) = std::env::var("OPENAI_API_BASE") {
            let provider = self
                .router
                .providers
                .entry("openai".to_string())
                .or_insert_with(|| InferenceProviderConfig {
                    provider_id: "openai".to_string(),
                    base_url: String::new(),
                    api_key_ref: String::new(),
                    provider_type: InferenceProviderType::Openai as i32,
                    enabled: true,
                    display_name: "OpenAI".to_string(),
                    description: "OpenAI API".to_string(),
                    metadata: std::collections::HashMap::new(),
                    max_concurrent_requests: 0,
                    timeout_seconds: 0,
                    created_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    updated_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                });
            provider.base_url = url;
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            self.router
                .providers
                .entry("openai".to_string())
                .or_insert_with(|| InferenceProviderConfig {
                    provider_id: "openai".to_string(),
                    base_url: ho_std::constants::OPENAI_BASE_URL.to_string(),
                    api_key_ref: "custody://openai".to_string(),
                    provider_type: InferenceProviderType::Openai as i32,
                    enabled: true,
                    display_name: "OpenAI".to_string(),
                    description: "OpenAI API".to_string(),
                    metadata: std::collections::HashMap::new(),
                    max_concurrent_requests: 0,
                    timeout_seconds: 0,
                    created_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    updated_at: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                });
        }

        self
    }
}

#[cfg(test)]
mod proxy_config_tests {
    use super::*;

    #[test]
    fn test_default_proxy_config() {
        let config = ProxyConfig::default();
        assert!(config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert!(config.capture.enabled);
    }

    #[test]
    fn test_proxy_config_from_toml() {
        // Load template from fixture file
        let toml = include_str!("../tests/fixtures/proxy_config_template.toml");

        let config: ProxyConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:9090");
        assert!(config.router.model_routes.contains_key("llama-*"));
        assert!(!config.capture.include_chunks);
        assert_eq!(config.capture.max_sessions, 1000);
        assert_eq!(config.router.version, 1);
    }
}
