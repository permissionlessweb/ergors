//! Akash Deployment Workflow Manager
//!
//! This module provides a state machine for managing Akash deployments:
//! - Creates and tracks workflow sessions
//! - Manages step-by-step progression through deployment phases
//! - Persists workflow state to Cnidarium storage
//! - Supports concurrent workflows via HD path derivation

use crate::deploy::authz::AkashAuthzManager;
use crate::deploy::certificate::CertificateManager;
use crate::deploy::cosmos_client::CosmosClient;
use crate::deploy::sdl::SdlTemplateManager;
use crate::deploy::tx_lifecycle::TxLifecycle;
use crate::storage::ErgorsStorage;
use anyhow::{anyhow, Result};
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::{
    AkashBidInfo, AkashBidState, AkashDeploymentWorkflow, AkashLeaseIdInfo,
    AkashProviderSelection, AkashWorkflowOptions, AkashWorkflowStatus,
    AkashWorkflowStep, CosmosKeyStore, EndpointTestResult, GrantRequestParams, GrantRequestStatus,
    GrantType, WorkflowGrantState, WorkflowInputRequired, WorkflowInputType,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
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

    // ==================== Step Execution ====================

    /// Advance the workflow to the next step
    pub async fn advance_workflow(
        &self,
        session_id: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        let mut workflow = self
            .get_workflow(session_id)
            .await?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        // Check if workflow can advance
        if workflow.status == AkashWorkflowStatus::Completed as i32
            || workflow.status == AkashWorkflowStatus::Cancelled as i32
        {
            return Err(anyhow!("Workflow is already finished"));
        }

        // Execute current step and determine next
        let current = AkashWorkflowStep::try_from(workflow.current_step)
            .unwrap_or(AkashWorkflowStep::Unspecified);

        workflow.status = AkashWorkflowStatus::Running as i32;

        let result = match current {
            AkashWorkflowStep::KeySelection => {
                self.execute_key_selection(&workflow).await
            }
            AkashWorkflowStep::BalanceCheck => {
                self.execute_balance_check(&workflow).await
            }
            AkashWorkflowStep::GrantRequest => {
                self.execute_grant_request(&mut workflow).await
            }
            AkashWorkflowStep::GrantWait => {
                self.execute_grant_wait(&mut workflow).await
            }
            AkashWorkflowStep::AuthzSetup => {
                self.execute_authz_setup(&mut workflow).await
            }
            AkashWorkflowStep::FeegrantSetup => {
                self.execute_feegrant_setup(&workflow).await
            }
            AkashWorkflowStep::SdlConfiguration => {
                self.execute_sdl_configuration(&workflow).await
            }
            AkashWorkflowStep::CertificateSetup => {
                self.execute_certificate_setup(&workflow).await
            }
            AkashWorkflowStep::DeploymentCreate => {
                self.execute_deployment_create(&workflow).await
            }
            AkashWorkflowStep::BidWait => {
                self.execute_bid_wait(&workflow).await
            }
            AkashWorkflowStep::ProviderSelection => {
                self.execute_provider_selection(&workflow).await
            }
            AkashWorkflowStep::LeaseCreate => {
                self.execute_lease_create(&workflow).await
            }
            AkashWorkflowStep::ManifestSend => {
                self.execute_manifest_send(&workflow).await
            }
            AkashWorkflowStep::EndpointRetrieval => {
                self.execute_endpoint_retrieval(&mut workflow).await
            }
            AkashWorkflowStep::EndpointTesting => {
                self.execute_endpoint_testing(&mut workflow).await
            }
            AkashWorkflowStep::Complete => {
                workflow.status = AkashWorkflowStatus::Completed as i32;
                workflow.completed_at = Some(current_timestamp());
                Ok(AkashWorkflowStep::Complete)
            }
            AkashWorkflowStep::Failed | AkashWorkflowStep::Unspecified => {
                Err(anyhow!("Workflow in invalid state"))
            }
        };

        match result {
            Ok(next_step) => {
                workflow.current_step = next_step as i32;
                workflow.retry_count = 0;
                workflow.last_error.clear();

                if next_step == AkashWorkflowStep::Complete {
                    workflow.status = AkashWorkflowStatus::Completed as i32;
                    workflow.completed_at = Some(current_timestamp());
                }
            }
            Err(e) => {
                workflow.retry_count += 1;
                workflow.last_error = e.to_string();

                if workflow.retry_count >= workflow.max_retries {
                    workflow.current_step = AkashWorkflowStep::Failed as i32;
                    workflow.status = AkashWorkflowStatus::Failed as i32;
                }
            }
        }

        workflow.updated_at = Some(current_timestamp());
        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?;

        Ok(workflow)
    }

    /// Run workflow until completion or error
    pub async fn run_to_completion(
        &self,
        session_id: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        loop {
            let workflow = self.advance_workflow(session_id).await?;

            match AkashWorkflowStatus::try_from(workflow.status) {
                Ok(AkashWorkflowStatus::Completed) => return Ok(workflow),
                Ok(AkashWorkflowStatus::Failed) => {
                    return Err(anyhow!("Workflow failed: {}", workflow.last_error));
                }
                Ok(AkashWorkflowStatus::Cancelled) => {
                    return Err(anyhow!("Workflow was cancelled"));
                }
                _ => continue,
            }
        }
    }

    // ==================== Step Implementations ====================

    async fn execute_key_selection(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Key is already selected during workflow creation
        tracing::info!(
            "Using key '{}' at address {}",
            workflow.selected_key_name,
            workflow.account_address
        );
        Ok(AkashWorkflowStep::BalanceCheck)
    }

    async fn execute_balance_check(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Query balance via REST API
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}",
            self.rest_endpoint.trim_end_matches('/'),
            workflow.account_address
        );

        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to query balance"));
        }

        let balance_json: serde_json::Value = response.json().await?;

        let mut uakt_balance = 0u64;
        if let Some(balances) = balance_json["balances"].as_array() {
            for balance in balances {
                if balance["denom"] == "uakt" {
                    uakt_balance = balance["amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                }
            }
        }

        if uakt_balance == 0 {
            return Err(anyhow!(
                "Account has no AKT balance. Please fund the account first."
            ));
        }

        tracing::info!("Account balance: {} uAKT", uakt_balance);

        // Check if we need to request grants from another node
        if !workflow.request_grant_from.is_empty() {
            return Ok(AkashWorkflowStep::GrantRequest);
        }

        Ok(AkashWorkflowStep::AuthzSetup)
    }

    async fn execute_grant_request(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Check if grant request is configured
        if workflow.request_grant_from.is_empty() {
            tracing::info!("No grant request configured, skipping to authz setup");
            return Ok(AkashWorkflowStep::AuthzSetup);
        }

        tracing::info!(
            "Requesting authz/feegrant from granter node (pubkey: {} bytes)",
            workflow.request_grant_from.len()
        );

        // Create grant request params
        let params = GrantRequestParams {
            duration_seconds: if workflow.grant_duration_seconds > 0 {
                workflow.grant_duration_seconds
            } else {
                86400 // Default to 24 hours
            },
            spend_limit_uakt: if workflow.grant_spend_limit_uakt > 0 {
                workflow.grant_spend_limit_uakt
            } else {
                5_000_000 // Default to 5 AKT
            },
            msg_types: vec![
                "/akash.deployment.v1beta3.MsgCreateDeployment".to_string(),
                "/akash.deployment.v1beta3.MsgUpdateDeployment".to_string(),
                "/akash.deployment.v1beta3.MsgCloseDeployment".to_string(),
                "/akash.market.v1beta4.MsgCreateLease".to_string(),
            ],
            purpose: if workflow.grant_purpose.is_empty() {
                "Akash deployment workflow".to_string()
            } else {
                workflow.grant_purpose.clone()
            },
        };

        // Create workflow grant state to track the request
        let grant_state = WorkflowGrantState {
            request_id: 0, // Will be set after contract submission
            granter_pubkey: workflow.request_grant_from.clone(),
            granter_address: String::new(), // Will be resolved from pubkey
            grant_type: GrantType::AuthzAndFeegrant as i32,
            params: Some(params),
            status: GrantRequestStatus::Pending as i32,
            submitted_at: Some(current_timestamp()),
            tx_hash: String::new(),
            rejection_reason: String::new(),
        };

        workflow.grant_request = Some(grant_state);

        // In production: submit request to grant-manager contract
        // For now, transition to wait state
        tracing::info!("Grant request submitted, waiting for approval...");

        Ok(AkashWorkflowStep::GrantWait)
    }

    async fn execute_grant_wait(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        let grant_state = workflow
            .grant_request
            .as_mut()
            .ok_or_else(|| anyhow!("No grant request in progress"))?;

        let status = GrantRequestStatus::try_from(grant_state.status)
            .unwrap_or(GrantRequestStatus::Unspecified);

        match status {
            GrantRequestStatus::Confirmed => {
                tracing::info!("Grant request confirmed on-chain");
                Ok(AkashWorkflowStep::AuthzSetup)
            }
            GrantRequestStatus::Rejected => {
                let reason = &grant_state.rejection_reason;
                Err(anyhow!("Grant request rejected: {}", reason))
            }
            GrantRequestStatus::Cancelled | GrantRequestStatus::Expired => {
                Err(anyhow!("Grant request cancelled or expired"))
            }
            GrantRequestStatus::Pending
            | GrantRequestStatus::Approved
            | GrantRequestStatus::Broadcasted => {
                // Still waiting - in production would poll contract
                tracing::info!("Grant request status: {:?}, waiting...", status);

                // For now, simulate approval after short delay
                // In production: poll contract for status updates
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                // Simulate successful grant (for testing)
                grant_state.status = GrantRequestStatus::Confirmed as i32;
                grant_state.tx_hash = "simulated_tx_hash".to_string();

                Ok(AkashWorkflowStep::AuthzSetup)
            }
            GrantRequestStatus::Unspecified => {
                Err(anyhow!("Invalid grant request status"))
            }
        }
    }

    async fn execute_authz_setup(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Check existing grants
        let status = self
            .authz_manager
            .check_existing_grants(&workflow.account_address, &workflow.account_address)
            .await?;

        if status.has_all_grants() {
            tracing::info!("All required authz grants already exist");
        } else {
            tracing::info!(
                "Missing authz grants: {:?}. Manual grant may be required.",
                status.missing_grants
            );

            // Store grant info for tracking
            let grant = self.authz_manager.create_grant_record(
                &workflow.account_address,
                &workflow.account_address,
                &status.existing_grants.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                24,
                "", // TX hash would be filled after broadcast
            );
            workflow.authz_grants.push(grant);
        }

        Ok(AkashWorkflowStep::FeegrantSetup)
    }

    async fn execute_feegrant_setup(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Check existing feegrant
        let has_feegrant = self
            .authz_manager
            .has_active_feegrant(&workflow.account_address, &workflow.account_address)
            .await?;

        if has_feegrant {
            tracing::info!("Active feegrant allowance exists");
        } else {
            tracing::info!("No feegrant found. Continuing without feegrant.");
        }

        Ok(AkashWorkflowStep::SdlConfiguration)
    }

    async fn execute_sdl_configuration(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Check if SDL is already configured
        if workflow.configured_sdl.is_some() {
            tracing::info!("SDL already configured");
            return Ok(AkashWorkflowStep::CertificateSetup);
        }

        // SDL needs to be configured externally via configure_sdl method
        Err(anyhow!("SDL not configured. Call configure_workflow_sdl() first."))
    }

    async fn execute_certificate_setup(
        &self,
        _workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Certificate setup would query existing certs and create if needed
        tracing::info!("Certificate setup (assuming external management)");
        Ok(AkashWorkflowStep::DeploymentCreate)
    }

    async fn execute_deployment_create(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        // Deployment creation requires SDL
        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;

        tracing::info!("Creating deployment from template '{}'", sdl.template_name);

        // Actual deployment creation would happen here via transaction
        Ok(AkashWorkflowStep::BidWait)
    }

    async fn execute_bid_wait(
        &self,
        _workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        tracing::info!("Waiting for bids from providers...");
        // In production, poll for bids here
        Ok(AkashWorkflowStep::ProviderSelection)
    }

    async fn execute_provider_selection(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        if workflow.provider.is_none() {
            return Err(anyhow!("No provider selected. Call select_workflow_provider() first."));
        }
        Ok(AkashWorkflowStep::LeaseCreate)
    }

    async fn execute_lease_create(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        let provider = workflow
            .provider
            .as_ref()
            .ok_or_else(|| anyhow!("No provider selected"))?;

        tracing::info!("Creating lease with provider {}", provider.provider_address);
        Ok(AkashWorkflowStep::ManifestSend)
    }

    async fn execute_manifest_send(
        &self,
        _workflow: &AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        tracing::info!("Sending manifest to provider...");
        Ok(AkashWorkflowStep::EndpointRetrieval)
    }

    async fn execute_endpoint_retrieval(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        tracing::info!("Retrieving deployment endpoints from Akash provider...");

        // Extract deployment info from workflow
        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deployment info in workflow"))?;

        let provider_selection = workflow
            .provider
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provider selected in workflow"))?;

        // Create Akash client to query provider
        let akash_config = crate::deploy::akash::AkashConfig {
            key_name: workflow.selected_key_name.clone(),
            keyring_backend: "os".to_string(),
            node: workflow.node_endpoint.clone(),
            chain_id: workflow.chain_id.clone(),
            account_address: workflow.account_address.clone(),
            gas: "auto".to_string(),
            gas_adjustment: 1.3,
            gas_prices: "0.0025uakt".to_string(),
            sign_mode: "amino-json".to_string(),
        };

        let akash_client = crate::deploy::akash::AkashClient::new(akash_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Akash client: {}", e))?;

        // Construct provider gRPC URI
        // Providers typically expose gRPC on port 8443
        let provider_grpc = if provider_selection.provider_address.starts_with("http") {
            provider_selection.provider_address.clone()
        } else {
            // If just an address, construct the gRPC endpoint
            format!("https://{}:8443", provider_selection.provider_address)
        };

        // Parse sequences from strings
        let dseq = deployment
            .deployment_sequence
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Invalid dseq: {}", e))?;
        let gseq = deployment
            .group_sequence
            .parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid gseq: {}", e))?;
        let oseq = deployment
            .order_sequence
            .parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid oseq: {}", e))?;

        // Query endpoints from provider
        match akash_client
            .query_deployment_endpoints(
                &provider_grpc,
                &workflow.account_address,
                dseq,
                gseq,
                oseq,
            )
            .await
        {
            Ok(endpoints) => {
                tracing::info!("Successfully retrieved {} service endpoints", endpoints.len());
                for (service, uri) in &endpoints {
                    tracing::info!("  {} -> {}", service, uri);
                }
                // Store endpoints in workflow
                workflow.endpoints = endpoints;
                workflow.updated_at = Some(current_timestamp());
                // Return EndpointTesting to proceed to testing step
                Ok(AkashWorkflowStep::EndpointTesting)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to retrieve endpoints from provider: {}. Proceeding to endpoint testing anyway.",
                    e
                );
                // Even if query fails, proceed to testing (endpoints might be set manually)
                Ok(AkashWorkflowStep::EndpointTesting)
            }
        }
    }

    async fn execute_endpoint_testing(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<AkashWorkflowStep> {
        tracing::info!("Testing deployment endpoints...");

        // Test each endpoint
        for (service_name, endpoint) in workflow.endpoints.clone() {
            let test_result = self.test_endpoint(&endpoint).await;
            workflow.test_results.push(EndpointTestResult {
                service_name: service_name.clone(),
                endpoint_uri: endpoint.clone(),
                success: test_result.is_ok(),
                response_time_ms: test_result.as_ref().copied().unwrap_or(0),
                test_type: "connectivity".to_string(),
                error_message: test_result.err().map(|e| e.to_string()).unwrap_or_default(),
                tested_at: Some(current_timestamp()),
            });
        }

        Ok(AkashWorkflowStep::Complete)
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

// ========================================
// Automated Workflow Engine
// ========================================

/// Result of a workflow execution.
#[derive(Debug)]
pub enum WorkflowResult {
    /// Workflow completed successfully
    Completed(AkashDeploymentWorkflow),
    /// Workflow failed
    Failed(AkashDeploymentWorkflow),
    /// Workflow needs user input to continue
    NeedsInput(AkashDeploymentWorkflow, WorkflowInputRequired),
}

/// Automated workflow engine for Akash deployments.
///
/// Provides a single entry point for running deployments to completion
/// with proper transaction lifecycle management.
pub struct WorkflowEngine {
    storage: Arc<ErgorsStorage>,
    cosmos: Arc<CosmosClient>,
    cert_manager: Arc<CertificateManager>,
    tx_lifecycle: Arc<TxLifecycle>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    chain_id: String,
    rest_endpoint: String,
}

impl WorkflowEngine {
    /// Create a new workflow engine.
    pub fn new(
        storage: Arc<ErgorsStorage>,
        cosmos: Arc<CosmosClient>,
        cert_manager: Arc<CertificateManager>,
        tx_lifecycle: Arc<TxLifecycle>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        chain_id: String,
        rest_endpoint: String,
    ) -> Self {
        Self {
            storage,
            cosmos,
            cert_manager,
            tx_lifecycle,
            key_store,
            chain_id,
            rest_endpoint,
        }
    }

    /// Run workflow to completion (or until user input needed).
    ///
    /// This is the main entry point for automated deployments.
    pub async fn run(&self, session_id: &str, opts: AkashWorkflowOptions) -> Result<WorkflowResult> {
        // Get workflow and set options
        let mut workflow = self
            .storage
            .get_akash_workflow(session_id)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        workflow.options = Some(opts.clone());
        workflow.status = AkashWorkflowStatus::Running as i32;
        self.save_workflow(&workflow).await?;

        // Run the workflow loop
        loop {
            let current_step = AkashWorkflowStep::try_from(workflow.current_step)
                .unwrap_or(AkashWorkflowStep::Unspecified);

            tracing::info!("Executing workflow step: {:?}", current_step);

            let step_result = match current_step {
                AkashWorkflowStep::KeySelection => {
                    self.handle_key_selection(&mut workflow).await
                }
                AkashWorkflowStep::BalanceCheck => {
                    self.handle_balance_check(&mut workflow, &opts).await
                }
                AkashWorkflowStep::GrantRequest | AkashWorkflowStep::GrantWait => {
                    if opts.skip_grants {
                        self.skip_to_step(&mut workflow, AkashWorkflowStep::SdlConfiguration)
                            .await
                    } else {
                        self.handle_grant_step(&mut workflow).await
                    }
                }
                AkashWorkflowStep::AuthzSetup | AkashWorkflowStep::FeegrantSetup => {
                    if opts.skip_grants {
                        self.skip_to_step(&mut workflow, AkashWorkflowStep::SdlConfiguration)
                            .await
                    } else {
                        Ok(StepOutcome::Next(AkashWorkflowStep::SdlConfiguration))
                    }
                }
                AkashWorkflowStep::SdlConfiguration => {
                    self.handle_sdl_config(&mut workflow).await
                }
                AkashWorkflowStep::CertificateSetup => {
                    self.handle_certificate(&mut workflow).await
                }
                AkashWorkflowStep::DeploymentCreate => {
                    self.handle_deployment_create(&mut workflow).await
                }
                AkashWorkflowStep::BidWait => {
                    self.handle_bid_wait(&mut workflow, &opts).await
                }
                AkashWorkflowStep::ProviderSelection => {
                    if opts.auto_select_bid {
                        self.auto_select_provider(&mut workflow, &opts).await
                    } else {
                        // Return needing input for bid selection
                        Ok(StepOutcome::NeedsInput(WorkflowInputRequired {
                            input_type: WorkflowInputType::BidSelection as i32,
                            available_bids: workflow.available_bids.clone(),
                            grant_request: None,
                            message: "Select a provider from available bids".to_string(),
                        }))
                    }
                }
                AkashWorkflowStep::LeaseCreate => {
                    self.handle_lease_create(&mut workflow).await
                }
                AkashWorkflowStep::ManifestSend => {
                    self.handle_manifest_send(&mut workflow).await
                }
                AkashWorkflowStep::EndpointRetrieval => {
                    self.handle_endpoint_retrieval(&mut workflow).await
                }
                AkashWorkflowStep::EndpointTesting => {
                    self.handle_endpoint_testing(&mut workflow).await
                }
                AkashWorkflowStep::Complete => {
                    workflow.status = AkashWorkflowStatus::Completed as i32;
                    workflow.completed_at = Some(current_timestamp());
                    self.save_workflow(&workflow).await?;
                    return Ok(WorkflowResult::Completed(workflow));
                }
                AkashWorkflowStep::Failed | AkashWorkflowStep::Unspecified => {
                    return Ok(WorkflowResult::Failed(workflow));
                }
            };

            // Process step outcome
            match step_result {
                Ok(StepOutcome::Next(next_step)) => {
                    workflow.current_step = next_step as i32;
                    workflow.retry_count = 0;
                    workflow.last_error.clear();
                    workflow.updated_at = Some(current_timestamp());
                    self.save_workflow(&workflow).await?;
                }
                Ok(StepOutcome::NeedsInput(input_required)) => {
                    workflow.status = AkashWorkflowStatus::Paused as i32;
                    workflow.updated_at = Some(current_timestamp());
                    self.save_workflow(&workflow).await?;
                    return Ok(WorkflowResult::NeedsInput(workflow, input_required));
                }
                Err(e) => {
                    workflow.retry_count += 1;
                    workflow.last_error = e.to_string();
                    workflow.updated_at = Some(current_timestamp());

                    if workflow.retry_count >= opts.max_retries.max(3) {
                        workflow.current_step = AkashWorkflowStep::Failed as i32;
                        workflow.status = AkashWorkflowStatus::Failed as i32;
                        self.save_workflow(&workflow).await?;
                        return Ok(WorkflowResult::Failed(workflow));
                    }

                    tracing::warn!(
                        "Step failed (attempt {}/{}): {}",
                        workflow.retry_count,
                        opts.max_retries.max(3),
                        e
                    );
                    self.save_workflow(&workflow).await?;
                }
            }
        }
    }

    /// Resume workflow after user provides input.
    pub async fn resume(&self, session_id: &str) -> Result<WorkflowResult> {
        let workflow = self
            .storage
            .get_akash_workflow(session_id)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))?
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?;

        let opts = workflow.options.clone().unwrap_or_default();
        self.run(session_id, opts).await
    }

    // ==================== Step Handlers ====================

    async fn handle_key_selection(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        tracing::info!(
            "Using key '{}' at address {}",
            workflow.selected_key_name,
            workflow.account_address
        );
        Ok(StepOutcome::Next(AkashWorkflowStep::BalanceCheck))
    }

    async fn handle_balance_check(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<StepOutcome> {
        let balance = self
            .cosmos
            .query_balance(&workflow.account_address, "uakt")
            .await?;

        let amount: u64 = balance.amount.parse().unwrap_or(0);
        let min_balance = if opts.min_balance_uakt > 0 {
            opts.min_balance_uakt
        } else {
            5_000_000 // Default 5 AKT minimum
        };

        tracing::info!("Account balance: {} uAKT (minimum: {} uAKT)", amount, min_balance);

        if amount < min_balance {
            return Err(anyhow!(
                "Insufficient balance: {} < {} uakt",
                amount,
                min_balance
            ));
        }

        // Check if grants are needed
        if workflow.request_grant_from.is_empty() || opts.skip_grants {
            Ok(StepOutcome::Next(AkashWorkflowStep::SdlConfiguration))
        } else {
            Ok(StepOutcome::Next(AkashWorkflowStep::GrantRequest))
        }
    }

    async fn handle_grant_step(&self, _workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        // Simplified grant handling - in production would interact with contract
        tracing::info!("Skipping grant setup (not yet implemented in automated mode)");
        Ok(StepOutcome::Next(AkashWorkflowStep::SdlConfiguration))
    }

    async fn handle_sdl_config(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        if workflow.configured_sdl.is_some() {
            tracing::info!("SDL already configured");
            Ok(StepOutcome::Next(AkashWorkflowStep::CertificateSetup))
        } else {
            // Need SDL configuration
            Ok(StepOutcome::NeedsInput(WorkflowInputRequired {
                input_type: WorkflowInputType::SdlConfiguration as i32,
                available_bids: vec![],
                grant_request: None,
                message: "SDL configuration required".to_string(),
            }))
        }
    }

    async fn handle_certificate(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        tracing::info!("Setting up certificate for {}", workflow.account_address);

        let cert_info = self
            .cert_manager
            .get_or_create(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                &workflow.account_address,
            )
            .await?;

        workflow.certificate_info = Some(cert_info);
        tracing::info!("Certificate ready");

        Ok(StepOutcome::Next(AkashWorkflowStep::DeploymentCreate))
    }

    async fn handle_deployment_create(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<StepOutcome> {
        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;

        tracing::info!("Creating deployment from template '{}'", sdl.template_name);

        // Build MsgCreateDeployment
        // Note: This requires parsing the SDL and building the proper deployment message
        // For now, this is a placeholder - full implementation requires SDL parsing

        // TODO: Implement proper deployment creation
        // let msg = build_create_deployment_msg(workflow, sdl)?;
        // let result = self.tx_lifecycle.sign_broadcast_wait(...).await?;
        // let dseq = result.extract_dseq().ok_or_else(|| anyhow!("No dseq in response"))?;

        tracing::warn!("Deployment creation not yet fully implemented - proceeding to bid wait");

        Ok(StepOutcome::Next(AkashWorkflowStep::BidWait))
    }

    async fn handle_bid_wait(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<StepOutcome> {
        let wait_blocks = if opts.bid_wait_blocks > 0 {
            opts.bid_wait_blocks
        } else {
            2 // Default to 2 blocks (~12 seconds on Akash)
        };

        tracing::info!("Waiting {} blocks for bids (~{}s)...", wait_blocks, wait_blocks * 6);

        // Wait for bids (approximately 6 seconds per block on Akash)
        tokio::time::sleep(Duration::from_secs(wait_blocks as u64 * 6)).await;

        // Query bids from chain
        if let Some(deployment) = &workflow.deployment {
            let dseq: u64 = deployment.deployment_sequence.parse().unwrap_or(0);
            if dseq > 0 {
                let bids = self
                    .cosmos
                    .query_open_bids(&workflow.account_address, dseq)
                    .await?;

                // Convert to proto types
                let bid_infos: Vec<AkashBidInfo> = bids
                    .iter()
                    .map(|b| AkashBidInfo {
                        owner: b.owner.clone(),
                        dseq: b.dseq,
                        gseq: b.gseq,
                        oseq: b.oseq,
                        provider: b.provider.clone(),
                        price_denom: b.price_denom.clone(),
                        price_amount: b.price_amount.clone(),
                        state: AkashBidState::Open as i32,
                        created_at: 0,
                    })
                    .collect();

                workflow.available_bids = bid_infos;
                tracing::info!("Found {} open bids", workflow.available_bids.len());
            }
        }

        if workflow.available_bids.is_empty() {
            tracing::warn!("No bids received yet");
        }

        Ok(StepOutcome::Next(AkashWorkflowStep::ProviderSelection))
    }

    async fn auto_select_provider(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<StepOutcome> {
        if workflow.available_bids.is_empty() {
            return Err(anyhow!("No bids available to select from"));
        }

        // Filter by trusted providers if specified
        let mut candidates = workflow.available_bids.clone();
        if !opts.trusted_providers.is_empty() {
            candidates.retain(|bid| opts.trusted_providers.contains(&bid.provider));
        }

        if candidates.is_empty() {
            return Err(anyhow!("No bids from trusted providers"));
        }

        // Select cheapest bid
        let selected = candidates
            .iter()
            .min_by(|a, b| {
                let price_a: u64 = a.price_amount.parse().unwrap_or(u64::MAX);
                let price_b: u64 = b.price_amount.parse().unwrap_or(u64::MAX);
                price_a.cmp(&price_b)
            })
            .ok_or_else(|| anyhow!("Failed to select bid"))?;

        tracing::info!(
            "Auto-selected provider {} with price {} {}",
            selected.provider,
            selected.price_amount,
            selected.price_denom
        );

        let price: u64 = selected.price_amount.parse().unwrap_or(0);
        workflow.provider = Some(AkashProviderSelection {
            provider_address: selected.provider.clone(),
            reputation_score: 100, // Would query reputation system
            bid_price_uakt: price,
            total_bids_received: workflow.available_bids.len() as u32,
            selected_at: Some(current_timestamp()),
            is_trusted_provider: opts.trusted_providers.contains(&selected.provider),
        });

        Ok(StepOutcome::Next(AkashWorkflowStep::LeaseCreate))
    }

    async fn handle_lease_create(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        let provider = workflow
            .provider
            .as_ref()
            .ok_or_else(|| anyhow!("No provider selected"))?;

        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment info"))?;

        tracing::info!("Creating lease with provider {}", provider.provider_address);

        // Build MsgCreateLease
        // let msg = MsgCreateLease { bid_id: ... };
        // let result = self.tx_lifecycle.sign_broadcast_wait(...).await?;

        // Store lease ID
        let dseq: u64 = deployment.deployment_sequence.parse().unwrap_or(0);
        let gseq: u32 = deployment.group_sequence.parse().unwrap_or(1);
        let oseq: u32 = deployment.order_sequence.parse().unwrap_or(1);

        workflow.lease_id_info = Some(AkashLeaseIdInfo {
            owner: workflow.account_address.clone(),
            dseq,
            gseq,
            oseq,
            provider: provider.provider_address.clone(),
        });

        tracing::warn!("Lease creation not yet fully implemented");
        Ok(StepOutcome::Next(AkashWorkflowStep::ManifestSend))
    }

    async fn handle_manifest_send(&self, _workflow: &mut AkashDeploymentWorkflow) -> Result<StepOutcome> {
        tracing::info!("Sending manifest to provider...");
        // TODO: Implement manifest sending to provider
        tracing::warn!("Manifest send not yet fully implemented");
        Ok(StepOutcome::Next(AkashWorkflowStep::EndpointRetrieval))
    }

    async fn handle_endpoint_retrieval(
        &self,
        _workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<StepOutcome> {
        tracing::info!("Retrieving deployment endpoints...");
        // TODO: Query provider for service endpoints
        tracing::warn!("Endpoint retrieval not yet fully implemented");
        Ok(StepOutcome::Next(AkashWorkflowStep::EndpointTesting))
    }

    async fn handle_endpoint_testing(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<StepOutcome> {
        tracing::info!("Testing endpoints...");

        // Test each endpoint
        for endpoint in &workflow.service_endpoints {
            let result = self.test_endpoint(&endpoint.external_uri).await;
            workflow.test_results.push(EndpointTestResult {
                service_name: endpoint.service_name.clone(),
                endpoint_uri: endpoint.external_uri.clone(),
                success: result.is_ok(),
                response_time_ms: result.as_ref().copied().unwrap_or(0),
                test_type: "http_connect".to_string(),
                error_message: result.err().map(|e| e.to_string()).unwrap_or_default(),
                tested_at: Some(current_timestamp()),
            });
        }

        Ok(StepOutcome::Next(AkashWorkflowStep::Complete))
    }

    async fn skip_to_step(
        &self,
        _workflow: &mut AkashDeploymentWorkflow,
        step: AkashWorkflowStep,
    ) -> Result<StepOutcome> {
        tracing::info!("Skipping to step: {:?}", step);
        Ok(StepOutcome::Next(step))
    }

    // ==================== Helper Methods ====================

    async fn save_workflow(&self, workflow: &AkashDeploymentWorkflow) -> Result<()> {
        self.storage
            .put_akash_workflow(workflow)
            .await
            .map_err(|e| anyhow!("Storage error: {}", e))
    }

    async fn test_endpoint(&self, endpoint: &str) -> Result<u64> {
        let start = std::time::Instant::now();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let response = client.get(endpoint).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Endpoint returned status: {}", response.status()));
        }

        Ok(start.elapsed().as_millis() as u64)
    }
}

/// Internal step outcome for workflow engine.
enum StepOutcome {
    /// Proceed to next step
    Next(AkashWorkflowStep),
    /// Needs user input
    NeedsInput(WorkflowInputRequired),
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
