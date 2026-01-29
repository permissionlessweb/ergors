//! Transaction signing for Cosmos SDK chains.
//!
//! Uses cosmrs for transaction building and signing with keys from
//! the encrypted KeyStore (ho-std).

use anyhow::{anyhow, Result};
use cosmrs::tx::{Body, Fee, Raw, SignDoc, SignerInfo};
use cosmrs::{Any, Coin};
use ho_std::keys::cosmos::CosmosKeyPair;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::CosmosKeyStore;
use reqwest::Client as HttpClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Transaction signer using KeyStore.
pub struct TxSigner {
    /// Key manager (locked by default, unlocked with password)
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    /// Key store containing encrypted keys
    key_store: Arc<RwLock<CosmosKeyStore>>,
    /// Chain ID for signing
    chain_id: String,
    /// REST endpoint for account queries
    rest_endpoint: String,
    /// HTTP client
    http: HttpClient,
}

impl TxSigner {
    /// Create a new signer.
    pub fn new(
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        chain_id: String,
        rest_endpoint: String,
    ) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");

        Self {
            key_manager,
            key_store,
            chain_id,
            rest_endpoint,
            http,
        }
    }

    /// Sign a message and return the signed transaction bytes (base64 encoded).
    ///
    /// # Arguments
    /// * `key_name` - Name of the key in the store
    /// * `account_index` - HD account index (0 for default)
    /// * `msg` - The protobuf-encoded message (Any)
    /// * `gas_limit` - Gas limit for the transaction
    /// * `gas_price_uakt` - Gas price in uakt
    /// * `memo` - Optional memo
    pub async fn sign_msg(
        &self,
        key_name: &str,
        account_index: u32,
        msg: Any,
        gas_limit: u64,
        gas_price_uakt: u64,
        memo: Option<&str>,
    ) -> Result<String> {
        // Get the keypair
        let keypair = self.get_keypair(key_name, account_index).await?;
        let address = keypair.akash_address()?;

        // Query account info (number and sequence)
        let (account_number, sequence) = self.query_account_info(&address).await?;

        // Build the transaction
        let signed_tx = self.build_and_sign_tx(
            &keypair,
            msg,
            account_number,
            sequence,
            gas_limit,
            gas_price_uakt,
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

    /// Sign multiple messages in a single transaction.
    pub async fn sign_msgs(
        &self,
        key_name: &str,
        account_index: u32,
        msgs: Vec<Any>,
        gas_limit: u64,
        gas_price_uakt: u64,
        memo: Option<&str>,
    ) -> Result<String> {
        let keypair = self.get_keypair(key_name, account_index).await?;
        let address = keypair.akash_address()?;
        let (account_number, sequence) = self.query_account_info(&address).await?;

        let signed_tx = self.build_and_sign_multi_tx(
            &keypair,
            msgs,
            account_number,
            sequence,
            gas_limit,
            gas_price_uakt,
            memo.unwrap_or(""),
        )?;

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

    /// Query account info from chain (account_number, sequence).
    pub async fn query_account_info(&self, address: &str) -> Result<(u64, u64)> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.rest_endpoint.trim_end_matches('/'),
            address
        );

        tracing::debug!("Querying account info: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            // Check if account doesn't exist (new account)
            if status.as_u16() == 404 || body.contains("account") && body.contains("not found") {
                tracing::info!("Account {} not found on chain, using 0/0", address);
                return Ok((0, 0));
            }

            return Err(anyhow!("Account query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;

        // Parse account info from response
        // The response structure varies by account type, but we need account_number and sequence
        let account = json
            .get("account")
            .ok_or_else(|| anyhow!("Missing 'account' in response"))?;

        // Handle BaseAccount directly or wrapped in other account types
        let base_account = if account.get("base_account").is_some() {
            account.get("base_account").unwrap()
        } else {
            account
        };

        let account_number = base_account
            .get("account_number")
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);

        let sequence = base_account
            .get("sequence")
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);

        Ok((account_number, sequence))
    }

    /// Build and sign a single-message transaction.
    fn build_and_sign_tx(
        &self,
        keypair: &CosmosKeyPair,
        msg: Any,
        account_number: u64,
        sequence: u64,
        gas_limit: u64,
        gas_price_uakt: u64,
        memo: &str,
    ) -> Result<Raw> {
        self.build_and_sign_multi_tx(
            keypair,
            vec![msg],
            account_number,
            sequence,
            gas_limit,
            gas_price_uakt,
            memo,
        )
    }

    /// Build and sign a multi-message transaction.
    fn build_and_sign_multi_tx(
        &self,
        keypair: &CosmosKeyPair,
        msgs: Vec<Any>,
        account_number: u64,
        sequence: u64,
        gas_limit: u64,
        gas_price_uakt: u64,
        memo: &str,
    ) -> Result<Raw> {
        use cosmrs::crypto::secp256k1::SigningKey;

        // Convert keypair private key to cosmrs SigningKey
        let signing_key = SigningKey::from_slice(keypair.private_key())
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let public_key = signing_key.public_key();

        // Build transaction body
        let body = Body::new(msgs, memo, 0u32); // timeout_height = 0 (no timeout)

        // Calculate fee (gas_limit * gas_price)
        let fee_amount = gas_limit * gas_price_uakt;
        let fee = Fee::from_amount_and_gas(
            Coin {
                denom: "uakt".parse().expect("valid denom"),
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
            .chain_id
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

    /// Get the address for a key.
    pub async fn get_address(&self, key_name: &str, account_index: u32) -> Result<String> {
        let keypair = self.get_keypair(key_name, account_index).await?;
        keypair.akash_address()
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
        // Simple test that msg_to_any works
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
