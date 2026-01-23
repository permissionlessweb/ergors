//! Bootstrap Handler (Coordinator Side)
//!
//! Handles incoming bootstrap requests from new nodes,
//! verifies their identity, and distributes API key shares.

use super::{verify_challenge_response, BootstrapConfig};
use ho_std::ephemeral::EphemeralKeyManager;
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::{NodePrivKey, NodePubkey};
use ho_std::secret_sharing::{self, Secret, SharingMode};
use ho_std::types::ergors::network::v1::{
    IdentityChallenge, KeySharingMode, SecretShare, SecretSharingConfig,
};
use ho_std::types::ergors::orch::v1::{BootstrapRequest, BootstrapResponse};
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Pending challenge that awaits response
#[derive(Debug, Clone)]
struct PendingChallenge {
    challenge: IdentityChallenge,
    created_at: Instant,
    challenged_pubkey: Vec<u8>,
}

/// Bootstrap handler for coordinator nodes
///
/// Manages the bootstrap process including:
/// - Challenge generation and verification
/// - Identity registration
/// - API key distribution
pub struct BootstrapHandler {
    /// Our node's private key for signing
    node_privkey: Arc<NodePrivKey>,
    /// Pending challenges indexed by challenge_id
    pending_challenges: RwLock<HashMap<String, PendingChallenge>>,
    /// Authorized providers and their API keys (encrypted)
    provider_keys: RwLock<HashMap<String, Vec<u8>>>,
    /// Secret sharing configuration per provider
    provider_configs: RwLock<HashMap<String, SecretSharingConfig>>,
    /// Ephemeral key manager for caching
    ephemeral_manager: Arc<EphemeralKeyManager>,
    /// Bootstrap configuration
    config: BootstrapConfig,
    /// Identity contract address (if deployed)
    identity_contract: RwLock<Option<String>>,
}

impl BootstrapHandler {
    /// Create a new bootstrap handler
    pub fn new(
        node_privkey: Arc<NodePrivKey>,
        ephemeral_manager: Arc<EphemeralKeyManager>,
        config: BootstrapConfig,
    ) -> Self {
        Self {
            node_privkey,
            pending_challenges: RwLock::new(HashMap::new()),
            provider_keys: RwLock::new(HashMap::new()),
            provider_configs: RwLock::new(HashMap::new()),
            ephemeral_manager,
            config,
            identity_contract: RwLock::new(None),
        }
    }

    /// Set the identity contract address
    pub fn set_identity_contract(&self, address: String) {
        let mut contract = self.identity_contract.write().unwrap();
        *contract = Some(address);
    }

    /// Get the identity contract address
    pub fn identity_contract(&self) -> Option<String> {
        self.identity_contract.read().unwrap().clone()
    }

    /// Configure a provider for key sharing
    pub fn configure_provider(
        &self,
        provider: &str,
        encrypted_key: Vec<u8>,
        config: SecretSharingConfig,
    ) {
        self.provider_keys
            .write()
            .unwrap()
            .insert(provider.to_string(), encrypted_key);
        self.provider_configs
            .write()
            .unwrap()
            .insert(provider.to_string(), config);
        debug!("Configured provider '{}' for key sharing", provider);
    }

    /// Generate a challenge for a node
    pub fn create_challenge(&self, challenged_pubkey: &[u8]) -> HoResult<IdentityChallenge> {
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);

        let challenge_id = format!("challenge-{}", uuid::Uuid::new_v4());

        let expires_at = std::time::SystemTime::now()
            .checked_add(Duration::from_secs(self.config.challenge_timeout_secs))
            .unwrap_or(std::time::SystemTime::now());

        let secs = expires_at
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let challenge = IdentityChallenge {
            challenge_id: challenge_id.clone(),
            nonce: nonce.to_vec(),
            challenged_pubkey: challenged_pubkey.to_vec(),
            expires_at: Some(pbjson_types::Timestamp {
                seconds: secs,
                nanos: 0,
            }),
        };

