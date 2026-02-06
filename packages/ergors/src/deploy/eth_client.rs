//! Ethereum JSON-RPC query client.
//!
//! REST/RPC-based queries to Ethereum nodes. Follows the same structural
//! pattern as cosmos_client.rs: a simple struct with query methods using
//! reqwest for HTTP transport and serde_json::Value for response parsing.

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use serde_json::{json, Value};
use std::time::Duration;

/// Ethereum RPC endpoint configuration.
#[derive(Debug, Clone)]
pub struct EthEndpoints {
    /// JSON-RPC endpoint URL
    pub rpc_url: String,
    /// Chain ID for transaction signing context
    pub chain_id: u64,
    /// HTTP request timeout
    pub timeout: Duration,
}

impl EthEndpoints {
    /// Ethereum mainnet defaults (public RPC).
    pub fn mainnet() -> Self {
        Self {
            rpc_url: "https://eth.llamarpc.com".into(),
            chain_id: 1,
            timeout: Duration::from_secs(30),
        }
    }

    /// Sepolia testnet defaults.
    pub fn sepolia() -> Self {
        Self {
            rpc_url: "https://rpc.sepolia.org".into(),
            chain_id: 11155111,
            timeout: Duration::from_secs(30),
        }
    }

    /// Custom endpoint.
    pub fn custom(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            timeout: Duration::from_secs(30),
        }
    }

    /// Override with optional values (for CLI flag merging).
    pub fn with_overrides(mut self, rpc_url: Option<&str>, chain_id: Option<u64>) -> Self {
        if let Some(url) = rpc_url {
            if !url.is_empty() {
                self.rpc_url = url.into();
            }
        }
        if let Some(id) = chain_id {
            self.chain_id = id;
        }
        self
    }
}

/// Ethereum JSON-RPC client for balance, nonce, and gas queries.
pub struct EthClient {
    endpoints: EthEndpoints,
    http: HttpClient,
    next_id: std::sync::atomic::AtomicU64,
}

impl EthClient {
    pub fn new(endpoints: EthEndpoints) -> Result<Self> {
        let http = HttpClient::builder().timeout(endpoints.timeout).build()?;
        Ok(Self {
            endpoints,
            http,
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.endpoints.chain_id
    }

    pub fn rpc_url(&self) -> &str {
        &self.endpoints.rpc_url
    }

    /// Make a JSON-RPC call and return the result field.
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        tracing::debug!("ETH RPC -> {} {}", method, params);

        let resp = self
            .http
            .post(&self.endpoints.rpc_url)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("RPC request failed ({}): {}", status, text));
        }

        let json: Value = resp.json().await?;

        if let Some(error) = json.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("RPC error ({}): {}", code, message));
        }

        json.get("result")
            .cloned()
            .ok_or_else(|| anyhow!("Missing 'result' in RPC response"))
    }

    fn hex_to_u64(hex_str: &str) -> Result<u64> {
        let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
        u64::from_str_radix(stripped, 16).map_err(|e| anyhow!("Invalid hex '{}': {}", hex_str, e))
    }

    fn hex_to_u128(hex_str: &str) -> Result<u128> {
        let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
        u128::from_str_radix(stripped, 16)
            .map_err(|e| anyhow!("Invalid hex '{}': {}", hex_str, e))
    }

    /// Query ETH balance for an address (in wei).
    pub async fn query_balance(&self, address: &str) -> Result<u128> {
        let result = self
            .rpc_call("eth_getBalance", json!([address, "latest"]))
            .await?;
        let hex_balance = result
            .as_str()
            .ok_or_else(|| anyhow!("Balance result is not a string"))?;
        Self::hex_to_u128(hex_balance)
    }

    /// Query the next nonce for an address.
    pub async fn query_nonce(&self, address: &str) -> Result<u64> {
        let result = self
            .rpc_call("eth_getTransactionCount", json!([address, "latest"]))
            .await?;
        let hex_nonce = result
            .as_str()
            .ok_or_else(|| anyhow!("Nonce result is not a string"))?;
        Self::hex_to_u64(hex_nonce)
    }

    /// Query the current gas price (in wei).
    pub async fn query_gas_price(&self) -> Result<u128> {
        let result = self.rpc_call("eth_gasPrice", json!([])).await?;
        let hex_price = result
            .as_str()
            .ok_or_else(|| anyhow!("Gas price result is not a string"))?;
        Self::hex_to_u128(hex_price)
    }

    /// Estimate gas for a transaction call object.
    pub async fn estimate_gas(&self, tx: Value) -> Result<u64> {
        let result = self.rpc_call("eth_estimateGas", json!([tx])).await?;
        let hex_gas = result
            .as_str()
            .ok_or_else(|| anyhow!("Gas estimate result is not a string"))?;
        Self::hex_to_u64(hex_gas)
    }

    /// Query the current block number.
    pub async fn query_block_number(&self) -> Result<u64> {
        let result = self.rpc_call("eth_blockNumber", json!([])).await?;
        let hex_block = result
            .as_str()
            .ok_or_else(|| anyhow!("Block number result is not a string"))?;
        Self::hex_to_u64(hex_block)
    }

    /// Query the chain ID from the node.
    pub async fn query_chain_id(&self) -> Result<u64> {
        let result = self.rpc_call("eth_chainId", json!([])).await?;
        let hex_chain = result
            .as_str()
            .ok_or_else(|| anyhow!("Chain ID result is not a string"))?;
        Self::hex_to_u64(hex_chain)
    }

    /// Send a raw signed transaction.
    pub async fn send_raw_transaction(&self, raw_tx: &str) -> Result<String> {
        let result = self
            .rpc_call("eth_sendRawTransaction", json!([raw_tx]))
            .await?;
        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Transaction hash result is not a string"))
    }

    /// Get transaction receipt by hash.
    pub async fn query_tx_receipt(&self, tx_hash: &str) -> Result<Option<Value>> {
        let result = self
            .rpc_call("eth_getTransactionReceipt", json!([tx_hash]))
            .await?;
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    /// Get EIP-1559 fee data.
    pub async fn query_fee_data(&self) -> Result<FeeData> {
        let priority_result = self.rpc_call("eth_maxPriorityFeePerGas", json!([])).await;

        let max_priority_fee = match priority_result {
            Ok(val) => {
                let hex = val.as_str().unwrap_or("0x0");
                Self::hex_to_u128(hex).unwrap_or(1_500_000_000)
            }
            Err(_) => 1_500_000_000, // 1.5 gwei fallback
        };

        let block = self
            .rpc_call("eth_getBlockByNumber", json!(["latest", false]))
            .await?;

        let base_fee = block
            .get("baseFeePerGas")
            .and_then(|b| b.as_str())
            .and_then(|hex| Self::hex_to_u128(hex).ok())
            .unwrap_or(0);

        Ok(FeeData {
            base_fee,
            max_priority_fee,
            max_fee: base_fee.saturating_mul(2).saturating_add(max_priority_fee),
        })
    }
}

