//! Ephemeral key cache with automatic cleanup
//!
//! Provides thread-safe storage for ephemeral keys with periodic cleanup
//! of expired entries.

use super::key::{EphemeralKey, EphemeralKeyScope};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for the ephemeral key cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of keys to store
    pub max_keys: usize,
    /// Default TTL for new keys
    pub default_ttl: Duration,
    /// How often to run the cleanup task
    pub cleanup_interval: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_keys: 100,
            default_ttl: Duration::from_secs(3600), // 1 hour
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// Thread-safe cache for ephemeral keys
pub struct EphemeralKeyCache {
    /// Keys indexed by key_id
    keys: RwLock<HashMap<String, EphemeralKey>>,
    /// Provider to key_id mapping for quick lookup
    provider_keys: RwLock<HashMap<String, String>>,
    /// Cache configuration
    config: CacheConfig,
}

impl EphemeralKeyCache {
    /// Create a new ephemeral key cache with default configuration
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a new ephemeral key cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
            provider_keys: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Get the cache configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Store a key in the cache
    ///
    /// Returns false if the cache is full and cannot evict expired keys
    pub fn store(&self, key: EphemeralKey) -> bool {
        let key_id = key.key_id().to_string();
        let provider = key.provider().map(String::from);

        let mut keys = self.keys.write().unwrap();

        // Check capacity
        if keys.len() >= self.config.max_keys && !keys.contains_key(&key_id) {
            // Try to evict expired keys first
            let expired: Vec<String> = keys
                .iter()
                .filter(|(_, k)| k.is_expired())
                .map(|(id, _)| id.clone())
                .collect();

            for id in expired {
                keys.remove(&id);
            }

            if keys.len() >= self.config.max_keys {
                warn!(
                    "Ephemeral key cache full ({} keys), cannot store new key",
                    keys.len()
                );
                return false;
            }
        }

        // Update provider mapping if applicable
        if let Some(ref p) = provider {
            let mut provider_keys = self.provider_keys.write().unwrap();
            provider_keys.insert(p.clone(), key_id.clone());
        }

        keys.insert(key_id.clone(), key);
        debug!("Stored ephemeral key: {}", key_id);
        true
    }

    /// Get a key by its ID
    ///
    /// Returns None if the key doesn't exist or is expired
    pub fn get(&self, key_id: &str) -> Option<Vec<u8>> {
        let keys = self.keys.read().unwrap();
        keys.get(key_id)
            .and_then(|k| k.key_material().map(|m| m.to_vec()))
    }

    /// Get a key by provider name
    ///
    /// Returns None if no key exists for this provider or it's expired
    pub fn get_by_provider(&self, provider: &str) -> Option<Vec<u8>> {
        let provider_keys = self.provider_keys.read().unwrap();
        let key_id = provider_keys.get(provider)?;

        let keys = self.keys.read().unwrap();
        keys.get(key_id)
            .and_then(|k| k.key_material().map(|m| m.to_vec()))
    }

    /// Check if a key exists and is valid
    pub fn contains(&self, key_id: &str) -> bool {
        let keys = self.keys.read().unwrap();
        keys.get(key_id).map(|k| !k.is_expired()).unwrap_or(false)
    }

    /// Check if a provider has a valid key
    pub fn has_provider_key(&self, provider: &str) -> bool {
        let provider_keys = self.provider_keys.read().unwrap();
        if let Some(key_id) = provider_keys.get(provider) {
            self.contains(key_id)
        } else {
            false
        }
    }

    /// Remove a key by its ID
    pub fn remove(&self, key_id: &str) -> bool {
        let mut keys = self.keys.write().unwrap();
        if let Some(key) = keys.remove(key_id) {
            // Also remove from provider mapping
            if let Some(provider) = key.provider() {
                let mut provider_keys = self.provider_keys.write().unwrap();
                provider_keys.remove(provider);
            }
            debug!("Removed ephemeral key: {}", key_id);
            true
        } else {
            false
        }
    }

    /// Remove a key by provider name
    pub fn remove_by_provider(&self, provider: &str) -> bool {
        let key_id = {
            let provider_keys = self.provider_keys.read().unwrap();
            provider_keys.get(provider).cloned()
        };

        if let Some(key_id) = key_id {
            self.remove(&key_id)
        } else {
            false
        }
    }

