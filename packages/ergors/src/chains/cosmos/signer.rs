//! Generic transaction signing for Cosmos SDK chains.
//!
//! Parameterized by chain configuration (denom, prefix, chain_id).

use anyhow::{anyhow, Result};
use cosmrs::tx::{Body, Fee, Raw, SignDoc, SignerInfo};
use cosmrs::{Any, Coin};
use ho_std::keys::cosmos::CosmosKeyPair;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::CosmosKeyStore;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::client::CosmosBaseClient;

/// Generic transaction signer for any Cosmos SDK chain.
pub struct CosmosSigner {
    /// Key manager (locked by default, unlocked with password)
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    /// Key store containing encrypted keys
    key_store: Arc<RwLock<CosmosKeyStore>>,
    /// Cosmos base client for account queries
    base_client: CosmosBaseClient,
}

impl CosmosSigner {
    /// Create a new signer with the given base client.
    pub fn new(
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        base_client: CosmosBaseClient,
    ) -> Self {
        Self {
            key_manager,
            key_store,
            base_client,
        }
    }

    /// Sign a single message and return the signed transaction bytes (base64 encoded).
    ///
    /// # Arguments
    /// * `key_name` - Name of the key in the store
    /// * `account_index` - HD account index (0 for default)
    /// * `msg` - The protobuf-encoded message (Any)
    /// * `gas_limit` - Gas limit for the transaction
    /// * `gas_price_amount` - Gas price in native denom (e.g., uakt, uosmo)
    /// * `memo` - Optional memo
    pub async fn sign_msg(
        &self,
        key_name: &str,
        account_index: u32,
        msg: Any,
        gas_limit: u64,
        gas_price_amount: u64,
        memo: Option<&str>,
    ) -> Result<String> {
        self.sign_msgs(
            key_name,
            account_index,
            vec![msg],
            gas_limit,
            gas_price_amount,
            memo,
        )
        .await
    }

    /// Sign multiple messages in a single transaction.
    pub async fn sign_msgs(
        &self,
        key_name: &str,
        account_index: u32,
        msgs: Vec<Any>,
        gas_limit: u64,
        gas_price_amount: u64,
        memo: Option<&str>,
    ) -> Result<String> {
        // Get the keypair
        let keypair = self.get_keypair(key_name, account_index).await?;

        // Generate address using the chain's bech32 prefix
        let address = keypair.address(self.base_client.bech32_prefix())?;

        // Query account info (number and sequence)
        let (account_number, sequence) = self.base_client.query_account_info(&address).await?;

        // Build and sign the transaction
        let signed_tx = self.build_and_sign_tx(
            &keypair,
            msgs,
            account_number,
            sequence,
            gas_limit,
            gas_price_amount,
            memo.unwrap_or(""),
        )?;

        // Encode to base64
        let tx_bytes = signed_tx
            .to_bytes()
            .map_err(|e| anyhow!("Failed to encode tx: {:?}", e))?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &tx_bytes,
        ))
    }

    /// Get keypair from key store.
    async fn get_keypair(&self, key_name: &str, account_index: u32) -> Result<CosmosKeyPair> {
        let store = self.key_store.read().await;
        let mut manager = self.key_manager.write().await;

        let encrypted_key = EncryptedCosmosKeyManager::get_key_by_name(&store, key_name)
            .ok_or_else(|| anyhow!("Key '{}' not found", key_name))?;

        if !manager.is_unlocked() {
            return Err(anyhow!("Key manager is locked - call unlock() first"));
        }

        manager.get_keypair(encrypted_key, account_index)
    }

    /// Build and sign a multi-message transaction.
    fn build_and_sign_tx(
        &self,
        keypair: &CosmosKeyPair,
        msgs: Vec<Any>,
        account_number: u64,
        sequence: u64,
        gas_limit: u64,
        gas_price_amount: u64,
        memo: &str,
    ) -> Result<Raw> {
        use cosmrs::crypto::secp256k1::SigningKey;

        // Convert keypair private key to cosmrs SigningKey
        let signing_key = SigningKey::from_slice(keypair.private_key())
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let public_key = signing_key.public_key();

        // Build transaction body
        let body = Body::new(msgs, memo, 0u32); // timeout_height = 0 (no timeout)

        // Calculate fee (gas_limit * gas_price) using chain's native denom
        let fee_amount = gas_limit * gas_price_amount;
        let fee = Fee::from_amount_and_gas(
            Coin {
                denom: self.base_client.denom().parse().expect("valid denom"),
                amount: fee_amount.into(),
            },
            gas_limit,
        );

        // Build signer info
        let signer_info = SignerInfo::single_direct(Some(public_key), sequence);

        // Build auth info
        let auth_info = signer_info.auth_info(fee);

        // Create sign doc
        let chain_id = self
            .base_client
            .chain_id()
            .parse()
            .map_err(|_| anyhow!("Invalid chain_id"))?;

        let sign_doc = SignDoc::new(&body, &auth_info, &chain_id, account_number)
            .map_err(|e| anyhow!("Failed to create sign doc: {}", e))?;

        // Sign
        let raw_tx = sign_doc
            .sign(&signing_key)
            .map_err(|e| anyhow!("Signing failed: {}", e))?;

        Ok(raw_tx)
    }

    /// Get the address for a key using the chain's bech32 prefix.
    pub async fn get_address(&self, key_name: &str, account_index: u32) -> Result<String> {
        let keypair = self.get_keypair(key_name, account_index).await?;
        keypair.address(self.base_client.bech32_prefix())
    }

    /// Get reference to the base client
    pub fn base_client(&self) -> &CosmosBaseClient {
        &self.base_client
    }
}

/// Convert a prost message to cosmrs::Any.
pub fn msg_to_any<M: prost::Message>(msg: &M, type_url: &str) -> Any {
    Any {
        type_url: type_url.to_string(),
        value: msg.encode_to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_to_any() {
        use ho_std::types::ergors::cosmos::base::v1beta1::Coin;

        let coin = Coin {
            denom: "uakt".to_string(),
            amount: "1000000".to_string(),
        };

        let any = msg_to_any(&coin, "/cosmos.base.v1beta1.Coin");
        assert_eq!(any.type_url, "/cosmos.base.v1beta1.Coin");
        assert!(!any.value.is_empty());
    }
}
