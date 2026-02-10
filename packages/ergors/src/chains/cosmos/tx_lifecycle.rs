//! Transaction lifecycle management.
//!
//! Handles the full lifecycle: sign → broadcast → wait for finality → parse events.
//! Akash has ~6 second block times.

use anyhow::{anyhow, Result};
use cosmrs::Any;
use reqwest::Client as HttpClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use super::signer::CosmosSigner;
use super::broadcaster::BroadcastResponse;

/// Default gas limit for transactions.
pub const DEFAULT_GAS_LIMIT: u64 = 500_000;

/// Default gas price in uakt (0.025 AKT per unit gas).
pub const DEFAULT_GAS_PRICE: u64 = 25000;

/// Block time in seconds (Akash ~6s).
pub const BLOCK_TIME_SECS: u64 = 6;

/// Maximum time to wait for finality (10 blocks = ~60s).
pub const MAX_FINALITY_WAIT_SECS: u64 = 60;

/// Transaction lifecycle manager.
///
/// Provides a single entry point for sign → broadcast → finality → parse.
pub struct TxLifecycle {
    signer: Arc<CosmosSigner>,
    rest_endpoint: String,
    http: HttpClient,
    /// Poll interval for finality check
    poll_interval: Duration,
    /// Maximum wait time for finality
    max_wait: Duration,
}

/// Result of a successful transaction.
#[derive(Debug, Clone)]
pub struct TxResult {
    /// Transaction hash
    pub hash: String,
    /// Block height where tx was included
    pub height: u64,
    /// Response code (0 = success)
    pub code: u32,
    /// Gas used
    pub gas_used: u64,
    /// Raw log (error details if code != 0)
    pub raw_log: String,
    /// Transaction events (for parsing dseq, lease_id, etc.)
    pub events: Vec<TxEvent>,
}

/// A transaction event.
#[derive(Debug, Clone)]
pub struct TxEvent {
    pub event_type: String,
    pub attributes: Vec<(String, String)>,
}

impl TxLifecycle {
    /// Create a new transaction lifecycle manager.
    pub fn new(signer: Arc<CosmosSigner>, rest_endpoint: String) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");

