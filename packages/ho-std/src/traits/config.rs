//! Configuration-related traits for CW-HO system

use anyhow::Context;
use async_trait::async_trait;
use std::path::Path;

use crate::commonware::error::CommonwareNetworkResult;
use crate::error::HoResult;

/// Core trait for application configuration
pub trait HoConfigTrait {
    type NetworkConfig;
    type Identity;
    type StorageConfig;
    type LLMConfig;
    type HoConfigResult;

    fn load<P: AsRef<Path> + std::fmt::Display>(path: P) -> HoResult<Self>
    where
        Self: Sized;
    fn save<P: AsRef<Path>>(&self, path: P) -> HoResult<()>;

    /// Get network configuration
    fn default() -> Self;
    /// Get network configuration
    fn from_file(path: &str) -> HoResult<Self>
    where
        Self: Sized;

    /// Get network configuration
    fn file_path(&self) -> &str;
    /// Get network configuration
    fn network(&self) -> &Self::NetworkConfig;

    /// Get node identity
    fn identity(&self) -> &Self::Identity;

    /// Get storage configuration
    fn storage(&self) -> &Self::StorageConfig;

    /// Get LLM configuration
    fn llm(&self) -> &Self::LLMConfig;

    /// Validate configuration
    fn validate(&self) -> Self::HoConfigResult;

    /// Set network config
    fn set_network_config(&mut self, config: Self::NetworkConfig);

    /// Set identity
    fn set_identity(&mut self, identity: Self::Identity);

    /// Set storage config
    fn set_storage_config(&mut self, config: Self::StorageConfig);

    /// Set LLM config
    fn set_llm_config(&mut self, config: Self::LLMConfig);
}

/// Core trait for storage configuration
// #[async_trait]
pub trait StorageConfigTrait {
    // async fn new<P>(data_dir: P) -> HoResult<Self>
    // where
    //     Self: Sized;

    /// Get data directory
    fn data_dir(&self) -> &str;

    /// Get maximum size in MB
    fn max_size_mb(&self) -> u32;

    /// Check if compression is enabled
    fn is_compression_enabled(&self) -> bool;

    /// Set data directory
    fn set_data_dir(&mut self, dir: String);

    /// Set maximum size
    fn set_max_size_mb(&mut self, size: u32);

    /// Enable/disable compression
    fn set_compression(&mut self, enabled: bool);

    /// Validate storage configuration
    fn validate(&self) -> HoResult<()> {
        if self.data_dir().is_empty() {
            return Err(crate::error::HoError::Config(
                "Data directory cannot be empty".to_string(),
            ));
        }
        if self.max_size_mb() == 0 {
            return Err(crate::error::HoError::Config(
                "Max size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Core trait for LLM router configuration
pub trait LLMRouterConfigTrait {
    /// Get default provider name
    fn default_provider(&self) -> &str;

    /// Get timeout in seconds
    fn timeout_seconds(&self) -> u32;

    /// Get retry attempts
    fn retry_attempts(&self) -> u32;

    /// Remove provider
    fn remove_provider(&mut self, name: &str);

    /// Set default provider
    fn set_default_provider(&mut self, name: String);

    /// Set timeout
    fn set_timeout(&mut self, timeout: u32);

    /// Set retry attempts
    fn set_retry_attempts(&mut self, attempts: u32);

    /// Validate LLM configuration
    fn validate(&self) -> HoResult<()> {
        if self.default_provider().is_empty() {
            return Err(crate::error::HoError::Config(
                "Default provider must be set".to_string(),
            ));
        }
        if self.timeout_seconds() == 0 {
            return Err(crate::error::HoError::Config(
                "Timeout must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Core trait for LLM configuration
pub trait LLMConfigTrait {
    /// Get temperature
    fn temperature(&self) -> f64;

    /// Get max tokens
    fn max_tokens(&self) -> u32;

    /// Get top-p value
    fn top_p(&self) -> f64;

    /// Get stop sequences
    fn stop_sequences(&self) -> &[String];

    /// Set temperature
    fn set_temperature(&mut self, temp: f64);

    /// Set max tokens
    fn set_max_tokens(&mut self, tokens: u32);

    /// Set top-p
    fn set_top_p(&mut self, top_p: f64);

    /// Add stop sequence
    fn add_stop_sequence(&mut self, sequence: String);

    /// Clear stop sequences
    fn clear_stop_sequences(&mut self);

    /// Validate LLM configuration
    fn validate(&self) -> HoResult<()> {
        if !(0.0..=2.0).contains(&self.temperature()) {
            return Err(crate::error::HoError::Config(
                "Temperature must be between 0.0 and 2.0".to_string(),
            ));
        }
        if self.max_tokens() == 0 {
            return Err(crate::error::HoError::Config(
                "Max tokens must be greater than 0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.top_p()) {
            return Err(crate::error::HoError::Config(
                "Top-p must be between 0.0 and 1.0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Core trait for network configuration
pub trait NetworkConfigTrait {
    /// Get bootstrap peers
    fn from_toml(&self) -> toml::Table;

    /// validate a network configuration is valid
    fn validate(&self) -> CommonwareNetworkResult<()>;

    /// Get bootstrap peers
    fn bootstrap_peers(&self) -> &[String];

    /// Get listen port
    fn listen_port(&self) -> u32;

    /// Get listen address
    fn listen_address(&self) -> &str;

    /// Get maximum peers
    fn max_peers(&self) -> u32;

    /// Get connection timeout
    fn connection_timeout_ms(&self) -> u32;

    /// Check if discovery is enabled
    fn is_discovery_enabled(&self) -> bool;
}
