//! Generic transaction broadcasting for Cosmos SDK chains.

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::CosmosBaseClient;

/// Transaction broadcaster for sending signed transactions to any Cosmos SDK chain.
pub struct CosmosBroadcaster {
    http_client: HttpClient,
    base_client: CosmosBaseClient,
    gas_adjustment: f64,
}

impl CosmosBroadcaster {
    /// Create a new broadcaster with the given base client.
    pub fn new(base_client: CosmosBaseClient) -> Self {
        Self {
            http_client: HttpClient::new(),
            base_client,
            gas_adjustment: 1.3,
        }
    }

    /// Create a new broadcaster with custom gas adjustment.
    pub fn with_gas_adjustment(mut self, gas_adjustment: f64) -> Self {
        self.gas_adjustment = gas_adjustment;
        self
    }

    /// Broadcast a signed transaction.
    pub async fn broadcast_tx(&self, signed_tx: &str) -> Result<BroadcastResponse> {
        let url = format!(
            "{}/cosmos/tx/v1beta1/txs",
            self.base_client.rest_endpoint()
        );

        let payload = json!({
            "tx_bytes": signed_tx,
            "mode": "BROADCAST_MODE_SYNC"
        });

        tracing::debug!("Broadcasting transaction to: {}", url);

        let response = self.http_client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Broadcast failed: {}", error_text));
        }

        let broadcast_response: BroadcastResponse = response.json().await?;
        Ok(broadcast_response)
    }

    /// Estimate gas for a transaction.
    pub async fn estimate_gas(&self, unsigned_tx: &Value) -> Result<u64> {
        let url = format!(
            "{}/cosmos/tx/v1beta1/simulate",
            self.base_client.rest_endpoint()
        );

        let payload = json!({
            "tx": unsigned_tx,
        });

        tracing::debug!("Estimating gas at: {}", url);

        let response = self.http_client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Gas estimation failed: {}", error_text));
        }

        let sim_response: SimulateResponse = response.json().await?;
        let gas_used = sim_response.gas_info.gas_used.parse::<u64>()?;

        // Apply gas adjustment
        let adjusted_gas = (gas_used as f64 * self.gas_adjustment) as u64;
        Ok(adjusted_gas)
    }

    /// Get reference to the base client
    pub fn base_client(&self) -> &CosmosBaseClient {
        &self.base_client
    }
}

/// Response from transaction broadcast.
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

/// Response from transaction simulation.
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
