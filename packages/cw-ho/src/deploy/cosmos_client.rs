//! Direct Cosmos blockchain query client.
//!
//! REST-based queries to Cosmos SDK chains. No magic, just functions.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::cosmos::base::v1beta1::Coin;
use ho_std::types::ergors::orch::v1::AkashDeployConfig;
use reqwest::Client as HttpClient;
use std::time::Duration;

/// Cosmos blockchain endpoints
#[derive(Debug, Clone)]
pub struct CosmosEndpoints {
    pub rest: String,
    pub chain_id: String,
    pub timeout: Duration,
}

impl CosmosEndpoints {
    /// Akash mainnet defaults
    pub fn akash_mainnet() -> Self {
        Self {
            rest: "https://rest-akash.ecostake.com".into(),
            chain_id: "akashnet-2".into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// From config proto
    pub fn from_akash_config(config: &AkashDeployConfig) -> Self {
        let rest = if config.rest_endpoint.is_empty() {
            // Derive from rpc by replacing rpc-> rest (convention)
            if !config.rpc_endpoint.is_empty() {
                config.rpc_endpoint.replace("rpc-", "rest-").replace(":443", "")
            } else {
                String::new()
            }
        } else {
            config.rest_endpoint.clone()
        };

        Self {
            rest: if rest.is_empty() {
                "https://rest-akash.ecostake.com".into()
            } else {
                rest
            },
            chain_id: if config.chain_id.is_empty() {
                "akashnet-2".into()
            } else {
                config.chain_id.clone()
            },
            timeout: Duration::from_secs(30),
        }
    }

    /// Override with CLI flags
    pub fn with_overrides(mut self, rest: Option<&str>, chain_id: Option<&str>) -> Self {
        if let Some(r) = rest {
            if !r.is_empty() {
                self.rest = r.into();
            }
        }
        if let Some(c) = chain_id {
            if !c.is_empty() {
                self.chain_id = c.into();
            }
        }
        self
    }
}

/// Direct Cosmos blockchain client
pub struct CosmosClient {
    endpoints: CosmosEndpoints,
    http: HttpClient,
}

impl CosmosClient {
    pub fn new(endpoints: CosmosEndpoints) -> Result<Self> {
        let http = HttpClient::builder()
            .timeout(endpoints.timeout)
            .build()?;
        Ok(Self { endpoints, http })
    }

    pub fn chain_id(&self) -> &str {
        &self.endpoints.chain_id
    }

    pub fn rest_endpoint(&self) -> &str {
        &self.endpoints.rest
    }

    /// Query balance for specific denom
    ///
    /// REST: /cosmos/bank/v1beta1/balances/{address}/by_denom?denom={denom}
    pub async fn query_balance(&self, address: &str, denom: &str) -> Result<Coin> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}/by_denom?denom={}",
            self.endpoints.rest.trim_end_matches('/'),
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

    /// Query all balances for an address
    ///
    /// REST: /cosmos/bank/v1beta1/balances/{address}
    pub async fn query_all_balances(&self, address: &str) -> Result<Vec<Coin>> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}",
            self.endpoints.rest.trim_end_matches('/'),
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

