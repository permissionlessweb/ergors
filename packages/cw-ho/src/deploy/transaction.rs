//! Transaction utilities for Akash Network
//!
//! This module provides utilities for signing and broadcasting transactions
//! to the Akash Network.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Transaction configuration
#[derive(Debug, Clone)]
pub struct TxConfig {
    /// Chain ID
    pub chain_id: String,
    /// Gas limit
    pub gas_limit: u64,
    /// Gas price in uakt (micro-AKT)
    pub gas_price: u64,
    /// Gas adjustment factor
    pub gas_adjustment: f64,
    /// Transaction memo
    pub memo: Option<String>,
    /// Sign mode (amino-json or direct)
    pub sign_mode: String,
}

impl Default for TxConfig {
    fn default() -> Self {
        Self {
            chain_id: "akashnet-2".to_string(),
            gas_limit: 1000000,
            gas_price: 5000, // 0.005 AKT
            gas_adjustment: 1.3,
            memo: None,
            sign_mode: "amino-json".to_string(),
        }
    }
}

/// Transaction broadcaster for sending signed transactions to Akash
pub struct TxBroadcaster {
    http_client: HttpClient,
    rest_endpoint: String,
    config: TxConfig,
}

impl TxBroadcaster {
    pub fn new(rest_endpoint: String, config: TxConfig) -> Self {
        Self {
            http_client: HttpClient::new(),
            rest_endpoint,
            config,
        }
    }

    /// Broadcast a signed transaction
    pub async fn broadcast_tx(&self, signed_tx: &str) -> Result<BroadcastResponse> {
        let url = format!("{}/cosmos/tx/v1beta1/txs", self.rest_endpoint);

        let payload = json!({
            "tx_bytes": signed_tx,
            "mode": "BROADCAST_MODE_SYNC"
        });

        let response = self.http_client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Broadcast failed: {}", error_text));
        }

        let broadcast_response: BroadcastResponse = response.json().await?;
        Ok(broadcast_response)
    }

    /// Estimate gas for a transaction
    pub async fn estimate_gas(&self, unsigned_tx: &Value) -> Result<u64> {
        let url = format!("{}/cosmos/tx/v1beta1/simulate", self.rest_endpoint);

        let payload = json!({
            "tx": unsigned_tx,
        });

        let response = self.http_client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Gas estimation failed: {}", error_text));
        }

        let sim_response: SimulateResponse = response.json().await?;
        let gas_used = sim_response.gas_info.gas_used.parse::<u64>()?;

        // Apply gas adjustment
        let adjusted_gas = (gas_used as f64 * self.config.gas_adjustment) as u64;
        Ok(std::cmp::max(adjusted_gas, self.config.gas_limit))
    }
}

/// Response from transaction broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResponse {
    pub tx_response: TxResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxResponse {
    pub height: String,
    pub txhash: String,
    pub codespace: String,
    pub code: u32,
    pub data: String,
    pub raw_log: String,
    pub logs: Vec<TxLog>,
    pub info: String,
    pub gas_wanted: String,
    pub gas_used: String,
    pub tx: Option<Value>,
    pub timestamp: String,
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxLog {
    pub msg_index: u32,
    pub log: String,
    pub events: Vec<Value>,
}

/// Response from transaction simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateResponse {
    pub gas_info: GasInfo,
    pub result: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasInfo {
    pub gas_wanted: String,
    pub gas_used: String,
}

/// Simple keyring implementation for demonstration
/// In production, this should integrate with proper key management
pub struct SimpleKeyring {
    keys: HashMap<String, KeyInfo>,
}

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub name: String,
    pub address: String,
    pub public_key: String,
    // In production, store encrypted private key
    pub private_key_placeholder: String,
}

impl Default for SimpleKeyring {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleKeyring {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    /// Add a key (placeholder - in reality would generate or import)
    pub fn add_key(&mut self, name: &str, address: &str) -> Result<()> {
        let key_info = KeyInfo {
            name: name.to_string(),
            address: address.to_string(),
            public_key: format!("placeholder-pubkey-for-{}", name),
            private_key_placeholder: format!("placeholder-privkey-for-{}", name),
        };

        self.keys.insert(name.to_string(), key_info);
        Ok(())
    }

    /// Get key by name
    pub fn get_key(&self, name: &str) -> Result<&KeyInfo> {
        self.keys
            .get(name)
            .ok_or_else(|| anyhow!("Key '{}' not found", name))
    }

    /// List all keys
    pub fn list_keys(&self) -> Vec<&KeyInfo> {
        self.keys.values().collect()
    }
}