        Self {
            signer,
            rest_endpoint,
            http,
            poll_interval: Duration::from_secs(BLOCK_TIME_SECS),
            max_wait: Duration::from_secs(MAX_FINALITY_WAIT_SECS),
        }
    }

    /// Sign, broadcast, and wait for finality.
    ///
    /// # Arguments
    /// * `key_name` - Key name in the store
    /// * `account_index` - HD account index
    /// * `msg` - Protobuf message to send
    /// * `gas_limit` - Gas limit (use DEFAULT_GAS_LIMIT if unsure)
    /// * `gas_price` - Gas price in uakt (use DEFAULT_GAS_PRICE if unsure)
    /// * `memo` - Optional transaction memo
    pub async fn sign_broadcast_wait(
        &self,
        key_name: &str,
        account_index: u32,
        msg: Any,
        gas_limit: u64,
        gas_price: u64,
        memo: Option<&str>,
    ) -> Result<TxResult> {
        self.sign_broadcast_wait_multi(key_name, account_index, vec![msg], gas_limit, gas_price, memo)
            .await
    }

    /// Sign, broadcast multiple messages, and wait for finality.
    pub async fn sign_broadcast_wait_multi(
        &self,
        key_name: &str,
        account_index: u32,
        msgs: Vec<Any>,
        gas_limit: u64,
        gas_price: u64,
        memo: Option<&str>,
    ) -> Result<TxResult> {
        // Sign the transaction
        let signed_tx_base64 = self
            .signer
            .sign_msgs(key_name, account_index, msgs, gas_limit, gas_price, memo)
            .await?;

        tracing::info!("Transaction signed, broadcasting...");

        // Broadcast
        let broadcast_response = self.broadcast(&signed_tx_base64).await?;

        let tx_hash = &broadcast_response.tx_response.txhash;
        let initial_code = broadcast_response.tx_response.code;

        if initial_code != 0 {
            // Transaction was rejected immediately
            return Err(anyhow!(
                "Transaction rejected (code {}): {}",
                initial_code,
                broadcast_response.tx_response.raw_log
            ));
        }

        tracing::info!("Broadcast successful, tx_hash: {}, waiting for finality...", tx_hash);

        // Wait for finality
        let result = self.wait_for_finality(tx_hash).await?;

        Ok(result)
    }

    /// Broadcast a signed transaction.
    async fn broadcast(&self, signed_tx_base64: &str) -> Result<BroadcastResponse> {
        let url = format!("{}/cosmos/tx/v1beta1/txs", self.rest_endpoint.trim_end_matches('/'));

        let payload = serde_json::json!({
            "tx_bytes": signed_tx_base64,
            "mode": "BROADCAST_MODE_SYNC"
        });

        let response = self.http.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Broadcast failed: {}", error_text));
        }

        let broadcast_response: BroadcastResponse = response.json().await?;
        Ok(broadcast_response)
    }

    /// Poll for transaction finality.
    async fn wait_for_finality(&self, tx_hash: &str) -> Result<TxResult> {
        let url = format!(
            "{}/cosmos/tx/v1beta1/txs/{}",
            self.rest_endpoint.trim_end_matches('/'),
            tx_hash
        );

        let start = std::time::Instant::now();

        loop {
            // Check timeout
            if start.elapsed() > self.max_wait {
                return Err(anyhow!(
                    "Transaction {} not included in block after {}s",
                    tx_hash,
                    self.max_wait.as_secs()
                ));
            }

            // Query transaction
            let response = self.http.get(&url).send().await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;

                    // Check if tx is included (has height > 0)
                    if let Some(tx_resp) = json.get("tx_response") {
                        let height_str = tx_resp
                            .get("height")
                            .and_then(|h| h.as_str())
                            .unwrap_or("0");
                        let height: u64 = height_str.parse().unwrap_or(0);

                        if height > 0 {
                            // Transaction is confirmed!
                            let code = tx_resp
                                .get("code")
                                .and_then(|c| c.as_u64())
                                .unwrap_or(0) as u32;

                            let gas_used = tx_resp
                                .get("gas_used")
                                .and_then(|g| g.as_str())
                                .unwrap_or("0")
                                .parse()
                                .unwrap_or(0);

                            let raw_log = tx_resp
                                .get("raw_log")
                                .and_then(|l| l.as_str())
                                .unwrap_or("")
                                .to_string();

                            let events = self.parse_events(tx_resp);

                            tracing::info!(
                                "Transaction confirmed at height {}, code: {}, gas_used: {}",
                                height,
                                code,
                                gas_used
                            );

                            return Ok(TxResult {
                                hash: tx_hash.to_string(),
                                height,
                                code,
                                gas_used,
                                raw_log,
                                events,
                            });
                        }
                    }
                }
                Ok(resp) if resp.status().as_u16() == 404 => {
                    // Transaction not yet indexed, keep polling
                    tracing::debug!("Transaction {} not yet indexed, polling...", tx_hash);
                }
                Ok(resp) => {
                    tracing::warn!(
                        "Unexpected response status {} when querying tx {}",
                        resp.status(),
                        tx_hash
                    );
                }
                Err(e) => {
                    tracing::warn!("Error querying tx {}: {}", tx_hash, e);
                }
            }

            // Wait before next poll
            sleep(self.poll_interval).await;
        }
    }

    /// Parse events from transaction response.
    fn parse_events(&self, tx_resp: &serde_json::Value) -> Vec<TxEvent> {
        let mut events = Vec::new();

        if let Some(event_array) = tx_resp.get("events").and_then(|e| e.as_array()) {
            for event in event_array {
                let event_type = event
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut attributes = Vec::new();

                if let Some(attr_array) = event.get("attributes").and_then(|a| a.as_array()) {
                    for attr in attr_array {
                        let key = attr
                            .get("key")
                            .and_then(|k| k.as_str())
                            .unwrap_or("")
                            .to_string();
                        let value = attr
                            .get("value")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        attributes.push((key, value));
                    }
                }

                events.push(TxEvent {
                    event_type,
                    attributes,
                });
            }
        }

        events
    }
}

