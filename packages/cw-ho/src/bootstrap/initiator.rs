//! Bootstrap Initiator (Node Side)
//!
//! Handles the node-side bootstrap process:
//! - Requesting challenges from coordinator
//! - Signing challenge responses
//! - Processing received secret shares
//! - Reconstructing and caching API keys

use super::{create_challenge_response, BootstrapConfig, BootstrapState};
use ho_std::ephemeral::EphemeralKeyManager;
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::secret_sharing::{self, DecryptedShare, Secret, SharingMode};
use ho_std::types::ergors::network::v1::{
    IdentityChallenge, KeySharingMode, NodeIdentity, SecretShare,
};
use ho_std::types::ergors::orch::v1::{BootstrapMethod, BootstrapRequest, BootstrapResponse};
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Bootstrap initiator for regular (non-coordinator) nodes
///
/// Manages the bootstrap process from the node's perspective:
/// - Requests and responds to identity challenges
/// - Collects and decrypts secret shares
/// - Reconstructs API keys from shares
/// - Caches keys in the ephemeral manager
pub struct BootstrapInitiator {
    /// Our node's private key
    node_privkey: Arc<NodePrivKey>,
    /// Our node identity
    identity: NodeIdentity,
    /// Ephemeral key manager for caching decrypted keys
    ephemeral_manager: Arc<EphemeralKeyManager>,
    /// Bootstrap configuration
    config: BootstrapConfig,
    /// Current bootstrap state
    state: RwLock<BootstrapState>,
    /// Collected shares per provider
    collected_shares: RwLock<HashMap<String, Vec<DecryptedShare>>>,
}

impl BootstrapInitiator {
    /// Create a new bootstrap initiator
    pub fn new(
        node_privkey: Arc<NodePrivKey>,
        identity: NodeIdentity,
        ephemeral_manager: Arc<EphemeralKeyManager>,
        config: BootstrapConfig,
    ) -> Self {
        Self {
            node_privkey,
            identity,
            ephemeral_manager,
            config,
            state: RwLock::new(BootstrapState::AwaitingChallenge),
            collected_shares: RwLock::new(HashMap::new()),
        }
    }

    /// Get the current bootstrap state
    pub fn state(&self) -> BootstrapState {
        self.state.read().unwrap().clone()
    }

    /// Process a received identity challenge
    pub fn receive_challenge(&self, challenge: IdentityChallenge) -> HoResult<()> {
        let mut state = self.state.write().unwrap();

        // Verify we're expecting a challenge
        if !matches!(*state, BootstrapState::AwaitingChallenge) {
            return Err(HoError::Cfg(format!(
                "Unexpected challenge, current state: {:?}",
                *state
            )));
        }

        *state = BootstrapState::ChallengeReceived { challenge };
        debug!("Received identity challenge from coordinator");
        Ok(())
    }

    /// Create a bootstrap request with signed challenge response
    pub fn create_bootstrap_request(&self) -> HoResult<BootstrapRequest> {
        let state = self.state.read().unwrap();

        let challenge = match &*state {
            BootstrapState::ChallengeReceived { challenge } => challenge.clone(),
            _ => {
                return Err(HoError::Cfg(format!(
                    "No challenge received, current state: {:?}",
                    *state
                )))
            }
        };

        drop(state); // Release lock before signing

        // Create challenge response by signing the nonce
        let challenge_response = create_challenge_response(&challenge, &self.node_privkey)?;

        // Build the bootstrap request
        let request = BootstrapRequest {
            bootstrap_method: Some(BootstrapMethod::default()),
            identity: Some(self.identity.clone()),
            challenge_response: Some(challenge_response),
            requested_providers: self.config.requested_providers.clone(),
            preferred_mode: self.config.preferred_mode.into(),
        };

        // Update state
        let mut state = self.state.write().unwrap();
        *state = BootstrapState::AwaitingResponse;

        info!("Created bootstrap request for providers: {:?}", self.config.requested_providers);
        Ok(request)
    }

