//! Grant Request Handler (Granter Service)
//!
//! This module provides the granter-side service for handling incoming
//! grant requests from other nodes. It:
//! - Processes incoming grant requests according to acceptance mode
//! - Manages whitelist of authorized requesters
//! - Broadcasts MsgGrant and MsgGrantAllowance transactions when approved
//! - Tracks active grants and spending

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{
    GrantAcceptanceMode, GrantDefaults, GrantLimits, GrantRequest, GrantRequestParams,
    GrantRequestStatus, GrantType, GranterConfig, GranterInfo, WhitelistEntry,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Default grant parameters
pub const DEFAULT_MAX_DURATION_SECONDS: u64 = 172800; // 48 hours
pub const DEFAULT_MAX_SPEND_LIMIT_UAKT: u64 = 10_000_000; // 10 AKT
pub const DEFAULT_AUTO_APPROVE_DELAY: u64 = 0;

/// Granter service for handling incoming grant requests
pub struct GranterService {
    /// Configuration for this granter
    config: Arc<RwLock<GranterConfig>>,
    /// Pending requests awaiting manual approval
    pending_requests: Arc<RwLock<HashMap<u64, GrantRequest>>>,
    /// Active grants issued by this granter
    active_grants: Arc<RwLock<Vec<GrantRequest>>>,
    /// Request ID counter
    next_request_id: Arc<RwLock<u64>>,
    /// Node's cosmos address (for signing grant transactions)
    granter_address: String,
    /// Node's public key
    node_pubkey: Vec<u8>,
}

impl GranterService {
    /// Create a new granter service
    pub fn new(granter_address: String, node_pubkey: Vec<u8>) -> Self {
        Self {
            config: Arc::new(RwLock::new(GranterConfig {
                enabled: false,
                mode: GrantAcceptanceMode::RejectAll as i32,
                contract_address: String::new(),
                defaults: Some(GrantDefaults {
                    max_duration_seconds: DEFAULT_MAX_DURATION_SECONDS,
                    max_spend_limit_uakt: DEFAULT_MAX_SPEND_LIMIT_UAKT,
                    allowed_msg_types: default_allowed_messages(),
                    auto_approve_delay_seconds: DEFAULT_AUTO_APPROVE_DELAY,
                }),
                whitelist: vec![],
            })),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            active_grants: Arc::new(RwLock::new(Vec::new())),
            next_request_id: Arc::new(RwLock::new(1)),
            granter_address,
            node_pubkey,
        }
    }

    /// Create granter service from config
    pub fn from_config(
        config: GranterConfig,
        granter_address: String,
        node_pubkey: Vec<u8>,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            active_grants: Arc::new(RwLock::new(Vec::new())),
            next_request_id: Arc::new(RwLock::new(1)),
            granter_address,
            node_pubkey,
        }
    }

    /// Enable/disable the granter service
    pub async fn set_enabled(&self, enabled: bool) {
        let mut config = self.config.write().await;
        config.enabled = enabled;
        info!("Granter service enabled: {}", enabled);
    }

    /// Set the acceptance mode
    pub async fn set_mode(&self, mode: GrantAcceptanceMode) {
        let mut config = self.config.write().await;
        config.mode = mode as i32;
        info!("Granter acceptance mode set to: {:?}", mode);
    }

    /// Set the contract address for the grant manager
    pub async fn set_contract_address(&self, address: &str) {
        let mut config = self.config.write().await;
        config.contract_address = address.to_string();
    }

    /// Update default grant parameters
    pub async fn set_defaults(&self, defaults: GrantDefaults) {
        let mut config = self.config.write().await;
        config.defaults = Some(defaults);
    }

    // ==================== Whitelist Management ====================

    /// Add a node to the whitelist
    pub async fn whitelist_add(
        &self,
        requester_pubkey: Vec<u8>,
        custom_limits: Option<GrantLimits>,
        note: &str,
    ) {
        let mut config = self.config.write().await;

        // Remove existing entry if present
        config.whitelist.retain(|e| e.requester_pubkey != requester_pubkey);

        config.whitelist.push(WhitelistEntry {
            requester_pubkey,
            custom_limits,
            note: note.to_string(),
            added_at: Some(current_timestamp()),
        });

        info!("Added node to whitelist");
    }

    /// Remove a node from the whitelist
    pub async fn whitelist_remove(&self, requester_pubkey: &[u8]) -> bool {
        let mut config = self.config.write().await;
        let initial_len = config.whitelist.len();
        config.whitelist.retain(|e| e.requester_pubkey != requester_pubkey);
        let removed = config.whitelist.len() < initial_len;
        if removed {
            info!("Removed node from whitelist");
        }
        removed
    }

    /// Check if a node is whitelisted
    pub async fn is_whitelisted(&self, requester_pubkey: &[u8]) -> bool {
        let config = self.config.read().await;
        config.whitelist.iter().any(|e| e.requester_pubkey == requester_pubkey)
    }

    /// Get whitelist entry for a requester
    pub async fn get_whitelist_entry(&self, requester_pubkey: &[u8]) -> Option<WhitelistEntry> {
        let config = self.config.read().await;
        config.whitelist.iter()
            .find(|e| e.requester_pubkey == requester_pubkey)
            .cloned()
    }

    /// List all whitelisted nodes
    pub async fn list_whitelist(&self) -> Vec<WhitelistEntry> {
        let config = self.config.read().await;
        config.whitelist.clone()
    }

    // ==================== Request Handling ====================

    /// Handle an incoming grant request
    /// Returns the request with updated status
    pub async fn handle_request(
        &self,
        requester_pubkey: Vec<u8>,
        grantee_address: String,
        grant_type: GrantType,
        params: GrantRequestParams,
    ) -> Result<GrantRequest> {
        let config = self.config.read().await;

        if !config.enabled {
            return Err(anyhow!("Granter service is disabled"));
        }

        // Validate request parameters against defaults
        let defaults = config.defaults.as_ref()
            .ok_or_else(|| anyhow!("No defaults configured"))?;

        if params.duration_seconds > defaults.max_duration_seconds {
            return Err(anyhow!(
                "Requested duration {} exceeds max {}",
                params.duration_seconds,
                defaults.max_duration_seconds
            ));
        }

        if params.spend_limit_uakt > defaults.max_spend_limit_uakt {
            return Err(anyhow!(
                "Requested spend limit {} exceeds max {}",
                params.spend_limit_uakt,
                defaults.max_spend_limit_uakt
            ));
        }

        // Generate request ID
        let request_id = {
            let mut id = self.next_request_id.write().await;
            let current = *id;
            *id += 1;
            current
        };

        let mut request = GrantRequest {
            id: request_id,
            requester_pubkey: requester_pubkey.clone(),
            grantee_address,
            granter_pubkey: self.node_pubkey.clone(),
            granter_address: self.granter_address.clone(),
            grant_type: grant_type as i32,
            params: Some(params),
            status: GrantRequestStatus::Pending as i32,
            created_at: Some(current_timestamp()),
            updated_at: Some(current_timestamp()),
            tx_hash: String::new(),
            rejection_reason: String::new(),
        };

        // Check whitelist for custom limits
        let whitelist_entry = config.whitelist.iter()
            .find(|e| e.requester_pubkey == requester_pubkey);

        if let Some(entry) = whitelist_entry {
            if let Some(ref limits) = entry.custom_limits {
                // Apply custom limits if stricter
                if let Some(ref mut req_params) = request.params {
                    let max_dur = limits.max_duration_seconds;
                    if max_dur > 0 && req_params.duration_seconds > max_dur {
                        req_params.duration_seconds = max_dur;
                    }
                    let max_spend = limits.max_spend_limit_uakt;
                    if max_spend > 0 && req_params.spend_limit_uakt > max_spend {
                        req_params.spend_limit_uakt = max_spend;
                    }
                }
            }
        }

        // Process based on acceptance mode
        let mode = GrantAcceptanceMode::try_from(config.mode)
            .unwrap_or(GrantAcceptanceMode::RejectAll);

        drop(config); // Release read lock before potential writes

        match mode {
            GrantAcceptanceMode::AcceptAll => {
                info!("Auto-approving grant request {} (accept-all mode)", request_id);
                request.status = GrantRequestStatus::Approved as i32;
                // In production: broadcast grant transactions here
                self.approve_and_broadcast(&mut request).await?;
            }
            GrantAcceptanceMode::RejectAll => {
                info!("Auto-rejecting grant request {} (reject-all mode)", request_id);
                request.status = GrantRequestStatus::Rejected as i32;
                request.rejection_reason = "Granter is in reject-all mode".to_string();
            }
            GrantAcceptanceMode::Whitelist => {
                if self.is_whitelisted(&requester_pubkey).await {
                    info!("Auto-approving whitelisted request {}", request_id);
                    request.status = GrantRequestStatus::Approved as i32;
                    self.approve_and_broadcast(&mut request).await?;
                } else {
                    info!("Rejecting non-whitelisted request {}", request_id);
                    request.status = GrantRequestStatus::Rejected as i32;
                    request.rejection_reason = "Requester not whitelisted".to_string();
                }
            }
            GrantAcceptanceMode::Manual => {
                info!("Queuing request {} for manual approval", request_id);
                let mut pending = self.pending_requests.write().await;
                pending.insert(request_id, request.clone());
            }
            GrantAcceptanceMode::Unspecified => {
                request.status = GrantRequestStatus::Rejected as i32;
                request.rejection_reason = "Invalid acceptance mode".to_string();
            }
        }

        request.updated_at = Some(current_timestamp());
        Ok(request)
    }

    /// Manually approve a pending request
    pub async fn approve_request(&self, request_id: u64) -> Result<GrantRequest> {
        let mut pending = self.pending_requests.write().await;
        let mut request = pending.remove(&request_id)
            .ok_or_else(|| anyhow!("Request {} not found in pending queue", request_id))?;

        drop(pending);

        request.status = GrantRequestStatus::Approved as i32;
        self.approve_and_broadcast(&mut request).await?;

        Ok(request)
    }

    /// Manually reject a pending request
    pub async fn reject_request(&self, request_id: u64, reason: &str) -> Result<GrantRequest> {
        let mut pending = self.pending_requests.write().await;
        let mut request = pending.remove(&request_id)
            .ok_or_else(|| anyhow!("Request {} not found in pending queue", request_id))?;

        request.status = GrantRequestStatus::Rejected as i32;
        request.rejection_reason = reason.to_string();
        request.updated_at = Some(current_timestamp());

        info!("Rejected request {}: {}", request_id, reason);
        Ok(request)
    }

    /// List pending requests
    pub async fn list_pending_requests(&self) -> Vec<GrantRequest> {
        let pending = self.pending_requests.read().await;
        pending.values().cloned().collect()
    }

    /// Approve request and broadcast grant transactions
    async fn approve_and_broadcast(&self, request: &mut GrantRequest) -> Result<()> {
        // Clone params to avoid borrow checker issues
        let params = request.params.clone()
            .ok_or_else(|| anyhow!("No parameters in request"))?;

        let grant_type = GrantType::try_from(request.grant_type)
            .unwrap_or(GrantType::Unspecified);

        info!(
            "Broadcasting grant transactions for request {} (type: {:?})",
            request.id, grant_type
        );

        // Build and broadcast transactions based on grant type
        match grant_type {
            GrantType::AuthzOnly => {
                self.broadcast_authz_grant(request, &params).await?;
            }
            GrantType::FeegrantOnly => {
                self.broadcast_feegrant(request, &params).await?;
            }
            GrantType::AuthzAndFeegrant => {
                self.broadcast_authz_grant(request, &params).await?;
                self.broadcast_feegrant(request, &params).await?;
            }
            GrantType::Unspecified => {
                return Err(anyhow!("Invalid grant type"));
            }
        }

        request.status = GrantRequestStatus::Broadcasted as i32;
        request.updated_at = Some(current_timestamp());

        // Track active grant
        let mut active = self.active_grants.write().await;
        active.push(request.clone());

        Ok(())
    }

    /// Broadcast MsgGrant transaction
    async fn broadcast_authz_grant(
        &self,
        request: &mut GrantRequest,
        params: &GrantRequestParams,
    ) -> Result<()> {
        info!(
            "Broadcasting MsgGrant: granter={}, grantee={}, msg_types={:?}",
            self.granter_address, request.grantee_address, params.msg_types
        );

        // In production: build and sign MsgGrant transaction
        // For now, simulate successful broadcast
        request.tx_hash = format!("authz_tx_{}", request.id);

        debug!("MsgGrant broadcast complete: {}", request.tx_hash);
        Ok(())
    }

    /// Broadcast MsgGrantAllowance transaction
    async fn broadcast_feegrant(
        &self,
        request: &mut GrantRequest,
        params: &GrantRequestParams,
    ) -> Result<()> {
        info!(
            "Broadcasting MsgGrantAllowance: granter={}, grantee={}, limit={} uakt",
            self.granter_address, request.grantee_address, params.spend_limit_uakt
        );

        // In production: build and sign MsgGrantAllowance transaction
        // For now, simulate successful broadcast
        if request.tx_hash.is_empty() {
            request.tx_hash = format!("feegrant_tx_{}", request.id);
        } else {
            request.tx_hash = format!("{},feegrant_tx_{}", request.tx_hash, request.id);
        }

        debug!("MsgGrantAllowance broadcast complete");
        Ok(())
    }

    /// Confirm a grant was included on-chain
    pub async fn confirm_grant(&self, request_id: u64, tx_hash: &str) -> Result<()> {
        let mut active = self.active_grants.write().await;

        if let Some(grant) = active.iter_mut().find(|g| g.id == request_id) {
            grant.status = GrantRequestStatus::Confirmed as i32;
            grant.tx_hash = tx_hash.to_string();
            grant.updated_at = Some(current_timestamp());
            info!("Grant {} confirmed on-chain: {}", request_id, tx_hash);
        }

        Ok(())
    }

    // ==================== Query Methods ====================

    /// Get granter info
    pub async fn get_info(&self) -> GranterInfo {
        let config = self.config.read().await;
        let active = self.active_grants.read().await;

        let total_granted: u64 = active.iter()
            .filter_map(|g| g.params.as_ref())
            .map(|p| p.spend_limit_uakt)
            .sum();

        GranterInfo {
            node_pubkey: self.node_pubkey.clone(),
            granter_address: self.granter_address.clone(),
            mode: config.mode,
            defaults: config.defaults.clone(),
            active_grants: active.len() as u32,
            total_granted_uakt: total_granted,
            registered_at: Some(current_timestamp()),
        }
    }

    /// Get current config
    pub async fn get_config(&self) -> GranterConfig {
        self.config.read().await.clone()
    }

    /// List active grants
    pub async fn list_active_grants(&self) -> Vec<GrantRequest> {
        self.active_grants.read().await.clone()
    }
}