impl TxResult {
    /// Check if the transaction succeeded (code == 0).
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    /// Get an attribute value from events by event type and key.
    pub fn get_attribute(&self, event_type: &str, key: &str) -> Option<&str> {
        for event in &self.events {
            if event.event_type == event_type {
                for (k, v) in &event.attributes {
                    if k == key {
                        return Some(v);
                    }
                }
            }
        }
        None
    }

    /// Extract deployment sequence (dseq) from create_deployment events.
    pub fn extract_dseq(&self) -> Option<u64> {
        // Look for akash.deployment.v1beta3.EventDeploymentCreated or similar
        for event in &self.events {
            if event.event_type.contains("deployment") || event.event_type.contains("Deployment") {
                for (key, value) in &event.attributes {
                    if key == "dseq" || key == "deployment-id.dseq" {
                        return value.parse().ok();
                    }
                }
            }
        }

        // Also check message attributes
        self.get_attribute("message", "dseq")
            .and_then(|v| v.parse().ok())
    }

    /// Extract lease ID from create_lease events.
    pub fn extract_lease_id(&self) -> Option<LeaseIdParts> {
        for event in &self.events {
            if event.event_type.contains("lease") || event.event_type.contains("Lease") {
                let mut owner = None;
                let mut dseq = None;
                let mut gseq = None;
                let mut oseq = None;
                let mut provider = None;

                for (key, value) in &event.attributes {
                    match key.as_str() {
                        "owner" | "lease-id.owner" => owner = Some(value.clone()),
                        "dseq" | "lease-id.dseq" => dseq = value.parse().ok(),
                        "gseq" | "lease-id.gseq" => gseq = value.parse().ok(),
                        "oseq" | "lease-id.oseq" => oseq = value.parse().ok(),
                        "provider" | "lease-id.provider" => provider = Some(value.clone()),
                        _ => {}
                    }
                }

                if let (Some(owner), Some(dseq), Some(gseq), Some(oseq), Some(provider)) =
                    (owner, dseq, gseq, oseq, provider)
                {
                    return Some(LeaseIdParts {
                        owner,
                        dseq,
                        gseq,
                        oseq,
                        provider,
                    });
                }
            }
        }
        None
    }
}

/// Parsed lease ID parts from transaction events.
#[derive(Debug, Clone)]
pub struct LeaseIdParts {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_result_is_success() {
        let result = TxResult {
            hash: "ABC123".to_string(),
            height: 100,
            code: 0,
            gas_used: 50000,
            raw_log: "".to_string(),
            events: vec![],
        };

        assert!(result.is_success());

        let failed = TxResult {
            code: 1,
            ..result.clone()
        };

        assert!(!failed.is_success());
    }

    #[test]
    fn test_get_attribute() {
        let result = TxResult {
            hash: "ABC123".to_string(),
            height: 100,
            code: 0,
            gas_used: 50000,
            raw_log: "".to_string(),
            events: vec![TxEvent {
                event_type: "deployment".to_string(),
                attributes: vec![
                    ("owner".to_string(), "akash1abc".to_string()),
                    ("dseq".to_string(), "12345".to_string()),
                ],
            }],
        };

        assert_eq!(result.get_attribute("deployment", "dseq"), Some("12345"));
        assert_eq!(result.get_attribute("deployment", "owner"), Some("akash1abc"));
        assert_eq!(result.get_attribute("deployment", "missing"), None);
        assert_eq!(result.get_attribute("other", "dseq"), None);
    }
}
