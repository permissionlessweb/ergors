//! API Key Distribution Manager
//!
//! Manages the distribution of API keys to authorized nodes using
//! the secret sharing protocol.
//!
//! ## Hybrid Ownership Model
//!
//! - **Shared providers** (anthropic, openai): Coordinator distributes via Shamir
//! - **Local providers** (ollama): Per-node, no distribution needed
//!
//! ## Flow
//!
//! ```text
//! Coordinator                          Authorized Nodes
//!     │                                      │
//!     │  1. Get master API key from storage  │
//!     │  2. Decrypt with node key            │
//!     │  3. Split using Shamir (k,n)         │
//!     │  4. Encrypt each share for recipient │
//!     │                                      │
//!     │────── Distribute Shares ────────────►│
//!     │                                      │
//!     │                      5. Decrypt share│
//!     │                      6. Reconstruct  │
//!     │                      7. Cache key    │
//! ```

pub mod startup;

pub use startup::{is_coordinator_node_type, KeyDistributionSystem};

use crate::bootstrap::BootstrapHandler;
use ho_std::ephemeral::EphemeralKeyManager;
use ho_std::error::{HoError, HoResult};
use ho_std::keys::commonware::{NodePrivKey, NodePubkey};
use ho_std::secret_sharing::{self, Secret, SharingMode};
use ho_std::types::ergors::network::v1::{
    KeySharingMode, ProviderOwnership, SecretShare, SecretSharingConfig,
};
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Provider configuration for key distribution
#[derive(Debug, Clone)]
pub struct ProviderDistributionConfig {
    /// Provider name (e.g., "anthropic", "openai")
    pub name: String,
    /// Ownership model (shared or local)
    pub ownership: ProviderOwnership,
    /// Shamir threshold (k shares needed)
    pub threshold: u8,
    /// Total shares to generate (n)
    pub total_shares: u8,
    /// Encrypted master key (in storage)
    pub encrypted_key: Option<Vec<u8>>,
}

impl Default for ProviderDistributionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        }
    }
}

/// API Key Distributor for coordinator nodes
///
/// Handles the secure distribution of API keys to authorized nodes
/// using configurable secret sharing (Shamir or Direct).
pub struct ApiKeyDistributor {
    /// Provider configurations
    provider_configs: RwLock<HashMap<String, ProviderDistributionConfig>>,
    /// Our node's private key
    node_privkey: Arc<NodePrivKey>,
    /// Bootstrap handler for challenge verification
    bootstrap_handler: Arc<BootstrapHandler>,
    /// Ephemeral key manager
    key_manager: Arc<EphemeralKeyManager>,
}

impl ApiKeyDistributor {
    /// Create a new API key distributor
    pub fn new(
        node_privkey: Arc<NodePrivKey>,
        bootstrap_handler: Arc<BootstrapHandler>,
        key_manager: Arc<EphemeralKeyManager>,
    ) -> Self {
        Self {
            provider_configs: RwLock::new(HashMap::new()),
            node_privkey,
            bootstrap_handler,
            key_manager,
        }
    }

    /// Configure a provider for distribution
    pub fn configure_provider(&self, config: ProviderDistributionConfig) {
        let name = config.name.clone();
        self.provider_configs
            .write()
            .unwrap()
            .insert(name.clone(), config.clone());

        // Also configure in bootstrap handler
        if let Some(ref encrypted_key) = config.encrypted_key {
            let sharing_config = SecretSharingConfig {
                mode: if config.ownership == ProviderOwnership::Local {
                    KeySharingMode::Direct.into()
                } else {
                    KeySharingMode::Shamir.into()
                },
                threshold: config.threshold as u32,
                total_shares: config.total_shares as u32,
            };
            self.bootstrap_handler.configure_provider(
                &name,
                encrypted_key.clone(),
                sharing_config,
            );
        }

        info!(
            "Configured provider '{}' for distribution (ownership: {:?}, threshold: {}/{})",
            name, config.ownership, config.threshold, config.total_shares
        );
    }

