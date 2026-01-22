//! Authz and Feegrant Management for Akash Deployments
//!
//! This module provides functionality for setting up and managing:
//! - Authz generic permissions for deployment operations
//! - Feegrant allowances with spend limits and expiration
//! - Querying existing grants and allowances
//! - Revoking permissions

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{AkashAuthzGrant, AkashFeegrantAllowance};
use pbjson_types::Timestamp;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};

/// Akash deployment message type URLs for authz grants
pub mod msg_types {
    /// Create deployment authorization
    pub const MSG_CREATE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgCreateDeployment";
    /// Update deployment authorization
    pub const MSG_UPDATE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgUpdateDeployment";
    /// Close deployment authorization
    pub const MSG_CLOSE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgCloseDeployment";
    /// Create lease authorization
    pub const MSG_CREATE_LEASE: &str = "/akash.market.v1beta4.MsgCreateLease";
    /// Close bid authorization
    pub const MSG_CLOSE_BID: &str = "/akash.market.v1beta4.MsgCloseBid";
    /// Withdraw lease authorization
    pub const MSG_WITHDRAW_LEASE: &str = "/akash.market.v1beta4.MsgWithdrawLease";
    /// Create certificate authorization
    pub const MSG_CREATE_CERTIFICATE: &str = "/akash.cert.v1beta3.MsgCreateCertificate";
    /// Revoke certificate authorization
    pub const MSG_REVOKE_CERTIFICATE: &str = "/akash.cert.v1beta3.MsgRevokeCertificate";

    /// All deployment-related message types for full workflow authorization
    pub fn all_deployment_msg_types() -> Vec<&'static str> {
        vec![
            MSG_CREATE_DEPLOYMENT,
            MSG_UPDATE_DEPLOYMENT,
            MSG_CLOSE_DEPLOYMENT,
            MSG_CREATE_LEASE,
            MSG_CLOSE_BID,
            MSG_WITHDRAW_LEASE,
            MSG_CREATE_CERTIFICATE,
            MSG_REVOKE_CERTIFICATE,
        ]
    }
}

/// Default authz expiration (24 hours)
pub const DEFAULT_AUTHZ_EXPIRATION_HOURS: u64 = 24;

/// Default feegrant spend limit (5 AKT in uakt)
pub const DEFAULT_SPEND_LIMIT_UAKT: u64 = 5_000_000;

/// Manager for Authz and Feegrant operations
pub struct AkashAuthzManager {
    http_client: HttpClient,
    rest_endpoint: String,
    chain_id: String,
}

