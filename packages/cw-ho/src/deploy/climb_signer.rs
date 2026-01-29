//! Layer-climb integration for Akash transaction signing.
//!
//! Provides a clean interface to create layer-climb SigningClients
//! from our EncryptedCosmosKeyManager.

use anyhow::{anyhow, Result};
use bip32::DerivationPath;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::{AkashDeployConfig, CosmosKeyStore};
use layer_climb::prelude::*;
use layer_climb::transaction::{SequenceStrategy, SequenceStrategyKind};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::endpoint_manager::{EndpointManager, EndpointType};

/// Create a layer-climb SigningClient with automatic endpoint failover.
///
/// This version uses the EndpointManager to try multiple endpoints if connection fails.
/// Recommended for production use.
pub async fn create_signing_client_with_failover(
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    key_name: &str,
    account_index: u32,
    akash_config: &AkashDeployConfig,
) -> Result<SigningClient> {
    let endpoint_manager = EndpointManager::from_config(akash_config);

    // Get the mnemonic (do this once, not in retry loop)
    let mnemonic = {
        let mut key_manager_guard = key_manager.write().await;
        let key_store_guard = key_store.read().await;

        if !key_manager_guard.is_unlocked() {
            return Err(anyhow!(
                "Key manager is locked - must unlock before signing"
            ));
        }

        // Find the key
        let encrypted_key = key_store_guard
            .keys
            .iter()
            .find(|k| k.key_name == key_name)
            .ok_or_else(|| anyhow!("Key '{}' not found", key_name))?;

        // Decrypt the mnemonic
        key_manager_guard.decrypt_mnemonic(encrypted_key)?
    };

    // Prepare derivation path (will be recreated in each retry)
    let derivation_path_str = format!("m/44'/118'/0'/0/{}", account_index);
    let derivation_path: DerivationPath = derivation_path_str
        .parse()
        .map_err(|e| anyhow!("Invalid derivation path '{}': {:?}", derivation_path_str, e))?;

    // Try to create SigningClient with endpoint failover
    let client = endpoint_manager
        .execute_with_failover(EndpointType::Grpc, |grpc_endpoint| {
            let mnemonic_clone = mnemonic.clone();
            let derivation_path_clone = derivation_path.clone();
            let chain_id = akash_config.chain_id.clone();
            let rpc_endpoints = akash_config.rpc_endpoints.clone();
            async move {
                // Create KeySigner for this attempt
                let key_signer =
                    KeySigner::new_mnemonic_str(&mnemonic_clone, Some(&derivation_path_clone))?;

                // Use first RPC endpoint if available (for queries/simulation)
                let rpc_endpoint = if !rpc_endpoints.is_empty() {
                    Some(rpc_endpoints[0].clone())
                } else {
                    None
                };

                tracing::debug!(
                    "ChainConfig: grpc={}, rpc={:?}",
                    grpc_endpoint,
                    rpc_endpoint
                );

                let chain_config = ChainConfig {
                    chain_id: chain_id
                        .parse()
                        .map_err(|e| anyhow!("Invalid chain ID: {}", e))?,
                    grpc_endpoint: Some(grpc_endpoint),
                    rpc_endpoint,
                    grpc_web_endpoint: None,
                    address_kind: AddrKind::Cosmos {
                        prefix: "akash".to_string(),
                    },
                    gas_denom: "uakt".to_string(),
                    gas_price: 0.025,
                };

                SigningClient::new(chain_config, key_signer, None).await
            }
        })
        .await?;

    // Set QueryAndIncrement sequence strategy
    let mut client = client;
    client.sequence_strategy = SequenceStrategy::new(SequenceStrategyKind::QueryAndIncrement);

    tracing::info!(
        "SigningClient created successfully for chain: {}",
        akash_config.chain_id
    );
    Ok(client)
}