    /// Process a bootstrap response from the coordinator
    pub fn process_bootstrap_response(&self, response: BootstrapResponse) -> HoResult<()> {
        // Check status (using "success" or "error" in status field)
        if response.status != "success" {
            let mut state = self.state.write().unwrap();
            *state = BootstrapState::Failed {
                reason: response.summary.clone(),
            };
            return Err(HoError::Cfg(format!(
                "Bootstrap failed: {}",
                response.summary
            )));
        }

        info!(
            "Received bootstrap response with {} shares",
            response.secret_shares.len()
        );

        // Process each received share
        for share in &response.secret_shares {
            if let Err(e) = self.process_share(share) {
                warn!("Failed to process share for provider '{}': {}", share.provider, e);
            }
        }

        // Try to reconstruct keys for each provider
        let providers: Vec<String> = self
            .collected_shares
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();

        for provider in providers {
            if let Err(e) = self.try_reconstruct_key(&provider) {
                warn!("Failed to reconstruct key for provider '{}': {}", provider, e);
            }
        }

        // Update state
        let mut state = self.state.write().unwrap();
        *state = BootstrapState::Completed {
            shares: response.secret_shares.clone(),
        };

        Ok(())
    }

    /// Process a single secret share
    fn process_share(&self, share: &SecretShare) -> HoResult<()> {
        // Decrypt the share using our private key
        let decrypted_secret =
            ho_std::secret_sharing::decrypt_from_sender(&share.encrypted_value, &self.node_privkey)?;

        let decrypted_share = DecryptedShare::new(share.index as u8, decrypted_secret.as_bytes().to_vec());

        // Store the decrypted share
        let mut shares = self.collected_shares.write().unwrap();
        shares
            .entry(share.provider.clone())
            .or_insert_with(Vec::new)
            .push(decrypted_share);

        debug!(
            "Processed share {} for provider '{}'",
            share.index, share.provider
        );
        Ok(())
    }

    /// Try to reconstruct an API key from collected shares
    fn try_reconstruct_key(&self, provider: &str) -> HoResult<()> {
        let shares = self.collected_shares.read().unwrap();
        let provider_shares = shares
            .get(provider)
            .ok_or_else(|| HoError::Cfg(format!("No shares for provider '{}'", provider)))?;

        if provider_shares.is_empty() {
            return Err(HoError::Cfg(format!(
                "No shares collected for provider '{}'",
                provider
            )));
        }

        // Determine sharing mode from the first share
        // In direct mode, we just have one share that IS the secret
        let mode = if provider_shares.len() == 1 {
            SharingMode::Direct
        } else {
            // Assume 2-of-n threshold for Shamir
            SharingMode::shamir(provider_shares.len() as u8, provider_shares.len() as u8)
        };

        // Reconstruct the secret
        let secret = secret_sharing::reconstruct_secret(provider_shares, mode)?;

        // Cache in ephemeral manager
        self.ephemeral_manager.store_provider_key(
            &mut OsRng,
            provider,
            secret.as_bytes(),
            None,
        )?;

        info!(
            "Reconstructed and cached API key for provider '{}' ({} bytes)",
            provider,
            secret.as_bytes().len()
        );
        Ok(())
    }

    /// Check if bootstrap is complete
    pub fn is_complete(&self) -> bool {
        self.state.read().unwrap().is_complete()
    }

    /// Check if bootstrap has failed
    pub fn is_failed(&self) -> bool {
        self.state.read().unwrap().is_failed()
    }

    /// Get the failure reason if failed
    pub fn failure_reason(&self) -> Option<String> {
        self.state
            .read()
            .unwrap()
            .failure_reason()
            .map(String::from)
    }

    /// Get list of providers with cached keys
    pub fn cached_providers(&self) -> Vec<String> {
        self.ephemeral_manager.list_providers()
    }

