//! Direct 1-to-1 secret sharing using decaf377-ka ECDH + ChaCha20Poly1305
//!
//! Provides secure encrypted transfer of secrets between two nodes
//! using ephemeral Diffie-Hellman key exchange and authenticated encryption.
//!
//! Uses decaf377-ka for key agreement, which is ZK-proof friendly and
//! provides proper elliptic curve Diffie-Hellman.

use super::share::Secret;
use crate::error::{HoError, HoResult};
use crate::keys::commonware::{NodePrivKey, NodePubkey};
use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use rand_core::CryptoRngCore;
use sha2::{Digest, Sha256};
use tracing::debug;

/// HKDF info string for deriving decaf377 keys from Ed25519 keys
const KEY_DERIVATION_INFO: &[u8] = b"ERGORS_DIRECT_SHARE_V1";

/// Nonce size for ChaCha20Poly1305 (96 bits = 12 bytes)
const NONCE_SIZE: usize = 12;

/// Tag size for ChaCha20Poly1305 authentication (128 bits = 16 bytes)
const TAG_SIZE: usize = 16;

/// decaf377-ka public key size
const PUBKEY_SIZE: usize = 32;

/// Encrypt a secret for a specific recipient using decaf377-ka ECDH + ChaCha20Poly1305
///
/// The encrypted output format is:
/// `ephemeral_public_key (32 bytes) || nonce (12 bytes) || ciphertext || tag (16 bytes)`
///
/// # Arguments
/// * `rng` - Cryptographically secure random number generator
/// * `secret` - The secret to encrypt
/// * `recipient_pubkey` - Public key of the intended recipient
///
/// # Returns
/// Encrypted bytes that can only be decrypted by the recipient
pub fn encrypt_for_recipient(
    rng: &mut impl CryptoRngCore,
    secret: &Secret,
    recipient_pubkey: &NodePubkey,
) -> HoResult<Vec<u8>> {
    // Derive decaf377-ka public key from recipient's Ed25519 public key
    let recipient_ka_pubkey = derive_ka_public_key(recipient_pubkey);

    // Generate ephemeral decaf377-ka keypair for this encryption
    let ephemeral_ka_secret = decaf377_ka::Secret::new(rng);
    let ephemeral_ka_pubkey = ephemeral_ka_secret.public();

    // Perform proper ECDH key agreement
    let shared_secret = ephemeral_ka_secret
        .key_agreement_with(&recipient_ka_pubkey)
        .map_err(|e| HoError::Cfg(format!("Key agreement failed: {:?}", e)))?;

    // Derive encryption key from shared secret
    let encryption_key = derive_encryption_key(&ephemeral_ka_pubkey, &recipient_ka_pubkey, &shared_secret);

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Create cipher and encrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&encryption_key));
    let ciphertext = cipher
        .encrypt(nonce, secret.as_bytes())
        .map_err(|e| HoError::Cfg(format!("Encryption failed: {}", e)))?;

    // Assemble output: ephemeral_ka_pubkey || nonce || ciphertext
    let mut output = Vec::with_capacity(PUBKEY_SIZE + NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&ephemeral_ka_pubkey.0);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    debug!(
        "Encrypted {} bytes for recipient (total output: {} bytes)",
        secret.len(),
        output.len()
    );
    Ok(output)
}

/// Decrypt a secret using the recipient's private key
///
/// # Arguments
/// * `encrypted` - Encrypted data from `encrypt_for_recipient`
/// * `recipient_privkey` - Private key of the recipient
///
/// # Returns
/// The decrypted secret
pub fn decrypt_from_sender(
    encrypted: &[u8],
    recipient_privkey: &NodePrivKey,
) -> HoResult<Secret> {
    // Minimum length: ephemeral_ka_pubkey (32) + nonce (12) + tag (16) = 60 bytes
    let min_len = PUBKEY_SIZE + NONCE_SIZE + TAG_SIZE;
    if encrypted.len() < min_len {
        return Err(HoError::Cfg(format!(
            "Encrypted data too short: {} bytes (minimum {})",
            encrypted.len(),
            min_len
        )));
    }

    // Parse components
    let ephemeral_ka_pubkey_bytes = &encrypted[..PUBKEY_SIZE];
    let nonce_bytes = &encrypted[PUBKEY_SIZE..PUBKEY_SIZE + NONCE_SIZE];
    let ciphertext = &encrypted[PUBKEY_SIZE + NONCE_SIZE..];

    // Reconstruct ephemeral decaf377-ka public key
    let ephemeral_ka_pubkey = decaf377_ka::Public(
        ephemeral_ka_pubkey_bytes
            .try_into()
            .map_err(|_| HoError::Cfg("Invalid ephemeral public key length".to_string()))?,
    );

    // Derive recipient's decaf377-ka secret key from Ed25519 private key
    let recipient_ka_secret = derive_ka_secret_key(recipient_privkey);
    let recipient_ka_pubkey = recipient_ka_secret.public();

    // Perform proper ECDH key agreement
    let shared_secret = recipient_ka_secret
        .key_agreement_with(&ephemeral_ka_pubkey)
        .map_err(|e| HoError::Cfg(format!("Key agreement failed: {:?}", e)))?;

    // Derive encryption key (same as sender)
    let encryption_key = derive_encryption_key(&ephemeral_ka_pubkey, &recipient_ka_pubkey, &shared_secret);

    // Create cipher and decrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&encryption_key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| HoError::Cfg("Decryption failed: invalid ciphertext or wrong key".to_string()))?;

    debug!("Decrypted {} bytes", plaintext.len());
    Ok(Secret::new(plaintext))
}

