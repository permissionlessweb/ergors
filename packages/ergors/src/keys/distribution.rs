//! API Key Distribution Manager
//!
//! Manages the distribution of API keys to authorized nodes using
//! the secret sharing protocol.
//!
//! ## Hybrid Ownership Model
//!
//! - **Shared providers** (anthropic, openai): Coordinator distributes via Shamir
//! - **Local providers** (ollama): Per-node, no distribution needed
//!
//! ## Flow
//!
//! ```text
//! Coordinator                          Authorized Nodes
//!     │                                      │
//!     │  1. Get master API key from storage  │
//!     │  2. Decrypt with node key            │
//!     │  3. Split using Shamir (k,n)         │
//!     │  4. Encrypt each share for recipient │
//!     │                                      │
//!     │────── Distribute Shares ────────────►│
//!     │                                      │
//!     │                      5. Decrypt share│
//!     │                      6. Reconstruct  │
//!     │                      7. Cache key    │
//! ```

pub mod startup;

pub use startup::{is_coordinator_node_type, KeyDistributionSystem};

// TODO: Re-integrate bootstrap with new architecture
// use crate::deploy::BootstrapHandler;
use ho_std::ephemeral::EphemeralKeyManager;
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::{NodePrivKey, NodePubkey};
use ho_std::secret_sharing::{self, Secret, SharingMode};
use ho_std::types::ergors::network::v1::{
    KeySharingMode, ProviderOwnership, SecretShare, SecretSharingConfig,
};
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Provider configuration for key distribution
#[derive(Debug, Clone)]
pub struct ProviderDistributionConfig {
    /// Provider name (e.g., "anthropic", "openai")
    pub name: String,
    /// Ownership model (shared or local)
    pub ownership: ProviderOwnership,
    /// Shamir threshold (k shares needed)
    pub threshold: u8,
    /// Total shares to generate (n)
    pub total_shares: u8,
    /// Encrypted master key (in storage)
    pub encrypted_key: Option<Vec<u8>>,
}

impl Default for ProviderDistributionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        }
    }
}

/// API Key Distributor for coordinator nodes
///
/// Handles the secure distribution of API keys to authorized nodes
/// using configurable secret sharing (Shamir or Direct).
pub struct ApiKeyDistributor {
    /// Provider configurations
    provider_configs: RwLock<HashMap<String, ProviderDistributionConfig>>,
    /// Our node's private key
    node_privkey: Arc<NodePrivKey>,
    /// Ephemeral key manager
    key_manager: Arc<EphemeralKeyManager>,
}

impl ApiKeyDistributor {
    /// Create a new API key distributor
    pub fn new(
        node_privkey: Arc<NodePrivKey>,
        key_manager: Arc<EphemeralKeyManager>,
    ) -> Self {
        Self {
            provider_configs: RwLock::new(HashMap::new()),
            node_privkey,
            key_manager,
        }
    }

    /// Configure a provider for distribution
    pub fn configure_provider(&self, config: ProviderDistributionConfig) {
        let name = config.name.clone();
        self.provider_configs
            .write()
            .unwrap()
            .insert(name.clone(), config.clone());

        // Note: Provider configuration is now stored locally only.
        // Bootstrap orchestrator will read from this config when needed.

        info!(
            "Configured provider '{}' for distribution (ownership: {:?}, threshold: {}/{})",
            name, config.ownership, config.threshold, config.total_shares
        );
    }

    /// Get provider configuration
    pub fn get_provider_config(&self, provider: &str) -> Option<ProviderDistributionConfig> {
        self.provider_configs.read().unwrap().get(provider).cloned()
    }

    /// List all configured providers
    pub fn list_providers(&self) -> Vec<String> {
        self.provider_configs.read().unwrap().keys().cloned().collect()
    }

