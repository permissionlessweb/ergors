//! Bootstrap Configuration Generator
//!
//! Generates complete configurations for newly bootstrapped nodes including:
//! - Ed25519 node identity
//! - Network configuration (P2P, bootstrap peers)
//! - Encrypted custody files
//! - TOML config serialization

use crate::custody::PasswordEncryptedCustody;
use crate::error::{HoError, HoResult};
use crate::keys::commonware::NodePrivKey;
use crate::traits::{NodeIdentityCustody, NodeIdentityTrait};
use crate::types::ergors::network::v1::{HostOs, NetworkConfig, NodeIdentity, NodeType};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Parameters for generating a new node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBootstrapParams {
    /// Type of node to create (Coordinator, Executor, etc.)
    pub node_type: NodeType,
    /// Host address (IP or hostname)
    pub host: String,
    /// P2P listen port
    pub p2p_port: u16,
    /// HTTP API port
    pub api_port: u16,
    /// SSH port (for future SSH bootstrap)
    pub ssh_port: u16,
    /// Bootstrap peer addresses for initial P2P connection
    pub bootstrap_peers: Vec<String>,
    /// Custody encryption password (temporary, for bootstrap transfer)
    pub custody_password: String,
}

impl Default for NodeBootstrapParams {
    fn default() -> Self {
        Self {
            node_type: NodeType::Executor,
            host: "0.0.0.0".to_string(),
            p2p_port: 26969,
            api_port: 8080,
            ssh_port: 22,
            bootstrap_peers: Vec::new(),
            custody_password: String::new(),
        }
    }
}

/// Complete node configuration ready for deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node identity (public key only, private key in custody)
    pub identity: NodeIdentity,
    /// Network configuration
    pub network: NetworkConfig,
    /// Encrypted custody data (contains private key)
    pub custody_data: Vec<u8>,
}

/// Bootstrap configuration generator
pub struct BootstrapConfigGenerator;

impl BootstrapConfigGenerator {
    pub fn new() -> Self {
        Self
    }

    /// Generate complete configuration for a new node
    ///
    /// This creates:
    /// 1. New Ed25519 identity keypair
    /// 2. Network configuration with bootstrap peers
    /// 3. Encrypted custody file containing the private key
    ///
    /// The custody password should be a temporary password that will be
    /// transmitted securely over the bootstrap channel, then the node
    /// should re-encrypt with its own permanent password.
    pub async fn generate_node_config(&self, params: NodeBootstrapParams) -> HoResult<NodeConfig> {
        // Validate params
        if params.custody_password.is_empty() {
            return Err(HoError::BootstrapError(
                "custody_password cannot be empty".to_string(),
            ));
        }

        // Create temp directory for custody file
        let temp_dir = std::env::temp_dir().join(format!("bootstrap_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            HoError::BootstrapError(format!("Failed to create temp dir: {}", e))
        })?;
        let custody_path = temp_dir.join("identity.custody");
        let custody_path_str = custody_path.to_str().ok_or_else(|| {
            HoError::BootstrapError("Invalid temp path encoding".to_string())
        })?;

        // Create custody with file path and generate identity
        let custody = PasswordEncryptedCustody::new(custody_path_str);
        custody.create_identity(&params.custody_password, None).map_err(|e| {
            HoError::BootstrapError(format!("Failed to create identity: {}", e))
        })?;

        // Unlock custody to access the public key
        custody.unlock(&params.custody_password).await.map_err(|e| {
            HoError::BootstrapError(format!("Failed to unlock custody: {}", e))
        })?;

        let pubkey = custody.public_key().map_err(|e| {
            HoError::BootstrapError(format!("Failed to get public key: {}", e))
        })?;

        // Build NodeIdentity with public key
        let mut identity = NodeIdentity::new();
        identity.node_type = params.node_type.as_str_name().to_string();
        identity.host = params.host.clone();
        identity.p2p_port = params.p2p_port as u32;
        identity.api_port = params.api_port as u32;
        identity.ssh_port = params.ssh_port as u32;
        identity.os = HostOs::Linux as i32;
        identity.user = "ergors".to_string();
        identity.set_public_key(&pubkey);

        // Create network config
        let network = NetworkConfig {
            bootstrap_peers: params.bootstrap_peers.clone(),
            ..Default::default()
        };

        // Read the custody file that was written by create_identity
        let custody_data = std::fs::read(&custody_path).map_err(|e| {
            HoError::BootstrapError(format!("Failed to read custody file: {}", e))
        })?;

        // Clean up temp directory
        let _ = std::fs::remove_dir_all(&temp_dir);

        Ok(NodeConfig {
            identity,
            network,
            custody_data,
        })
    }