    /// Get provider configuration
    pub fn get_provider_config(&self, provider: &str) -> Option<ProviderDistributionConfig> {
        self.provider_configs.read().unwrap().get(provider).cloned()
    }

    /// List all configured providers
    pub fn list_providers(&self) -> Vec<String> {
        self.provider_configs.read().unwrap().keys().cloned().collect()
    }

    /// List shared providers (those that need distribution)
    pub fn list_shared_providers(&self) -> Vec<String> {
        self.provider_configs
            .read()
            .unwrap()
            .iter()
            .filter(|(_, config)| config.ownership == ProviderOwnership::Shared)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// List local providers (per-node only)
    pub fn list_local_providers(&self) -> Vec<String> {
        self.provider_configs
            .read()
            .unwrap()
            .iter()
            .filter(|(_, config)| config.ownership == ProviderOwnership::Local)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Generate shares for a provider
    ///
    /// # Arguments
    /// * `provider` - Provider name
    /// * `recipients` - Public keys of recipients
    ///
    /// # Returns
    /// Vector of encrypted shares, one per recipient
    pub fn generate_shares(
        &self,
        provider: &str,
        recipients: &[NodePubkey],
    ) -> HoResult<Vec<SecretShare>> {
        let config = self.get_provider_config(provider).ok_or_else(|| {
            HoError::Cfg(format!("Provider '{}' not configured", provider))
        })?;

        let encrypted_key = config.encrypted_key.ok_or_else(|| {
            HoError::Cfg(format!("No key configured for provider '{}'", provider))
        })?;

        // For local providers, use direct mode
        if config.ownership == ProviderOwnership::Local {
            return Err(HoError::Cfg(format!(
                "Provider '{}' is local-only, not distributed",
                provider
            )));
        }

        // Determine sharing mode based on config
        let mode = if recipients.len() == 1 {
            SharingMode::Direct
        } else {
            SharingMode::shamir(config.threshold, config.total_shares)
        };

        // The encrypted_key is the raw API key for now
        // In production, this would be decrypted from secure storage
        let secret = Secret::new(encrypted_key);

        // Split the secret
        let encrypted_shares =
            secret_sharing::split_secret(&mut OsRng, &secret, mode, recipients)?;

        // Convert to proto format
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
                    config: Some(SecretSharingConfig {
                        mode: mode_proto.into(),
                        threshold: config.threshold as u32,
                        total_shares: config.total_shares as u32,
                    }),
                }
            })
            .collect();

        debug!(
            "Generated {} shares for provider '{}' (mode: {:?})",
            proto_shares.len(),
            provider,
            mode
        );

        Ok(proto_shares)
    }

    /// Distribute API key to specific nodes
    ///
    /// Returns the shares to be sent via the network
    pub fn distribute_to_nodes(
        &self,
        provider: &str,
        authorized_nodes: &[NodePubkey],
    ) -> HoResult<Vec<(NodePubkey, SecretShare)>> {
        let shares = self.generate_shares(provider, authorized_nodes)?;

        // Pair each share with its recipient
        let distribution: Vec<(NodePubkey, SecretShare)> = shares
            .into_iter()
            .zip(authorized_nodes.iter().cloned())
            .map(|(share, recipient)| (recipient, share))
            .collect();

        info!(
            "Prepared distribution of '{}' to {} nodes",
            provider,
            distribution.len()
        );

        Ok(distribution)
    }

    /// Set/update an API key for a provider
    pub fn set_provider_key(&self, provider: &str, api_key: Vec<u8>) -> HoResult<()> {
        let mut configs = self.provider_configs.write().unwrap();

        if let Some(config) = configs.get_mut(provider) {
            config.encrypted_key = Some(api_key.clone());

            // Update bootstrap handler
            let sharing_config = SecretSharingConfig {
                mode: if config.ownership == ProviderOwnership::Local {
                    KeySharingMode::Direct.into()
                } else {
                    KeySharingMode::Shamir.into()
                },
                threshold: config.threshold as u32,
                total_shares: config.total_shares as u32,
            };
            self.bootstrap_handler.configure_provider(
                provider,
                api_key,
                sharing_config,
            );

            info!("Updated API key for provider '{}'", provider);
            Ok(())
        } else {
            Err(HoError::Cfg(format!("Provider '{}' not configured", provider)))
        }
    }

    /// Remove a provider's key (for rotation)
    pub fn remove_provider_key(&self, provider: &str) {
        if let Some(config) = self.provider_configs.write().unwrap().get_mut(provider) {
            config.encrypted_key = None;
            info!("Removed API key for provider '{}'", provider);
        }
    }

    /// Check if a provider has a key configured
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.provider_configs
            .read()
            .unwrap()
            .get(provider)
            .map(|c| c.encrypted_key.is_some())
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for ApiKeyDistributor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyDistributor")
            .field("providers", &self.list_providers())
            .field("shared_providers", &self.list_shared_providers())
            .field("local_providers", &self.list_local_providers())
            .finish()
    }
}

