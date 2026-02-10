//! Akash-specific blockchain query client.
//!
//! Wraps CosmosBaseClient with Akash Network query endpoints for
//! certificates, deployments, bids, leases, escrow, and providers.

use anyhow::{anyhow, Result};
use base64::prelude::{Engine, BASE64_STANDARD};
use ho_std::types::akash::cert::v1::{Certificate, CertificateResponse, State};
use ho_std::types::ergors::orch::v1::AkashDeployConfig;
use reqwest::Client as HttpClient;

use crate::chains::cosmos::{ChainConfig, CosmosBaseClient};
use super::types::*;

/// Akash-specific blockchain client.
///
/// Composes a `CosmosBaseClient` for generic queries and adds Akash-specific
/// REST endpoints for deployments, bids, leases, escrow, and certificates.
pub struct AkashClient {
    base: CosmosBaseClient,
    http: HttpClient,
}

impl AkashClient {
    /// Create a new AkashClient with Akash mainnet defaults.
    pub fn new() -> Result<Self> {
        let config = ChainConfig::akash();
        let http = HttpClient::builder().timeout(config.timeout).build()?;
        let base = CosmosBaseClient::new(config)?;
        Ok(Self { base, http })
    }

    /// Create from an existing CosmosBaseClient.
    pub fn from_base(base: CosmosBaseClient) -> Result<Self> {
        let http = HttpClient::builder()
            .timeout(base.config().timeout)
            .build()?;
        Ok(Self { base, http })
    }

    /// Create from AkashDeployConfig proto.
    pub fn from_akash_config(config: &AkashDeployConfig) -> Result<Self> {
        // Use first endpoint from rest_endpoints array
        let rest = if !config.rest_endpoints.is_empty() {
            config.rest_endpoints[0].clone()
        } else if !config.rpc_endpoints.is_empty() {
            config.rpc_endpoints[0]
                .replace("rpc-", "rest-")
                .replace(":443", "")
        } else {
            String::new()
        };

        let rest_endpoint = if rest.is_empty() {
            "https://rest-akash.ecostake.com".to_string()
        } else {
            rest
        };

        let chain_id = if config.chain_id.is_empty() {
            "akashnet-2".to_string()
        } else {
            config.chain_id.clone()
        };

        let chain_config = ChainConfig::akash()
            .with_endpoints(Some(rest_endpoint), None)
            .with_chain_id(chain_id);

        let http = HttpClient::builder()
            .timeout(chain_config.timeout)
            .build()?;
        let base = CosmosBaseClient::new(chain_config)?;
        Ok(Self { base, http })
    }

    /// Get reference to the underlying CosmosBaseClient.
    pub fn base(&self) -> &CosmosBaseClient {
        &self.base
    }

    /// Get chain ID.
    pub fn chain_id(&self) -> &str {
        self.base.chain_id()
    }

    /// Get REST endpoint.
    pub fn rest_endpoint(&self) -> &str {
        self.base.rest_endpoint()
    }

    // ===== Centralized Akash REST Endpoint Builders =====

    fn cert_list_endpoint(rest: &str, owner: &str) -> String {
        format!(
            "{}/akash/cert/v1/certificates/list?filter.owner={}&pagination.limit=1000&filter.state=valid&pagination.count_total=true",
            rest.trim_end_matches('/'),
            owner
        )
    }

    fn deployment_info_endpoint(rest: &str, owner: &str, dseq: u64) -> String {
        format!(
            "{}/akash/deployment/v1beta4/deployments/info?id.owner={}&id.dseq={}",
            rest.trim_end_matches('/'),
            owner,
            dseq
        )
    }

    fn deployment_list_endpoint(rest: &str, owner: &str, state: &str) -> String {
        format!(
            "{}/akash/deployment/v1beta4/deployments/list?filters.owner={}&filters.state={}&pagination.limit=1000&pagination.count_total=true",
            rest.trim_end_matches('/'),
            owner,
            state
        )
    }

    fn bid_list_endpoint(rest: &str, owner: &str, dseq: u64) -> String {
        format!(
            "{}/akash/market/v1beta5/bids/list?filters.owner={}&filters.dseq={}",
            rest.trim_end_matches('/'),
            owner,
            dseq
        )
    }

    fn lease_info_endpoint(
        rest: &str,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        provider: &str,
    ) -> String {
        format!(
            "{}/akash/market/v1beta5/leases/info?id.owner={}&id.dseq={}&id.gseq={}&id.oseq={}&id.provider={}",
            rest.trim_end_matches('/'),
            owner,
            dseq,
            gseq,
            oseq,
            provider
        )
    }

