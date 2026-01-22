//! Secret Sharing Module
//!
//! Provides dual-mode secret sharing for secure API key distribution:
//!
//! ## Direct Mode (1-to-1)
//! Simple encrypted transfer between two nodes using ECDH + ChaCha20Poly1305.
//! Best for sharing a secret with a single recipient.
//!
//! ## Shamir Mode (n-of-m threshold)
//! Threshold secret sharing where k-of-n shares are needed to reconstruct.
//! Best for distributing secrets across multiple nodes with fault tolerance.
//!
//! # Example
//!
//! ```ignore
//! use ho_std::secret_sharing::{Secret, SharingMode, split_secret, reconstruct_secret};
//! use rand::rngs::OsRng;
//!
//! // Create a secret
//! let secret = Secret::from_str("sk-api-key-12345");
//!
//! // Split using Shamir (2-of-3 threshold)
//! let mode = SharingMode::shamir(2, 3);
//! let recipients = vec![pubkey1, pubkey2, pubkey3];
//! let shares = split_secret(&mut OsRng, &secret, mode, &recipients)?;
//!
//! // Reconstruct from any 2 shares
//! let decrypted = decrypt_and_collect_shares(&shares[0..2], &my_privkey)?;
//! let reconstructed = reconstruct_secret(&decrypted, mode)?;
//! ```

pub mod direct;
pub mod feldman;
pub mod shamir;
pub mod share;

pub use direct::{decrypt_from_sender, encrypt_for_recipient};
pub use feldman::{generate_salt, FeldmanCommitment};
pub use share::{DecryptedShare, EncryptedShare, Secret, Share, SharingMode};
pub use shamir::{reconstruct, split};

use crate::error::HoResult;
use crate::keys::commonware::{NodePrivKey, NodePubkey};
use rand_core::CryptoRngCore;
use tracing::debug;

/// Split a secret according to the sharing mode and encrypt for recipients
///
/// # Arguments
/// * `rng` - Cryptographically secure random number generator
/// * `secret` - The secret to share
/// * `mode` - Sharing mode (Direct or Shamir)
/// * `recipients` - Public keys of recipients (1 for Direct, n for Shamir)
///
/// # Returns
/// Vector of encrypted shares, one per recipient
pub fn split_secret(
    rng: &mut impl CryptoRngCore,
    secret: &Secret,
    mode: SharingMode,
    recipients: &[NodePubkey],
) -> HoResult<Vec<EncryptedShare>> {
    match mode {
        SharingMode::Direct => {
            // Direct mode: encrypt for single recipient
            if recipients.len() != 1 {
                return Err(crate::llm::HoError::Cfg(
                    "Direct mode requires exactly 1 recipient".to_string(),
                ));
            }

            let encrypted = direct::encrypt_for_recipient(rng, secret, &recipients[0])?;
            Ok(vec![EncryptedShare {
                index: 1,
                encrypted_value: encrypted,
                recipient_pubkey: recipients[0].0.encode(),
                mode,
                commitment: None,
            }])
        }
        SharingMode::Shamir { threshold, total } => {
            // Shamir mode: split into shares and encrypt each
            if recipients.len() < total as usize {
                return Err(crate::llm::HoError::Cfg(format!(
                    "Shamir mode requires {} recipients, got {}",
                    total,
                    recipients.len()
                )));
            }

            // Split the secret
            let shares = shamir::split(rng, secret, threshold, total)?;

            // Generate commitment for verifiable secret sharing
            // For simplicity, we create a single commitment hash for all shares
            let salt = feldman::generate_salt(rng);
            let share_values: Vec<Vec<u8>> = shares.iter().map(|s| s.value.clone()).collect();
            let commitment = FeldmanCommitment::new(&share_values, salt);
            let commitment_bytes = commitment.to_bytes();

            // Encrypt each share for its recipient
            let encrypted_shares: HoResult<Vec<EncryptedShare>> = shares
                .iter()
                .zip(recipients.iter())
                .map(|(share, recipient)| {
                    let share_secret = Secret::new(share.value.clone());
                    let encrypted = direct::encrypt_for_recipient(rng, &share_secret, recipient)?;

                    Ok(EncryptedShare {
                        index: share.index,
                        encrypted_value: encrypted,
                        recipient_pubkey: recipient.0.encode(),
                        mode,
                        commitment: Some(commitment_bytes.clone()),
                    })
                })
                .collect();

            debug!(
                "Split secret into {} Shamir shares (threshold {})",
                total, threshold
            );
            encrypted_shares
        }
    }
}

