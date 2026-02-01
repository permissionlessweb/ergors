//! Gateway cryptography utilities.
//!
//! Shared encryption/decryption functions for gateway secrets.
//! Uses ChaCha20Poly1305 with node pubkey-derived keys.

use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Encryption method identifier for gateway secrets
pub const GATEWAY_SECRET_ENCRYPTION_METHOD: &str = "node_key_chacha20poly1305_v1";

/// Derives a 256-bit encryption key from the node's public key.
/// This provides at-rest encryption without requiring runtime passwords.
/// Security relies on access control and audit logging.
pub fn derive_gateway_encryption_key(node_pubkey: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"gateway_secret_encryption_v1:");
    hasher.update(node_pubkey);
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a secret value using the node's public key as the encryption key source.
/// Returns (encrypted_data, nonce).
pub fn encrypt_gateway_secret(value: &str, node_pubkey: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let key = derive_gateway_encryption_key(node_pubkey);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok((encrypted, nonce_bytes.to_vec()))
}

/// Decrypt a secret value using the node's public key as the encryption key source.
pub fn decrypt_gateway_secret(encrypted: &[u8], nonce: &[u8], node_pubkey: &[u8]) -> Result<String, String> {
    if nonce.len() != 12 {
        return Err("Invalid nonce size".to_string());
    }

    let key = derive_gateway_encryption_key(node_pubkey);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(nonce);

    let decrypted = cipher
        .decrypt(nonce, encrypted)
        .map_err(|_| "Decryption failed - corrupted data or wrong key".to_string())?;

    String::from_utf8(decrypted).map_err(|_| "Decrypted data is not valid UTF-8".to_string())
}