    fn lease_list_endpoint(rest: &str, owner: &str) -> String {
        format!(
            "{}/akash/market/v1beta5/leases/list?filters.owner={}",
            rest.trim_end_matches('/'),
            owner
        )
    }

    fn escrow_accounts_endpoint(rest: &str, scope: &str, xid: &str) -> String {
        format!(
            "{}/akash/escrow/v1/accounts/list?scope={}&xid={}",
            rest.trim_end_matches('/'),
            scope,
            xid
        )
    }

    // ===== Certificate Queries =====

    /// Query certificates for an owner address.
    pub async fn query_certificates(&self, owner: &str) -> Result<Vec<CertificateResponse>> {
        let url = Self::cert_list_endpoint(self.rest_endpoint(), owner);
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
        for cert_json in certs {
            if let (Some(serial), Some(cert_data)) = (
                cert_json.get("serial").and_then(|s| s.as_str()),
                cert_json.get("certificate"),
            ) {
                let state_str = cert_data
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("invalid");
                let state = match state_str {
                    "valid" => State::Valid as i32,
                    "revoked" => State::Revoked as i32,
                    _ => State::Invalid as i32,
                };

                let cert_bytes = cert_data
                    .get("cert")
                    .and_then(|c| c.as_str())
                    .and_then(|s| BASE64_STANDARD.decode(s).ok())
                    .unwrap_or_default();

                let pubkey_bytes = cert_data
                    .get("pubkey")
                    .and_then(|p| p.as_str())
                    .and_then(|s| BASE64_STANDARD.decode(s).ok())
                    .unwrap_or_default();

                result.push(CertificateResponse {
                    certificate: Some(Certificate {
                        state,
                        cert: cert_bytes,
                        pubkey: pubkey_bytes,
                    }),
                    serial: serial.to_string(),
                });
            }
        }
        Ok(result)
    }

    /// Query valid certificate for an owner (first valid cert found).
    pub async fn query_valid_certificate(&self, owner: &str) -> Result<Option<Certificate>> {
        let responses = self.query_certificates(owner).await?;
        for cert_response in responses {
            if let Some(cert) = cert_response.certificate {
                if cert.state == State::Valid as i32 {
                    return Ok(Some(cert));
                }
            }
        }
        Ok(None)
    }

    // ===== Deployment Queries =====

