//! Ephemeral Key Management System
//!
//! Provides memory-only, time-limited keys for secure API key caching.
//!
//! ## Features
//!
//! - **Memory-only storage**: Keys never touch disk
//! - **Auto-zeroization**: Keys are securely cleared on drop
//! - **Time-limited**: Default 1-hour TTL with automatic cleanup
//! - **Provider mapping**: Quick lookup of keys by provider name
//! - **Thread-safe**: Safe for concurrent access
//!
//! ## Example
//!
//! ```ignore
//! use ho_std::ephemeral::{EphemeralKeyManager, EphemeralKeyScope};
//! use std::time::Duration;
//!
//! let manager = EphemeralKeyManager::new(Duration::from_secs(3600));
//! manager.start_cleanup_task();
//!
//! // Store a provider key
//! let key_id = manager.store_provider_key("anthropic", &decrypted_api_key, None)?;
//!
//! // Later, retrieve it
//! let api_key = manager.get_provider_key("anthropic")?;
//! ```

pub mod cache;
pub mod derivation;
pub mod key;

pub use cache::{CacheConfig, EphemeralKeyCache};
pub use derivation::{derive_ephemeral_key, derive_provider_key, derive_session_key};
pub use key::{EphemeralKey, EphemeralKeyScope};

use crate::error::{HoError, HoResult};
use rand_core::CryptoRngCore;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Default TTL for ephemeral keys (1 hour)
pub const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Default cleanup interval (60 seconds)
pub const DEFAULT_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Ephemeral key manager for API key caching
///
/// Provides a high-level interface for storing and retrieving
/// ephemeral keys with automatic cleanup of expired keys.
pub struct EphemeralKeyManager {
    /// The underlying key cache
    cache: Arc<EphemeralKeyCache>,
    /// Default TTL for new keys
    default_ttl: Duration,
    /// Whether the cleanup task is running
    cleanup_running: Arc<AtomicBool>,
    /// Shutdown signal for cleanup task
    shutdown: Arc<AtomicBool>,
}

impl EphemeralKeyManager {
    /// Create a new ephemeral key manager with default TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self::with_config(CacheConfig {
            default_ttl,
            ..Default::default()
        })
    }

    /// Create a new ephemeral key manager with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        let default_ttl = config.default_ttl;
        Self {
            cache: Arc::new(EphemeralKeyCache::with_config(config)),
            default_ttl,
            cleanup_running: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the default TTL
    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }

    /// Get access to the underlying cache
    pub fn cache(&self) -> &EphemeralKeyCache {
        &self.cache
    }

    /// Start the background cleanup task
    ///
    /// This spawns a background thread that periodically removes expired keys.
    /// The task will run until the manager is dropped or `stop_cleanup_task` is called.
    pub fn start_cleanup_task(&self) {
        if self
            .cleanup_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            debug!("Cleanup task already running");
            return;
        }

        let cache = Arc::clone(&self.cache);
        let shutdown = Arc::clone(&self.shutdown);
        let cleanup_running = Arc::clone(&self.cleanup_running);
        let interval = self.cache.config().cleanup_interval;

        std::thread::spawn(move || {
            info!("Ephemeral key cleanup task started (interval: {:?})", interval);

            while !shutdown.load(Ordering::SeqCst) {
                std::thread::sleep(interval);

                if shutdown.load(Ordering::SeqCst) {
                    break;
                }

                let removed = cache.cleanup_expired();
                if removed > 0 {
                    debug!("Cleanup task removed {} expired keys", removed);
                }
            }

            cleanup_running.store(false, Ordering::SeqCst);
            info!("Ephemeral key cleanup task stopped");
        });
    }

    /// Stop the background cleanup task
    pub fn stop_cleanup_task(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Check if the cleanup task is running
    pub fn is_cleanup_running(&self) -> bool {
        self.cleanup_running.load(Ordering::SeqCst)
    }

    /// Generate a unique key ID
    fn generate_key_id(&self, rng: &mut impl CryptoRngCore, prefix: &str) -> String {
        let mut random_bytes = [0u8; 16];
        rng.fill_bytes(&mut random_bytes);
        format!("{}-{}", prefix, hex::encode(random_bytes))
    }

    /// Store a provider API key
    ///
    /// # Arguments
    /// * `rng` - Random number generator for key ID generation
    /// * `provider` - Provider name (e.g., "anthropic", "openai")
    /// * `key_material` - The raw API key bytes
    /// * `ttl` - Optional custom TTL (uses default if None)
    ///
    /// # Returns
    /// The generated key ID
    pub fn store_provider_key(
        &self,
        rng: &mut impl CryptoRngCore,
        provider: &str,
        key_material: &[u8],
        ttl: Option<Duration>,
    ) -> HoResult<String> {
        let key_id = self.generate_key_id(rng, &format!("provider-{}", provider));
        let ttl = ttl.unwrap_or(self.default_ttl);

        let key = EphemeralKey::new(
            key_id.clone(),
            key_material.to_vec(),
            ttl,
            EphemeralKeyScope::Provider,
            Some(provider.to_string()),
        );

        if self.cache.store(key) {
            debug!("Stored provider key for '{}' with TTL {:?}", provider, ttl);
            Ok(key_id)
        } else {
            Err(HoError::Cfg("Ephemeral key cache full".to_string()))
        }
    }

    /// Get a provider API key
    ///
    /// # Arguments
    /// * `provider` - Provider name
    ///
    /// # Returns
    /// The API key bytes if available and not expired
    pub fn get_provider_key(&self, provider: &str) -> Option<Vec<u8>> {
        self.cache.get_by_provider(provider)
    }

    /// Check if a provider has a valid key
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.cache.has_provider_key(provider)
    }

    /// Store a session key
    ///
    /// # Arguments
    /// * `rng` - Random number generator
    /// * `key_material` - The session key bytes
    /// * `ttl` - Optional custom TTL
    ///
    /// # Returns
    /// The generated key ID
    pub fn store_session_key(
        &self,
        rng: &mut impl CryptoRngCore,
        key_material: &[u8],
        ttl: Option<Duration>,
    ) -> HoResult<String> {
        let key_id = self.generate_key_id(rng, "session");
        let ttl = ttl.unwrap_or(self.default_ttl);

        let key = EphemeralKey::new(
            key_id.clone(),
            key_material.to_vec(),
            ttl,
            EphemeralKeyScope::Session,
            None,
        );

        if self.cache.store(key) {
            debug!("Stored session key with TTL {:?}", ttl);
            Ok(key_id)
        } else {
            Err(HoError::Cfg("Ephemeral key cache full".to_string()))
        }
    }

    /// Get a key by its ID
    pub fn get_key(&self, key_id: &str) -> Option<Vec<u8>> {
        self.cache.get(key_id)
    }

    /// Remove a key by its ID
    pub fn remove_key(&self, key_id: &str) -> bool {
        self.cache.remove(key_id)
    }

    /// Remove a provider's key
    pub fn remove_provider_key(&self, provider: &str) -> bool {
        self.cache.remove_by_provider(provider)
    }

    /// Invalidate all keys
    ///
    /// Use this when the node is shutting down or when security requires
    /// clearing all cached keys.
    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
        info!("All ephemeral keys invalidated");
    }

    /// Get the number of cached keys
    pub fn key_count(&self) -> usize {
        self.cache.len()
    }

    /// Get the number of valid (non-expired) keys
    pub fn valid_key_count(&self) -> usize {
        self.cache.valid_count()
    }

    /// List all providers with keys
    pub fn list_providers(&self) -> Vec<String> {
        self.cache.list_providers()
    }

    /// Derive and store a provider key from a master secret
    ///
    /// # Arguments
    /// * `rng` - Random number generator
    /// * `master_secret` - The master secret to derive from
    /// * `provider` - Provider name
    /// * `ttl` - Optional custom TTL
    ///
    /// # Returns
    /// The generated key ID
    pub fn derive_and_store_provider_key(
        &self,
        rng: &mut impl CryptoRngCore,
        master_secret: &[u8],
        provider: &str,
        ttl: Option<Duration>,
    ) -> HoResult<String> {
        let derived = derive_provider_key(master_secret, provider);
        self.store_provider_key(rng, provider, &derived, ttl)
    }
}

