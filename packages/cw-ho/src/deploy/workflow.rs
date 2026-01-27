//! Akash Deployment Workflow Manager
//!
//! This module provides a state machine for managing Akash deployments:
//! - Creates and tracks workflow sessions
//! - Manages step-by-step progression through deployment phases
//! - Persists workflow state to Cnidarium storage
//! - Supports concurrent workflows via HD path derivation

use crate::deploy::authz::AkashAuthzManager;
use crate::deploy::sdl::SdlTemplateManager;
use crate::storage::ErgorsStorage;
use anyhow::{anyhow, Result};
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashProviderSelection, AkashWorkflowStatus,
    AkashWorkflowStep, GrantRequestStatus,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum number of retries for failed steps
pub const MAX_RETRY_COUNT: u32 = 3;

/// Default workflow timeout in seconds
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 3600;

/// Workflow manager for Akash deployments
pub struct AkashWorkflowManager {
    storage: Arc<ErgorsStorage>,
    authz_manager: AkashAuthzManager,
    sdl_manager: SdlTemplateManager,
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    rest_endpoint: String,
    chain_id: String,
}

impl AkashWorkflowManager {
    pub fn new(
        storage: Arc<ErgorsStorage>,
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        rest_endpoint: String,
        chain_id: String,
    ) -> Self {
        let authz_manager = AkashAuthzManager::new(rest_endpoint.clone(), chain_id.clone());
        let sdl_manager = SdlTemplateManager::new();

        Self {
            storage,
            authz_manager,
            sdl_manager,
            key_manager,
            rest_endpoint,
            chain_id,
        }
    }

    // ==================== Workflow Lifecycle ====================

    /// Create a new deployment workflow session using the default key
    pub async fn create_workflow_default(&self) -> Result<AkashDeploymentWorkflow> {
        let key_store = self
            .storage
            .get_cosmos_key_store()
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?
            .ok_or_else(|| anyhow!("No cosmos key store found. Import a key with `ergors keys import-mnemonic`"))?;

        let default_key_name = EncryptedCosmosKeyManager::get_default_key_name(&key_store)
            .ok_or_else(|| anyhow!("No default key set. Use `ergors keys set-default --key-name <name>`"))?
            .to_string();

        self.create_workflow(&default_key_name, 0).await
    }

    /// Create a new deployment workflow session
    pub async fn create_workflow(
        &self,
        key_name: &str,
        hd_account_index: u32,
    ) -> Result<AkashDeploymentWorkflow> {
        let session_id = Uuid::new_v4().to_string();

        // Get account info from key store
        let key_store = self
            .storage
            .get_cosmos_key_store()
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?
            .ok_or_else(|| anyhow!("No cosmos key store found"))?;

        let account = key_store
            .derived_accounts
            .iter()
            .find(|a| a.key_name == key_name && a.account_index == hd_account_index)
            .ok_or_else(|| anyhow!("Key '{}' with index {} not found", key_name, hd_account_index))?;

        let now = current_timestamp();

        let workflow = AkashDeploymentWorkflow {
            session_id: session_id.clone(),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            selected_key_name: key_name.to_string(),
            account_address: account.address.clone(),
            hd_account_index,
            authz_grants: vec![],
            feegrants: vec![],
            configured_sdl: None,
            deployment: None,
            provider: None,
            endpoints: HashMap::new(),
            test_results: vec![],
            last_error: String::new(),
            retry_count: 0,
            created_at: Some(now),
            updated_at: Some(now),
            completed_at: None,
            chain_id: self.chain_id.clone(),
            node_endpoint: self.rest_endpoint.clone(),
            max_retries: MAX_RETRY_COUNT,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            // Grant request fields (optional, configured via CLI flags)
            grant_request: None,
            request_grant_from: vec![],
            grant_duration_seconds: 0,
            grant_spend_limit_uakt: 0,
            grant_purpose: String::new(),
            // Automated workflow fields
            available_bids: vec![],
            certificate_info: None,
            lease_id_info: None,
            options: None,
            service_endpoints: vec![],
        };

        // Persist to storage
        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;

        Ok(workflow)
    }