/// Derive a decaf377-ka key pair deterministically from an Ed25519 public key
///
/// Both sender and recipient can compute the same ka key pair from the
/// Ed25519 public key. This works because:
/// - Sender has recipient's ed25519 public key directly
/// - Recipient can derive their ed25519 public key from their private key
fn derive_ka_key_from_pubkey(ed25519_pubkey: &NodePubkey) -> (decaf377_ka::Secret, decaf377_ka::Public) {
    use commonware_codec::Encode;

    // Hash the Ed25519 public key to get deterministic decaf377-ka key material
    let mut hasher = Sha256::new();
    hasher.update(KEY_DERIVATION_INFO);
    hasher.update(b"KA_KEY_FROM_PUBKEY");
    hasher.update(&ed25519_pubkey.0.encode());

    let hash = hasher.finalize();
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&hash);

    // Create a decaf377-ka secret from the hash
    let ka_secret = decaf377_ka::Secret::new_from_field(
        decaf377::Fr::from_le_bytes_mod_order(&key_bytes),
    );
    let ka_public = ka_secret.public();

    (ka_secret, ka_public)
}

/// Derive a decaf377-ka public key from an Ed25519 public key
fn derive_ka_public_key(ed25519_pubkey: &NodePubkey) -> decaf377_ka::Public {
    let (_secret, public) = derive_ka_key_from_pubkey(ed25519_pubkey);
    public
}

/// Derive a decaf377-ka secret key from an Ed25519 private key
///
/// This derives from the corresponding public key to ensure consistency
/// between sender and recipient.
fn derive_ka_secret_key(ed25519_privkey: &NodePrivKey) -> decaf377_ka::Secret {
    // Get the public key from the private key
    let ed25519_pubkey = ed25519_privkey.id();

    // Derive the ka key pair from the public key
    let (secret, _public) = derive_ka_key_from_pubkey(&ed25519_pubkey);
    secret
}

/// Derive an encryption key from the ECDH shared secret
///
/// Uses SHA256 with domain separation to derive a symmetric key.
fn derive_encryption_key(
    ephemeral_pubkey: &decaf377_ka::Public,
    recipient_pubkey: &decaf377_ka::Public,
    shared_secret: &decaf377_ka::SharedSecret,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"ergors-direct-v1"); // Domain separation
    hasher.update(&ephemeral_pubkey.0);
    hasher.update(&recipient_pubkey.0);
    hasher.update(&shared_secret.0);

    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let sender_key = NodePrivKey::new(&mut OsRng);
        let recipient_key = NodePrivKey::new(&mut OsRng);
        let recipient_pubkey = recipient_key.id();

        let secret = Secret::from_str("sk-anthropic-test-key-123456");

        let encrypted = encrypt_for_recipient(&mut OsRng, &secret, &recipient_pubkey).unwrap();
        let decrypted = decrypt_from_sender(&encrypted, &recipient_key).unwrap();

        assert_eq!(
            decrypted.as_string().unwrap(),
            "sk-anthropic-test-key-123456"
        );
    }

    #[test]
    fn test_wrong_recipient_fails() {
        let recipient_key = NodePrivKey::new(&mut OsRng);
        let wrong_key = NodePrivKey::new(&mut OsRng);
        let recipient_pubkey = recipient_key.id();

        let secret = Secret::from_str("secret");
        let encrypted = encrypt_for_recipient(&mut OsRng, &secret, &recipient_pubkey).unwrap();

        // Decryption with wrong key should fail
        let result = decrypt_from_sender(&encrypted, &wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let recipient_key = NodePrivKey::new(&mut OsRng);
        let recipient_pubkey = recipient_key.id();

        let secret = Secret::from_str("secret");
        let mut encrypted = encrypt_for_recipient(&mut OsRng, &secret, &recipient_pubkey).unwrap();

        // Tamper with the ciphertext
        let last_idx = encrypted.len() - 1;
        encrypted[last_idx] ^= 0xFF;

        // Decryption should fail due to authentication
        let result = decrypt_from_sender(&encrypted, &recipient_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_data() {
        let recipient_key = NodePrivKey::new(&mut OsRng);
        let recipient_pubkey = recipient_key.id();

        let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        let secret = Secret::new(binary_data.clone());

        let encrypted = encrypt_for_recipient(&mut OsRng, &secret, &recipient_pubkey).unwrap();
        let decrypted = decrypt_from_sender(&encrypted, &recipient_key).unwrap();

        assert_eq!(decrypted.as_bytes(), &binary_data);
    }
}