/// Decrypt a share using the recipient's private key
///
/// # Arguments
/// * `share` - The encrypted share
/// * `privkey` - Recipient's private key
///
/// # Returns
/// Decrypted share ready for reconstruction
pub fn decrypt_share(share: &EncryptedShare, privkey: &NodePrivKey) -> HoResult<DecryptedShare> {
    let decrypted = direct::decrypt_from_sender(&share.encrypted_value, privkey)?;
    Ok(DecryptedShare {
        index: share.index,
        value: decrypted.as_bytes().to_vec(),
    })
}

/// Reconstruct a secret from decrypted shares
///
/// # Arguments
/// * `shares` - Decrypted shares (at least threshold number for Shamir)
/// * `mode` - The sharing mode used when splitting
///
/// # Returns
/// The reconstructed secret
pub fn reconstruct_secret(shares: &[DecryptedShare], mode: SharingMode) -> HoResult<Secret> {
    match mode {
        SharingMode::Direct => {
            // Direct mode: just unwrap the single share
            if shares.len() != 1 {
                return Err(crate::llm::HoError::Cfg(format!(
                    "Direct mode expects 1 share, got {}",
                    shares.len()
                )));
            }
            Ok(Secret::new(shares[0].value.clone()))
        }
        SharingMode::Shamir { threshold, .. } => {
            // Shamir mode: reconstruct using Lagrange interpolation
            shamir::reconstruct(shares, threshold)
        }
    }
}

/// Encode trait for ed25519 types
trait Encode {
    fn encode(&self) -> Vec<u8>;
}

impl Encode for commonware_cryptography::ed25519::PublicKey {
    fn encode(&self) -> Vec<u8> {
        commonware_codec::Encode::encode(self).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_direct_mode() {
        let recipient_key = NodePrivKey::new(&mut OsRng);
        let recipient_pubkey = recipient_key.id();

        let secret = Secret::from_str("sk-test-api-key");
        let mode = SharingMode::direct();

        let shares = split_secret(&mut OsRng, &secret, mode, &[recipient_pubkey]).unwrap();
        assert_eq!(shares.len(), 1);

        let decrypted = decrypt_share(&shares[0], &recipient_key).unwrap();
        let reconstructed = reconstruct_secret(&[decrypted], mode).unwrap();

        assert_eq!(reconstructed.as_string().unwrap(), "sk-test-api-key");
    }

    #[test]
    fn test_shamir_mode() {
        // Create 3 recipient keys
        let keys: Vec<NodePrivKey> = (0..3).map(|_| NodePrivKey::new(&mut OsRng)).collect();
        let pubkeys: Vec<NodePubkey> = keys.iter().map(|k| k.id()).collect();

        let secret = Secret::from_str("sk-anthropic-secret-key");
        let mode = SharingMode::shamir(2, 3);

        let shares = split_secret(&mut OsRng, &secret, mode, &pubkeys).unwrap();
        assert_eq!(shares.len(), 3);

        // Decrypt any 2 shares
        let decrypted: Vec<DecryptedShare> = vec![
            decrypt_share(&shares[0], &keys[0]).unwrap(),
            decrypt_share(&shares[2], &keys[2]).unwrap(),
        ];

        let reconstructed = reconstruct_secret(&decrypted, mode).unwrap();
        assert_eq!(
            reconstructed.as_string().unwrap(),
            "sk-anthropic-secret-key"
        );
    }

    #[test]
    fn test_insufficient_shamir_shares() {
        let keys: Vec<NodePrivKey> = (0..3).map(|_| NodePrivKey::new(&mut OsRng)).collect();
        let pubkeys: Vec<NodePubkey> = keys.iter().map(|k| k.id()).collect();

        let secret = Secret::from_str("secret");
        let mode = SharingMode::shamir(2, 3);

        let shares = split_secret(&mut OsRng, &secret, mode, &pubkeys).unwrap();

        // Only decrypt 1 share (below threshold)
        let decrypted = vec![decrypt_share(&shares[0], &keys[0]).unwrap()];

        // Should fail with insufficient shares
        let result = reconstruct_secret(&decrypted, mode);
        assert!(result.is_err());
    }
}
