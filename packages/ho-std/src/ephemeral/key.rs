//! Ephemeral key types with memory-only storage and auto-zeroization
//!
//! Keys are stored only in memory and automatically zeroed on drop.
//! They have a configurable TTL (default 1 hour) after which they become invalid.

use std::time::Instant;

/// Scope of an ephemeral key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EphemeralKeyScope {
    /// Key for a specific API provider
    Provider,
    /// Key for node-to-node communication
    NodeComm,
    /// Key for session encryption
    Session,
}

/// An ephemeral key that exists only in memory and auto-zeroizes on drop
pub struct EphemeralKey {
    /// Unique identifier for this key
    key_id: String,
    /// The key material (sensitive - zeroized on drop)
    key_material: Vec<u8>,
    /// When this key was created
    created_at: Instant,
    /// When this key expires
    expires_at: Instant,
    /// Scope/purpose of this key
    scope: EphemeralKeyScope,
    /// Associated provider (if scope is Provider)
    provider: Option<String>,
}

impl EphemeralKey {
    /// Create a new ephemeral key
    ///
    /// # Arguments
    /// * `key_id` - Unique identifier for this key
    /// * `key_material` - The raw key bytes
    /// * `ttl` - Time-to-live duration
    /// * `scope` - Purpose of this key
    /// * `provider` - Optional provider name (for Provider scope)
    pub fn new(
        key_id: String,
        key_material: Vec<u8>,
        ttl: std::time::Duration,
        scope: EphemeralKeyScope,
        provider: Option<String>,
    ) -> Self {
        let now = Instant::now();
        Self {
            key_id,
            key_material,
            created_at: now,
            expires_at: now + ttl,
            scope,
            provider,
        }
    }

    /// Get the key ID
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Check if this key has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    /// Get the key material if not expired
    ///
    /// Returns None if the key has expired
    pub fn key_material(&self) -> Option<&[u8]> {
        if self.is_expired() {
            None
        } else {
            Some(&self.key_material)
        }
    }

    /// Get the key material without checking expiry
    ///
    /// Use with caution - prefer `key_material()` for normal access
    pub fn key_material_unchecked(&self) -> &[u8] {
        &self.key_material
    }

    /// Get the scope of this key
    pub fn scope(&self) -> EphemeralKeyScope {
        self.scope
    }

    /// Get the associated provider (if any)
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// Get when this key was created
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get when this key expires
    pub fn expires_at(&self) -> Instant {
        self.expires_at
    }

    /// Get remaining time until expiry
    pub fn remaining_ttl(&self) -> Option<std::time::Duration> {
        let now = Instant::now();
        if now >= self.expires_at {
            None
        } else {
            Some(self.expires_at - now)
        }
    }
}

impl Drop for EphemeralKey {
    fn drop(&mut self) {
        // Securely clear the key material
        for byte in &mut self.key_material {
            *byte = 0;
        }
        // Clear the key_id as well
        self.key_id.clear();
        if let Some(ref mut p) = self.provider {
            p.clear();
        }
    }
}

impl std::fmt::Debug for EphemeralKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EphemeralKey")
            .field("key_id", &self.key_id)
            .field("key_len", &self.key_material.len())
            .field("scope", &self.scope)
            .field("provider", &self.provider)
            .field("is_expired", &self.is_expired())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_key_not_expired() {
        let key = EphemeralKey::new(
            "test-key".to_string(),
            vec![1, 2, 3, 4],
            Duration::from_secs(3600),
            EphemeralKeyScope::Provider,
            Some("anthropic".to_string()),
        );

        assert!(!key.is_expired());
        assert!(key.key_material().is_some());
        assert_eq!(key.key_material().unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn test_key_expired() {
        let key = EphemeralKey::new(
            "test-key".to_string(),
            vec![1, 2, 3, 4],
            Duration::from_millis(1),
            EphemeralKeyScope::Provider,
            None,
        );

        // Sleep to let it expire
        std::thread::sleep(Duration::from_millis(5));

        assert!(key.is_expired());
        assert!(key.key_material().is_none());
    }

    #[test]
    fn test_key_zeroization() {
        let key_material = vec![0xAA; 32];
        let _ptr = key_material.as_ptr();

        {
            let _key = EphemeralKey::new(
                "test-key".to_string(),
                key_material,
                Duration::from_secs(3600),
                EphemeralKeyScope::Session,
                None,
            );
            // Key is dropped here
        }

        // Note: This test is best-effort - we can't guarantee memory isn't reused
        // In a real scenario, use a memory sanitizer to verify zeroization
    }
}