    /// Query deployment by owner and dseq.
    pub async fn query_deployment(&self, owner: &str, dseq: u64) -> Result<DeploymentInfo> {
        let url = Self::deployment_info_endpoint(self.rest_endpoint(), owner, dseq);
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

    /// Query deployments for an owner filtered by state.
    pub async fn query_deployments(
        &self,
        owner: &str,
        state: &str,
    ) -> Result<Vec<DeploymentInfo>> {
        let url = Self::deployment_list_endpoint(self.rest_endpoint(), owner, state);
        tracing::debug!("Querying deployments: {}", url);

        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Deployments query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let empty_vec = vec![];
        let deployments = json
            .get("deployments")
            .and_then(|d| d.as_array())
            .unwrap_or(&empty_vec);

        let mut result = Vec::new();
        for deployment_entry in deployments {
            let deployment = deployment_entry.get("deployment");
            if let Some(d) = deployment {
                let deployment_id = d.get("deployment_id");
                if let Some(id) = deployment_id {
                    let state_str =
                        d.get("state").and_then(|s| s.as_str()).unwrap_or("invalid");
                    result.push(DeploymentInfo {
                        owner: id
                            .get("owner")
                            .and_then(|o| o.as_str())
                            .unwrap_or("")
                            .to_string(),
                        dseq: id
                            .get("dseq")
                            .and_then(|ds| ds.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                        state: match state_str {
                            "active" => DeploymentState::Active,
                            "closed" => DeploymentState::Closed,
                            _ => DeploymentState::Invalid,
                        },
                    });
                }
            }
        }
        Ok(result)
    }

    /// Query active deployments for an owner.
    pub async fn query_active_deployments(&self, owner: &str) -> Result<Vec<DeploymentInfo>> {
        self.query_deployments(owner, "active").await
    }

    // ===== Bid Queries =====

    /// Query bids for a deployment.
    pub async fn query_bids(&self, owner: &str, dseq: u64) -> Result<Vec<BidInfo>> {
        let url = Self::bid_list_endpoint(self.rest_endpoint(), owner, dseq);
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
                let bid_id = b.get("id");
                let price = b.get("price");

                if let (Some(id), Some(p)) = (bid_id, price) {
                    let state_str =
                        b.get("state").and_then(|s| s.as_str()).unwrap_or("invalid");
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
                        bseq: id
                            .get("bseq")
                            .and_then(|b| b.as_u64())
                            .unwrap_or(0) as u32,
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

    /// Query open bids for a deployment.
    pub async fn query_open_bids(&self, owner: &str, dseq: u64) -> Result<Vec<BidInfo>> {
        let bids = self.query_bids(owner, dseq).await?;
        Ok(bids
            .into_iter()
            .filter(|b| b.state == BidState::Open)
            .collect())
    }

    // ===== Lease Queries =====

    /// Query lease by ID.
    pub async fn query_lease(
        &self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        provider: &str,
    ) -> Result<LeaseInfo> {
        let url =
            Self::lease_info_endpoint(self.rest_endpoint(), owner, dseq, gseq, oseq, provider);
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
    pub async fn query_leases(&self, owner: &str) -> Result<Vec<LeaseInfo>> {
        let url = Self::lease_list_endpoint(self.rest_endpoint(), owner);
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
                    let state_str =
                        l.get("state").and_then(|s| s.as_str()).unwrap_or("invalid");

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

    // ===== Provider Queries =====

    /// Query provider information by address.
    pub async fn query_provider(&self, owner: &str) -> Result<ProviderInfo> {
        let url = format!(
            "{}/akash/provider/v1beta4/providers/{}",
            self.rest_endpoint(),
            owner
        );
        tracing::debug!("Querying provider: {}", url);

        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Provider query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let provider = json
            .get("provider")
            .ok_or_else(|| anyhow!("Missing 'provider' in response"))?;

        let host_uri = provider
            .get("host_uri")
            .and_then(|h| h.as_str())
            .ok_or_else(|| anyhow!("Missing 'host_uri' in provider info"))?
            .to_string();

        let email = provider
            .get("info")
            .and_then(|i| i.get("email"))
            .and_then(|e| e.as_str())
            .unwrap_or("")
            .to_string();

        let website = provider
            .get("info")
            .and_then(|i| i.get("website"))
            .and_then(|w| w.as_str())
            .unwrap_or("")
            .to_string();

        Ok(ProviderInfo {
            owner: owner.to_string(),
            host_uri,
            email,
            website,
        })
    }

    // ===== Escrow Queries =====

    /// Query escrow account for a deployment.
    pub async fn query_deployment_escrow(
        &self,
        owner: &str,
        dseq: u64,
    ) -> Result<Option<EscrowAccountInfo>> {
        let xid = format!("{}/{}", owner, dseq);
        let url = Self::escrow_accounts_endpoint(self.rest_endpoint(), "deployment", &xid);
        tracing::debug!("Querying escrow account: {}", url);

        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Escrow query failed ({}): {}", status, body));
        }

        let json: serde_json::Value = resp.json().await?;
        let empty_vec = vec![];
        let accounts = json
            .get("accounts")
            .and_then(|a| a.as_array())
            .unwrap_or(&empty_vec);

        if let Some(account) = accounts.first() {
            let state = account.get("state");
            if let Some(s) = state {
                let state_str = s
                    .get("state")
                    .and_then(|st| st.as_str())
                    .unwrap_or("invalid");

                let funds = s
                    .get("funds")
                    .and_then(|f| f.as_array())
                    .unwrap_or(&empty_vec);

                let mut total_uakt = 0u64;
                let mut balances = Vec::new();

                for fund in funds {
                    let denom = fund
                        .get("denom")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let amount_str = fund
                        .get("amount")
                        .and_then(|a| a.as_str())
                        .unwrap_or("0");

                    if denom == "uakt" {
                        total_uakt = amount_str.parse().unwrap_or(0);
                    }

                    balances.push(EscrowBalance {
                        denom: denom.to_string(),
                        amount: amount_str.to_string(),
                    });
                }

                return Ok(Some(EscrowAccountInfo {
                    owner: owner.to_string(),
                    dseq,
                    state: match state_str {
                        "open" => EscrowState::Open,
                        "closed" => EscrowState::Closed,
                        "overdrawn" => EscrowState::Overdrawn,
                        _ => EscrowState::Invalid,
                    },
                    balances,
                    total_uakt,
                }));
            }
        }
        Ok(None)
    }
}

// ===== Delegating methods to base client =====

impl std::ops::Deref for AkashClient {
    type Target = CosmosBaseClient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_akash_client_creation() {
        let client = AkashClient::new().unwrap();
        assert_eq!(client.chain_id(), "akashnet-2");
        assert!(client.rest_endpoint().contains("akash"));
    }
}