impl Drop for EphemeralKeyManager {
    fn drop(&mut self) {
        // Signal cleanup task to stop
        self.shutdown.store(true, Ordering::SeqCst);
        // Invalidate all keys for security
        self.cache.invalidate_all();
    }
}

impl std::fmt::Debug for EphemeralKeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EphemeralKeyManager")
            .field("key_count", &self.key_count())
            .field("valid_count", &self.valid_key_count())
            .field("default_ttl", &self.default_ttl)
            .field("cleanup_running", &self.is_cleanup_running())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_store_and_get_provider_key() {
        let manager = EphemeralKeyManager::new(DEFAULT_TTL);
        let api_key = b"sk-anthropic-test-key-123456";

        let key_id = manager
            .store_provider_key(&mut OsRng, "anthropic", api_key, None)
            .unwrap();

        assert!(manager.has_provider_key("anthropic"));
        assert_eq!(
            manager.get_provider_key("anthropic"),
            Some(api_key.to_vec())
        );

        // Also retrievable by key_id
        assert_eq!(manager.get_key(&key_id), Some(api_key.to_vec()));
    }

    #[test]
    fn test_multiple_providers() {
        let manager = EphemeralKeyManager::new(DEFAULT_TTL);

        manager
            .store_provider_key(&mut OsRng, "anthropic", b"key-1", None)
            .unwrap();
        manager
            .store_provider_key(&mut OsRng, "openai", b"key-2", None)
            .unwrap();

        assert!(manager.has_provider_key("anthropic"));
        assert!(manager.has_provider_key("openai"));
        assert_eq!(manager.list_providers().len(), 2);
    }

    #[test]
    fn test_remove_provider_key() {
        let manager = EphemeralKeyManager::new(DEFAULT_TTL);

        manager
            .store_provider_key(&mut OsRng, "anthropic", b"key-1", None)
            .unwrap();

        assert!(manager.remove_provider_key("anthropic"));
        assert!(!manager.has_provider_key("anthropic"));
    }

    #[test]
    fn test_invalidate_all() {
        let manager = EphemeralKeyManager::new(DEFAULT_TTL);

        manager
            .store_provider_key(&mut OsRng, "anthropic", b"key-1", None)
            .unwrap();
        manager
            .store_provider_key(&mut OsRng, "openai", b"key-2", None)
            .unwrap();

        manager.invalidate_all();

        assert_eq!(manager.key_count(), 0);
        assert!(!manager.has_provider_key("anthropic"));
        assert!(!manager.has_provider_key("openai"));
    }

    #[test]
    fn test_derive_and_store() {
        let manager = EphemeralKeyManager::new(DEFAULT_TTL);
        let master_secret = b"master-secret-for-testing";

        manager
            .derive_and_store_provider_key(&mut OsRng, master_secret, "anthropic", None)
            .unwrap();

        assert!(manager.has_provider_key("anthropic"));

        // Derived key should be 32 bytes
        let key = manager.get_provider_key("anthropic").unwrap();
        assert_eq!(key.len(), 32);
    }
}