/// EIP-1559 fee data.
#[derive(Debug, Clone)]
pub struct FeeData {
    pub base_fee: u128,
    pub max_priority_fee: u128,
    pub max_fee: u128,
}

/// Format wei as ETH string (for display).
pub fn wei_to_eth_string(wei: u128) -> String {
    let eth = wei as f64 / 1_000_000_000_000_000_000.0;
    format!("{:.6} ETH", eth)
}

/// Format wei as gwei string (for gas prices).
pub fn wei_to_gwei_string(wei: u128) -> String {
    let gwei = wei as f64 / 1_000_000_000.0;
    format!("{:.2} gwei", gwei)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mainnet_defaults() {
        let endpoints = EthEndpoints::mainnet();
        assert_eq!(endpoints.chain_id, 1);
        assert!(!endpoints.rpc_url.is_empty());
    }

    #[test]
    fn test_sepolia_defaults() {
        let endpoints = EthEndpoints::sepolia();
        assert_eq!(endpoints.chain_id, 11155111);
    }

    #[test]
    fn test_custom_endpoints() {
        let endpoints = EthEndpoints::custom("http://localhost:8545", 31337);
        assert_eq!(endpoints.rpc_url, "http://localhost:8545");
        assert_eq!(endpoints.chain_id, 31337);
    }

    #[test]
    fn test_with_overrides() {
        let endpoints =
            EthEndpoints::mainnet().with_overrides(Some("https://custom.rpc"), Some(5));
        assert_eq!(endpoints.rpc_url, "https://custom.rpc");
        assert_eq!(endpoints.chain_id, 5);
    }

    #[test]
    fn test_empty_override_ignored() {
        let endpoints = EthEndpoints::mainnet().with_overrides(Some(""), None);
        assert_eq!(endpoints.rpc_url, "https://eth.llamarpc.com");
        assert_eq!(endpoints.chain_id, 1);
    }

    #[test]
    fn test_hex_to_u64() {
        assert_eq!(EthClient::hex_to_u64("0x0").unwrap(), 0);
        assert_eq!(EthClient::hex_to_u64("0xa").unwrap(), 10);
        assert_eq!(EthClient::hex_to_u64("0xff").unwrap(), 255);
        assert_eq!(EthClient::hex_to_u64("0x1234").unwrap(), 4660);
    }

    #[test]
    fn test_hex_to_u128() {
        assert_eq!(EthClient::hex_to_u128("0x0").unwrap(), 0);
        assert_eq!(
            EthClient::hex_to_u128("0xde0b6b3a7640000").unwrap(),
            1_000_000_000_000_000_000
        );
    }

    #[test]
    fn test_wei_to_eth_string() {
        assert_eq!(
            wei_to_eth_string(1_000_000_000_000_000_000),
            "1.000000 ETH"
        );
        assert_eq!(wei_to_eth_string(0), "0.000000 ETH");
    }

    #[test]
    fn test_wei_to_gwei_string() {
        assert_eq!(wei_to_gwei_string(1_000_000_000), "1.00 gwei");
        assert_eq!(wei_to_gwei_string(20_000_000_000), "20.00 gwei");
    }

    #[test]
    fn test_new_client() {
        let client = EthClient::new(EthEndpoints::mainnet());
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.chain_id(), 1);
        assert_eq!(client.rpc_url(), "https://eth.llamarpc.com");
    }
}
