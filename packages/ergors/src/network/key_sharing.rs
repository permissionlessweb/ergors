//! Key Sharing Protocol Handler (Network Channel 4)
//!
//! Handles key sharing messages over the P2P network:
//! - Key share requests from new nodes
//! - Key share responses from coordinators
//! - Key revocation broadcasts
//! - Key heartbeat/refresh messages

// TODO: Re-integrate bootstrap with new architecture
// use crate::bootstrap::{BootstrapHandler, BootstrapInitiator};
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
