//! Startup Integration for Key Distribution System
//!
//! Provides integration helpers for the server startup flow,
//! initializing the ephemeral key manager, bootstrap handlers,
//! and distribution manager.

use super::{ApiKeyDistributor, default_provider_configs};
use crate::bootstrap::{BootstrapConfig, BootstrapHandler, BootstrapInitiator};
use crate::network::KeySharingHandler;
use ho_std::ephemeral::{EphemeralKeyManager, DEFAULT_TTL};
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::types::ergors::network::v1::NodeIdentity;
use std::sync::Arc;
use tracing::{debug, info};

/// Key distribution system state
///
/// Holds all the components needed for API key distribution:
/// - Ephemeral key manager for caching
/// - Bootstrap handler (coordinator) or initiator (node)
/// - Key sharing handler for network messages
/// - API key distributor (coordinator only)
pub struct KeyDistributionSystem {
    /// Ephemeral key manager for caching decrypted keys
    pub ephemeral_manager: Arc<EphemeralKeyManager>,
    /// Bootstrap handler (coordinator only)
    pub bootstrap_handler: Option<Arc<BootstrapHandler>>,
    /// Bootstrap initiator (non-coordinator only)
    pub bootstrap_initiator: Option<Arc<BootstrapInitiator>>,
    /// Key sharing network handler
    pub key_sharing_handler: Option<Arc<KeySharingHandler>>,
    /// API key distributor (coordinator only)
    pub distributor: Option<Arc<ApiKeyDistributor>>,
    /// Whether this node is a coordinator
    pub is_coordinator: bool,
}

impl KeyDistributionSystem {
    /// Initialize the key distribution system for a coordinator node
    ///
    /// # Arguments
    /// * `node_privkey` - The coordinator's private key
    /// * `config` - Optional custom bootstrap configuration
    pub fn new_coordinator(
        node_privkey: Arc<NodePrivKey>,
        config: Option<BootstrapConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let ephemeral_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));

        let bootstrap_handler = Arc::new(BootstrapHandler::new(
            node_privkey.clone(),
            ephemeral_manager.clone(),
            config,
        ));

        let distributor = Arc::new(ApiKeyDistributor::new(
            node_privkey.clone(),
            bootstrap_handler.clone(),
            ephemeral_manager.clone(),
        ));

        let key_sharing_handler = Arc::new(KeySharingHandler::new_coordinator(
            bootstrap_handler.clone(),
            ephemeral_manager.clone(),
            node_privkey,
        ));

        info!("Initialized key distribution system for coordinator");

        Self {
            ephemeral_manager,
            bootstrap_handler: Some(bootstrap_handler),
            bootstrap_initiator: None,
            key_sharing_handler: Some(key_sharing_handler),
            distributor: Some(distributor),
            is_coordinator: true,
        }
    }

    /// Initialize the key distribution system for a regular (non-coordinator) node
    ///
    /// # Arguments
    /// * `node_privkey` - The node's private key
    /// * `identity` - The node's identity
    /// * `config` - Optional custom bootstrap configuration
    pub fn new_node(
        node_privkey: Arc<NodePrivKey>,
        identity: NodeIdentity,
        config: Option<BootstrapConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let ephemeral_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));

        let bootstrap_initiator = Arc::new(BootstrapInitiator::new(
            node_privkey.clone(),
            identity,
            ephemeral_manager.clone(),
            config,
        ));

        let key_sharing_handler = Arc::new(KeySharingHandler::new_node(
            bootstrap_initiator.clone(),
            ephemeral_manager.clone(),
            node_privkey,
        ));

        info!("Initialized key distribution system for node");

        Self {
            ephemeral_manager,
            bootstrap_handler: None,
            bootstrap_initiator: Some(bootstrap_initiator),
            key_sharing_handler: Some(key_sharing_handler),
            distributor: None,
            is_coordinator: false,
        }
    }

    /// Start the ephemeral key cleanup task
    pub fn start_cleanup_task(&self) {
        self.ephemeral_manager.start_cleanup_task();
        debug!("Started ephemeral key cleanup task");
    }

    /// Stop the ephemeral key cleanup task
    pub fn stop_cleanup_task(&self) {
        self.ephemeral_manager.stop_cleanup_task();
    }

    /// Configure default providers (coordinator only)
    pub fn configure_default_providers(&self) -> HoResult<()> {
        let distributor = self.distributor.as_ref().ok_or_else(|| {
            HoError::Cfg("Distributor only available on coordinator".to_string())
        })?;

        for config in default_provider_configs() {
            distributor.configure_provider(config);
        }

        info!("Configured default providers for key distribution");
        Ok(())
    }

    /// Set the identity contract address (coordinator only)
    pub fn set_identity_contract(&self, address: &str) -> HoResult<()> {
        let handler = self.bootstrap_handler.as_ref().ok_or_else(|| {
            HoError::Cfg("Bootstrap handler only available on coordinator".to_string())
        })?;

        handler.set_identity_contract(address.to_string());
        info!("Set identity contract address: {}", address);
        Ok(())
    }

    /// Set an API key for a provider (coordinator only)
    pub fn set_provider_key(&self, provider: &str, api_key: Vec<u8>) -> HoResult<()> {
        let distributor = self.distributor.as_ref().ok_or_else(|| {
            HoError::Cfg("Distributor only available on coordinator".to_string())
        })?;

        distributor.set_provider_key(provider, api_key)
    }

    /// Check if a provider key is available
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.ephemeral_manager.has_provider_key(provider)
    }

    /// Get a provider's API key (if cached)
    pub fn get_provider_key(&self, provider: &str) -> Option<Vec<u8>> {
        self.ephemeral_manager.get_provider_key(provider)
    }

    /// List providers with cached keys
    pub fn list_cached_providers(&self) -> Vec<String> {
        self.ephemeral_manager.list_providers()
    }

    /// Invalidate all cached keys
    pub fn invalidate_all(&self) {
        self.ephemeral_manager.invalidate_all();
        info!("Invalidated all cached keys");
    }
}