        // Store pending challenge
        let pending = PendingChallenge {
            challenge: challenge.clone(),
            created_at: Instant::now(),
            challenged_pubkey: challenged_pubkey.to_vec(),
        };

        self.pending_challenges
            .write()
            .unwrap()
            .insert(challenge_id, pending);

        info!("Created identity challenge for pubkey");
        Ok(challenge)
    }

    /// Handle an incoming bootstrap request
    pub fn handle_bootstrap_request(
        &self,
        request: &BootstrapRequest,
    ) -> HoResult<BootstrapResponse> {
        // Verify we have a challenge response
        let challenge_response = request.challenge_response.as_ref().ok_or_else(|| {
            HoError::Cfg("Bootstrap request missing challenge response".to_string())
        })?;

        // Look up the pending challenge
        let pending = {
            let challenges = self.pending_challenges.read().unwrap();
            challenges
                .get(&challenge_response.challenge_id)
                .cloned()
                .ok_or_else(|| {
                    HoError::Cfg(format!(
                        "Unknown challenge ID: {}",
                        challenge_response.challenge_id
                    ))
                })?
        };

        // Check challenge expiry
        let timeout = Duration::from_secs(self.config.challenge_timeout_secs);
        if pending.created_at.elapsed() > timeout {
            // Remove expired challenge
            self.pending_challenges
                .write()
                .unwrap()
                .remove(&challenge_response.challenge_id);
            return Err(HoError::Cfg("Challenge has expired".to_string()));
        }

        // Verify the challenge response signature
        if !verify_challenge_response(&pending.challenge, challenge_response)? {
            warn!("Challenge response verification failed");
            return Ok(BootstrapResponse {
                id: uuid::Uuid::new_v4().to_string(),
                target_node: String::new(),
                status: "error".to_string(),
                summary: "Challenge verification failed".to_string(),
                timestamp: None,
                duration_ms: 0,
                identity_contract_address: String::new(),
                secret_shares: vec![],
                next_challenge: None,
            });
        }

        // Remove the used challenge
        self.pending_challenges
            .write()
            .unwrap()
            .remove(&challenge_response.challenge_id);

        // Get the requester's public key from the identity
        let requester_pubkey = if let Some(ref identity) = request.identity {
            let pubkey_bytes = identity.public_key.as_ref().ok_or_else(|| {
                HoError::Cfg("Identity missing public key".to_string())
            })?;
            NodePubkey::from_bytes(pubkey_bytes).ok_or_else(|| {
                HoError::Cfg("Invalid public key in identity".to_string())
            })?
        } else {
            return Err(HoError::Cfg("Bootstrap request missing identity".to_string()));
        };

        // Determine sharing mode
        let sharing_mode = match KeySharingMode::try_from(request.preferred_mode).unwrap_or(KeySharingMode::Direct) {
            KeySharingMode::Direct => SharingMode::Direct,
            KeySharingMode::Shamir => {
                // Use default 2-of-3 if not configured
                SharingMode::shamir(2, 3)
            }
            _ => SharingMode::Direct,
        };

        // Generate shares for requested providers
        let mut secret_shares = Vec::new();
        for provider in &request.requested_providers {
            match self.generate_provider_shares(&requester_pubkey, provider, sharing_mode) {
                Ok(shares) => secret_shares.extend(shares),
                Err(e) => {
                    warn!("Failed to generate shares for provider '{}': {}", provider, e);
                }
            }
        }

        let node_id = format!("node-{}", uuid::Uuid::new_v4());

        info!(
            "Bootstrap successful for node {}, distributed {} shares",
            node_id,
            secret_shares.len()
        );

        Ok(BootstrapResponse {
            id: uuid::Uuid::new_v4().to_string(),
            target_node: node_id,
            status: "success".to_string(),
            summary: format!("Distributed {} shares", secret_shares.len()),
            timestamp: None,
            duration_ms: 0,
            identity_contract_address: self.identity_contract().unwrap_or_default(),
            secret_shares,
            next_challenge: None,
        })
    }

    /// Generate secret shares for a provider
    fn generate_provider_shares(
        &self,
        recipient: &NodePubkey,
        provider: &str,
        mode: SharingMode,
    ) -> HoResult<Vec<SecretShare>> {
        // Get the provider's encrypted key
        let encrypted_key = self
            .provider_keys
            .read()
            .unwrap()
            .get(provider)
            .cloned()
            .ok_or_else(|| HoError::Cfg(format!("Provider '{}' not configured", provider)))?;

        // Get provider config
        let provider_config = self
            .provider_configs
            .read()
            .unwrap()
            .get(provider)
            .cloned();

        // Decrypt the key using our node key (placeholder - in real impl would use custody)
        // For now, assume encrypted_key is the raw key
        let secret = Secret::new(encrypted_key);

        // Split the secret
        let encrypted_shares =
            secret_sharing::split_secret(&mut OsRng, &secret, mode, &[recipient.clone()])?;

        // Convert to proto shares
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
                    config: provider_config,
                }
            })
            .collect();

        debug!(
            "Generated {} shares for provider '{}'",
            proto_shares.len(),
            provider
        );
        Ok(proto_shares)
    }

    /// Clean up expired challenges
    pub fn cleanup_expired_challenges(&self) -> usize {
        let timeout = Duration::from_secs(self.config.challenge_timeout_secs);
        let mut challenges = self.pending_challenges.write().unwrap();

        let expired: Vec<String> = challenges
            .iter()
            .filter(|(_, pending)| pending.created_at.elapsed() > timeout)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            challenges.remove(&id);
        }

        if count > 0 {
            debug!("Cleaned up {} expired challenges", count);
        }

        count
    }
}

