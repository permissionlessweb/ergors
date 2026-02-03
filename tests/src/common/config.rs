//! Test configuration utilities
//!
//! Provides helpers for generating and managing test configurations.

use serde::{Deserialize, Serialize};

/// Default test configuration values
pub struct TestDefaults;

impl TestDefaults {
    /// Default recursion depth for fractal tests
    pub const RECURSION_DEPTH: u32 = 3;

    /// Default golden ratio tolerance
    pub const GOLDEN_RATIO_TOLERANCE: f64 = 0.001;

    /// Default fractal coherence threshold
    pub const FRACTAL_COHERENCE_THRESHOLD: f64 = 0.9;

    /// Default network timeout in seconds
    pub const NETWORK_TIMEOUT_SECS: u64 = 30;

    /// Default storage snapshot interval
    pub const SNAPSHOT_INTERVAL_SECS: u64 = 60;

    /// Default LLM request timeout
    pub const LLM_TIMEOUT_SECS: u64 = 10;

    /// Default test password for custody
    pub const TEST_PASSWORD: &'static str = "test-password-12345";
}

/// Test network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestNetworkConfig {
    /// Number of nodes in test network
    pub node_count: usize,
    /// Base port for P2P communication
    pub base_port: u16,
    /// Enable network partitioning tests
    pub enable_partitioning: bool,
    /// Latency range in milliseconds
    pub latency_range_ms: (u64, u64),
}

impl Default for TestNetworkConfig {
    fn default() -> Self {
        Self {
            node_count: 4, // Tetrahedral
            base_port: 50100,
            enable_partitioning: false,
            latency_range_ms: (10, 100),
        }
    }
}

/// Test storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStorageConfig {
    /// Enable snapshots
    pub enable_snapshots: bool,
    /// Snapshot interval in seconds
    pub snapshot_interval_secs: u64,
    /// Enable compaction
    pub enable_compaction: bool,
}

impl Default for TestStorageConfig {
    fn default() -> Self {
        Self {
            enable_snapshots: true,
            snapshot_interval_secs: TestDefaults::SNAPSHOT_INTERVAL_SECS,
            enable_compaction: false,
        }
    }
}

/// Test LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestLlmConfig {
    /// Use mock providers only
    pub mock_only: bool,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Enable cost tracking
    pub track_costs: bool,
}

impl Default for TestLlmConfig {
    fn default() -> Self {
        Self {
            mock_only: true,
            timeout_secs: TestDefaults::LLM_TIMEOUT_SECS,
            track_costs: true,
        }
    }
}

/// Master test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct TestConfig {
    /// Network configuration
    pub network: TestNetworkConfig,
    /// Storage configuration
    pub storage: TestStorageConfig,
    /// LLM configuration
    pub llm: TestLlmConfig,
}


impl TestConfig {
    /// Load configuration from TOML file
    pub fn from_toml(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: TestConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to TOML file
    pub fn to_toml(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TestConfig::default();
        assert_eq!(config.network.node_count, 4);
        assert_eq!(config.network.base_port, 50100);
        assert!(config.llm.mock_only);
        assert!(config.storage.enable_snapshots);
    }

    #[test]
    fn test_config_serialization() {
        let config = TestConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.network.node_count, deserialized.network.node_count);
    }
}

// address derivation 
// keys subcommands: // packages/ergors/src/keys/mod.rs:23
