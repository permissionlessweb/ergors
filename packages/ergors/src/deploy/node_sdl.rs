//! SDL Template Generator for Ergors Node Bootstrap
//!
//! Generates Akash SDL YAML for deploying coordinator and executor nodes
//! with correct image tags, resource specifications, and network configuration.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::network::v1::NodeType;
use serde::{Deserialize, Serialize};

/// Configuration for generating node SDL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBootstrapConfig {
    /// Node type (Coordinator, Executor, etc.)
    pub node_type: NodeType,
    /// Docker image tag (from build-image.sh output)
    pub image_tag: String,
    /// P2P listen port (default: 26969)
    pub p2p_port: u16,
    /// HTTP API port (default: 8080)
    pub api_port: u16,
    /// Bootstrap peer addresses for initial network connection
    pub bootstrap_peers: Vec<String>,
    /// Custom environment variables
    pub env_vars: Vec<(String, String)>,
}

impl Default for NodeBootstrapConfig {
    fn default() -> Self {
        Self {
            node_type: NodeType::Executor,
            image_tag: "ghcr.io/permissionlessweb/ergors:latest".to_string(),
            p2p_port: 26969,
            api_port: 8080,
            bootstrap_peers: Vec::new(),
            env_vars: Vec::new(),
        }
    }
}

/// SDL generator for ergors nodes
pub struct NodeSdlGenerator {
    base_config: NodeBootstrapConfig,
}

impl NodeSdlGenerator {
    /// Create a new SDL generator with the given config
    pub fn new(config: NodeBootstrapConfig) -> Self {
        Self {
            base_config: config,
        }
    }

    /// Generate SDL for an executor node (minimal resources)
    ///
    /// Executor nodes handle task execution and don't need heavy resources.
    pub fn generate_executor_sdl(&self) -> Result<String> {
        self.generate_sdl(
            &self.base_config,
            "2",      // CPU cores
            "4Gi",    // RAM
            "20Gi",   // Storage
        )
    }

    /// Generate SDL for a coordinator node (moderate resources)
    ///
    /// Coordinators manage the network and need more resources for state management.
    pub fn generate_coordinator_sdl(&self) -> Result<String> {
        self.generate_sdl(
            &self.base_config,
            "4",      // CPU cores
            "8Gi",    // RAM
            "50Gi",   // Storage
        )
    }

    /// Generate SDL with custom resource specifications
    pub fn generate_custom_sdl(
        &self,
        config: &NodeBootstrapConfig,
        cpu: &str,
        memory: &str,
        storage: &str,
    ) -> Result<String> {
        self.generate_sdl(config, cpu, memory, storage)
    }

    /// Internal SDL generation with resource parameters
    fn generate_sdl(
        &self,
        config: &NodeBootstrapConfig,
        cpu: &str,
        memory: &str,
        storage: &str,
    ) -> Result<String> {
        // Validate config
        if config.image_tag.is_empty() {
            return Err(anyhow!("image_tag cannot be empty"));
        }

        // Build environment variables
        let mut env_section = String::new();
        env_section.push_str(&format!("      - NODE_TYPE={}\n", config.node_type.as_str_name()));
        env_section.push_str(&format!("      - P2P_PORT={}\n", config.p2p_port));
        env_section.push_str(&format!("      - API_PORT={}\n", config.api_port));

        // Add bootstrap peers if any
        if !config.bootstrap_peers.is_empty() {
            let peers = config.bootstrap_peers.join(",");
            env_section.push_str(&format!("      - BOOTSTRAP_PEERS={}\n", peers));
        }

        // Add custom env vars
        for (key, value) in &config.env_vars {
            env_section.push_str(&format!("      - {}={}\n", key, value));
        }

        // Generate SDL YAML
        // NOTE: Akash SDL v2.0 format, minimal and correct
        let sdl = format!(
            r#"---
version: "2.0"

services:
  ergors:
    image: {}
    expose:
      - port: {}
        as: 80
        to:
          - global: true
      - port: {}
        as: 26969
        proto: tcp
        to:
          - global: true
    env:
{}
profiles:
  compute:
    ergors:
      resources:
        cpu:
          units: {}
        memory:
          size: {}
        storage:
          - size: {}
  placement:
    akash:
      pricing:
        ergors:
          denom: uakt
          amount: 10000

deployment:
  ergors:
    akash:
      profile: ergors
      count: 1
"#,
            config.image_tag,
            config.api_port,
            config.p2p_port,
            env_section.trim_end(),
            cpu,
            memory,
            storage
        );

        Ok(sdl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_executor_sdl() {
        let config = NodeBootstrapConfig {
            node_type: NodeType::Executor,
            image_tag: "ghcr.io/test/ergors:v1.0.0".to_string(),
            bootstrap_peers: vec!["ergo123@1.2.3.4:26969".to_string()],
            ..Default::default()
        };

        let generator = NodeSdlGenerator::new(config);
        let sdl = generator.generate_executor_sdl().unwrap();

        // Verify SDL contains critical elements
        assert!(sdl.contains("version: \"2.0\""));
        assert!(sdl.contains("ghcr.io/test/ergors:v1.0.0"));
        assert!(sdl.contains("NODE_TYPE=NODE_TYPE_EXECUTOR"));
        assert!(sdl.contains("BOOTSTRAP_PEERS=ergo123@1.2.3.4:26969"));
        assert!(sdl.contains("units: 2"));  // Executor CPU
        assert!(sdl.contains("size: 4Gi")); // Executor RAM
    }

    #[test]
    fn test_generate_coordinator_sdl() {
        let config = NodeBootstrapConfig {
            node_type: NodeType::Coordinator,
            image_tag: "ghcr.io/test/ergors:latest".to_string(),
            ..Default::default()
        };

        let generator = NodeSdlGenerator::new(config);
        let sdl = generator.generate_coordinator_sdl().unwrap();

        assert!(sdl.contains("NODE_TYPE=NODE_TYPE_COORDINATOR"));
        assert!(sdl.contains("units: 4"));  // Coordinator CPU
        assert!(sdl.contains("size: 8Gi")); // Coordinator RAM
    }

    #[test]
    fn test_empty_image_tag_fails() {
        let config = NodeBootstrapConfig {
            image_tag: String::new(),
            ..Default::default()
        };

        let generator = NodeSdlGenerator::new(config);
        let result = generator.generate_executor_sdl();
        assert!(result.is_err());
    }
}