    /// List shared providers (those that need distribution)
    pub fn list_shared_providers(&self) -> Vec<String> {
        self.provider_configs
            .read()
            .unwrap()
            .iter()
            .filter(|(_, config)| config.ownership == ProviderOwnership::Shared)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// List local providers (per-node only)
    pub fn list_local_providers(&self) -> Vec<String> {
        self.provider_configs
            .read()
            .unwrap()
            .iter()
            .filter(|(_, config)| config.ownership == ProviderOwnership::Local)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Generate shares for a provider
    ///
    /// # Arguments
    /// * `provider` - Provider name
    /// * `recipients` - Public keys of recipients
    ///
    /// # Returns
    /// Vector of encrypted shares, one per recipient
    pub fn generate_shares(
        &self,
        provider: &str,
        recipients: &[NodePubkey],
    ) -> HoResult<Vec<SecretShare>> {
        let config = self.get_provider_config(provider).ok_or_else(|| {
            HoError::Cfg(format!("Provider '{}' not configured", provider))
        })?;

        let encrypted_key = config.encrypted_key.ok_or_else(|| {
            HoError::Cfg(format!("No key configured for provider '{}'", provider))
        })?;

        // For local providers, use direct mode
        if config.ownership == ProviderOwnership::Local {
            return Err(HoError::Cfg(format!(
                "Provider '{}' is local-only, not distributed",
                provider
            )));
        }

        // Determine sharing mode based on config
        let mode = if recipients.len() == 1 {
            SharingMode::Direct
        } else {
            SharingMode::shamir(config.threshold, config.total_shares)
        };

        // The encrypted_key is the raw API key for now
        // In production, this would be decrypted from secure storage
        let secret = Secret::new(encrypted_key);

        // Split the secret
        let encrypted_shares =
            secret_sharing::split_secret(&mut OsRng, &secret, mode, recipients)?;

        // Convert to proto format
        let proto_shares: Vec<SecretShare> = encrypted_shares
            .into_iter()
            .map(|share| {
                let mode_proto = match share.mode {
                    SharingMode::Direct => KeySharingMode::Direct,
                    SharingMode::Shamir { .. } => KeySharingMode::Shamir,
                };

                SecretShare {
                    share_id: format!("share-{}", uuid::Uuid::new_v4()),
                    index: share.index as u32,
                    encrypted_value: share.encrypted_value,
                    recipient_pubkey: share.recipient_pubkey,
                    provider: provider.to_string(),
                    expires_at: None,
                    mode: mode_proto.into(),
                    commitment: share.commitment.unwrap_or_default(),
                    config: Some(SecretSharingConfig {
                        mode: mode_proto.into(),
                        threshold: config.threshold as u32,
                        total_shares: config.total_shares as u32,
                    }),
                }
            })
            .collect();

        debug!(
            "Generated {} shares for provider '{}' (mode: {:?})",
            proto_shares.len(),
            provider,
            mode
        );

        Ok(proto_shares)
    }

    /// Distribute API key to specific nodes
    ///
    /// Returns the shares to be sent via the network
    pub fn distribute_to_nodes(
        &self,
        provider: &str,
        authorized_nodes: &[NodePubkey],
    ) -> HoResult<Vec<(NodePubkey, SecretShare)>> {
        let shares = self.generate_shares(provider, authorized_nodes)?;

        // Pair each share with its recipient
        let distribution: Vec<(NodePubkey, SecretShare)> = shares
            .into_iter()
            .zip(authorized_nodes.iter().cloned())
            .map(|(share, recipient)| (recipient, share))
            .collect();

        info!(
            "Prepared distribution of '{}' to {} nodes",
            provider,
            distribution.len()
        );

        Ok(distribution)
    }

    /// Set/update an API key for a provider
    pub fn set_provider_key(&self, provider: &str, api_key: Vec<u8>) -> HoResult<()> {
        let mut configs = self.provider_configs.write().unwrap();

        if let Some(config) = configs.get_mut(provider) {
            config.encrypted_key = Some(api_key);

            // Note: API key is now stored in config only.
            // Bootstrap orchestrator will read from this when distributing keys.
            info!("Updated API key for provider '{}'", provider);
            Ok(())
        } else {
            Err(HoError::Cfg(format!("Provider '{}' not configured", provider)))
        }
    }

    /// Remove a provider's key (for rotation)
    pub fn remove_provider_key(&self, provider: &str) {
        if let Some(config) = self.provider_configs.write().unwrap().get_mut(provider) {
            config.encrypted_key = None;
            info!("Removed API key for provider '{}'", provider);
        }
    }

    /// Check if a provider has a key configured
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.provider_configs
            .read()
            .unwrap()
            .get(provider)
            .map(|c| c.encrypted_key.is_some())
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for ApiKeyDistributor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyDistributor")
            .field("providers", &self.list_providers())
            .field("shared_providers", &self.list_shared_providers())
            .field("local_providers", &self.list_local_providers())
            .finish()
    }
}

/// Create default provider configurations
pub fn default_provider_configs() -> Vec<ProviderDistributionConfig> {
    vec![
        ProviderDistributionConfig {
            name: "anthropic".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        },
        ProviderDistributionConfig {
            name: "openai".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        },
        ProviderDistributionConfig {
            name: "ollama".to_string(),
            ownership: ProviderOwnership::Local,
            threshold: 1,
            total_shares: 1,
            encrypted_key: None,
        },
    ]
}


//! Startup Integration for Key Distribution System
//!
//! Provides integration helpers for the server startup flow,
//! initializing the ephemeral key manager, bootstrap handlers,
//! and distribution manager.

use super::{ApiKeyDistributor, default_provider_configs};
// TODO: Re-integrate bootstrap with new architecture
// use crate::deploy::{BootstrapConfig, BootstrapHandler, BootstrapInitiator};
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
    /// Key sharing network handler
    pub key_sharing_handler: Option<Arc<KeySharingHandler>>,
    /// API key distributor (coordinator only)
    pub distributor: Option<Arc<ApiKeyDistributor>>,
    /// Whether this node is a coordinator
    pub is_coordinator: bool,
}

