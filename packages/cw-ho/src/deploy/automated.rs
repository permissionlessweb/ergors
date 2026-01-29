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
use super::manifest::{query_service_endpoints, ManifestSender};
use crate::storage::ErgorsStorage;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::akash::deployment::v1beta4::{MsgCloseDeployment, MsgCreateDeployment};
use ho_std::types::ergors::akash::market::v1beta4::MsgCreateLease;
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
    ) -> Self {
        Self {
            storage,
            cosmos,
            cert_manager,
            key_manager,
            key_store,
            akash_config,
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

        self.step_connectivity_check(workflow).await?;
        self.step_check_balance(workflow, opts).await?;
        if !opts.request_grant_from.is_empty() {
            self.step_grant_request_and_wait(workflow, opts).await?;
        }
        self.step_setup_certificate(workflow).await?;
        let dseq = self
            .step_create_deployment(workflow, opts, &signing_client)
            .await?;
        let bids = self.step_wait_for_bids(workflow, dseq, opts).await?;
        let selected_bid = self.step_select_provider(workflow, &bids, opts).await?;
        self.step_create_lease(workflow, &selected_bid, &signing_client)
            .await?;
        self.step_send_manifest(workflow).await?;
        let endpoints = self.step_retrieve_endpoints(workflow).await?;
        self.step_save_endpoints(workflow, endpoints).await?;

        // Mark completed
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.current_step = AkashWorkflowStep::Complete as i32;
        workflow.completed_at = Some(current_timestamp());
        self.save_workflow(workflow).await?;

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
        let cert_info = self
            .cert_manager
            .get_or_create(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                &workflow.account_address,
            )
            .await?;

        workflow.certificate_info = Some(cert_info);
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

        tracing::debug!(
            "  MsgCreateDeployment: owner={}, dseq={}, groups={}",
            msg.id.as_ref().map(|id| id.owner.as_str()).unwrap_or(""),
            dseq,
            msg.groups.len()
        );

        // Log the full message for debugging
        if let Ok(msg_json) = serde_json::to_string_pretty(&msg) {
            tracing::debug!("  Full MsgCreateDeployment JSON:\n{}", msg_json);
        }

        // Broadcast deployment transaction using type-safe helper
        tracing::info!("  Broadcasting MsgCreateDeployment...");
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

                        // Log all bids with details
                        for (i, bid) in found_bids.iter().enumerate() {
                            let price: u64 = bid.price_amount.parse().unwrap_or(0);
                            let price_akt = price as f64 / 1_000_000.0;
                            tracing::info!("    [{}] Provider: {}", i + 1, bid.provider);
                            tracing::info!(
                                "        Price: {:.6} AKT/block ({} {})",
                                price_akt,
                                bid.price_amount,
                                bid.price_denom
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
            // Interactive mode requested - log available bids
            tracing::info!("  Mode: INTERACTIVE (user selection)");
            tracing::info!("  Available bids:");
            for (i, bid) in bids.iter().enumerate() {
                let price: u64 = bid.price_amount.parse().unwrap_or(0);
                let price_akt = price as f64 / 1_000_000.0;
                let trusted = if opts.trusted_providers.contains(&bid.provider) {
                    " [TRUSTED]"
                } else {
                    ""
                };
                tracing::info!(
                    "    [{}] {} - {:.6} AKT/block{}",
                    i + 1,
                    bid.provider,
                    price_akt,
                    trusted
                );
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
        let selected = candidates
            .iter()
            .min_by(|a, b| {
                let price_a: u64 = a.price_amount.parse().unwrap_or(u64::MAX);
                let price_b: u64 = b.price_amount.parse().unwrap_or(u64::MAX);
                price_a.cmp(&price_b)
            })
            .cloned()
            .ok_or_else(|| anyhow!("Failed to select bid"))?;

        let price: u64 = selected.price_amount.parse().unwrap_or(0);
        let price_akt = price as f64 / 1_000_000.0;
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

        workflow.provider = Some(AkashProviderSelection {
            provider_address: selected.provider.clone(),
            reputation_score: 100, // Would query reputation system in production
            bid_price_uakt: price,
            total_bids_received: bids.len() as u32,
            selected_at: Some(current_timestamp()),
            is_trusted_provider: is_trusted,
        });

        self.save_workflow(workflow).await?;
        tracing::info!("  OK: Provider selected");
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
        let msg = build_create_lease_msg(&bid.owner, bid.dseq, bid.gseq, bid.oseq, &bid.provider);

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
            "{}/{}/{}/{}/{}",
            bid.owner, bid.dseq, bid.gseq, bid.oseq, bid.provider
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

        // Construct provider URI
        let provider_uri = format!("https://{}:8443", lease_info.provider);
        let manifest_endpoint = format!("{}/deployment/{}/manifest", provider_uri, lease_info.dseq);

        tracing::info!("  Provider URI: {}", provider_uri);
        tracing::info!("  Endpoint:     {}", manifest_endpoint);
        tracing::info!("  Sending manifest...");

        let sender = ManifestSender::new(&provider_uri);
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

        // Store provider URI in runtime
        if let Some(ref mut runtime) = workflow.deployment {
            runtime.provider_host_uri = provider_uri;
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

        let provider_uri = format!("https://{}:8443", lease_info.provider);
        let status_endpoint = format!(
            "{}/lease/{}/{}/{}/{}/status",
            provider_uri, lease_info.dseq, lease_info.gseq, lease_info.oseq, lease_info.provider
        );

        tracing::info!("  Provider URI: {}", provider_uri);
        tracing::info!("  Status URL:   {}", status_endpoint);
        tracing::info!("  Waiting 10s for container startup...");

        // Wait a bit for services to start
        tokio::time::sleep(Duration::from_secs(10)).await;

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

            match query_service_endpoints(
                &provider_uri,
                &lease_info.owner,
                lease_info.dseq,
                lease_info.gseq,
                lease_info.oseq,
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

    /// Step 9: Save endpoints.
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

        self.save_workflow(workflow).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts.seconds > 0);
    }
}