/// Create a layer-climb SigningClient from our key manager.
///
/// This unlocks the key manager, retrieves the mnemonic, and creates
/// a SigningClient that can be used for transaction signing.
///
/// NOTE: This version does not include endpoint failover. Use
/// `create_signing_client_with_failover` for production deployments.
pub async fn create_signing_client(
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    key_name: &str,
    account_index: u32,
    chain_config: ChainConfig,
) -> Result<SigningClient> {
    // Get the mnemonic for this key
    let mnemonic = {
        let mut key_manager_guard = key_manager.write().await;
        let key_store_guard = key_store.read().await;

        if !key_manager_guard.is_unlocked() {
            return Err(anyhow!(
                "Key manager is locked - must unlock before signing"
            ));
        }

        // Find the key
        let encrypted_key = key_store_guard
            .keys
            .iter()
            .find(|k| k.key_name == key_name)
            .ok_or_else(|| anyhow!("Key '{}' not found", key_name))?;

        // Decrypt the mnemonic
        key_manager_guard.decrypt_mnemonic(encrypted_key)?
    };

    // Create a KeySigner from the mnemonic
    // Use Cosmos derivation path (118) with the specified account index
    let derivation_path_str = format!("m/44'/118'/0'/0/{}", account_index);
    let derivation_path: DerivationPath = derivation_path_str
        .parse()
        .map_err(|e| anyhow!("Invalid derivation path '{}': {:?}", derivation_path_str, e))?;
    let key_signer = KeySigner::new_mnemonic_str(&mnemonic, Some(&derivation_path))?;

    // Create SigningClient with QueryAndIncrement sequence strategy
    // This queries sequence once on first tx, then increments locally for subsequent txs
    let mut client = SigningClient::new(chain_config, key_signer, None).await?;
    client.sequence_strategy = SequenceStrategy::new(SequenceStrategyKind::QueryAndIncrement);

    Ok(client)
}

/// Create a layer-climb ChainConfig from Akash deployment config.
pub fn chain_config_from_akash(akash_config: &AkashDeployConfig) -> Result<ChainConfig> {
    use layer_climb::prelude::ChainId;

    // Parse chain ID
    let chain_id: ChainId = akash_config
        .chain_id
        .parse()
        .map_err(|e| anyhow!("Invalid chain ID '{}': {}", akash_config.chain_id, e))?;

    // Determine which endpoint to use (use first from arrays)
    let (grpc_endpoint, rpc_endpoint) = if !akash_config.grpc_endpoints.is_empty() {
        (Some(akash_config.grpc_endpoints[0].clone()), None)
    } else if !akash_config.rpc_endpoints.is_empty() {
        (None, Some(akash_config.rpc_endpoints[0].clone()))
    } else {
        // Default to Akash mainnet RPC
        (None, Some("https://rpc.akash.network:443".to_string()))
    };

    Ok(ChainConfig {
        chain_id,
        grpc_endpoint,
        rpc_endpoint,
        grpc_web_endpoint: None,
        // Akash uses Cosmos-style addresses with "akash" prefix
        address_kind: AddrKind::Cosmos {
            prefix: "akash".to_string(),
        },
        gas_denom: "uakt".to_string(),
        gas_price: 0.025, // 0.025 uakt per gas unit
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_config_from_akash() {
        let akash_config = AkashDeployConfig {
            chain_id: "akashnet-2".to_string(),
            rpc_endpoints: vec!["https://rpc.akash.network:443".to_string()],
            grpc_endpoints: vec!["grpc.akash.network:9090".to_string()],
            rest_endpoints: vec!["https://rest.akash.network".to_string()],
            ..Default::default()
        };

        let chain_config = chain_config_from_akash(&akash_config).unwrap();
        assert_eq!(chain_config.gas_denom, "uakt");
        assert_eq!(chain_config.gas_price, 0.025);
        assert_eq!(
            chain_config.grpc_endpoint,
            Some("grpc.akash.network:9090".to_string())
        );
    }

    #[test]
    fn test_chain_config_with_rpc_only() {
        let akash_config = AkashDeployConfig {
            chain_id: "akashnet-2".to_string(),
            rpc_endpoints: vec!["https://rpc.akash.network:443".to_string()],
            grpc_endpoints: Vec::new(),
            rest_endpoints: vec!["https://rest.akash.network".to_string()],
            ..Default::default()
        };

        let chain_config = chain_config_from_akash(&akash_config).unwrap();
        assert_eq!(chain_config.grpc_endpoint, None);
        assert_eq!(
            chain_config.rpc_endpoint,
            Some("https://rpc.akash.network:443".to_string())
        );
    }
}