    /// Generate encrypted custody file separately
    ///
    /// This is useful when you need to transmit custody data
    /// separately from the config (e.g., via different channels).
    pub fn generate_custody(
        &self,
        _identity: &NodeIdentity,
        private_key: &NodePrivKey,
        password: &str,
    ) -> HoResult<Vec<u8>> {
        if password.is_empty() {
            return Err(HoError::BootstrapError(
                "custody password cannot be empty".to_string(),
            ));
        }

        // Create temporary custody path for generation
        let temp_dir = std::env::temp_dir().join(format!("bootstrap_custody_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            HoError::BootstrapError(format!("Failed to create temp dir: {}", e))
        })?;
        let custody_path = temp_dir.join("identity.custody");
        let custody_path_str = custody_path.to_str().ok_or_else(|| {
            HoError::BootstrapError("Invalid temp path encoding".to_string())
        })?;
        let custody = PasswordEncryptedCustody::new(custody_path_str);

        // Import the identity with the private key
        custody.import_identity(private_key, password, None).map_err(|e| {
            HoError::BootstrapError(format!("Failed to import private key: {}", e))
        })?;

        // Read the custody file that was created
        let data = std::fs::read(&custody_path).map_err(|e| {
            HoError::BootstrapError(format!("Failed to read custody file: {}", e))
        })?;

        // Clean up temp directory
        let _ = std::fs::remove_dir_all(&temp_dir);

        Ok(data)
    }

    /// Serialize node config to TOML format
    ///
    /// This produces a config.toml file that ergors can load.
    /// The custody file is NOT included (it's binary and separate).
    pub fn to_toml(&self, config: &NodeConfig) -> HoResult<String> {
        #[derive(Serialize)]
        struct ConfigToml<'a> {
            identity: &'a NodeIdentity,
            network: &'a NetworkConfig,
        }

        let toml_data = ConfigToml {
            identity: &config.identity,
            network: &config.network,
        };

        toml::to_string_pretty(&toml_data).map_err(|e| {
            HoError::BootstrapError(format!("Failed to serialize TOML: {}", e))
        })
    }

    /// Generate a secure temporary bootstrap password
    ///
    /// This password is used for the initial custody encryption during bootstrap.
    /// After the node receives it, it should re-encrypt with its own permanent password.
    pub fn generate_bootstrap_password() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }
}

impl Default for BootstrapConfigGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_node_config() {
        let params = NodeBootstrapParams {
            node_type: NodeType::Executor,
            host: "127.0.0.1".to_string(),
            p2p_port: 26969,
            api_port: 8080,
            ssh_port: 22,
            bootstrap_peers: vec!["peer1@1.2.3.4:26969".to_string()],
            custody_password: "test_password_123".to_string(),
        };

        let generator = BootstrapConfigGenerator::new();
        let config = generator.generate_node_config(params).await.unwrap();

        // Verify identity was created
        assert!(config.identity.public_key.is_some());
        assert_eq!(config.identity.node_type, "NODE_TYPE_EXECUTOR");
        assert_eq!(config.identity.p2p_port, 26969);

        // Verify network config
        assert_eq!(config.network.bootstrap_peers.len(), 1);
        assert_eq!(config.network.bootstrap_peers[0], "peer1@1.2.3.4:26969");

        // Verify custody data exists
        assert!(!config.custody_data.is_empty());
    }

    #[tokio::test]
    async fn test_empty_password_fails() {
        let params = NodeBootstrapParams {
            custody_password: String::new(),
            ..Default::default()
        };

        let generator = BootstrapConfigGenerator::new();
        let result = generator.generate_node_config(params).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_bootstrap_password() {
        let pwd1 = BootstrapConfigGenerator::generate_bootstrap_password();
        let pwd2 = BootstrapConfigGenerator::generate_bootstrap_password();

        // Should be 64 hex chars (32 bytes)
        assert_eq!(pwd1.len(), 64);
        assert_eq!(pwd2.len(), 64);

        // Should be different (random)
        assert_ne!(pwd1, pwd2);
    }

    #[tokio::test]
    async fn test_to_toml() {
        let params = NodeBootstrapParams {
            custody_password: "test123".to_string(),
            ..Default::default()
        };

        let generator = BootstrapConfigGenerator::new();
        let config = generator.generate_node_config(params).await.unwrap();
        let toml_str = generator.to_toml(&config).unwrap();

        // Verify TOML contains expected fields
        assert!(toml_str.contains("node_type"));
        assert!(toml_str.contains("p2p_port"));
        assert!(toml_str.contains("bootstrap_peers"));
    }
}