    /// Check if a provider key is available
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.ephemeral_manager.has_provider_key(provider)
    }

    /// Get a provider's API key (if cached)
    pub fn get_provider_key(&self, provider: &str) -> Option<Vec<u8>> {
        self.ephemeral_manager.get_provider_key(provider)
    }

    /// Reset the bootstrap state for retry
    pub fn reset(&self) {
        let mut state = self.state.write().unwrap();
        *state = BootstrapState::AwaitingChallenge;
        self.collected_shares.write().unwrap().clear();
        debug!("Bootstrap initiator reset for retry");
    }
}

impl std::fmt::Debug for BootstrapInitiator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootstrapInitiator")
            .field("state", &self.state())
            .field(
                "collected_providers",
                &self.collected_shares.read().unwrap().len(),
            )
            .field("cached_providers", &self.cached_providers())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::ephemeral::DEFAULT_TTL;

    fn create_test_identity() -> NodeIdentity {
        NodeIdentity {
            host: "localhost".to_string(),
            p2p_port: 8080,
            api_port: 8081,
            user: "test".to_string(),
            os: 0,
            ssh_port: 22,
            node_type: "executor".to_string(),
            public_key: Some(vec![1, 2, 3, 4]),
        }
    }

    fn create_test_initiator() -> BootstrapInitiator {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let identity = create_test_identity();
        let ephemeral = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        BootstrapInitiator::new(privkey, identity, ephemeral, BootstrapConfig::default())
    }

    #[test]
    fn test_initial_state() {
        let initiator = create_test_initiator();
        assert!(matches!(initiator.state(), BootstrapState::AwaitingChallenge));
        assert!(!initiator.is_complete());
        assert!(!initiator.is_failed());
    }

    #[test]
    fn test_receive_challenge() {
        let initiator = create_test_initiator();

        let challenge = IdentityChallenge {
            challenge_id: "test-challenge".to_string(),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8],
            challenged_pubkey: vec![],
            expires_at: None,
        };

        initiator.receive_challenge(challenge).unwrap();

        assert!(matches!(
            initiator.state(),
            BootstrapState::ChallengeReceived { .. }
        ));
    }

    #[test]
    fn test_create_bootstrap_request() {
        let initiator = create_test_initiator();

        let challenge = IdentityChallenge {
            challenge_id: "test-challenge".to_string(),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8],
            challenged_pubkey: vec![],
            expires_at: None,
        };

        initiator.receive_challenge(challenge).unwrap();
        let request = initiator.create_bootstrap_request().unwrap();

        assert!(request.challenge_response.is_some());
        assert!(!request.requested_providers.is_empty());
        assert!(matches!(initiator.state(), BootstrapState::AwaitingResponse));
    }

    #[test]
    fn test_process_failed_response() {
        let initiator = create_test_initiator();

        // Skip to awaiting response state
        {
            let mut state = initiator.state.write().unwrap();
            *state = BootstrapState::AwaitingResponse;
        }

        let response = BootstrapResponse {
            id: "test".to_string(),
            target_node: String::new(),
            status: "error".to_string(),
            summary: "Unauthorized".to_string(),
            timestamp: None,
            duration_ms: 0,
            identity_contract_address: String::new(),
            secret_shares: vec![],
            next_challenge: None,
        };

        let result = initiator.process_bootstrap_response(response);
        assert!(result.is_err());
        assert!(initiator.is_failed());
        assert_eq!(initiator.failure_reason(), Some("Unauthorized".to_string()));
    }

    #[test]
    fn test_reset() {
        let initiator = create_test_initiator();

        // Move to failed state
        {
            let mut state = initiator.state.write().unwrap();
            *state = BootstrapState::Failed {
                reason: "test".to_string(),
            };
        }

        initiator.reset();

        assert!(matches!(initiator.state(), BootstrapState::AwaitingChallenge));
    }
}
