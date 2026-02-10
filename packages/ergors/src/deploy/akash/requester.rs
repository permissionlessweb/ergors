//! Grant Request Initiator (Requester Service)
//!
//! This module provides the requester-side service for requesting
//! grants from granter nodes. It:
//! - Submits grant requests to the grant-manager contract
//! - Polls for request status updates
//! - Handles grant confirmation and rejection

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{
    GrantRequest, GrantRequestParams, GrantRequestStatus, GrantType,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Default timeout for waiting for grant approval
pub const DEFAULT_GRANT_TIMEOUT_SECONDS: u64 = 300; // 5 minutes

/// Poll interval for checking grant status
pub const GRANT_POLL_INTERVAL_SECONDS: u64 = 5;

/// Requester service for submitting grant requests
pub struct GrantRequesterService {
    /// Node's public key (requester identity)
    node_pubkey: Vec<u8>,
    /// Node's cosmos address (grantee)
    grantee_address: String,
    /// Contract address for grant-manager
    contract_address: String,
    /// Active requests being tracked
    active_requests: Arc<RwLock<HashMap<u64, GrantRequest>>>,
    /// Request ID counter (local tracking)
    next_local_id: Arc<RwLock<u64>>,
}

impl GrantRequesterService {
    /// Create a new requester service
    pub fn new(
        node_pubkey: Vec<u8>,
        grantee_address: String,
        contract_address: String,
    ) -> Self {
        Self {
            node_pubkey,
            grantee_address,
            contract_address,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
            next_local_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Set contract address
    pub fn set_contract_address(&mut self, address: &str) {
        self.contract_address = address.to_string();
    }

    /// Request a grant from a specific granter
    pub async fn request_grant(
        &self,
        granter_pubkey: Vec<u8>,
        grant_type: GrantType,
        params: GrantRequestParams,
    ) -> Result<GrantRequest> {
        if self.contract_address.is_empty() {
            return Err(anyhow!("Grant manager contract address not configured"));
        }

        info!(
            "Submitting grant request to granter (pubkey: {} bytes)",
            granter_pubkey.len()
        );

        // Generate local tracking ID
        let local_id = {
            let mut id = self.next_local_id.write().await;
            let current = *id;
            *id += 1;
            current
        };

        let request = GrantRequest {
            id: local_id, // Will be replaced by contract-assigned ID
            requester_pubkey: self.node_pubkey.clone(),
            grantee_address: self.grantee_address.clone(),
            granter_pubkey: granter_pubkey.clone(),
            granter_address: String::new(), // Will be resolved by contract
            grant_type: grant_type as i32,
            params: Some(params.clone()),
            status: GrantRequestStatus::Pending as i32,
            created_at: Some(current_timestamp()),
            updated_at: Some(current_timestamp()),
            tx_hash: String::new(),
            rejection_reason: String::new(),
        };

        // In production: submit to contract via ExecuteMsg::RequestGrant
        // For now, track locally and simulate contract submission
        let mut active = self.active_requests.write().await;
        active.insert(local_id, request.clone());

        info!(
            "Grant request submitted: local_id={}, type={:?}, duration={}s, limit={} uakt",
            local_id,
            grant_type,
            params.duration_seconds,
            params.spend_limit_uakt
        );

        Ok(request)
    }

    /// Request authz and feegrant together (common case)
    pub async fn request_authz_and_feegrant(
        &self,
        granter_pubkey: Vec<u8>,
        duration_seconds: u64,
        spend_limit_uakt: u64,
        purpose: &str,
    ) -> Result<GrantRequest> {
        let params = GrantRequestParams {
            duration_seconds,
            spend_limit_uakt,
            msg_types: default_akash_msg_types(),
            purpose: purpose.to_string(),
        };

        self.request_grant(granter_pubkey, GrantType::AuthzAndFeegrant, params).await
    }

    /// Find available granters from contract
    pub async fn find_available_granters(
        &self,
        _params: &GrantRequestParams,
    ) -> Result<Vec<Vec<u8>>> {
        if self.contract_address.is_empty() {
            return Err(anyhow!("Grant manager contract address not configured"));
        }

        // In production: query contract via QueryMsg::FindGranters
        // For now, return empty list (caller should specify granter explicitly)
        warn!("find_available_granters not implemented - specify granter explicitly");
        Ok(vec![])
    }

    /// Query status of a grant request
    pub async fn query_request_status(&self, request_id: u64) -> Result<GrantRequest> {
        // Check local cache first
        let active = self.active_requests.read().await;
        if let Some(request) = active.get(&request_id) {
            return Ok(request.clone());
        }
        drop(active);

        // In production: query contract via QueryMsg::Request
        Err(anyhow!("Request {} not found", request_id))
    }

    /// Wait for grant to be confirmed with timeout
    pub async fn wait_for_grant(
        &self,
        request_id: u64,
        timeout_seconds: u64,
    ) -> Result<GrantRequest> {
        let deadline = SystemTime::now() + Duration::from_secs(timeout_seconds);

        loop {
            let request = self.query_request_status(request_id).await?;

            let status = GrantRequestStatus::try_from(request.status)
                .unwrap_or(GrantRequestStatus::Unspecified);

            match status {
                GrantRequestStatus::Confirmed => {
                    info!("Grant request {} confirmed: tx_hash={}", request_id, request.tx_hash);
                    return Ok(request);
                }
                GrantRequestStatus::Rejected => {
                    return Err(anyhow!(
                        "Grant request rejected: {}",
                        request.rejection_reason
                    ));
                }
                GrantRequestStatus::Cancelled => {
                    return Err(anyhow!("Grant request was cancelled"));
                }
                GrantRequestStatus::Expired => {
                    return Err(anyhow!("Grant request expired"));
                }
                GrantRequestStatus::Pending
                | GrantRequestStatus::Approved
                | GrantRequestStatus::Broadcasted => {
                    // Still waiting
                    if SystemTime::now() > deadline {
                        return Err(anyhow!(
                            "Timeout waiting for grant (status: {:?})",
                            status
                        ));
                    }

                    debug!("Grant request {} status: {:?}, waiting...", request_id, status);
                    tokio::time::sleep(Duration::from_secs(GRANT_POLL_INTERVAL_SECONDS)).await;
                }
                GrantRequestStatus::Unspecified => {
                    return Err(anyhow!("Invalid grant request status"));
                }
            }
        }
    }

    /// Cancel a pending grant request
    pub async fn cancel_request(&self, request_id: u64) -> Result<()> {
        let mut active = self.active_requests.write().await;

        if let Some(request) = active.get_mut(&request_id) {
            let status = GrantRequestStatus::try_from(request.status)
                .unwrap_or(GrantRequestStatus::Unspecified);

            if status != GrantRequestStatus::Pending {
                return Err(anyhow!(
                    "Can only cancel pending requests (current: {:?})",
                    status
                ));
            }

            request.status = GrantRequestStatus::Cancelled as i32;
            request.updated_at = Some(current_timestamp());

            // In production: send CancelRequest to contract
            info!("Grant request {} cancelled", request_id);
            Ok(())
        } else {
            Err(anyhow!("Request {} not found", request_id))
        }
    }

    /// Update request status (called when receiving updates from contract events)
    pub async fn update_request_status(
        &self,
        request_id: u64,
        status: GrantRequestStatus,
        tx_hash: Option<&str>,
        rejection_reason: Option<&str>,
    ) -> Result<()> {
        let mut active = self.active_requests.write().await;

        if let Some(request) = active.get_mut(&request_id) {
            request.status = status as i32;
            if let Some(hash) = tx_hash {
                request.tx_hash = hash.to_string();
            }
            if let Some(reason) = rejection_reason {
                request.rejection_reason = reason.to_string();
            }
            request.updated_at = Some(current_timestamp());

            info!("Grant request {} status updated to {:?}", request_id, status);
            Ok(())
        } else {
            Err(anyhow!("Request {} not found", request_id))
        }
    }

    /// List all active/pending requests
    pub async fn list_requests(&self) -> Vec<GrantRequest> {
        self.active_requests.read().await.values().cloned().collect()
    }

    /// List pending requests only
    pub async fn list_pending_requests(&self) -> Vec<GrantRequest> {
        self.active_requests
            .read()
            .await
            .values()
            .filter(|r| r.status == GrantRequestStatus::Pending as i32)
            .cloned()
            .collect()
    }

    /// Get confirmed grants (for verification)
    pub async fn list_confirmed_grants(&self) -> Vec<GrantRequest> {
        self.active_requests
            .read()
            .await
            .values()
            .filter(|r| r.status == GrantRequestStatus::Confirmed as i32)
            .cloned()
            .collect()
    }

    /// Clear completed/rejected requests from tracking
    pub async fn cleanup_completed(&self) -> usize {
        let mut active = self.active_requests.write().await;
        let initial_len = active.len();

        active.retain(|_, r| {
            let status = GrantRequestStatus::try_from(r.status)
                .unwrap_or(GrantRequestStatus::Unspecified);
            matches!(
                status,
                GrantRequestStatus::Pending
                    | GrantRequestStatus::Approved
                    | GrantRequestStatus::Broadcasted
            )
        });

        let removed = initial_len - active.len();
        if removed > 0 {
            debug!("Cleaned up {} completed/rejected grant requests", removed);
        }
        removed
    }
}

/// Default Akash message types to authorize
fn default_akash_msg_types() -> Vec<String> {
    vec![
        "/akash.deployment.v1beta3.MsgCreateDeployment".to_string(),
        "/akash.deployment.v1beta3.MsgUpdateDeployment".to_string(),
        "/akash.deployment.v1beta3.MsgCloseDeployment".to_string(),
        "/akash.market.v1beta4.MsgCreateLease".to_string(),
        "/akash.market.v1beta4.MsgCloseBid".to_string(),
    ]
}

/// Get current timestamp
fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_requester_service_creation() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            "akash1contract...".to_string(),
        );

        let requests = service.list_requests().await;
        assert!(requests.is_empty());
    }

    #[tokio::test]
    async fn test_submit_grant_request() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            "akash1contract...".to_string(),
        );

        let request = service
            .request_authz_and_feegrant(
                vec![10, 20, 30, 40],
                86400,
                5_000_000,
                "Test deployment",
            )
            .await
            .unwrap();

        assert_eq!(request.status, GrantRequestStatus::Pending as i32);
        assert_eq!(request.grant_type, GrantType::AuthzAndFeegrant as i32);

        // Should be in active requests
        let requests = service.list_requests().await;
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_pending_request() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            "akash1contract...".to_string(),
        );

        let request = service
            .request_authz_and_feegrant(
                vec![10, 20, 30, 40],
                86400,
                5_000_000,
                "Test",
            )
            .await
            .unwrap();

        service.cancel_request(request.id).await.unwrap();

        let updated = service.query_request_status(request.id).await.unwrap();
        assert_eq!(updated.status, GrantRequestStatus::Cancelled as i32);
    }

    #[tokio::test]
    async fn test_update_request_status() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            "akash1contract...".to_string(),
        );

        let request = service
            .request_authz_and_feegrant(
                vec![10, 20, 30, 40],
                86400,
                5_000_000,
                "Test",
            )
            .await
            .unwrap();

        service
            .update_request_status(
                request.id,
                GrantRequestStatus::Confirmed,
                Some("tx_hash_123"),
                None,
            )
            .await
            .unwrap();

        let updated = service.query_request_status(request.id).await.unwrap();
        assert_eq!(updated.status, GrantRequestStatus::Confirmed as i32);
        assert_eq!(updated.tx_hash, "tx_hash_123");
    }

    #[tokio::test]
    async fn test_cleanup_completed() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            "akash1contract...".to_string(),
        );

        // Create multiple requests
        let req1 = service
            .request_authz_and_feegrant(vec![10], 86400, 5_000_000, "Test1")
            .await
            .unwrap();
        let req2 = service
            .request_authz_and_feegrant(vec![20], 86400, 5_000_000, "Test2")
            .await
            .unwrap();

        // Confirm one, reject another
        service
            .update_request_status(req1.id, GrantRequestStatus::Confirmed, Some("tx1"), None)
            .await
            .unwrap();
        service
            .update_request_status(req2.id, GrantRequestStatus::Rejected, None, Some("rejected"))
            .await
            .unwrap();

        // Cleanup should remove both
        let removed = service.cleanup_completed().await;
        assert_eq!(removed, 2);

        let requests = service.list_requests().await;
        assert!(requests.is_empty());
    }

    #[tokio::test]
    async fn test_no_contract_address() {
        let service = GrantRequesterService::new(
            vec![1, 2, 3, 4],
            "akash1grantee...".to_string(),
            String::new(), // No contract address
        );

        let result = service
            .request_authz_and_feegrant(vec![10], 86400, 5_000_000, "Test")
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not configured"));
    }
}