impl KeyDistributionSystem {
    // TODO: Refactor to use new bootstrap architecture (BootstrapOrchestrator + BootstrapReceiver)

    /// Initialize the key distribution system for a coordinator node
    ///
    /// # Arguments
    /// * `node_privkey` - The coordinator's private key
    pub fn new_coordinator(
        node_privkey: Arc<NodePrivKey>,
    ) -> Self {
        let ephemeral_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));

        let distributor = Some(Arc::new(ApiKeyDistributor::new(
            node_privkey.clone(),
            ephemeral_manager.clone(),
        )));

        let key_sharing_handler = Some(Arc::new(KeySharingHandler::new_coordinator(
            ephemeral_manager.clone(),
            node_privkey,
        )));

        info!("Initialized key distribution system for coordinator");

        Self {
            ephemeral_manager,
            key_sharing_handler,
            distributor,
            is_coordinator: true,
        }
    }

    /// Initialize the key distribution system for a regular (non-coordinator) node
    ///
    /// # Arguments
    /// * `node_privkey` - The node's private key
    /// * `identity` - The node's identity
    pub fn new_node(
        node_privkey: Arc<NodePrivKey>,
        _identity: NodeIdentity,
    ) -> Self {
        let ephemeral_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));

        let key_sharing_handler = Some(Arc::new(KeySharingHandler::new_node(
            ephemeral_manager.clone(),
            node_privkey,
        )));

        info!("Initialized key distribution system for node");

        Self {
            ephemeral_manager,
            key_sharing_handler,
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

    // Note: Identity contract management moved to BootstrapOrchestrator in new architecture

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


//! Key Sharing Protocol Handler (Network Channel 4)
//!
//! Handles key sharing messages over the P2P network:
//! - Key share requests from new nodes
//! - Key share responses from coordinators
//! - Key revocation broadcasts
//! - Key heartbeat/refresh messages

// TODO: Re-integrate bootstrap with new architecture
// use crate::deploy::{BootstrapHandler, BootstrapInitiator};
use ho_std::ephemeral::EphemeralKeyManager;
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::{NodePrivKey, NodePubkey};
use ho_std::types::ergors::network::v1::{
    key_sharing_message::MessageType, KeyHeartbeat, KeyRevocation, KeyShareRequest,
    KeyShareResponse, KeySharingMessage,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Channel ID for key sharing protocol
pub const KEY_SHARING_CHANNEL: u8 = 4;

/// Key sharing handler for processing network messages
pub struct KeySharingHandler {
    /// Ephemeral key manager
    key_manager: Arc<EphemeralKeyManager>,
    /// Our node's private key
    node_privkey: Arc<NodePrivKey>,
    /// Whether we're a coordinator
    is_coordinator: bool,
    /// Revoked public keys
    revoked_keys: RwLock<Vec<Vec<u8>>>,
}

impl KeySharingHandler {
    /// Create a new key sharing handler for coordinator nodes
    pub fn new_coordinator(
        key_manager: Arc<EphemeralKeyManager>,
        node_privkey: Arc<NodePrivKey>,
    ) -> Self {
        Self {
            key_manager,
            node_privkey,
            is_coordinator: true,
            revoked_keys: RwLock::new(Vec::new()),
        }
    }

    /// Create a new key sharing handler for regular nodes
    pub fn new_node(
        key_manager: Arc<EphemeralKeyManager>,
        node_privkey: Arc<NodePrivKey>,
    ) -> Self {
        Self {
            key_manager,
            node_privkey,
            is_coordinator: false,
            revoked_keys: RwLock::new(Vec::new()),
        }
    }

    /// Handle an incoming key sharing message
    ///
    /// Returns an optional response message to send back
    pub async fn handle_message(
        &self,
        from: &NodePubkey,
        message: KeySharingMessage,
    ) -> HoResult<Option<KeySharingMessage>> {
        // Check if sender is revoked
        let from_bytes = commonware_codec::Encode::encode(&from.0).to_vec();
        if self.is_revoked(&from_bytes).await {
            warn!("Ignoring message from revoked node");
            return Ok(None);
        }

        let message_type = message.message_type.ok_or_else(|| {
            HoError::Cfg("Key sharing message missing message type".to_string())
        })?;

        match message_type {
            MessageType::Request(request) => self.handle_request(from, request).await,
            MessageType::Response(response) => self.handle_response(from, response).await,
            MessageType::Revocation(revocation) => self.handle_revocation(from, revocation).await,
            MessageType::Heartbeat(heartbeat) => self.handle_heartbeat(from, heartbeat).await,
        }
    }

    /// Handle a key share request (coordinator only)
    async fn handle_request(
        &self,
        from: &NodePubkey,
        _request: KeyShareRequest,
    ) -> HoResult<Option<KeySharingMessage>> {
        if !self.is_coordinator {
            warn!("Non-coordinator received key share request, ignoring");
            return Ok(None);
        }

        debug!("Received key share request from {:?}", from);

        // P2P key sharing requests are not yet integrated with new bootstrap architecture
        // Keys are currently distributed via file transfer during bootstrap
        Ok(Some(KeySharingMessage {
            message_type: Some(MessageType::Response(KeyShareResponse {
                approved: false,
                rejection_reason: "P2P key sharing not yet implemented. Keys are distributed during bootstrap via file transfer.".to_string(),
                shares: vec![],
                next_challenge: None,
            })),
        }))
    }

    /// Handle a key share response (node only)
    ///
    /// Note: P2P key sharing responses are not yet integrated with new bootstrap
    /// architecture. Keys are currently received via file transfer during bootstrap.
    async fn handle_response(
        &self,
        _from: &NodePubkey,
        response: KeyShareResponse,
    ) -> HoResult<Option<KeySharingMessage>> {
        if self.is_coordinator {
            warn!("Coordinator received key share response, ignoring");
            return Ok(None);
        }

        debug!("Received key share response from coordinator");

        if !response.approved {
            warn!("Key share request rejected: {}", response.rejection_reason);
        } else {
            info!("Key share approved but P2P distribution not yet integrated with new bootstrap architecture");
        }

        Ok(None)
    }

    /// Handle a key revocation message
    async fn handle_revocation(
        &self,
        _from: &NodePubkey,
        revocation: KeyRevocation,
    ) -> HoResult<Option<KeySharingMessage>> {
        // Only accept revocations from coordinators
        // TODO: Verify coordinator_signature

        info!(
            "Received key revocation for provider '{}': {}",
            revocation.provider, revocation.reason
        );

        // Add to revoked keys list
        {
            let mut revoked = self.revoked_keys.write().await;
            if !revoked.contains(&revocation.revoked_pubkey) {
                revoked.push(revocation.revoked_pubkey.clone());
            }
        }

        // Remove the provider key if we have it cached
        if self.key_manager.has_provider_key(&revocation.provider) {
            self.key_manager.remove_provider_key(&revocation.provider);
            info!("Removed revoked key for provider '{}'", revocation.provider);
        }

        Ok(None)
    }

    /// Handle a key heartbeat message
    async fn handle_heartbeat(
        &self,
        from: &NodePubkey,
        heartbeat: KeyHeartbeat,
    ) -> HoResult<Option<KeySharingMessage>> {
        debug!("Received key heartbeat from {:?}, active keys: {:?}", from, heartbeat.active_key_ids);

        // Heartbeats are informational - could be used for monitoring
        // or triggering key refresh if needed

        Ok(None)
    }

    /// Check if a public key is revoked
    async fn is_revoked(&self, pubkey: &[u8]) -> bool {
        self.revoked_keys.read().await.contains(&pubkey.to_vec())
    }

    /// Broadcast a key revocation (coordinator only)
    pub fn create_revocation(
        &self,
        provider: &str,
        revoked_pubkey: Vec<u8>,
        reason: &str,
    ) -> HoResult<KeySharingMessage> {
        if !self.is_coordinator {
            return Err(HoError::Cfg("Only coordinators can create revocations".to_string()));
        }

        // Sign the revocation
        let revocation_data = format!("{}:{}:{}", provider, hex::encode(&revoked_pubkey), reason);
        let signature = self.node_privkey.sign(
            Some(b"ERGORS_KEY_REVOCATION_V1"),
            revocation_data.as_bytes(),
        );
        let sig_bytes = commonware_codec::Encode::encode(&signature).to_vec();

        Ok(KeySharingMessage {
            message_type: Some(MessageType::Revocation(KeyRevocation {
                provider: provider.to_string(),
                revoked_pubkey,
                reason: reason.to_string(),
                coordinator_signature: sig_bytes,
                effective_at: None,
            })),
        })
    }

    /// Create a heartbeat message
    pub fn create_heartbeat(&self) -> KeySharingMessage {
        let pubkey_bytes = commonware_codec::Encode::encode(&self.node_privkey.id().0).to_vec();
        KeySharingMessage {
            message_type: Some(MessageType::Heartbeat(KeyHeartbeat {
                public_key: pubkey_bytes,
                challenge_response: None, // Can add signed challenge for verification
                active_key_ids: self.key_manager.list_providers(),
            })),
        }
    }
}

impl std::fmt::Debug for KeySharingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeySharingHandler")
            .field("is_coordinator", &self.is_coordinator)
            .field("cached_providers", &self.key_manager.list_providers())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::ephemeral::DEFAULT_TTL;
    use rand::rngs::OsRng;

    fn create_coordinator_handler() -> KeySharingHandler {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let key_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        KeySharingHandler::new_coordinator(key_manager, privkey)
    }

    #[test]
    fn test_coordinator_handler() {
        let handler = create_coordinator_handler();
        assert!(handler.is_coordinator);
    }

    #[test]
    fn test_create_revocation() {
        let handler = create_coordinator_handler();
        let revocation = handler.create_revocation(
            "anthropic",
            vec![1, 2, 3, 4],
            "Key compromised",
        ).unwrap();

        assert!(matches!(
            revocation.message_type,
            Some(MessageType::Revocation(_))
        ));
    }

    #[test]
    fn test_create_heartbeat() {
        let handler = create_coordinator_handler();
        let heartbeat = handler.create_heartbeat();

        assert!(matches!(
            heartbeat.message_type,
            Some(MessageType::Heartbeat(_))
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_coordinator_system() {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let system = KeyDistributionSystem::new_coordinator(privkey);

        assert!(system.is_coordinator);
        assert!(system.key_sharing_handler.is_some());
        assert!(system.distributor.is_some());
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
            bech32_address: None,
        };
        let system = KeyDistributionSystem::new_node(privkey, identity);

        assert!(!system.is_coordinator);
        assert!(system.key_sharing_handler.is_some());
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


#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::ephemeral::DEFAULT_TTL;

    fn create_test_distributor() -> ApiKeyDistributor {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let key_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        ApiKeyDistributor::new(privkey, key_manager)
    }

    #[test]
    fn test_configure_provider() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "anthropic".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: Some(b"sk-test-key".to_vec()),
        });

        assert!(distributor.get_provider_config("anthropic").is_some());
        assert!(distributor.has_provider_key("anthropic"));
    }

    #[test]
    fn test_list_providers() {
        let distributor = create_test_distributor();

        for config in default_provider_configs() {
            distributor.configure_provider(config);
        }

        assert_eq!(distributor.list_providers().len(), 3);
        assert!(distributor.list_shared_providers().contains(&"anthropic".to_string()));
        assert!(distributor.list_local_providers().contains(&"ollama".to_string()));
    }

    #[test]
    fn test_local_provider_no_distribution() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "ollama".to_string(),
            ownership: ProviderOwnership::Local,
            threshold: 1,
            total_shares: 1,
            encrypted_key: Some(b"local-key".to_vec()),
        });

        let recipient = NodePubkey::from_bytes(&[1u8; 32]);
        if let Some(recipient) = recipient {
            let result = distributor.generate_shares("ollama", &[recipient]);
            assert!(result.is_err()); // Local providers shouldn't be distributed
        }
    }

    #[test]
    fn test_set_and_remove_key() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "test".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        });

        assert!(!distributor.has_provider_key("test"));

        distributor.set_provider_key("test", b"new-key".to_vec()).unwrap();
        assert!(distributor.has_provider_key("test"));

        distributor.remove_provider_key("test");
        assert!(!distributor.has_provider_key("test"));
    }
}
