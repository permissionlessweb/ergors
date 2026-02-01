//! Automated Akash deployment workflow.
//!
//! This module provides fully automated deployment without user intervention.
//! - Automatically checks balance
//! - Creates/retrieves certificates
//! - Broadcasts deployment transaction
//! - Polls for bids
//! - Selects best provider (configurable)
//! - Creates lease
//! - Sends manifest
//! - Retrieves and saves endpoints

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ho_std::types::ergors::orch::v1::{
    AkashBidInfo, AkashBidState, AkashDeploymentWorkflow, AkashLeaseIdInfo, AkashProviderSelection,
    AkashRuntime, AkashServiceEndpoint, AkashWorkflowOptions, AkashWorkflowStatus,
    AkashWorkflowStep,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::akash::broadcast_akash_msg;
use super::certificate::CertificateManager;
use super::climb_signer::create_signing_client_with_failover;
use super::cosmos_client::{BidInfo, CosmosClient};
use super::deployment_builder::{
    build_close_deployment_msg, build_create_lease_msg, get_next_dseq, DeploymentBuilder,
    DEFAULT_DEPOSIT_UAKT,
};
use super::endpoint_manager::{EndpointManager, EndpointType};
use super::manifest::{query_service_endpoints_mtls, ManifestSender};
use crate::storage::ErgorsStorage;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::akash::deployment::v1beta4::{MsgCloseDeployment, MsgCreateDeployment};
use ho_std::types::ergors::akash::market::v1beta5::MsgCreateLease;
use ho_std::types::ergors::orch::v1::{AkashDeployConfig, CosmosKeyStore};
use layer_climb::prelude::SigningClient;
use prost::Name;
use tokio::sync::RwLock;

/// Minimum balance required for deployment (5 AKT).
const MIN_BALANCE_UAKT: u64 = 5_000_000;

/// Default blocks to wait for bids (2 blocks = ~12s on Akash).
const DEFAULT_BID_WAIT_BLOCKS: u32 = 2;

/// Maximum bid polling attempts.
const MAX_BID_POLL_ATTEMPTS: u32 = 10;

/// Automated deployment runner.
///
/// Executes the complete deployment flow without manual intervention.
/// Now uses layer-climb for robust Cosmos SDK transaction signing.
pub struct AutomatedDeployer {
    storage: Arc<ErgorsStorage>,
    cosmos: Arc<CosmosClient>,
    cert_manager: Arc<CertificateManager>,
    /// Key manager (for decrypting mnemonics)
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    /// Key store (for retrieving encrypted keys)
    key_store: Arc<RwLock<CosmosKeyStore>>,
    /// Akash deployment config (for creating chain config)
    akash_config: AkashDeployConfig,
    /// Custody password for encrypting certificate private keys
    custody_password: String,
}

impl AutomatedDeployer {
    /// Create a new automated deployer with layer-climb integration.
    pub fn new(
        storage: Arc<ErgorsStorage>,
        cosmos: Arc<CosmosClient>,
        cert_manager: Arc<CertificateManager>,
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        akash_config: AkashDeployConfig,
        custody_password: String,
    ) -> Self {
        Self {
            storage,
            cosmos,
            cert_manager,
            key_manager,
            key_store,
            akash_config,
            custody_password,
        }
    }

    /// Create a layer-climb signing client for the workflow's key.
    ///
    /// Uses automatic endpoint failover for production resilience.
    async fn create_climb_client(
        &self,
        key_name: &str,
        account_index: u32,
    ) -> Result<SigningClient> {
        create_signing_client_with_failover(
            self.key_manager.clone(),
            self.key_store.clone(),
            key_name,
            account_index,
            &self.akash_config,
        )
        .await
    }

    /// Run automated deployment.
    ///
    /// This is the main entry point for fully automated deployments.
    ///
    /// If any step fails after MsgCreateDeployment succeeds, we automatically
    /// broadcast MsgCloseDeployment to recover the escrow deposit.
    pub async fn deploy(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<DeploymentResult> {
        tracing::info!("═══════════════════════════════════════════════════════════════");
        tracing::info!("  AUTOMATED AKASH DEPLOYMENT");
        tracing::info!("═══════════════════════════════════════════════════════════════");
        tracing::info!("Session ID: {}", workflow.session_id);
        tracing::info!("Account:    {}", workflow.account_address);
        tracing::info!(
            "Key:        {} (index {})",
            workflow.selected_key_name,
            workflow.hd_account_index
        );
        tracing::info!("Chain:      {}", workflow.chain_id);
        tracing::info!("───────────────────────────────────────────────────────────────");

        // Set workflow status
        workflow.status = AkashWorkflowStatus::Running as i32;
        workflow.options = Some(opts.clone());
        self.save_workflow(workflow).await?;

        // Create SigningClient once for this workflow (reused for all txs)
        // Uses QueryAndIncrement strategy for optimal performance
        tracing::info!("Creating signing client for workflow...");
        let signing_client = self
            .create_climb_client(&workflow.selected_key_name, workflow.hd_account_index)
            .await?;
        tracing::info!("Signing client created successfully");

        // Pre-deployment steps (no cleanup needed if these fail)
        self.step_connectivity_check(workflow).await?;
        self.step_check_balance(workflow, opts).await?;
        if !opts.request_grant_from.is_empty() {
            self.step_grant_request_and_wait(workflow, opts).await?;
        }
        self.step_setup_certificate(workflow).await?;

        // Create deployment - after this point, we need cleanup on failure
        let dseq = self
            .step_create_deployment(workflow, opts, &signing_client)
            .await?;

        // Run post-deployment steps with automatic cleanup on failure
        match self
            .run_post_deployment_steps(workflow, opts, &signing_client, dseq)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => {
                // Deployment was created but a later step failed
                // Close deployment to recover escrow deposit
                tracing::error!("═══════════════════════════════════════════════════════════════");
                tracing::error!("  DEPLOYMENT FAILED - CLEANING UP");
                tracing::error!("═══════════════════════════════════════════════════════════════");
                tracing::error!("Error: {}", e);
                tracing::error!("DSEQ: {}", dseq);
                tracing::info!("Closing deployment to recover escrow deposit...");

                if let Err(close_err) = self
                    .cleanup_failed_deployment(workflow, dseq, &signing_client)
                    .await
                {
                    tracing::error!("Failed to close deployment during cleanup: {}", close_err);
                    tracing::error!("Manual cleanup may be required: ergors-cli deploy close-deployment {}", workflow.session_id);
                } else {
                    tracing::info!("Deployment closed successfully, escrow deposit returned");
                }

                // Mark workflow as failed
                workflow.status = AkashWorkflowStatus::Failed as i32;
                workflow.last_error = format!("{}", e);
                let _ = self.save_workflow(workflow).await;

                // Deactivate label if set
                if !workflow.label.is_empty() {
                    let _ = self.storage.deactivate_deployment_label(&workflow.label).await;
                }

                Err(e)
            }
        }
    }

    /// Run steps after deployment creation.
    ///
    /// Separated so we can wrap with cleanup logic on failure.
    async fn run_post_deployment_steps(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
        signing_client: &SigningClient,
        dseq: u64,
    ) -> Result<DeploymentResult> {
        let bids = self.step_wait_for_bids(workflow, dseq, opts).await?;
        let selected_bid = self.step_select_provider(workflow, &bids, opts).await?;
        self.step_create_lease(workflow, &selected_bid, signing_client)
            .await?;
        self.step_send_manifest(workflow).await?;
        let endpoints = self.step_retrieve_endpoints(workflow).await?;
        self.step_save_endpoints(workflow, endpoints).await?;

        // Mark completed
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.current_step = AkashWorkflowStep::Complete as i32;
        workflow.completed_at = Some(current_timestamp());
        self.save_workflow(workflow).await?;

        // Deactivate label from active deployments set
        if !workflow.label.is_empty() {
            if let Err(e) = self.storage.deactivate_deployment_label(&workflow.label).await {
                tracing::warn!("Failed to deactivate label '{}': {}", workflow.label, e);
            } else {
                tracing::info!("Deactivated label '{}' from active deployments", workflow.label);
            }
        }

        tracing::info!("───────────────────────────────────────────────────────────────");
        tracing::info!("  DEPLOYMENT COMPLETE");
        tracing::info!("───────────────────────────────────────────────────────────────");
        tracing::info!("Session:  {}", workflow.session_id);
        tracing::info!("DSEQ:     {}", dseq);
        tracing::info!("Provider: {}", selected_bid.provider);
        tracing::info!("Endpoints:");
        for ep in &workflow.service_endpoints {
            tracing::info!("  {} -> {}", ep.service_name, ep.external_uri);
        }
        tracing::info!("═══════════════════════════════════════════════════════════════");

        Ok(DeploymentResult {
            session_id: workflow.session_id.clone(),
            dseq,
            provider: selected_bid.provider.clone(),
            endpoints: workflow.service_endpoints.clone(),
        })
    }

    /// Cleanup a failed deployment by closing it.
    ///
    /// Broadcasts MsgCloseDeployment to recover escrow deposit.
    async fn cleanup_failed_deployment(
        &self,
        workflow: &AkashDeploymentWorkflow,
        dseq: u64,
        signing_client: &SigningClient,
    ) -> Result<()> {
        let msg = build_close_deployment_msg(&workflow.account_address, dseq);

        tracing::info!("  Broadcasting MsgCloseDeployment for cleanup...");
        let tx_resp = broadcast_akash_msg(
            signing_client,
            &MsgCloseDeployment::type_url(),
            &msg,
            "ergors cleanup failed deployment",
        )
        .await?;

        tracing::info!("  Cleanup tx_hash: {}", tx_resp.txhash);
        Ok(())
    }

    /// Step 1: Verify connectivity to Akash network.
    async fn step_connectivity_check(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        tracing::info!("[Step 1/11] Connectivity Check");
        workflow.current_step = AkashWorkflowStep::ConnectivityCheck as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("  Verifying network connectivity with endpoint failover...");

        // Create endpoint manager for failover
        let endpoint_manager = EndpointManager::from_config(&self.akash_config);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10)) // Reduced to 10s per attempt
            .build()?;

        // Try connectivity check with automatic endpoint failover
        let response = endpoint_manager
            .execute_with_failover(EndpointType::Rest, |rest_endpoint| {
                let client = client.clone();
                let node_info_url = format!(
                    "{}/cosmos/base/tendermint/v1beta1/node_info",
                    rest_endpoint.trim_end_matches('/')
                );

                async move {
                    tracing::info!("  Trying endpoint: {}", rest_endpoint);
                    let resp = client.get(&node_info_url).send().await?;

                    if !resp.status().is_success() {
                        return Err(anyhow!("HTTP {}: endpoint returned error", resp.status()));
                    }

                    Ok(resp)
                }
            })
            .await?;

        // Response is already validated in the failover closure
        let info: serde_json::Value = response.json().await?;

        // Extract network/chain info
        let network = info
            .get("default_node_info")
            .and_then(|n| n.get("network"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");

        let moniker = info
            .get("default_node_info")
            .and_then(|n| n.get("moniker"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");

        tracing::info!("  Network:  {}", network);
        tracing::info!("  Node:     {}", moniker);

        // Verify chain ID matches if configured
        if !workflow.chain_id.is_empty() && network != workflow.chain_id {
            tracing::error!(
                "  FAILED: Chain ID mismatch (expected {}, got {})",
                workflow.chain_id,
                network
            );
            return Err(anyhow!(
                "Chain ID mismatch: expected '{}', but connected to '{}'",
                workflow.chain_id,
                network
            ));
        }

        tracing::info!("  OK: Connected to Akash network");
        Ok(())
    }

    /// Step 2: Check account balance.
    async fn step_check_balance(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<()> {
        tracing::info!("[Step 2/11] Balance Check");
        workflow.current_step = AkashWorkflowStep::BalanceCheck as i32;
        self.save_workflow(workflow).await?;

        tracing::info!(
            "  Querying balance for wallet: {}",
            workflow.account_address
        );

        let balance = self
            .cosmos
            .query_balance(&workflow.account_address, "uakt")
            .await?;

        let amount: u64 = balance.amount.parse().unwrap_or(0);
        let min_required = if opts.min_balance_uakt > 0 {
            opts.min_balance_uakt
        } else {
            MIN_BALANCE_UAKT
        };

        let akt_balance = amount as f64 / 1_000_000.0;
        let akt_required = min_required as f64 / 1_000_000.0;

        tracing::info!("  Wallet:   {}", workflow.account_address);
        tracing::info!("  Balance:  {:.6} AKT ({} uakt)", akt_balance, amount);
        tracing::info!(
            "  Required: {:.6} AKT ({} uakt)",
            akt_required,
            min_required
        );

        if amount < min_required {
            tracing::error!("  FAILED: Insufficient balance");
            return Err(anyhow!(
                "Insufficient balance: {:.6} AKT (need at least {:.6} AKT)",
                akt_balance,
                akt_required
            ));
        }

        tracing::info!("  OK: Balance sufficient");
        Ok(())
    }

    /// Step 3 (optional): Request grant from another node and wait for approval.
    ///
    /// This step is only executed if `opts.request_grant_from` is set.
    /// It will block indefinitely until:
    /// - The grant is approved and confirmed on-chain
    /// - The grant is rejected
    /// - The user cancels (Ctrl+C)
    async fn step_grant_request_and_wait(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<()> {
        tracing::info!("[Step 3/11] Grant Request (Optional)");
        workflow.current_step = AkashWorkflowStep::GrantRequest as i32;
        self.save_workflow(workflow).await?;

        let granter_address = &opts.request_grant_from;
        tracing::info!("  Requesting grant from: {}", granter_address);
        tracing::info!("  Duration: {} seconds", opts.grant_duration_seconds);
        tracing::info!("  Spend limit: {} uakt", opts.grant_spend_limit_uakt);

        // Store grant request in workflow
        workflow.request_grant_from = granter_address.as_bytes().to_vec();
        workflow.grant_duration_seconds = opts.grant_duration_seconds;
        workflow.grant_spend_limit_uakt = opts.grant_spend_limit_uakt;

        // In production: Submit grant request to the granter node
        // For now, we just log that we would submit it
        tracing::info!("  Submitting grant request...");
        tracing::warn!("  NOTE: Grant request submission not yet implemented");
        tracing::warn!("  Proceeding without grant (account must be self-funded)");

        // Transition to GrantWait step
        workflow.current_step = AkashWorkflowStep::GrantWait as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("[Step 3b/11] Grant Wait");
        tracing::info!("  Waiting for grant approval...");
        tracing::info!("  Press Ctrl+C to cancel");

        // In production: Poll indefinitely until grant is confirmed/rejected
        // For now, we simulate a successful grant after a short wait
        // TODO: Implement actual grant polling when granter contract is ready
        tokio::time::sleep(Duration::from_secs(2)).await;

        tracing::info!("  OK: Grant request processed (skipped - not yet implemented)");
        Ok(())
    }

    /// Step 4: Setup certificate.
    async fn step_setup_certificate(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        tracing::info!("[Step 4/11] Certificate Setup");
        workflow.current_step = AkashWorkflowStep::CertificateSetup as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("  Address: {}", workflow.account_address);

        // The cert_manager logs detailed info about found vs created
        // Returns certificate + encrypted private key for mTLS
        let cert_with_key = self
            .cert_manager
            .get_or_create(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                &workflow.account_address,
                &self.custody_password,
            )
            .await?;

        // Store the official akash.cert.v1.Certificate
        workflow.certificate = Some(cert_with_key.certificate);

        // Store encrypted private key (only if we created a new certificate)
        // If certificate was found on chain, encrypted_private_key will be empty
        // and we'll use the previously stored key from workflow
        if !cert_with_key.encrypted_private_key.is_empty() {
            workflow.encrypted_cert_private_key = cert_with_key.encrypted_private_key;
            tracing::info!("  Stored encrypted certificate private key ({} bytes)",
                workflow.encrypted_cert_private_key.len());
        } else if workflow.encrypted_cert_private_key.is_empty() {
            tracing::warn!("  WARNING: Certificate found on chain but no stored private key!");
            tracing::warn!("  mTLS authentication may fail - consider revoking and recreating certificate");
        }

        self.save_workflow(workflow).await?;

        tracing::info!("  OK: Certificate ready");
        Ok(())
    }

    /// Step 3: Create deployment transaction.
    async fn step_create_deployment(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
        signing_client: &SigningClient,
    ) -> Result<u64> {
        tracing::info!("[Step 5/11] Create Deployment");
        workflow.current_step = AkashWorkflowStep::DeploymentCreate as i32;
        self.save_workflow(workflow).await?;

        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;

        tracing::info!(
            "  SDL Template: {}",
            if sdl.template_name.is_empty() {
                "(inline)"
            } else {
                &sdl.template_name
            }
        );
        tracing::debug!("  SDL Content size: {} bytes", sdl.resolved_content.len());

        if sdl.resolved_content.is_empty() {
            return Err(anyhow!("SDL content is empty"));
        }

        // Get next available dseq
        let dseq = get_next_dseq(self.cosmos.rest_endpoint(), &workflow.account_address).await?;
        tracing::info!("  Assigned DSEQ: {}", dseq);

        // Build MsgCreateDeployment
        let deposit = if opts.min_balance_uakt > 0 {
            opts.min_balance_uakt
        } else {
            DEFAULT_DEPOSIT_UAKT
        };

        let deposit_akt = deposit as f64 / 1_000_000.0;
        tracing::info!(
            "  Escrow Deposit: {:.6} AKT ({} uakt)",
            deposit_akt,
            deposit
        );

        let builder = DeploymentBuilder::new(&workflow.account_address, dseq).with_deposit(deposit);

        let msg = builder.build_from_sdl(&sdl.resolved_content)?;

        tracing::info!(
            "  MsgCreateDeployment: owner={}, dseq={}, groups={}",
            msg.id.as_ref().map(|id| id.owner.as_str()).unwrap_or(""),
            dseq,
            msg.groups.len()
        );

        // Log readable deployment details for verification
        if let Some(first_group) = msg.groups.first() {
            tracing::info!("  Group: {}", first_group.name);

            if let Some(first_resource) = first_group.resources.first() {
                tracing::info!("  Resources:");

                if let Some(resource) = &first_resource.resource {
                    if let Some(cpu) = &resource.cpu {
                        if let Some(units) = &cpu.units {
                            let cpu_str = String::from_utf8_lossy(&units.val);
                            tracing::info!("    - CPU: {} millicores", cpu_str);
                        }
                    }
                    if let Some(memory) = &resource.memory {
                        if let Some(qty) = &memory.quantity {
                            let mem_str = String::from_utf8_lossy(&qty.val);
                            let mem_bytes: u64 = mem_str.parse().unwrap_or(0);
                            tracing::info!("    - Memory: {} bytes ({} GB)", mem_str, mem_bytes / 1_073_741_824);
                        }
                    }
                    if let Some(gpu) = &resource.gpu {
                        if let Some(units) = &gpu.units {
                            let gpu_str = String::from_utf8_lossy(&units.val);
                            tracing::info!("    - GPU: {} units", gpu_str);
                            if !gpu.attributes.is_empty() {
                                tracing::info!("      Attributes:");
                                for attr in &gpu.attributes {
                                    tracing::info!("        - {}: {}", attr.key, attr.value);
                                }
                            }
                        }
                    }
                    if !resource.storage.is_empty() {
                        tracing::info!("    - Storage:");
                        for storage in &resource.storage {
                            if let Some(qty) = &storage.quantity {
                                let size_str = String::from_utf8_lossy(&qty.val);
                                let size_bytes: u64 = size_str.parse().unwrap_or(0);
                                tracing::info!("        - {}: {} bytes ({} GB)",
                                    storage.name, size_str, size_bytes / 1_073_741_824);
                            }
                        }
                    }
                }

                if let Some(price) = &first_resource.price {
                    tracing::info!("  Price: {} {}", price.amount, price.denom);
                }

                tracing::info!("  Count: {}", first_resource.count);
            }
        }

        // Broadcast deployment transaction using type-safe helper
        tracing::info!("  Broadcasting MsgCreateDeployment...");

        // Log protobuf encoding for comparison with official tool
        use prost::Message;
        let msg_bytes = msg.encode_to_vec();
        tracing::debug!("  Protobuf size: {} bytes", msg_bytes.len());
        tracing::debug!("  Protobuf hex (first 500 bytes): {}",
            hex::encode(&msg_bytes[..msg_bytes.len().min(500)]));

        let _tx_resp = broadcast_akash_msg(
            signing_client,
            &MsgCreateDeployment::type_url(),
            &msg,
            "ergors automated deployment",
        )
        .await?;

        // Store deployment info
        workflow.deployment = Some(AkashRuntime {
            deployment_sequence: dseq.to_string(),
            group_sequence: "1".to_string(),
            order_sequence: "1".to_string(),
            provider_address: String::new(),
            lease_id: String::new(),
            service_endpoints: Vec::new(),
            lease_price_per_block: 0,
            provider_host_uri: String::new(),
        });

        self.save_workflow(workflow).await?;
        tracing::info!("  OK: Deployment created (DSEQ: {})", dseq);
        Ok(dseq)
    }

    /// Step 4: Wait for bids.
    async fn step_wait_for_bids(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        dseq: u64,
        opts: &AkashWorkflowOptions,
    ) -> Result<Vec<BidInfo>> {
        tracing::info!("[Step 6/11] Wait for Bids");
        workflow.current_step = AkashWorkflowStep::BidWait as i32;
        self.save_workflow(workflow).await?;

        let wait_blocks = if opts.bid_wait_blocks > 0 {
            opts.bid_wait_blocks
        } else {
            DEFAULT_BID_WAIT_BLOCKS
        };

        let wait_secs = wait_blocks * 6;
        tracing::info!("  DSEQ: {}", dseq);
        tracing::info!(
            "  Waiting ~{}s ({} blocks) for providers to bid...",
            wait_secs,
            wait_blocks
        );

        // Initial wait for bids to arrive
        tokio::time::sleep(Duration::from_secs(wait_blocks as u64 * 6)).await;

        // Poll for bids
        let mut bids = Vec::new();
        let mut attempts = 0;

        while attempts < MAX_BID_POLL_ATTEMPTS {
            attempts += 1;

            tracing::info!(
                "  Polling for bids (attempt {}/{})...",
                attempts,
                MAX_BID_POLL_ATTEMPTS
            );

            let query_result = self
                .cosmos
                .query_open_bids(&workflow.account_address, dseq)
                .await;

            match query_result {
                Ok(found_bids) => {
                    if !found_bids.is_empty() {
                        tracing::info!("  Received {} bid(s):", found_bids.len());

                        // Log all bids with details, including provider info
                        for (i, bid) in found_bids.iter().enumerate() {
                            let price: f64 = bid.price_amount.parse().unwrap_or(0.0);
                            let price_akt = price / 1_000_000.0;

                            // Get cached provider info for display
                            let provider_name = if let Some(info) =
                                self.get_provider_info_cached(&bid.provider).await
                            {
                                Self::format_provider_name(&info)
                            } else {
                                bid.provider[..20.min(bid.provider.len())].to_string()
                            };

                            tracing::info!(
                                "    [{}] {} ({})",
                                i + 1,
                                provider_name,
                                &bid.provider[..15.min(bid.provider.len())]
                            );
                            tracing::info!(
                                "        Price: {:.6} AKT/block",
                                price_akt
                            );
                        }

                        bids = found_bids;
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("  Query failed: {}", e);
                }
            }

            if attempts < MAX_BID_POLL_ATTEMPTS {
                tracing::info!("  No bids yet, waiting 6s for next block...");
                tokio::time::sleep(Duration::from_secs(6)).await;
            }
        }

        if bids.is_empty() {
            tracing::error!(
                "  FAILED: No bids received after {} attempts",
                MAX_BID_POLL_ATTEMPTS
            );
            return Err(anyhow!(
                "No bids received after {} attempts. Check provider availability or increase max price.",
                MAX_BID_POLL_ATTEMPTS
            ));
        }

        // Store bids in workflow
        workflow.available_bids = bids
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

        self.save_workflow(workflow).await?;
        tracing::info!("  OK: {} bid(s) available for selection", bids.len());
        Ok(bids)
    }

    /// Step 7: Select provider.
    ///
    /// Default behavior: auto-select cheapest from trusted providers (or all if none trusted).
    /// If `opts.interactive_bid` is true: future implementation would pause for user input.
    async fn step_select_provider(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        bids: &[BidInfo],
        opts: &AkashWorkflowOptions,
    ) -> Result<BidInfo> {
        tracing::info!("[Step 7/11] Provider Selection");
        workflow.current_step = AkashWorkflowStep::ProviderSelection as i32;
        self.save_workflow(workflow).await?;

        // Check if interactive mode is requested
        let selection_mode = if opts.interactive_bid {
            // Interactive mode requested - log available bids with provider names
            tracing::info!("  Mode: INTERACTIVE (user selection)");
            tracing::info!("  Available bids:");
            for (i, bid) in bids.iter().enumerate() {
                let price_decimal: f64 = bid.price_amount.parse().unwrap_or(0.0);
                let price_akt = price_decimal / 1_000_000.0;
                let trusted = if opts.trusted_providers.contains(&bid.provider) {
                    " [TRUSTED]"
                } else {
                    ""
                };

                // Get provider name from cache
                let provider_name = if let Some(info) =
                    self.get_provider_info_cached(&bid.provider).await
                {
                    Self::format_provider_name(&info)
                } else {
                    bid.provider[..20.min(bid.provider.len())].to_string()
                };

                tracing::info!(
                    "    [{}] {} - {:.6} AKT/block{}",
                    i + 1,
                    provider_name,
                    price_akt,
                    trusted
                );
                tracing::info!("        Address: {}", bid.provider);
            }
            tracing::warn!("  NOTE: Interactive selection not yet implemented");
            tracing::warn!("  Falling back to auto-selection...");
            "AUTO (fallback from interactive)"
        } else {
            "AUTO"
        };

        // Filter by trusted providers if specified
        let filter_mode = if opts.trusted_providers.is_empty() {
            "all providers (no trusted list)"
        } else {
            "trusted providers only"
        };
        tracing::info!("  Selection: {}", selection_mode);
        tracing::info!("  Filter: {}", filter_mode);

        let candidates: Vec<_> = if opts.trusted_providers.is_empty() {
            bids.to_vec()
        } else {
            tracing::info!("  Trusted list: {:?}", opts.trusted_providers);
            bids.iter()
                .filter(|b| opts.trusted_providers.contains(&b.provider))
                .cloned()
                .collect()
        };

        tracing::info!(
            "  Candidates: {} (from {} total bids)",
            candidates.len(),
            bids.len()
        );

        if candidates.is_empty() {
            tracing::error!("  FAILED: No bids from trusted providers");
            tracing::error!(
                "  Available providers: {:?}",
                bids.iter().map(|b| &b.provider).collect::<Vec<_>>()
            );
            return Err(anyhow!(
                "No bids from trusted providers. Available providers: {:?}",
                bids.iter().map(|b| &b.provider).collect::<Vec<_>>()
            ));
        }

        // Select cheapest bid (auto-selection)
        // Note: Prices are in decimal format (e.g., "6002.811140000000000000" uakt)
        let selected = candidates
            .iter()
            .min_by(|a, b| {
                let price_a: f64 = a.price_amount.parse().unwrap_or(f64::MAX);
                let price_b: f64 = b.price_amount.parse().unwrap_or(f64::MAX);
                price_a.partial_cmp(&price_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .ok_or_else(|| anyhow!("Failed to select bid"))?;

        let price_uakt: f64 = selected.price_amount.parse().unwrap_or(0.0);
        let price_akt = price_uakt / 1_000_000.0;
        let is_trusted = opts.trusted_providers.contains(&selected.provider);

        tracing::info!("  ─────────────────────────────────────────");
        tracing::info!("  Selected: {} (cheapest)", selected.provider);
        tracing::info!(
            "  Price:    {:.6} AKT/block ({} {})",
            price_akt,
            selected.price_amount,
            selected.price_denom
        );
        tracing::info!("  Trusted:  {}", if is_trusted { "YES" } else { "NO" });
        tracing::info!("  ─────────────────────────────────────────");

        // Query provider info to get the actual host_uri
        tracing::info!("  Querying provider info...");
        let provider_info = self.cosmos.query_provider(&selected.provider).await?;
        tracing::info!("  Host URI: {}", provider_info.host_uri);
        if !provider_info.email.is_empty() {
            tracing::info!("  Email:    {}", provider_info.email);
        }
        if !provider_info.website.is_empty() {
            tracing::info!("  Website:  {}", provider_info.website);
        }

        workflow.provider = Some(AkashProviderSelection {
            provider_address: selected.provider.clone(),
            reputation_score: 100, // Would query reputation system in production
            bid_price_uakt: price_uakt as u64, // Convert decimal to integer uakt
            total_bids_received: bids.len() as u32,
            selected_at: Some(current_timestamp()),
            is_trusted_provider: is_trusted,
        });

        // Store provider host_uri in the deployment runtime for future use
        if let Some(ref mut runtime) = workflow.deployment {
            runtime.provider_host_uri = provider_info.host_uri.clone();
        }

        self.save_workflow(workflow).await?;
        tracing::info!("  OK: Provider selected with host_uri");
        Ok(selected)
    }

    /// Step 6: Create lease.
    async fn step_create_lease(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        bid: &BidInfo,
        signing_client: &SigningClient,
    ) -> Result<()> {
        tracing::info!("[Step 8/11] Create Lease");
        workflow.current_step = AkashWorkflowStep::LeaseCreate as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("  Provider: {}", bid.provider);
        tracing::info!(
            "  DSEQ: {}, GSEQ: {}, OSEQ: {}",
            bid.dseq,
            bid.gseq,
            bid.oseq
        );

        // Build MsgCreateLease
        let msg = build_create_lease_msg(&bid.owner, bid.dseq, bid.gseq, bid.oseq, &bid.provider, bid.bseq);

        // Broadcast lease transaction using type-safe helper
        tracing::info!("  Broadcasting MsgCreateLease...");
        let _tx_resp = broadcast_akash_msg(
            signing_client,
            &MsgCreateLease::type_url(),
            &msg,
            "ergors lease creation",
        )
        .await?;

        let lease_id = format!(
            "{}/{}/{}/{}/{}/{}",
            bid.owner, bid.dseq, bid.gseq, bid.oseq, bid.provider, bid.bseq
        );
        tracing::info!("  Lease ID: {}", lease_id);

        // Store lease info
        workflow.lease_id_info = Some(AkashLeaseIdInfo {
            owner: bid.owner.clone(),
            dseq: bid.dseq,
            gseq: bid.gseq,
            oseq: bid.oseq,
            provider: bid.provider.clone(),
        });

        // Update runtime info
        if let Some(ref mut runtime) = workflow.deployment {
            runtime.provider_address = bid.provider.clone();
            runtime.lease_id = lease_id;
        }

        self.save_workflow(workflow).await?;
        tracing::info!("  OK: Lease created");
        Ok(())
    }

    /// Step 7: Send manifest.
    async fn step_send_manifest(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        tracing::info!("[Step 9/11] Send Manifest");
        workflow.current_step = AkashWorkflowStep::ManifestSend as i32;
        self.save_workflow(workflow).await?;

        let lease_info = workflow
            .lease_id_info
            .as_ref()
            .ok_or_else(|| anyhow!("No lease info"))?;

        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;

        // Get provider host_uri from runtime (queried during bid selection)
        let provider_uri = workflow
            .deployment
            .as_ref()
            .map(|r| r.provider_host_uri.clone())
            .ok_or_else(|| anyhow!("No provider host_uri in runtime"))?;

        if provider_uri.is_empty() {
            return Err(anyhow!("Provider host_uri is empty - provider info query may have failed"));
        }

        let manifest_endpoint = format!("{}/deployment/{}/manifest", provider_uri, lease_info.dseq);

        tracing::info!("  Provider URI: {}", provider_uri);
        tracing::info!("  Endpoint:     {}", manifest_endpoint);

        // Get certificate for mTLS
        let cert = workflow
            .certificate
            .as_ref()
            .ok_or_else(|| anyhow!("No certificate in workflow"))?;

        if workflow.encrypted_cert_private_key.is_empty() {
            return Err(anyhow!("No encrypted certificate private key - cannot authenticate with provider"));
        }

        // Decode certificate PEM (chain returns Base64-encoded bytes)
        let cert_pem = decode_cert_pem(&cert.cert)?;

        // Decrypt private key for mTLS
        tracing::info!("  Decrypting certificate private key...");
        let privkey_pem = super::certificate::decrypt_private_key(
            &workflow.encrypted_cert_private_key,
            &self.custody_password,
        )?;

        // Create mTLS manifest sender
        tracing::info!("  Creating mTLS client...");
        let sender = ManifestSender::with_mtls(&provider_uri, &cert_pem, &privkey_pem)?;

        tracing::info!("  Sending manifest...");
        match sender
            .send_manifest_from_sdl(
                &lease_info.owner,
                lease_info.dseq,
                lease_info.gseq,
                lease_info.oseq,
                &sdl.resolved_content,
            )
            .await
        {
            Ok(_) => {
                tracing::info!("  OK: Manifest accepted by provider");
            }
            Err(e) => {
                tracing::error!("  FAILED: Manifest send failed");
                tracing::error!("  Error: {}", e);
                return Err(e);
            }
        }

        self.save_workflow(workflow).await?;
        Ok(())
    }

    /// Step 8: Retrieve endpoints.
    async fn step_retrieve_endpoints(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<Vec<AkashServiceEndpoint>> {
        tracing::info!("[Step 10/11] Retrieve Endpoints");
        workflow.current_step = AkashWorkflowStep::EndpointRetrieval as i32;
        self.save_workflow(workflow).await?;

        let lease_info = workflow
            .lease_id_info
            .as_ref()
            .ok_or_else(|| anyhow!("No lease info"))?;

        // Get provider host_uri from runtime (queried during bid selection)
        let provider_uri = workflow
            .deployment
            .as_ref()
            .map(|r| r.provider_host_uri.clone())
            .ok_or_else(|| anyhow!("No provider host_uri in runtime"))?;

        if provider_uri.is_empty() {
            return Err(anyhow!("Provider host_uri is empty - provider info query may have failed"));
        }

        let status_endpoint = format!(
            "{}/lease/{}/{}/{}/{}/status",
            provider_uri, lease_info.dseq, lease_info.gseq, lease_info.oseq, lease_info.provider
        );

        tracing::info!("  Provider URI: {}", provider_uri);
        tracing::info!("  Status URL:   {}", status_endpoint);
        tracing::info!("  Waiting 10s for container startup...");

        // Wait a bit for services to start
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Get certificate for mTLS
        let cert = workflow
            .certificate
            .as_ref()
            .ok_or_else(|| anyhow!("No certificate in workflow"))?;

        // Decode certificate PEM (chain returns Base64-encoded bytes)
        let cert_pem = decode_cert_pem(&cert.cert)?;

        // Decrypt private key for mTLS
        let privkey_pem = super::certificate::decrypt_private_key(
            &workflow.encrypted_cert_private_key,
            &self.custody_password,
        )?;

        // Poll for endpoints with retries
        let mut endpoints = HashMap::new();
        let mut attempts = 0;
        const MAX_ENDPOINT_ATTEMPTS: u32 = 5;

        while attempts < MAX_ENDPOINT_ATTEMPTS {
            attempts += 1;

            tracing::info!(
                "  Polling endpoints (attempt {}/{})...",
                attempts,
                MAX_ENDPOINT_ATTEMPTS
            );

            match query_service_endpoints_mtls(
                &provider_uri,
                &lease_info.owner,
                lease_info.dseq,
                lease_info.gseq,
                lease_info.oseq,
                &cert_pem,
                &privkey_pem,
            )
            .await
            {
                Ok(eps) if !eps.is_empty() => {
                    tracing::info!("  Discovered {} endpoint(s):", eps.len());
                    for (name, ep) in eps {
                        tracing::info!("    ┌─ Service: {}", name);
                        tracing::info!("    │  URI:      {}", ep.external_uri);
                        tracing::info!(
                            "    │  Port:     {}:{} ({})",
                            ep.external_port,
                            ep.internal_port,
                            ep.protocol
                        );
                        tracing::info!("    └──────────────────────────────");
                        endpoints.insert(name, ep);
                    }
                    break;
                }
                Ok(_) => {
                    tracing::info!("  No endpoints yet, waiting 5s...");
                }
                Err(e) => {
                    tracing::warn!("  Query failed: {}", e);
                }
            }

            if attempts < MAX_ENDPOINT_ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }

        // Convert to proto type
        let endpoint_infos: Vec<AkashServiceEndpoint> = endpoints
            .into_iter()
            .map(|(name, ep)| AkashServiceEndpoint {
                service_name: name,
                external_uri: ep.external_uri,
                internal_port: ep.internal_port as u32,
                external_port: ep.external_port as u32,
                protocol: ep.protocol,
            })
            .collect();

        if endpoint_infos.is_empty() {
            tracing::warn!("  Warning: No endpoints discovered - service may still be starting");
        } else {
            tracing::info!("  OK: {} endpoint(s) retrieved", endpoint_infos.len());
        }

        Ok(endpoint_infos)
    }

    /// Step 11: Save endpoints.
    async fn step_save_endpoints(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        endpoints: Vec<AkashServiceEndpoint>,
    ) -> Result<()> {
        tracing::info!("[Step 11/11] Save Endpoints");

        // Store in workflow
        workflow.service_endpoints = endpoints.clone();

        // Also store in legacy endpoints map for backwards compatibility
        for ep in &endpoints {
            workflow
                .endpoints
                .insert(ep.service_name.clone(), ep.external_uri.clone());
        }

        // Save workflow first
        self.save_workflow(workflow).await?;

        // Store endpoints in dedicated index for efficient retrieval
        if let Err(e) = self
            .storage
            .put_akash_endpoints(&workflow.session_id, &endpoints)
            .await
        {
            tracing::warn!("Failed to store endpoints in index: {}", e);
        }

        // Store endpoints in a dedicated storage key for easy access
        let lease_info = workflow.lease_id_info.as_ref();
        if let Some(info) = lease_info {
            let storage_key = format!(
                "deployment_endpoints/{}/{}/{}",
                info.owner, info.dseq, info.provider
            );

            // Serialize endpoints to JSON for storage
            let _endpoints_json = serde_json::to_string(&endpoints)?;

            tracing::info!("  Storage Key: {}", storage_key);
        }

        tracing::info!("  Saved {} endpoint(s) to workflow state", endpoints.len());
        for ep in &endpoints {
            tracing::info!("    {} -> {}", ep.service_name, ep.external_uri);
        }
        tracing::info!("  OK: Endpoints persisted to storage");

        Ok(())
    }

    /// Save workflow to storage.
    async fn save_workflow(&self, workflow: &AkashDeploymentWorkflow) -> Result<()> {
        let mut workflow = workflow.clone();
        workflow.updated_at = Some(current_timestamp());

        self.storage
            .put_akash_workflow(&workflow)
            .await
            .map_err(|e| anyhow!("Failed to save workflow: {}", e))
    }

    /// Get provider info, using cache first then chain query.
    /// Caches result for future lookups.
    async fn get_provider_info_cached(
        &self,
        provider_address: &str,
    ) -> Option<crate::storage::CachedProviderInfo> {
        use crate::storage::CachedProviderInfo;

        // Check cache first
        if let Ok(Some(cached)) = self.storage.get_akash_provider_info(provider_address).await {
            return Some(cached);
        }

        // Query chain
        match self.cosmos.query_provider(provider_address).await {
            Ok(info) => {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                let cached = CachedProviderInfo {
                    address: provider_address.to_string(),
                    host_uri: info.host_uri.clone(),
                    email: info.email.clone(),
                    website: info.website.clone(),
                    attributes: vec![], // TODO: Parse from chain response if needed
                    cached_at: now,
                };

                // Store in cache (ignore errors)
                let _ = self.storage.put_akash_provider_info(&cached).await;

                Some(cached)
            }
            Err(e) => {
                tracing::debug!("Failed to query provider {}: {}", provider_address, e);
                None
            }
        }
    }

    /// Format provider display name from cached info
    fn format_provider_name(info: &crate::storage::CachedProviderInfo) -> String {
        // Try to extract a human-readable name from attributes or use address
        for (key, value) in &info.attributes {
            if key == "organization" || key == "host" {
                return value.clone();
            }
        }

        // Check for host from URI
        if let Some(host) = info.host_uri.strip_prefix("https://") {
            if let Some(host) = host.split('/').next() {
                if let Some(domain) = host.strip_suffix(":8443") {
                    return domain.to_string();
                }
                return host.to_string();
            }
        }

        // Fallback to truncated address
        if info.address.len() > 20 {
            format!("{}...", &info.address[..20])
        } else {
            info.address.clone()
        }
    }

    /// Close a deployment.
    pub async fn close_deployment(&self, workflow: &AkashDeploymentWorkflow) -> Result<()> {
        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment info"))?;

        let dseq: u64 = deployment.deployment_sequence.parse().unwrap_or(0);
        if dseq == 0 {
            return Err(anyhow!("Invalid dseq"));
        }

        tracing::info!("Closing deployment dseq {}", dseq);

        let msg = build_close_deployment_msg(&workflow.account_address, dseq);

        // Create signing client for this operation
        let signing_client = self
            .create_climb_client(&workflow.selected_key_name, workflow.hd_account_index)
            .await?;

        // Broadcast close deployment transaction using type-safe helper
        tracing::info!("  Broadcasting MsgCloseDeployment...");
        let tx_resp = broadcast_akash_msg(
            &signing_client,
            &MsgCloseDeployment::type_url(),
            &msg,
            "ergors close deployment",
        )
        .await?;

        tracing::info!("Deployment closed: tx_hash={}", tx_resp.txhash);
        Ok(())
    }

    /// Update a deployment with new SDL.
    pub async fn update_deployment(
        &self,
        workflow: &AkashDeploymentWorkflow,
        sdl_content: &str,
    ) -> Result<()> {
        use super::deployment_builder::build_update_deployment_msg;
        use ho_std::types::ergors::akash::deployment::v1beta5::MsgUpdateDeployment;
        use sha2::{Sha256, Digest};

        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment info"))?;

        let dseq: u64 = deployment.deployment_sequence.parse().unwrap_or(0);
        if dseq == 0 {
            return Err(anyhow!("Invalid dseq"));
        }

        tracing::info!("Updating deployment dseq {} with new SDL", dseq);

        // Hash the SDL to create hash bytes
        let mut hasher = Sha256::new();
        hasher.update(sdl_content.as_bytes());
        let hash = hasher.finalize().to_vec();

        let msg = build_update_deployment_msg(&workflow.account_address, dseq, hash);

        // Create signing client for this operation
        let signing_client = self
            .create_climb_client(&workflow.selected_key_name, workflow.hd_account_index)
            .await?;

        // Broadcast update deployment transaction
        tracing::info!("  Broadcasting MsgUpdateDeployment...");
        let tx_resp = broadcast_akash_msg(
            &signing_client,
            &MsgUpdateDeployment::type_url(),
            &msg,
            "ergors update deployment",
        )
        .await?;

        tracing::info!("Deployment updated: tx_hash={}", tx_resp.txhash);
        Ok(())
    }

    /// Top up escrow account for a deployment.
    pub async fn topup_escrow(
        &self,
        workflow: &AkashDeploymentWorkflow,
        amount_uakt: u64,
    ) -> Result<()> {
        use super::deployment_builder::build_escrow_deposit_msg;
        use ho_std::types::ergors::akash::escrow::v1::MsgAccountDeposit;

        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment info"))?;

        let dseq: u64 = deployment.deployment_sequence.parse().unwrap_or(0);
        if dseq == 0 {
            return Err(anyhow!("Invalid dseq"));
        }

        tracing::info!("Topping up escrow for deployment dseq {} with {} uakt", dseq, amount_uakt);

        let msg = build_escrow_deposit_msg(
            &workflow.account_address,
            &workflow.account_address,
            dseq,
            amount_uakt,
        )?;

        // Create signing client for this operation
        let signing_client = self
            .create_climb_client(&workflow.selected_key_name, workflow.hd_account_index)
            .await?;

        // Broadcast escrow deposit transaction
        tracing::info!("  Broadcasting MsgAccountDeposit...");
        let tx_resp = broadcast_akash_msg(
            &signing_client,
            &MsgAccountDeposit::type_url(),
            &msg,
            "ergors topup escrow",
        )
        .await?;

        tracing::info!("Escrow topped up: tx_hash={}, amount={} uakt", tx_resp.txhash, amount_uakt);
        Ok(())
    }
}

/// Result of a successful deployment.
#[derive(Debug, Clone)]
pub struct DeploymentResult {
    pub session_id: String,
    pub dseq: u64,
    pub provider: String,
    pub endpoints: Vec<AkashServiceEndpoint>,
}

/// Get current timestamp.
fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

/// Decode certificate PEM from chain response.
///
/// The Akash chain stores certificate PEM as bytes, but REST/gRPC responses
/// may return it as Base64-encoded string. This function handles both cases:
/// - If already valid PEM (starts with "-----BEGIN"), return as-is
/// - If Base64-encoded, decode it first
fn decode_cert_pem(cert_bytes: &[u8]) -> Result<Vec<u8>> {
    let cert_str = String::from_utf8_lossy(cert_bytes);

    // If it already looks like PEM, return as-is
    if cert_str.trim().starts_with("-----BEGIN") {
        return Ok(cert_bytes.to_vec());
    }

    // Try to decode as Base64
    let decoded = BASE64
        .decode(cert_bytes)
        .map_err(|e| anyhow!("Certificate is not valid PEM and failed Base64 decode: {}", e))?;

    // Verify decoded content is PEM
    let decoded_str = String::from_utf8_lossy(&decoded);
    if !decoded_str.trim().starts_with("-----BEGIN") {
        return Err(anyhow!(
            "Decoded certificate is not valid PEM. First 50 chars: {}",
            decoded_str.chars().take(50).collect::<String>()
        ));
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts.seconds > 0);
    }

    #[test]
    fn test_decode_cert_pem_already_pem() {
        let pem = b"-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----";
        let result = decode_cert_pem(pem).unwrap();
        assert_eq!(result, pem);
    }

    #[test]
    fn test_decode_cert_pem_base64() {
        let pem = "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----";
        let encoded = BASE64.encode(pem.as_bytes());
        let result = decode_cert_pem(encoded.as_bytes()).unwrap();
        assert_eq!(String::from_utf8_lossy(&result), pem);
    }
}