    /// Get an existing workflow by session ID
    pub async fn get_workflow(&self, session_id: &str) -> Result<Option<AkashDeploymentWorkflow>> {
        self.storage
            .get_akash_workflow(session_id)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))
    }

    /// List all workflows
    pub async fn list_workflows(&self) -> Result<Vec<AkashDeploymentWorkflow>> {
        self.storage
            .list_akash_workflows()
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))
    }

    /// Cancel a workflow
    pub async fn cancel_workflow(&self, session_id: &str) -> Result<()> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        workflow.status = AkashWorkflowStatus::Cancelled as i32;
        workflow.updated_at = Some(current_timestamp());
        workflow.last_error = "Workflow cancelled by user".to_string();

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))
    }

    /// Delete a workflow
    pub async fn delete_workflow(&self, session_id: &str) -> Result<()> {
        self.storage
            .delete_akash_workflow(session_id)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))
    }

    // ==================== Configuration Methods ====================

    /// Configure SDL for a workflow
    pub async fn configure_workflow_sdl(
        &self,
        session_id: &str,
        template_name: &str,
        template: &str,
        values: &HashMap<String, String>,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        let configured = self.sdl_manager.configure_sdl(template_name, template, values)?;
        workflow.configured_sdl = Some(configured);
        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;
        Ok(workflow)
    }

    /// Select a provider for the workflow
    pub async fn select_workflow_provider(
        &self,
        session_id: &str,
        provider_address: &str,
        bid_price_uakt: u64,
        reputation_score: Option<u32>,
        is_trusted: bool,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        workflow.provider = Some(AkashProviderSelection {
            provider_address: provider_address.to_string(),
            reputation_score: reputation_score.unwrap_or(100),
            bid_price_uakt,
            total_bids_received: 1,
            selected_at: Some(current_timestamp()),
            is_trusted_provider: is_trusted,
        });
        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;
        Ok(workflow)
    }

    /// Set endpoints for the workflow (after deployment)
    pub async fn set_workflow_endpoints(
        &self,
        session_id: &str,
        endpoints: HashMap<String, String>,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        workflow.endpoints = endpoints;
        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;
        Ok(workflow)
    }

    /// Configure a grant request for the workflow
    /// This is called when the CLI specifies --request-grant-from flag
    pub async fn configure_grant_request(
        &self,
        session_id: &str,
        granter_pubkey: Vec<u8>,
        duration_seconds: u64,
        spend_limit_uakt: u64,
        purpose: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        workflow.request_grant_from = granter_pubkey;
        workflow.grant_duration_seconds = duration_seconds;
        workflow.grant_spend_limit_uakt = spend_limit_uakt;
        workflow.grant_purpose = purpose.to_string();
        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;

        tracing::info!(
            "Configured grant request: duration={}s, limit={} uakt",
            duration_seconds,
            spend_limit_uakt
        );

        Ok(workflow)
    }

    /// Update grant request status (called when receiving status from contract)
    pub async fn update_grant_status(
        &self,
        session_id: &str,
        status: GrantRequestStatus,
        tx_hash: Option<&str>,
        rejection_reason: Option<&str>,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        if let Some(grant_state) = workflow.grant_request.as_mut() {
            grant_state.status = status as i32;
            if let Some(hash) = tx_hash {
                grant_state.tx_hash = hash.to_string();
            }
            if let Some(reason) = rejection_reason {
                grant_state.rejection_reason = reason.to_string();
            }
        }

        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;

        Ok(workflow)
    }

    // ==================== Helper Methods ====================

    async fn test_endpoint(&self, endpoint: &str) -> Result<u64> {
        let start = std::time::Instant::now();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        let response = client.get(endpoint).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Endpoint returned status: {}", response.status()));
        }

        Ok(start.elapsed().as_millis() as u64)
    }

    /// Get SDL template manager for external use
    pub fn sdl_manager(&self) -> &SdlTemplateManager {
        &self.sdl_manager
    }

    /// Get authz manager for external use
    pub fn authz_manager(&self) -> &AkashAuthzManager {
        &self.authz_manager
    }
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

    #[test]
    fn test_workflow_step_progression() {
        // Test that steps follow expected order (with new grant steps)
        let steps = vec![
            AkashWorkflowStep::KeySelection,      // 1
            AkashWorkflowStep::BalanceCheck,      // 2
            AkashWorkflowStep::GrantRequest,      // 3 (new)
            AkashWorkflowStep::GrantWait,         // 4 (new)
            AkashWorkflowStep::AuthzSetup,        // 5
            AkashWorkflowStep::FeegrantSetup,     // 6
            AkashWorkflowStep::SdlConfiguration,  // 7
            AkashWorkflowStep::CertificateSetup,  // 8
            AkashWorkflowStep::DeploymentCreate,  // 9
            AkashWorkflowStep::BidWait,           // 10
            AkashWorkflowStep::ProviderSelection, // 11
            AkashWorkflowStep::LeaseCreate,       // 12
            AkashWorkflowStep::ManifestSend,      // 13
            AkashWorkflowStep::EndpointRetrieval, // 14
            AkashWorkflowStep::EndpointTesting,   // 15
            AkashWorkflowStep::Complete,          // 16
        ];

        for (i, step) in steps.iter().enumerate() {
            assert_eq!(*step as i32, (i + 1) as i32);
        }
    }

    #[test]
    fn test_grant_request_status_enum() {
        // Test grant request status values
        assert_eq!(GrantRequestStatus::Pending as i32, 1);
        assert_eq!(GrantRequestStatus::Approved as i32, 2);
        assert_eq!(GrantRequestStatus::Broadcasted as i32, 3);
        assert_eq!(GrantRequestStatus::Confirmed as i32, 4);
        assert_eq!(GrantRequestStatus::Rejected as i32, 5);
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts.seconds > 0);
    }
}