/// Create default provider configurations
pub fn default_provider_configs() -> Vec<ProviderDistributionConfig> {
    vec![
        ProviderDistributionConfig {
            name: "anthropic".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        },
        ProviderDistributionConfig {
            name: "openai".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        },
        ProviderDistributionConfig {
            name: "ollama".to_string(),
            ownership: ProviderOwnership::Local,
            threshold: 1,
            total_shares: 1,
            encrypted_key: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::BootstrapConfig;
    use ho_std::ephemeral::DEFAULT_TTL;

    fn create_test_distributor() -> ApiKeyDistributor {
        let privkey = Arc::new(NodePrivKey::new(&mut OsRng));
        let key_manager = Arc::new(EphemeralKeyManager::new(DEFAULT_TTL));
        let bootstrap_handler = Arc::new(BootstrapHandler::new(
            privkey.clone(),
            key_manager.clone(),
            BootstrapConfig::default(),
        ));
        ApiKeyDistributor::new(privkey, bootstrap_handler, key_manager)
    }

    #[test]
    fn test_configure_provider() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "anthropic".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: Some(b"sk-test-key".to_vec()),
        });

        assert!(distributor.get_provider_config("anthropic").is_some());
        assert!(distributor.has_provider_key("anthropic"));
    }

    #[test]
    fn test_list_providers() {
        let distributor = create_test_distributor();

        for config in default_provider_configs() {
            distributor.configure_provider(config);
        }

        assert_eq!(distributor.list_providers().len(), 3);
        assert!(distributor.list_shared_providers().contains(&"anthropic".to_string()));
        assert!(distributor.list_local_providers().contains(&"ollama".to_string()));
    }

    #[test]
    fn test_local_provider_no_distribution() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "ollama".to_string(),
            ownership: ProviderOwnership::Local,
            threshold: 1,
            total_shares: 1,
            encrypted_key: Some(b"local-key".to_vec()),
        });

        let recipient = NodePubkey::from_bytes(&[1u8; 32]);
        if let Some(recipient) = recipient {
            let result = distributor.generate_shares("ollama", &[recipient]);
            assert!(result.is_err()); // Local providers shouldn't be distributed
        }
    }

    #[test]
    fn test_set_and_remove_key() {
        let distributor = create_test_distributor();

        distributor.configure_provider(ProviderDistributionConfig {
            name: "test".to_string(),
            ownership: ProviderOwnership::Shared,
            threshold: 2,
            total_shares: 3,
            encrypted_key: None,
        });

        assert!(!distributor.has_provider_key("test"));

        distributor.set_provider_key("test", b"new-key".to_vec()).unwrap();
        assert!(distributor.has_provider_key("test"));

        distributor.remove_provider_key("test");
        assert!(!distributor.has_provider_key("test"));
    }
}