impl std::fmt::Debug for BootstrapHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootstrapHandler")
            .field(
                "pending_challenges",
                &self.pending_challenges.read().unwrap().len(),
            )
            .field(
                "configured_providers",
                &self.provider_keys.read().unwrap().len(),
            )
            .field("identity_contract", &self.identity_contract())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::ephemeral::DEFAULT_TTL;

    fn create_test_handler() -> BootstrapHandler {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let ephemeral = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        BootstrapHandler::new(privkey, ephemeral, BootstrapConfig::default())
    }

    #[test]
    fn test_create_challenge() {
        let handler = create_test_handler();
        let pubkey = vec![1, 2, 3, 4];

        let challenge = handler.create_challenge(&pubkey).unwrap();

        assert!(!challenge.challenge_id.is_empty());
        assert_eq!(challenge.nonce.len(), 32);
        assert!(handler
            .pending_challenges
            .read()
            .unwrap()
            .contains_key(&challenge.challenge_id));
    }

    #[test]
    fn test_configure_provider() {
        let handler = create_test_handler();

        handler.configure_provider(
            "anthropic",
            b"sk-test-key".to_vec(),
            SecretSharingConfig {
                mode: KeySharingMode::Direct.into(),
                threshold: 1,
                total_shares: 1,
            },
        );

        assert!(handler
            .provider_keys
            .read()
            .unwrap()
            .contains_key("anthropic"));
    }

    #[test]
    fn test_cleanup_expired_challenges() {
        let mut config = BootstrapConfig::default();
        config.challenge_timeout_secs = 0; // Expire immediately

        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let ephemeral = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        let handler = BootstrapHandler::new(privkey, ephemeral, config);

        handler.create_challenge(&[1, 2, 3]).unwrap();

        // Small delay to ensure expiry
        std::thread::sleep(Duration::from_millis(10));

        let cleaned = handler.cleanup_expired_challenges();
        assert_eq!(cleaned, 1);
        assert!(handler.pending_challenges.read().unwrap().is_empty());
    }
}
