//! Bootstrap Protocol with Key Sharing
//!
//! Handles the secure bootstrapping of new nodes including:
//! - Identity verification via challenge-response
//! - API key distribution using secret sharing
//! - Contract-based authorization
//!
//! ## Flow
//!
//! ```text
//! New Node                    Coordinator
//!     │                            │
//!     │──── Request Challenge ────►│
//!     │◄─── Challenge (nonce) ─────│
//!     │                            │
//!     │──── BootstrapRequest ─────►│
//!     │     (signed challenge)     │
//!     │                            │
//!     │◄─── BootstrapResponse ─────│
//!     │     (Secret shares)        │
//!     │                            │
//!     │  [Reconstruct API Keys]    │
//!     │  [Cache in Ephemeral Mgr]  │
//! ```

pub mod handler;
pub mod initiator;

pub use handler::BootstrapHandler;
pub use initiator::BootstrapInitiator;

use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::types::ergors::network::v1::{
    IdentityChallenge, IdentityChallengeResponse, KeySharingMode, SecretShare,
};

/// Bootstrap state for tracking in-progress bootstraps
#[derive(Debug, Clone)]
pub enum BootstrapState {
    /// Waiting for challenge from coordinator
    AwaitingChallenge,
    /// Have challenge, preparing response
    ChallengeReceived { challenge: IdentityChallenge },
    /// Sent bootstrap request, waiting for response
    AwaitingResponse,
    /// Bootstrap completed successfully
    Completed { shares: Vec<SecretShare> },
    /// Bootstrap failed
    Failed { reason: String },
}

impl BootstrapState {
    /// Check if bootstrap is complete
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Check if bootstrap has failed
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Get the failure reason if failed
    pub fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::Failed { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Configuration for bootstrap behavior
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Timeout for challenge response (seconds)
    pub challenge_timeout_secs: u64,
    /// Timeout for bootstrap completion (seconds)
    pub bootstrap_timeout_secs: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Preferred key sharing mode
    pub preferred_mode: KeySharingMode,
    /// Providers to request keys for
    pub requested_providers: Vec<String>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            challenge_timeout_secs: 30,
            bootstrap_timeout_secs: 120,
            max_retries: 3,
            preferred_mode: KeySharingMode::Direct,
            requested_providers: vec!["anthropic".to_string(), "openai".to_string()],
        }
    }
}

/// Create a challenge response by signing the challenge nonce
pub fn create_challenge_response(
    challenge: &IdentityChallenge,
    privkey: &NodePrivKey,
) -> HoResult<IdentityChallengeResponse> {
    // Sign the nonce with our private key
    let signature = privkey.sign(Some(b"ERGORS_IDENTITY_CHALLENGE_V1"), &challenge.nonce);

    // Get our public key bytes
    let public_key = privkey.id().0;
    let pubkey_bytes = commonware_codec::Encode::encode(&public_key).to_vec();
    let sig_bytes = commonware_codec::Encode::encode(&signature).to_vec();

    Ok(IdentityChallengeResponse {
        challenge_id: challenge.challenge_id.clone(),
        signature: sig_bytes,
        public_key: pubkey_bytes,
    })
}

/// Verify a challenge response
pub fn verify_challenge_response(
    challenge: &IdentityChallenge,
    response: &IdentityChallengeResponse,
) -> HoResult<bool> {
    use commonware_codec::DecodeExt;
    use commonware_cryptography::Verifier;
    use std::io::Cursor;

    // Decode the public key
    let pubkey_bytes = &response.public_key;
    let mut cursor = Cursor::new(pubkey_bytes.as_slice());
    let public_key: commonware_cryptography::ed25519::PublicKey =
        commonware_cryptography::ed25519::PublicKey::decode(&mut cursor)
            .map_err(|e| HoError::Cfg(format!("Invalid public key: {}", e)))?;

    // Decode the signature
    let sig_bytes = &response.signature;
    let mut cursor = Cursor::new(sig_bytes.as_slice());
    let signature: commonware_cryptography::ed25519::Signature =
        commonware_cryptography::ed25519::Signature::decode(&mut cursor)
            .map_err(|e| HoError::Cfg(format!("Invalid signature: {}", e)))?;

    // Verify the signature using commonware_cryptography
    // Ed25519 verification: check that signature was created with the private key
    // corresponding to public_key over the challenge nonce
    let valid = public_key.verify(
        Some(b"ERGORS_IDENTITY_CHALLENGE_V1"),
        &challenge.nonce,
        &signature,
    );

    Ok(valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_challenge_response_roundtrip() {
        let privkey = NodePrivKey::new(&mut OsRng);

        let challenge = IdentityChallenge {
            challenge_id: "test-challenge-123".to_string(),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            challenged_pubkey: vec![],
            expires_at: None,
        };

        let response = create_challenge_response(&challenge, &privkey).unwrap();
        assert!(verify_challenge_response(&challenge, &response).unwrap());
    }

    #[test]
    fn test_wrong_key_fails_verification() {
        let privkey1 = NodePrivKey::new(&mut OsRng);
        let privkey2 = NodePrivKey::new(&mut OsRng);

        let challenge = IdentityChallenge {
            challenge_id: "test-challenge".to_string(),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8],
            challenged_pubkey: vec![],
            expires_at: None,
        };

        // Sign with privkey1
        let response = create_challenge_response(&challenge, &privkey1).unwrap();

        // Replace public key with privkey2's
        let wrong_pubkey = privkey2.id().0;
        let wrong_pubkey_bytes = commonware_codec::Encode::encode(&wrong_pubkey).to_vec();

        let tampered_response = IdentityChallengeResponse {
            challenge_id: response.challenge_id,
            signature: response.signature,
            public_key: wrong_pubkey_bytes,
        };

        // Verification should fail
        assert!(!verify_challenge_response(&challenge, &tampered_response).unwrap());
    }
}