/// Default allowed message types for Akash deployments
fn default_allowed_messages() -> Vec<String> {
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
    async fn test_granter_service_creation() {
        let service = GranterService::new(
            "akash1test...".to_string(),
            vec![1, 2, 3, 4],
        );

        let config = service.get_config().await;
        assert!(!config.enabled);
        assert_eq!(config.mode, GrantAcceptanceMode::RejectAll as i32);
    }

    #[tokio::test]
    async fn test_whitelist_management() {
        let service = GranterService::new(
            "akash1test...".to_string(),
            vec![1, 2, 3, 4],
        );

        let pubkey = vec![10, 20, 30];

        // Add to whitelist
        service.whitelist_add(pubkey.clone(), None, "Test node").await;
        assert!(service.is_whitelisted(&pubkey).await);

        // Remove from whitelist
        assert!(service.whitelist_remove(&pubkey).await);
        assert!(!service.is_whitelisted(&pubkey).await);
    }

    #[tokio::test]
    async fn test_reject_all_mode() {
        let service = GranterService::new(
            "akash1granter...".to_string(),
            vec![1, 2, 3, 4],
        );

        service.set_enabled(true).await;
        service.set_mode(GrantAcceptanceMode::RejectAll).await;

        let result = service.handle_request(
            vec![10, 20, 30],
            "akash1grantee...".to_string(),
            GrantType::AuthzAndFeegrant,
            GrantRequestParams {
                duration_seconds: 3600,
                spend_limit_uakt: 1_000_000,
                msg_types: vec![],
                purpose: "Test".to_string(),
            },
        ).await.unwrap();

        assert_eq!(result.status, GrantRequestStatus::Rejected as i32);
    }

    #[tokio::test]
    async fn test_accept_all_mode() {
        let service = GranterService::new(
            "akash1granter...".to_string(),
            vec![1, 2, 3, 4],
        );

        service.set_enabled(true).await;
        service.set_mode(GrantAcceptanceMode::AcceptAll).await;

        let result = service.handle_request(
            vec![10, 20, 30],
            "akash1grantee...".to_string(),
            GrantType::AuthzOnly,
            GrantRequestParams {
                duration_seconds: 3600,
                spend_limit_uakt: 1_000_000,
                msg_types: vec![],
                purpose: "Test".to_string(),
            },
        ).await.unwrap();

        assert_eq!(result.status, GrantRequestStatus::Broadcasted as i32);
    }

    #[tokio::test]
    async fn test_whitelist_mode() {
        let service = GranterService::new(
            "akash1granter...".to_string(),
            vec![1, 2, 3, 4],
        );

        service.set_enabled(true).await;
        service.set_mode(GrantAcceptanceMode::Whitelist).await;

        let whitelisted_pubkey = vec![10, 20, 30];
        let non_whitelisted_pubkey = vec![40, 50, 60];

        service.whitelist_add(whitelisted_pubkey.clone(), None, "Trusted").await;

        // Whitelisted should be approved
        let result = service.handle_request(
            whitelisted_pubkey,
            "akash1grantee...".to_string(),
            GrantType::AuthzOnly,
            GrantRequestParams {
                duration_seconds: 3600,
                spend_limit_uakt: 1_000_000,
                msg_types: vec![],
                purpose: "Test".to_string(),
            },
        ).await.unwrap();
        assert_eq!(result.status, GrantRequestStatus::Broadcasted as i32);

        // Non-whitelisted should be rejected
        let result = service.handle_request(
            non_whitelisted_pubkey,
            "akash1grantee...".to_string(),
            GrantType::AuthzOnly,
            GrantRequestParams {
                duration_seconds: 3600,
                spend_limit_uakt: 1_000_000,
                msg_types: vec![],
                purpose: "Test".to_string(),
            },
        ).await.unwrap();
        assert_eq!(result.status, GrantRequestStatus::Rejected as i32);
    }

    #[tokio::test]
    async fn test_manual_approval() {
        let service = GranterService::new(
            "akash1granter...".to_string(),
            vec![1, 2, 3, 4],
        );

        service.set_enabled(true).await;
        service.set_mode(GrantAcceptanceMode::Manual).await;

        let request = service.handle_request(
            vec![10, 20, 30],
            "akash1grantee...".to_string(),
            GrantType::AuthzOnly,
            GrantRequestParams {
                duration_seconds: 3600,
                spend_limit_uakt: 1_000_000,
                msg_types: vec![],
                purpose: "Test".to_string(),
            },
        ).await.unwrap();

        assert_eq!(request.status, GrantRequestStatus::Pending as i32);

        // Should be in pending queue
        let pending = service.list_pending_requests().await;
        assert_eq!(pending.len(), 1);

        // Approve manually
        let approved = service.approve_request(request.id).await.unwrap();
        assert_eq!(approved.status, GrantRequestStatus::Broadcasted as i32);

        // Should no longer be pending
        let pending = service.list_pending_requests().await;
        assert!(pending.is_empty());
    }
}