    /// Query spendable balance (excludes locked/vesting)
    ///
    /// REST: /cosmos/bank/v1beta1/spendable_balances/{address}/by_denom?denom={denom}
    pub async fn query_spendable_balance(&self, address: &str, denom: &str) -> Result<Coin> {
        let url = format!(
            "{}/cosmos/bank/v1beta1/spendable_balances/{}/by_denom?denom={}",
            self.endpoints.rest.trim_end_matches('/'),
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

    // ===== Akash Certificate Queries =====

    /// Query certificates for an owner address.
    ///
    /// REST: /akash/cert/v1beta3/certificates/list?filter.owner={owner}
    pub async fn query_certificates(&self, owner: &str) -> Result<Vec<CertificateInfo>> {
        let url = format!(
            "{}/akash/cert/v1beta3/certificates/list?filter.owner={}",
            self.endpoints.rest.trim_end_matches('/'),
            owner
        );

        tracing::debug!("Querying certificates: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Certificate query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let empty_vec = vec![];
        let certs = json
            .get("certificates")
            .and_then(|c| c.as_array())
            .unwrap_or(&empty_vec);

        let mut result = Vec::new();
        for cert in certs {
            let certificate = cert.get("certificate");
            if let Some(c) = certificate {
                let state_str = c
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("invalid");
                let state = match state_str {
                    "valid" => CertState::Valid,
                    "revoked" => CertState::Revoked,
                    _ => CertState::Invalid,
                };

                result.push(CertificateInfo {
                    owner: owner.to_string(),
                    serial: cert
                        .get("serial")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    state,
                    cert_pem: c
                        .get("cert")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pubkey: c
                        .get("pubkey")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }

        Ok(result)
    }

    /// Query valid certificate for an owner (first valid cert found).
    pub async fn query_valid_certificate(&self, owner: &str) -> Result<Option<CertificateInfo>> {
        let certs = self.query_certificates(owner).await?;
        Ok(certs.into_iter().find(|c| c.state == CertState::Valid))
    }

    // ===== Akash Deployment Queries =====

    /// Query deployment by owner and dseq.
    ///
    /// REST: /akash/deployment/v1beta3/deployments/info?id.owner={owner}&id.dseq={dseq}
    pub async fn query_deployment(&self, owner: &str, dseq: u64) -> Result<DeploymentInfo> {
        let url = format!(
            "{}/akash/deployment/v1beta3/deployments/info?id.owner={}&id.dseq={}",
            self.endpoints.rest.trim_end_matches('/'),
            owner,
            dseq
        );

        tracing::debug!("Querying deployment: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Deployment query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let deployment = json
            .get("deployment")
            .ok_or_else(|| anyhow!("Missing 'deployment' in response"))?;

        let state_str = deployment
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("invalid");

        Ok(DeploymentInfo {
            owner: owner.to_string(),
            dseq,
            state: match state_str {
                "active" => DeploymentState::Active,
                "closed" => DeploymentState::Closed,
                _ => DeploymentState::Invalid,
            },
        })
    }

    // ===== Akash Bid Queries =====

    /// Query bids for a deployment.
    ///
    /// REST: /akash/market/v1beta4/bids/list?filters.owner={owner}&filters.dseq={dseq}
    pub async fn query_bids(&self, owner: &str, dseq: u64) -> Result<Vec<BidInfo>> {
        let url = format!(
            "{}/akash/market/v1beta4/bids/list?filters.owner={}&filters.dseq={}",
            self.endpoints.rest.trim_end_matches('/'),
            owner,
            dseq
        );

        tracing::debug!("Querying bids: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Bid query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let empty_vec = vec![];
        let bids = json
            .get("bids")
            .and_then(|b| b.as_array())
            .unwrap_or(&empty_vec);

        let mut result = Vec::new();
        for bid_entry in bids {
            let bid = bid_entry.get("bid");
            if let Some(b) = bid {
                let bid_id = b.get("bid_id");
                let price = b.get("price");

                if let (Some(id), Some(p)) = (bid_id, price) {
                    let state_str = b.get("state").and_then(|s| s.as_str()).unwrap_or("invalid");

                    result.push(BidInfo {
                        owner: id
                            .get("owner")
                            .and_then(|o| o.as_str())
                            .unwrap_or("")
                            .to_string(),
                        dseq: id
                            .get("dseq")
                            .and_then(|d| d.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                        gseq: id
                            .get("gseq")
                            .and_then(|g| g.as_u64())
                            .unwrap_or(0) as u32,
                        oseq: id
                            .get("oseq")
                            .and_then(|o| o.as_u64())
                            .unwrap_or(0) as u32,
                        provider: id
                            .get("provider")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_string(),
                        price_denom: p
                            .get("denom")
                            .and_then(|d| d.as_str())
                            .unwrap_or("uakt")
                            .to_string(),
                        price_amount: p
                            .get("amount")
                            .and_then(|a| a.as_str())
                            .unwrap_or("0")
                            .to_string(),
                        state: match state_str {
                            "open" => BidState::Open,
                            "active" => BidState::Active,
                            "lost" => BidState::Lost,
                            "closed" => BidState::Closed,
                            _ => BidState::Invalid,
                        },
                    });
                }
            }
        }

        Ok(result)
    }

    /// Query open bids for a deployment (filtered to only open bids).
    pub async fn query_open_bids(&self, owner: &str, dseq: u64) -> Result<Vec<BidInfo>> {
        let bids = self.query_bids(owner, dseq).await?;
        Ok(bids.into_iter().filter(|b| b.state == BidState::Open).collect())
    }

    // ===== Akash Lease Queries =====

    /// Query lease by ID.
    ///
    /// REST: /akash/market/v1beta4/leases/info?id.owner={owner}&id.dseq={dseq}&id.gseq={gseq}&id.oseq={oseq}&id.provider={provider}
    pub async fn query_lease(
        &self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        provider: &str,
    ) -> Result<LeaseInfo> {
        let url = format!(
            "{}/akash/market/v1beta4/leases/info?id.owner={}&id.dseq={}&id.gseq={}&id.oseq={}&id.provider={}",
            self.endpoints.rest.trim_end_matches('/'),
            owner,
            dseq,
            gseq,
            oseq,
            provider
        );

        tracing::debug!("Querying lease: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Lease query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let lease = json
            .get("lease")
            .ok_or_else(|| anyhow!("Missing 'lease' in response"))?;

        let state_str = lease
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("invalid");

        let price = lease.get("price");
        let (price_denom, price_amount) = if let Some(p) = price {
            (
                p.get("denom")
                    .and_then(|d| d.as_str())
                    .unwrap_or("uakt")
                    .to_string(),
                p.get("amount")
                    .and_then(|a| a.as_str())
                    .unwrap_or("0")
                    .to_string(),
            )
        } else {
            ("uakt".to_string(), "0".to_string())
        };

        Ok(LeaseInfo {
            owner: owner.to_string(),
            dseq,
            gseq,
            oseq,
            provider: provider.to_string(),
            state: match state_str {
                "active" => LeaseState::Active,
                "insufficient_funds" => LeaseState::InsufficientFunds,
                "closed" => LeaseState::Closed,
                _ => LeaseState::Invalid,
            },
            price_denom,
            price_amount,
        })
    }

    /// Query leases for an owner.
    ///
    /// REST: /akash/market/v1beta4/leases/list?filters.owner={owner}
    pub async fn query_leases(&self, owner: &str) -> Result<Vec<LeaseInfo>> {
        let url = format!(
            "{}/akash/market/v1beta4/leases/list?filters.owner={}",
            self.endpoints.rest.trim_end_matches('/'),
            owner
        );

        tracing::debug!("Querying leases: {}", url);

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Leases query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let empty_vec = vec![];
        let leases = json
            .get("leases")
            .and_then(|l| l.as_array())
            .unwrap_or(&empty_vec);

        let mut result = Vec::new();
        for lease_entry in leases {
            let lease = lease_entry.get("lease");
            if let Some(l) = lease {
                let lease_id = l.get("lease_id");
                let price = l.get("price");

                if let Some(id) = lease_id {
                    let state_str = l.get("state").and_then(|s| s.as_str()).unwrap_or("invalid");

                    let (price_denom, price_amount) = if let Some(p) = price {
                        (
                            p.get("denom")
                                .and_then(|d| d.as_str())
                                .unwrap_or("uakt")
                                .to_string(),
                            p.get("amount")
                                .and_then(|a| a.as_str())
                                .unwrap_or("0")
                                .to_string(),
                        )
                    } else {
                        ("uakt".to_string(), "0".to_string())
                    };

                    result.push(LeaseInfo {
                        owner: id
                            .get("owner")
                            .and_then(|o| o.as_str())
                            .unwrap_or("")
                            .to_string(),
                        dseq: id
                            .get("dseq")
                            .and_then(|d| d.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                        gseq: id
                            .get("gseq")
                            .and_then(|g| g.as_u64())
                            .unwrap_or(0) as u32,
                        oseq: id
                            .get("oseq")
                            .and_then(|o| o.as_u64())
                            .unwrap_or(0) as u32,
                        provider: id
                            .get("provider")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_string(),
                        state: match state_str {
                            "active" => LeaseState::Active,
                            "insufficient_funds" => LeaseState::InsufficientFunds,
                            "closed" => LeaseState::Closed,
                            _ => LeaseState::Invalid,
                        },
                        price_denom,
                        price_amount,
                    });
                }
            }
        }

        Ok(result)
    }

    /// Query active leases for an owner.
    pub async fn query_active_leases(&self, owner: &str) -> Result<Vec<LeaseInfo>> {
        let leases = self.query_leases(owner).await?;
        Ok(leases
            .into_iter()
            .filter(|l| l.state == LeaseState::Active)
            .collect())
    }
}

// ===== Query Result Types =====

/// Certificate information.
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub owner: String,
    pub serial: String,
    pub state: CertState,
    pub cert_pem: String,
    pub pubkey: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertState {
    Invalid,
    Valid,
    Revoked,
}

/// Deployment information.
#[derive(Debug, Clone)]
pub struct DeploymentInfo {
    pub owner: String,
    pub dseq: u64,
    pub state: DeploymentState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentState {
    Invalid,
    Active,
    Closed,
}

/// Bid information.
#[derive(Debug, Clone)]
pub struct BidInfo {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
    pub price_denom: String,
    pub price_amount: String,
    pub state: BidState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BidState {
    Invalid,
    Open,
    Active,
    Lost,
    Closed,
}

/// Lease information.
#[derive(Debug, Clone)]
pub struct LeaseInfo {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
    pub state: LeaseState,
    pub price_denom: String,
    pub price_amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseState {
    Invalid,
    Active,
    InsufficientFunds,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_akash_mainnet_defaults() {
        let endpoints = CosmosEndpoints::akash_mainnet();
        assert_eq!(endpoints.rest, "https://rest-akash.ecostake.com");
        assert_eq!(endpoints.chain_id, "akashnet-2");
    }

    #[test]
    fn test_with_overrides() {
        let endpoints = CosmosEndpoints::akash_mainnet()
            .with_overrides(Some("https://custom.endpoint"), Some("testnet-1"));
        assert_eq!(endpoints.rest, "https://custom.endpoint");
        assert_eq!(endpoints.chain_id, "testnet-1");
    }

    #[test]
    fn test_empty_overrides_ignored() {
        let endpoints = CosmosEndpoints::akash_mainnet().with_overrides(Some(""), None);
        assert_eq!(endpoints.rest, "https://rest-akash.ecostake.com");
        assert_eq!(endpoints.chain_id, "akashnet-2");
    }
}