impl AkashAuthzManager {
    pub fn new(rest_endpoint: String, chain_id: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            rest_endpoint: rest_endpoint.trim_end_matches('/').to_string(),
            chain_id,
        }
    }

    // ==================== Query Functions ====================

    /// Query existing authz grants for a granter/grantee pair
    pub async fn query_authz_grants(
        &self,
        granter: &str,
        grantee: &str,
    ) -> Result<Vec<AuthzGrantInfo>> {
        let url = format!(
            "{}/cosmos/authz/v1beta1/grants?granter={}&grantee={}",
            self.rest_endpoint, granter, grantee
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            // 404 or similar means no grants exist
            if status.as_u16() == 404 {
                return Ok(vec![]);
            }
            return Err(anyhow!("Query authz grants failed ({}): {}", status, error_text));
        }

        let grants_response: AuthzGrantsResponse = response.json().await?;
        Ok(grants_response.grants)
    }

    /// Query existing feegrant allowances for a granter/grantee pair
    pub async fn query_feegrant_allowance(
        &self,
        granter: &str,
        grantee: &str,
    ) -> Result<Option<FeegrantAllowanceInfo>> {
        let url = format!(
            "{}/cosmos/feegrant/v1beta1/allowance/{}/{}",
            self.rest_endpoint, granter, grantee
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            // 404 means no allowance exists
            if status.as_u16() == 404 {
                return Ok(None);
            }
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Query feegrant failed ({}): {}", status, error_text));
        }

        let allowance_response: FeegrantAllowanceResponse = response.json().await?;
        Ok(Some(allowance_response.allowance))
    }

    /// Check if a specific authz grant exists and is active
    pub async fn has_authz_grant(
        &self,
        granter: &str,
        grantee: &str,
        msg_type: &str,
    ) -> Result<bool> {
        let grants = self.query_authz_grants(granter, grantee).await?;

        for grant in grants {
            if let Some(auth) = &grant.authorization {
                // Check for GenericAuthorization with matching msg type
                if auth.type_url == "/cosmos.authz.v1beta1.GenericAuthorization" {
                    if let Some(msg) = &auth.msg {
                        if msg == msg_type {
                            // Check expiration
                            if let Some(exp) = &grant.expiration {
                                if !is_expired(exp) {
                                    return Ok(true);
                                }
                            } else {
                                // No expiration means it's valid
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    /// Check if a feegrant allowance exists and has remaining spend
    pub async fn has_active_feegrant(&self, granter: &str, grantee: &str) -> Result<bool> {
        match self.query_feegrant_allowance(granter, grantee).await? {
            Some(allowance) => {
                // Check expiration if basic allowance
                if let Some(exp) = allowance.expiration {
                    if is_expired(&exp) {
                        return Ok(false);
                    }
                }
                // Check spend limit if applicable
                if let Some(spend_limit) = &allowance.spend_limit {
                    for coin in spend_limit {
                        if coin.denom == "uakt" {
                            let amount: u64 = coin.amount.parse().unwrap_or(0);
                            if amount == 0 {
                                return Ok(false);
                            }
                        }
                    }
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    // ==================== Transaction Building ====================

    /// Build MsgGrant transaction for GenericAuthorization
    pub fn build_authz_grant_msg(
        &self,
        granter: &str,
        grantee: &str,
        msg_type: &str,
        expiration_hours: Option<u64>,
    ) -> Value {
        let expiration = calculate_expiration(expiration_hours.unwrap_or(DEFAULT_AUTHZ_EXPIRATION_HOURS));

        json!({
            "@type": "/cosmos.authz.v1beta1.MsgGrant",
            "granter": granter,
            "grantee": grantee,
            "grant": {
                "authorization": {
                    "@type": "/cosmos.authz.v1beta1.GenericAuthorization",
                    "msg": msg_type
                },
                "expiration": expiration
            }
        })
    }

    /// Build MsgRevoke transaction
    pub fn build_authz_revoke_msg(&self, granter: &str, grantee: &str, msg_type: &str) -> Value {
        json!({
            "@type": "/cosmos.authz.v1beta1.MsgRevoke",
            "granter": granter,
            "grantee": grantee,
            "msg_type_url": msg_type
        })
    }

    /// Build MsgGrantAllowance transaction for BasicAllowance
    pub fn build_feegrant_msg(
        &self,
        granter: &str,
        grantee: &str,
        spend_limit_uakt: Option<u64>,
        expiration_hours: Option<u64>,
    ) -> Value {
        let expiration = calculate_expiration(expiration_hours.unwrap_or(DEFAULT_AUTHZ_EXPIRATION_HOURS));
        let spend_limit = spend_limit_uakt.unwrap_or(DEFAULT_SPEND_LIMIT_UAKT);

        json!({
            "@type": "/cosmos.feegrant.v1beta1.MsgGrantAllowance",
            "granter": granter,
            "grantee": grantee,
            "allowance": {
                "@type": "/cosmos.feegrant.v1beta1.BasicAllowance",
                "spend_limit": [{
                    "denom": "uakt",
                    "amount": spend_limit.to_string()
                }],
                "expiration": expiration
            }
        })
    }

    /// Build MsgRevokeAllowance transaction
    pub fn build_feegrant_revoke_msg(&self, granter: &str, grantee: &str) -> Value {
        json!({
            "@type": "/cosmos.feegrant.v1beta1.MsgRevokeAllowance",
            "granter": granter,
            "grantee": grantee
        })
    }

    /// Build all authz grant messages for complete deployment workflow
    pub fn build_all_authz_grants(
        &self,
        granter: &str,
        grantee: &str,
        expiration_hours: Option<u64>,
    ) -> Vec<Value> {
        msg_types::all_deployment_msg_types()
            .into_iter()
            .map(|msg_type| {
                self.build_authz_grant_msg(granter, grantee, msg_type, expiration_hours)
            })
            .collect()
    }

    // ==================== High-Level Operations ====================

    /// Check and track authz grants, returning which ones need to be created
    pub async fn check_existing_grants(
        &self,
        granter: &str,
        grantee: &str,
    ) -> Result<AuthzStatus> {
        let mut status = AuthzStatus::default();

        // Check each deployment message type
        for msg_type in msg_types::all_deployment_msg_types() {
            let has_grant = self.has_authz_grant(granter, grantee, msg_type).await?;
            if has_grant {
                status.existing_grants.push(msg_type.to_string());
            } else {
                status.missing_grants.push(msg_type.to_string());
            }
        }

        // Check feegrant
        status.has_feegrant = self.has_active_feegrant(granter, grantee).await?;

        Ok(status)
    }

    /// Create tracking records for grants (for storage)
    pub fn create_grant_record(
        &self,
        granter: &str,
        grantee: &str,
        msg_types: &[&str],
        expiration_hours: u64,
        tx_hash: &str,
    ) -> AkashAuthzGrant {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let expiration_secs = now.as_secs() + (expiration_hours * 3600);

        AkashAuthzGrant {
            granter: granter.to_string(),
            grantee: grantee.to_string(),
            msg_type_urls: msg_types.iter().map(|s| s.to_string()).collect(),
            expiration: Some(Timestamp {
                seconds: expiration_secs as i64,
                nanos: 0,
            }),
            grant_tx_hash: tx_hash.to_string(),
            active: true,
        }
    }

    /// Create feegrant tracking record (for storage)
    pub fn create_feegrant_record(
        &self,
        granter: &str,
        grantee: &str,
        spend_limit_uakt: u64,
        expiration_hours: u64,
        tx_hash: &str,
        allowed_messages: Option<Vec<String>>,
    ) -> AkashFeegrantAllowance {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let expiration_secs = now.as_secs() + (expiration_hours * 3600);

        AkashFeegrantAllowance {
            granter: granter.to_string(),
            grantee: grantee.to_string(),
            spend_limit_uakt,
            spent_uakt: 0,
            expiration: Some(Timestamp {
                seconds: expiration_secs as i64,
                nanos: 0,
            }),
            allowed_messages: allowed_messages.unwrap_or_default(),
            grant_tx_hash: tx_hash.to_string(),
            active: true,
        }
    }
}

// ==================== Response Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthzGrantsResponse {
    pub grants: Vec<AuthzGrantInfo>,
    #[serde(default)]
    pub pagination: Option<PaginationResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthzGrantInfo {
    pub authorization: Option<AuthorizationInfo>,
    pub expiration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationInfo {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeegrantAllowanceResponse {
    pub allowance: FeegrantAllowanceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeegrantAllowanceInfo {
    pub granter: String,
    pub grantee: String,
    pub allowance: Option<AllowanceDetails>,
    pub spend_limit: Option<Vec<Coin>>,
    pub expiration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowanceDetails {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub spend_limit: Option<Vec<Coin>>,
    pub expiration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coin {
    pub denom: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationResponse {
    pub next_key: Option<String>,
    pub total: Option<String>,
}

/// Status of authz grants for a granter/grantee pair
#[derive(Debug, Clone, Default)]
pub struct AuthzStatus {
    /// Message types that already have active grants
    pub existing_grants: Vec<String>,
    /// Message types that need grants
    pub missing_grants: Vec<String>,
    /// Whether an active feegrant exists
    pub has_feegrant: bool,
}

impl AuthzStatus {
    /// Check if all required grants exist
    pub fn has_all_grants(&self) -> bool {
        self.missing_grants.is_empty()
    }

    /// Check if fully authorized (all grants + feegrant)
    pub fn is_fully_authorized(&self) -> bool {
        self.has_all_grants() && self.has_feegrant
    }
}

// ==================== Helper Functions ====================

/// Calculate expiration timestamp string (RFC3339 format)
fn calculate_expiration(hours: u64) -> String {
    let expiration = SystemTime::now() + Duration::from_secs(hours * 3600);
    let datetime: chrono::DateTime<chrono::Utc> = expiration.into();
    datetime.to_rfc3339()
}

/// Check if an RFC3339 timestamp is expired
fn is_expired(expiration: &str) -> bool {
    match chrono::DateTime::parse_from_rfc3339(expiration) {
        Ok(exp_time) => {
            let now = chrono::Utc::now();
            exp_time < now
        }
        Err(_) => {
            // If we can't parse, assume not expired
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_authz_grant_msg() {
        let manager = AkashAuthzManager::new(
            "https://rest-akash.ecostake.com".to_string(),
            "akashnet-2".to_string(),
        );

        let msg = manager.build_authz_grant_msg(
            "akash1granter",
            "akash1grantee",
            msg_types::MSG_CREATE_DEPLOYMENT,
            Some(24),
        );

        assert_eq!(msg["@type"], "/cosmos.authz.v1beta1.MsgGrant");
        assert_eq!(msg["granter"], "akash1granter");
        assert_eq!(msg["grantee"], "akash1grantee");
    }

    #[test]
    fn test_build_feegrant_msg() {
        let manager = AkashAuthzManager::new(
            "https://rest-akash.ecostake.com".to_string(),
            "akashnet-2".to_string(),
        );

        let msg = manager.build_feegrant_msg(
            "akash1granter",
            "akash1grantee",
            Some(5_000_000),
            Some(24),
        );

        assert_eq!(msg["@type"], "/cosmos.feegrant.v1beta1.MsgGrantAllowance");
        assert_eq!(msg["granter"], "akash1granter");
    }

    #[test]
    fn test_all_deployment_msg_types() {
        let msg_types = msg_types::all_deployment_msg_types();
        assert!(msg_types.len() >= 6);
        assert!(msg_types.contains(&msg_types::MSG_CREATE_DEPLOYMENT));
        assert!(msg_types.contains(&msg_types::MSG_CREATE_LEASE));
    }

    #[test]
    fn test_create_grant_record() {
        let manager = AkashAuthzManager::new(
            "https://rest-akash.ecostake.com".to_string(),
            "akashnet-2".to_string(),
        );

        let record = manager.create_grant_record(
            "akash1granter",
            "akash1grantee",
            &[msg_types::MSG_CREATE_DEPLOYMENT, msg_types::MSG_CREATE_LEASE],
            24,
            "ABC123TXHASH",
        );

        assert_eq!(record.granter, "akash1granter");
        assert_eq!(record.grantee, "akash1grantee");
        assert_eq!(record.msg_type_urls.len(), 2);
        assert!(record.active);
    }

    #[test]
    fn test_authz_status() {
        let mut status = AuthzStatus::default();
        // Empty missing_grants means all grants are present
        assert!(status.has_all_grants());

        status.missing_grants.push(msg_types::MSG_CREATE_DEPLOYMENT.to_string());
        assert!(!status.has_all_grants());

        status.missing_grants.clear();
        status.existing_grants.push(msg_types::MSG_CREATE_DEPLOYMENT.to_string());
        assert!(status.has_all_grants());
    }
}
