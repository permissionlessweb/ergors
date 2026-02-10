//! Generic Cosmos SDK chain configuration and types.
//!
//! Provides reusable configuration for any Cosmos SDK chain (Akash, Osmosis, Cosmos Hub, etc.).

use std::time::Duration;

/// Generic configuration for any Cosmos SDK chain.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    /// Chain ID (e.g., "akashnet-2", "osmosis-1", "cosmoshub-4")
    pub chain_id: String,
    /// Bech32 address prefix (e.g., "akash", "osmo", "cosmos")
    pub bech32_prefix: String,
    /// Native denom (e.g., "uakt", "uosmo", "uatom")
    pub denom: String,
    /// Gas prices string (e.g., "0.025uakt")
    pub gas_prices: String,
    /// RPC endpoints
    pub rpc_endpoints: Vec<String>,
    /// REST endpoints
    pub rest_endpoints: Vec<String>,
    /// Timeout for HTTP requests
    pub timeout: Duration,
}

impl ChainConfig {
    /// Akash mainnet configuration
    pub fn akash() -> Self {
        Self {
            chain_id: "akashnet-2".to_string(),
            bech32_prefix: "akash".to_string(),
            denom: "uakt".to_string(),
            gas_prices: "0.025uakt".to_string(),
            rpc_endpoints: vec![
                "https://rpc-akash.ecostake.com:443".to_string(),
                "https://akash-rpc.polkachu.com:443".to_string(),
            ],
            rest_endpoints: vec![
                "https://rest-akash.ecostake.com".to_string(),
                "https://akash-api.polkachu.com".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }
    }

    /// Osmosis mainnet configuration
    pub fn osmosis() -> Self {
        Self {
            chain_id: "osmosis-1".to_string(),
            bech32_prefix: "osmo".to_string(),
            denom: "uosmo".to_string(),
            gas_prices: "0.025uosmo".to_string(),
            rpc_endpoints: vec![
                "https://rpc-osmosis.ecostake.com:443".to_string(),
                "https://osmosis-rpc.polkachu.com:443".to_string(),
            ],
            rest_endpoints: vec![
                "https://rest-osmosis.ecostake.com".to_string(),
                "https://osmosis-api.polkachu.com".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }
    }

    /// Cosmos Hub mainnet configuration
    pub fn cosmos_hub() -> Self {
        Self {
            chain_id: "cosmoshub-4".to_string(),
            bech32_prefix: "cosmos".to_string(),
            denom: "uatom".to_string(),
            gas_prices: "0.025uatom".to_string(),
            rpc_endpoints: vec![
                "https://rpc-cosmoshub.ecostake.com:443".to_string(),
                "https://cosmos-rpc.polkachu.com:443".to_string(),
            ],
            rest_endpoints: vec![
                "https://rest-cosmoshub.ecostake.com".to_string(),
                "https://cosmos-api.polkachu.com".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }
    }

    /// Juno mainnet configuration
    pub fn juno() -> Self {
        Self {
            chain_id: "juno-1".to_string(),
            bech32_prefix: "juno".to_string(),
            denom: "ujuno".to_string(),
            gas_prices: "0.025ujuno".to_string(),
            rpc_endpoints: vec![
                "https://rpc-juno.ecostake.com:443".to_string(),
                "https://juno-rpc.polkachu.com:443".to_string(),
            ],
            rest_endpoints: vec![
                "https://rest-juno.ecostake.com".to_string(),
                "https://juno-api.polkachu.com".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }
    }

    /// Create custom chain configuration
    pub fn custom(
        chain_id: String,
        bech32_prefix: String,
        denom: String,
        gas_prices: String,
        rpc_endpoints: Vec<String>,
        rest_endpoints: Vec<String>,
    ) -> Self {
        Self {
            chain_id,
            bech32_prefix,
            denom,
            gas_prices,
            rpc_endpoints,
            rest_endpoints,
            timeout: Duration::from_secs(30),
        }
    }

    /// Get the first REST endpoint (primary)
    pub fn rest_endpoint(&self) -> Option<&str> {
        self.rest_endpoints.first().map(|s| s.as_str())
    }

    /// Get the first RPC endpoint (primary)
    pub fn rpc_endpoint(&self) -> Option<&str> {
        self.rpc_endpoints.first().map(|s| s.as_str())
    }

    /// Update with custom endpoints (for CLI overrides)
    pub fn with_endpoints(mut self, rest: Option<String>, rpc: Option<String>) -> Self {
        if let Some(r) = rest {
            if !r.is_empty() {
                self.rest_endpoints.insert(0, r);
            }
        }
        if let Some(rpc_endpoint) = rpc {
            if !rpc_endpoint.is_empty() {
                self.rpc_endpoints.insert(0, rpc_endpoint);
            }
        }
        self
    }

    /// Update chain ID (for CLI overrides)
    pub fn with_chain_id(mut self, chain_id: String) -> Self {
        if !chain_id.is_empty() {
            self.chain_id = chain_id;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_akash_config() {
        let config = ChainConfig::akash();
        assert_eq!(config.chain_id, "akashnet-2");
        assert_eq!(config.bech32_prefix, "akash");
        assert_eq!(config.denom, "uakt");
        assert!(!config.rest_endpoints.is_empty());
        assert!(!config.rpc_endpoints.is_empty());
    }

    #[test]
    fn test_osmosis_config() {
        let config = ChainConfig::osmosis();
        assert_eq!(config.chain_id, "osmosis-1");
        assert_eq!(config.bech32_prefix, "osmo");
        assert_eq!(config.denom, "uosmo");
    }

    #[test]
    fn test_custom_config() {
        let config = ChainConfig::custom(
            "testnet-1".to_string(),
            "test".to_string(),
            "utest".to_string(),
            "0.025utest".to_string(),
            vec!["https://rpc.test.com".to_string()],
            vec!["https://rest.test.com".to_string()],
        );
        assert_eq!(config.chain_id, "testnet-1");
        assert_eq!(config.bech32_prefix, "test");
        assert_eq!(config.denom, "utest");
    }

    #[test]
    fn test_with_endpoints() {
        let config = ChainConfig::akash().with_endpoints(
            Some("https://custom-rest.com".to_string()),
            Some("https://custom-rpc.com".to_string()),
        );
        assert_eq!(config.rest_endpoint(), Some("https://custom-rest.com"));
        assert_eq!(config.rpc_endpoint(), Some("https://custom-rpc.com"));
    }
}
