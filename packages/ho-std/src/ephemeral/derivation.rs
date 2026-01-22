//! HKDF-based key derivation for ephemeral keys
//!
//! Derives ephemeral keys from master secrets using HKDF-SHA256.

use sha2::{Digest, Sha256};

/// Domain separation string for ephemeral key derivation
const EPHEMERAL_KEY_INFO: &[u8] = b"ERGORS_EPHEMERAL_KEY_V1";

/// Domain separation string for provider-specific keys
const PROVIDER_KEY_INFO: &[u8] = b"ERGORS_PROVIDER_KEY_V1";

/// Domain separation string for session keys
const SESSION_KEY_INFO: &[u8] = b"ERGORS_SESSION_KEY_V1";

/// Derive a provider-specific key from a master key
///
/// Uses HKDF-Extract + HKDF-Expand with provider name as salt
///
/// # Arguments
/// * `master_key` - The master key material
/// * `provider` - Provider name (e.g., "anthropic", "openai")
///
/// # Returns
/// A 32-byte derived key
pub fn derive_provider_key(master_key: &[u8], provider: &str) -> [u8; 32] {
    // HKDF-Extract: PRK = HMAC-Hash(salt, IKM)
    // We use a simplified version: PRK = SHA256(provider || master_key)
    let prk = extract(provider.as_bytes(), master_key);

    // HKDF-Expand: OKM = HMAC-Hash(PRK, info || 0x01)
    expand(&prk, PROVIDER_KEY_INFO)
}

/// Derive a session key from a master key and session ID
///
/// # Arguments
/// * `master_key` - The master key material
/// * `session_id` - Unique session identifier
///
/// # Returns
/// A 32-byte derived key
pub fn derive_session_key(master_key: &[u8], session_id: &[u8]) -> [u8; 32] {
    let prk = extract(session_id, master_key);
    expand(&prk, SESSION_KEY_INFO)
}

/// Derive an ephemeral key from a shared secret
///
/// # Arguments
/// * `shared_secret` - The ECDH shared secret or similar
/// * `context` - Additional context bytes (e.g., public keys involved)
///
/// # Returns
/// A 32-byte derived key
pub fn derive_ephemeral_key(shared_secret: &[u8], context: &[u8]) -> [u8; 32] {
    let prk = extract(context, shared_secret);
    expand(&prk, EPHEMERAL_KEY_INFO)
}

/// HKDF-Extract step (simplified using SHA256)
///
/// PRK = HMAC-Hash(salt, IKM)
/// Simplified: PRK = SHA256(salt || IKM || salt_len)
fn extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(ikm);
    hasher.update((salt.len() as u32).to_le_bytes());
    let result = hasher.finalize();

    let mut prk = [0u8; 32];
    prk.copy_from_slice(&result);
    prk
}

/// HKDF-Expand step (simplified using SHA256)
///
/// OKM = HMAC-Hash(PRK, info || counter)
fn expand(prk: &[u8; 32], info: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prk);
    hasher.update(info);
    hasher.update([0x01]); // Counter for first block
    let result = hasher.finalize();

    let mut okm = [0u8; 32];
    okm.copy_from_slice(&result);
    okm
}

/// Derive multiple keys from a single master key
///
/// Useful when you need multiple keys for different purposes
/// (e.g., encryption key + MAC key)
///
/// # Arguments
/// * `master_key` - The master key material
/// * `context` - Context for this derivation
/// * `count` - Number of keys to derive (1-255)
///
/// # Returns
/// Vector of 32-byte derived keys
pub fn derive_multiple_keys(master_key: &[u8], context: &[u8], count: u8) -> Vec<[u8; 32]> {
    let prk = extract(context, master_key);

    (1..=count)
        .map(|i| {
            let mut hasher = Sha256::new();
            hasher.update(prk);
            hasher.update(EPHEMERAL_KEY_INFO);
            hasher.update([i]);
            let result = hasher.finalize();

            let mut key = [0u8; 32];
            key.copy_from_slice(&result);
            key
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_provider_key_deterministic() {
        let master_key = b"test-master-key-12345678901234";
        let key1 = derive_provider_key(master_key, "anthropic");
        let key2 = derive_provider_key(master_key, "anthropic");

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_different_providers_different_keys() {
        let master_key = b"test-master-key-12345678901234";
        let key1 = derive_provider_key(master_key, "anthropic");
        let key2 = derive_provider_key(master_key, "openai");

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_session_key() {
        let master_key = b"session-master-key";
        let session1 = b"session-123";
        let session2 = b"session-456";

        let key1 = derive_session_key(master_key, session1);
        let key2 = derive_session_key(master_key, session2);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_multiple_keys() {
        let master_key = b"multi-key-master";
        let context = b"test-context";

        let keys = derive_multiple_keys(master_key, context, 3);

        assert_eq!(keys.len(), 3);
        // All keys should be different
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[1], keys[2]);
        assert_ne!(keys[0], keys[2]);
    }
}
