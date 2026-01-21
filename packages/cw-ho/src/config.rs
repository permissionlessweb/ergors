use crate::define_wrapper;
use crate::traits::Wrap;

use camino::{Utf8Path, Utf8PathBuf};
use ho_std::custody::{PasswordEncryptedCustody, PlaintextCustody};
use ho_std::llm::{HoError, HoResult};
use ho_std::traits::{NodeIdentityCustody, NodeIdentityCustodyBackend};
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
            custody: None, // Custody config is managed separately
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
        Ok(DefaultFileOps::from_toml_file(path)?)
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
                    path.to_string()
                ),
            ))
        })?;
        Ok(toml::from_str(&contents)?)
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

    /// Create a plaintext custody backend from the identity in config
    ///
    /// WARNING: This uses plaintext key storage and should only be used for
    /// development/testing. For production, use `create_password_custody()`.
    pub fn create_plaintext_custody(&self) -> HoResult<PlaintextCustody> {
        PlaintextCustody::from_node_identity(self.identity())
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

    fn remove_provider(&mut self, name: &str) {
        todo!()
    }

    fn set_default_provider(&mut self, name: String) {
        todo!()
    }

    fn set_timeout(&mut self, timeout: u32) {
        todo!()
    }

    fn set_retry_attempts(&mut self, attempts: u32) {
        todo!()
    }
    fn validate(&self) -> HoResult<()> {
        // validate each llm provider has keys defined in .env file
        for llm in &self.0.entities {}
        Ok(())
    }
}

// ==============================================
// Proxy Configuration
// ==============================================

use crate::proxy::router::ProxyRouterConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the LLM proxy service
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
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

        // Override Anthropic base URL from env
        if let Ok(url) = std::env::var("ANTHROPIC_API_BASE") {
            config.router.anthropic_base_url = Some(url);
        }

        // Override OpenAI base URL from env
        if let Ok(url) = std::env::var("OPENAI_API_BASE") {
            config.router.openai_base_url = Some(url);
        }

        // Load API keys from env
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            config
                .router
                .provider_api_keys
                .insert("anthropic".to_string(), key);
        }

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config
                .router
                .provider_api_keys
                .insert("openai".to_string(), key);
        }

        config
    }

    /// Merge environment variables into existing config
    pub fn with_env_overrides(mut self) -> Self {
        // Environment variables take precedence
        if let Ok(url) = std::env::var("ANTHROPIC_API_BASE") {
            self.router.anthropic_base_url = Some(url);
        }

        if let Ok(url) = std::env::var("OPENAI_API_BASE") {
            self.router.openai_base_url = Some(url);
        }

        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.router
                .provider_api_keys
                .insert("anthropic".to_string(), key);
        }

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.router
                .provider_api_keys
                .insert("openai".to_string(), key);
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

        let config: ProxyConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:9090");
        assert_eq!(
            config.router.anthropic_base_url,
            Some("https://custom.anthropic.com".to_string())
        );
        assert!(config.router.model_routes.contains_key("llama-*"));
        assert!(!config.capture.include_chunks);
        assert_eq!(config.capture.max_sessions, 1000);
    }
}