    /// Remove all expired keys
    ///
    /// Returns the number of keys removed
    pub fn cleanup_expired(&self) -> usize {
        let mut keys = self.keys.write().unwrap();
        let mut provider_keys = self.provider_keys.write().unwrap();

        let expired: Vec<(String, Option<String>)> = keys
            .iter()
            .filter(|(_, k)| k.is_expired())
            .map(|(id, k)| (id.clone(), k.provider().map(String::from)))
            .collect();

        let count = expired.len();

        for (id, provider) in expired {
            keys.remove(&id);
            if let Some(p) = provider {
                provider_keys.remove(&p);
            }
        }

        if count > 0 {
            debug!("Cleaned up {} expired ephemeral keys", count);
        }

        count
    }

    /// Invalidate all keys
    ///
    /// Clears the entire cache
    pub fn invalidate_all(&self) {
        let mut keys = self.keys.write().unwrap();
        let mut provider_keys = self.provider_keys.write().unwrap();

        let count = keys.len();
        keys.clear();
        provider_keys.clear();

        debug!("Invalidated {} ephemeral keys", count);
    }

    /// Get the number of keys in the cache
    pub fn len(&self) -> usize {
        self.keys.read().unwrap().len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.keys.read().unwrap().is_empty()
    }

    /// Get the number of valid (non-expired) keys
    pub fn valid_count(&self) -> usize {
        self.keys
            .read()
            .unwrap()
            .values()
            .filter(|k| !k.is_expired())
            .count()
    }

    /// List all key IDs (for debugging)
    pub fn list_key_ids(&self) -> Vec<String> {
        self.keys.read().unwrap().keys().cloned().collect()
    }

    /// List all providers with keys (for debugging)
    pub fn list_providers(&self) -> Vec<String> {
        self.provider_keys.read().unwrap().keys().cloned().collect()
    }
}

impl Default for EphemeralKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EphemeralKeyCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EphemeralKeyCache")
            .field("key_count", &self.len())
            .field("valid_count", &self.valid_count())
            .field("max_keys", &self.config.max_keys)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_key(id: &str, provider: Option<&str>) -> EphemeralKey {
        EphemeralKey::new(
            id.to_string(),
            vec![1, 2, 3, 4],
            Duration::from_secs(3600),
            EphemeralKeyScope::Provider,
            provider.map(String::from),
        )
    }

    #[test]
    fn test_store_and_get() {
        let cache = EphemeralKeyCache::new();
        let key = create_test_key("test-1", Some("anthropic"));

        assert!(cache.store(key));
        assert!(cache.contains("test-1"));
        assert_eq!(cache.get("test-1"), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_get_by_provider() {
        let cache = EphemeralKeyCache::new();
        let key = create_test_key("test-1", Some("anthropic"));

        cache.store(key);

        assert!(cache.has_provider_key("anthropic"));
        assert_eq!(cache.get_by_provider("anthropic"), Some(vec![1, 2, 3, 4]));
        assert!(!cache.has_provider_key("openai"));
    }

    #[test]
    fn test_remove() {
        let cache = EphemeralKeyCache::new();
        let key = create_test_key("test-1", Some("anthropic"));

        cache.store(key);
        assert!(cache.remove("test-1"));
        assert!(!cache.contains("test-1"));
        assert!(!cache.has_provider_key("anthropic"));
    }

    #[test]
    fn test_cleanup_expired() {
        let config = CacheConfig {
            max_keys: 100,
            default_ttl: Duration::from_millis(1),
            cleanup_interval: Duration::from_secs(60),
        };
        let cache = EphemeralKeyCache::with_config(config);

        // Store a key that will expire quickly
        let key = EphemeralKey::new(
            "expiring".to_string(),
            vec![1, 2, 3],
            Duration::from_millis(1),
            EphemeralKeyScope::Provider,
            None,
        );
        cache.store(key);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(5));

        let cleaned = cache.cleanup_expired();
        assert_eq!(cleaned, 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_capacity_limit() {
        let config = CacheConfig {
            max_keys: 2,
            default_ttl: Duration::from_secs(3600),
            cleanup_interval: Duration::from_secs(60),
        };
        let cache = EphemeralKeyCache::with_config(config);

        cache.store(create_test_key("key-1", None));
        cache.store(create_test_key("key-2", None));

        // Third key should fail
        assert!(!cache.store(create_test_key("key-3", None)));
        assert_eq!(cache.len(), 2);
    }
}