impl Drop for KeyDistributionSystem {
    fn drop(&mut self) {
        // Stop cleanup task and clear keys
        self.stop_cleanup_task();
        self.invalidate_all();
        debug!("Key distribution system shut down");
    }
}

impl std::fmt::Debug for KeyDistributionSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyDistributionSystem")
            .field("is_coordinator", &self.is_coordinator)
            .field("cached_providers", &self.list_cached_providers())
            .finish()
    }
}

/// Determine if a node type is a coordinator
pub fn is_coordinator_node_type(node_type: &str) -> bool {
    node_type.to_uppercase().contains("COORDINATOR")
        || node_type == "NODE_TYPE_COORDINATOR"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_coordinator_system() {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let system = KeyDistributionSystem::new_coordinator(privkey, None);

        assert!(system.is_coordinator);
        assert!(system.bootstrap_handler.is_some());
        assert!(system.distributor.is_some());
        assert!(system.bootstrap_initiator.is_none());
    }

    #[test]
    fn test_node_system() {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let identity = NodeIdentity {
            host: "localhost".to_string(),
            p2p_port: 8080,
            api_port: 8081,
            user: "test".to_string(),
            os: 0,
            ssh_port: 22,
            node_type: "executor".to_string(),
            public_key: None,
        };
        let system = KeyDistributionSystem::new_node(privkey, identity, None);

        assert!(!system.is_coordinator);
        assert!(system.bootstrap_initiator.is_some());
        assert!(system.bootstrap_handler.is_none());
        assert!(system.distributor.is_none());
    }

    #[test]
    fn test_is_coordinator_node_type() {
        assert!(is_coordinator_node_type("NODE_TYPE_COORDINATOR"));
        assert!(is_coordinator_node_type("coordinator"));
        assert!(is_coordinator_node_type("COORDINATOR"));
        assert!(!is_coordinator_node_type("executor"));
        assert!(!is_coordinator_node_type("referee"));
    }
}
