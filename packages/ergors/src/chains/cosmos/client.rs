//! Generic Cosmos SDK REST client.
//!
//! Provides chain-agnostic query operations for any Cosmos SDK chain.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::cosmos::base::v1beta1::Coin;
use reqwest::Client as HttpClient;

use super::types::ChainConfig;

/// Generic Cosmos blockchain client for REST queries.
pub struct CosmosBaseClient {
    config: ChainConfig,
    http: HttpClient,
}

impl CosmosBaseClient {
    /// Create a new Cosmos client with the given chain configuration.
    pub fn new(config: ChainConfig) -> Result<Self> {
        let http = HttpClient::builder().timeout(config.timeout).build()?;
        Ok(Self { config, http })
    }

    /// Get the chain ID.
    pub fn chain_id(&self) -> &str {
        &self.config.chain_id
    }

    /// Get the primary REST endpoint.
    pub fn rest_endpoint(&self) -> &str {
        self.config
            .rest_endpoint()
            .expect("At least one REST endpoint required")
    }

    /// Get the bech32 prefix for address generation.
    pub fn bech32_prefix(&self) -> &str {
        &self.config.bech32_prefix
    }

    /// Get the native denom.
    pub fn denom(&self) -> &str {
        &self.config.denom
    }

    /// Get the full chain configuration.
    pub fn config(&self) -> &ChainConfig {
        &self.config
    }

    // ===== Generic Bank Module Queries =====

    /// Query balance for a specific denom.
    ///
    /// REST: /cosmos/bank/v1beta1/balances/{address}/by_denom?denom={denom}
    pub async fn query_balance(&self, address: &str, denom: &str) -> Result<Coin> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}/by_denom?denom={}",
            self.rest_endpoint().trim_end_matches('/'),
            address,
            denom
        );

        tracing::debug!("Querying balance: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Balance query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let bal = json
            .get("balance")
            .ok_or_else(|| anyhow!("Missing 'balance' in response"))?;

        Ok(Coin {
            denom: bal["denom"].as_str().unwrap_or(denom).into(),
            amount: bal["amount"].as_str().unwrap_or("0").into(),
        })
    }

    /// Query all balances for an address.
    ///
    /// REST: /cosmos/bank/v1beta1/balances/{address}
    pub async fn query_all_balances(&self, address: &str) -> Result<Vec<Coin>> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}",
            self.rest_endpoint().trim_end_matches('/'),
            address
        );

        tracing::debug!("Querying all balances: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("All balances query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let balances = json
            .get("balances")
            .and_then(|b| b.as_array())
            .ok_or_else(|| anyhow!("Missing 'balances' array in response"))?;

        balances
            .iter()
            .map(|b| {
                Ok(Coin {
                    denom: b["denom"].as_str().unwrap_or("").into(),
                    amount: b["amount"].as_str().unwrap_or("0").into(),
                })
            })
            .collect()
    }

    /// Query spendable balance (excludes locked/vesting).
    ///
    /// REST: /cosmos/bank/v1beta1/spendable_balances/{address}/by_denom?denom={denom}
    pub async fn query_spendable_balance(&self, address: &str, denom: &str) -> Result<Coin> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/spendable_balances/{}/by_denom?denom={}",
            self.rest_endpoint().trim_end_matches('/'),
            address,
            denom
        );

        tracing::debug!("Querying spendable balance: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Spendable balance query failed ({}): {}",
                status,
                body
            ));
        }

        let json: serde_json::Value = resp.json().await?;
        let bal = json
            .get("balance")
            .ok_or_else(|| anyhow!("Missing 'balance' in response"))?;

        Ok(Coin {
            denom: bal["denom"].as_str().unwrap_or(denom).into(),
            amount: bal["amount"].as_str().unwrap_or("0").into(),
        })
    }

    // ===== Generic Auth Module Queries =====

    /// Query account information (account_number, sequence).
    ///
    /// REST: /cosmos/auth/v1beta1/accounts/{address}
    pub async fn query_account_info(&self, address: &str) -> Result<(u64, u64)> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.rest_endpoint().trim_end_matches('/'),
            address
        );

        tracing::debug!("Querying account info: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            // Check if account doesn't exist (new account)
            if status.as_u16() == 404 || (body.contains("account") && body.contains("not found"))
            {
                tracing::info!("Account {} not found on chain, using 0/0", address);
                return Ok((0, 0));
            }

            return Err(anyhow!("Account query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;

        // Parse account info from response
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ChainConfig::akash();
        let client = CosmosBaseClient::new(config).unwrap();
        assert_eq!(client.chain_id(), "akashnet-2");
        assert_eq!(client.bech32_prefix(), "akash");
        assert_eq!(client.denom(), "uakt");
    }

    #[test]
    fn test_client_methods() {
        let config = ChainConfig::osmosis();
        let client = CosmosBaseClient::new(config).unwrap();
        assert_eq!(client.denom(), "uosmo");
        assert!(client.rest_endpoint().contains("osmosis"));
    }
}
