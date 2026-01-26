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
    AkashBidInfo, AkashBidState, AkashDeploymentWorkflow, AkashLeaseIdInfo,
    AkashProviderSelection, AkashRuntime, AkashServiceEndpoint, AkashWorkflowOptions,
    AkashWorkflowStatus, AkashWorkflowStep,
};
use pbjson_types::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::certificate::CertificateManager;
use super::cosmos_client::{BidInfo, CosmosClient};
use super::deployment_builder::{
    build_close_deployment_msg, build_create_lease_msg, get_next_dseq, DeploymentBuilder,
    DEFAULT_DEPOSIT_UAKT,
};
use super::manifest::{query_service_endpoints, ManifestSender};
use super::signer::{msg_to_any, msg_types, TxSigner};
use super::tx_lifecycle::{TxLifecycle, DEFAULT_GAS_LIMIT, DEFAULT_GAS_PRICE};
use crate::storage::ErgorsStorage;

/// Minimum balance required for deployment (5 AKT).
const MIN_BALANCE_UAKT: u64 = 5_000_000;

/// Default blocks to wait for bids (2 blocks = ~12s on Akash).
const DEFAULT_BID_WAIT_BLOCKS: u32 = 2;

/// Maximum bid polling attempts.
const MAX_BID_POLL_ATTEMPTS: u32 = 10;

/// Automated deployment runner.
///
/// Executes the complete deployment flow without manual intervention.
pub struct AutomatedDeployer {
    storage: Arc<ErgorsStorage>,
    cosmos: Arc<CosmosClient>,
    cert_manager: Arc<CertificateManager>,
    tx_lifecycle: Arc<TxLifecycle>,
    signer: Arc<TxSigner>,
}

impl AutomatedDeployer {
    /// Create a new automated deployer.
    pub fn new(
        storage: Arc<ErgorsStorage>,
        cosmos: Arc<CosmosClient>,
        cert_manager: Arc<CertificateManager>,
        tx_lifecycle: Arc<TxLifecycle>,
        signer: Arc<TxSigner>,
    ) -> Self {
        Self {
            storage,
            cosmos,
            cert_manager,
            tx_lifecycle,
            signer,
        }
    }

    /// Run automated deployment.
    ///
    /// This is the main entry point for fully automated deployments.
    pub async fn deploy(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<DeploymentResult> {
        tracing::info!(
            "Starting automated deployment for session {}",
            workflow.session_id
        );

        // Set workflow status
        workflow.status = AkashWorkflowStatus::Running as i32;
        workflow.options = Some(opts.clone());
        self.save_workflow(workflow).await?;

        // Step 1: Check balance
        self.step_check_balance(workflow, opts).await?;

        // Step 2: Setup certificate
        self.step_setup_certificate(workflow).await?;

        // Step 3: Create deployment
        let dseq = self.step_create_deployment(workflow, opts).await?;

        // Step 4: Wait for and collect bids
        let bids = self.step_wait_for_bids(workflow, dseq, opts).await?;

        // Step 5: Select provider
        let selected_bid = self.step_select_provider(workflow, &bids, opts).await?;

        // Step 6: Create lease
        self.step_create_lease(workflow, &selected_bid).await?;

        // Step 7: Send manifest
        self.step_send_manifest(workflow).await?;

        // Step 8: Retrieve endpoints
        let endpoints = self.step_retrieve_endpoints(workflow).await?;

        // Step 9: Save endpoints to storage
        self.step_save_endpoints(workflow, endpoints).await?;

        // Mark completed
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.current_step = AkashWorkflowStep::Complete as i32;
        workflow.completed_at = Some(current_timestamp());
        self.save_workflow(workflow).await?;

        tracing::info!(
            "Deployment completed successfully for session {}",
            workflow.session_id
        );

        Ok(DeploymentResult {
            session_id: workflow.session_id.clone(),
            dseq,
            provider: selected_bid.provider.clone(),
            endpoints: workflow.service_endpoints.clone(),
        })
    }

    /// Step 1: Check account balance.
    async fn step_check_balance(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<()> {
        workflow.current_step = AkashWorkflowStep::BalanceCheck as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("Checking balance for {}", workflow.account_address);

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

        tracing::info!("Account balance: {} uAKT (required: {})", amount, min_required);

        if amount < min_required {
            return Err(anyhow!(
                "Insufficient balance: {} uAKT (need at least {} uAKT)",
                amount,
                min_required
            ));
        }

        Ok(())
    }

    /// Step 2: Setup certificate.
    async fn step_setup_certificate(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        workflow.current_step = AkashWorkflowStep::CertificateSetup as i32;
        self.save_workflow(workflow).await?;

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
        self.save_workflow(workflow).await?;

        tracing::info!("Certificate ready");
        Ok(())
    }

    /// Step 3: Create deployment transaction.
    async fn step_create_deployment(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<u64> {
        workflow.current_step = AkashWorkflowStep::DeploymentCreate as i32;
        self.save_workflow(workflow).await?;

        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;

        tracing::info!("Creating deployment from template '{}'", sdl.template_name);

        // Get next available dseq
        let dseq = get_next_dseq(self.cosmos.rest_endpoint(), &workflow.account_address).await?;

        tracing::info!("Using dseq: {}", dseq);

        // Build MsgCreateDeployment
        // Use min_balance as a reasonable deposit (or default if not set)
        let deposit = if opts.min_balance_uakt > 0 {
            opts.min_balance_uakt
        } else {
            DEFAULT_DEPOSIT_UAKT
        };

        let builder = DeploymentBuilder::new(&workflow.account_address, dseq)
            .with_deposit(deposit);

        let msg = builder.build_from_sdl(&sdl.resolved_content)?;

        // Sign and broadcast
        let msg_any = msg_to_any(&msg, msg_types::MSG_CREATE_DEPLOYMENT);

        let result = self
            .tx_lifecycle
            .sign_broadcast_wait(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                msg_any,
                DEFAULT_GAS_LIMIT,
                DEFAULT_GAS_PRICE,
                Some("ergors automated deployment"),
            )
            .await?;

        if !result.is_success() {
            return Err(anyhow!(
                "Deployment creation failed (code {}): {}",
                result.code,
                result.raw_log
            ));
        }

        tracing::info!(
            "Deployment created: tx_hash={}, height={}",
            result.hash,
            result.height
        );

        // Extract dseq from events (verify it matches)
        let event_dseq = result.extract_dseq().unwrap_or(dseq);
        if event_dseq != dseq {
            tracing::warn!(
                "Dseq mismatch: expected {}, got {} from events",
                dseq,
                event_dseq
            );
        }

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
        Ok(dseq)
    }

    /// Step 4: Wait for bids.
    async fn step_wait_for_bids(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        dseq: u64,
        opts: &AkashWorkflowOptions,
    ) -> Result<Vec<BidInfo>> {
        workflow.current_step = AkashWorkflowStep::BidWait as i32;
        self.save_workflow(workflow).await?;

        let wait_blocks = if opts.bid_wait_blocks > 0 {
            opts.bid_wait_blocks
        } else {
            DEFAULT_BID_WAIT_BLOCKS
        };

        tracing::info!(
            "Waiting for bids (~{}s for {} blocks)...",
            wait_blocks * 6,
            wait_blocks
        );

        // Initial wait for bids to arrive
        tokio::time::sleep(Duration::from_secs(wait_blocks as u64 * 6)).await;

        // Poll for bids
        let mut bids = Vec::new();
        let mut attempts = 0;

        while attempts < MAX_BID_POLL_ATTEMPTS {
            attempts += 1;

            let query_result = self
                .cosmos
                .query_open_bids(&workflow.account_address, dseq)
                .await;

            match query_result {
                Ok(found_bids) => {
                    if !found_bids.is_empty() {
                        tracing::info!(
                            "Found {} open bids after {} attempts",
                            found_bids.len(),
                            attempts
                        );

                        // Log bids
                        for bid in &found_bids {
                            tracing::info!(
                                "  Bid from {}: {} {}",
                                bid.provider,
                                bid.price_amount,
                                bid.price_denom
                            );
                        }

                        bids = found_bids;
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Bid query attempt {} failed: {}", attempts, e);
                }
            }

            if attempts < MAX_BID_POLL_ATTEMPTS {
                tracing::info!(
                    "No bids yet (attempt {}/{}), waiting...",
                    attempts,
                    MAX_BID_POLL_ATTEMPTS
                );
                tokio::time::sleep(Duration::from_secs(6)).await; // Wait one more block
            }
        }

        if bids.is_empty() {
            return Err(anyhow!(
                "No bids received after {} attempts. Check provider availability.",
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
        Ok(bids)
    }

    /// Step 5: Select provider.
    async fn step_select_provider(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        bids: &[BidInfo],
        opts: &AkashWorkflowOptions,
    ) -> Result<BidInfo> {
        workflow.current_step = AkashWorkflowStep::ProviderSelection as i32;
        self.save_workflow(workflow).await?;

        // Filter by trusted providers if specified
        let candidates: Vec<_> = if opts.trusted_providers.is_empty() {
            bids.to_vec()
        } else {
            bids.iter()
                .filter(|b| opts.trusted_providers.contains(&b.provider))
                .cloned()
                .collect()
        };

        if candidates.is_empty() {
            return Err(anyhow!(
                "No bids from trusted providers. Available providers: {:?}",
                bids.iter().map(|b| &b.provider).collect::<Vec<_>>()
            ));
        }

        // Select cheapest bid
        let selected = candidates
            .iter()
            .min_by(|a, b| {
                let price_a: u64 = a.price_amount.parse().unwrap_or(u64::MAX);
                let price_b: u64 = b.price_amount.parse().unwrap_or(u64::MAX);
                price_a.cmp(&price_b)
            })
            .cloned()
            .ok_or_else(|| anyhow!("Failed to select bid"))?;

        tracing::info!(
            "Selected provider {} with price {} {}",
            selected.provider,
            selected.price_amount,
            selected.price_denom
        );

        // Store selection
        let is_trusted = opts.trusted_providers.contains(&selected.provider);
        let price: u64 = selected.price_amount.parse().unwrap_or(0);

        workflow.provider = Some(AkashProviderSelection {
            provider_address: selected.provider.clone(),
            reputation_score: 100, // Would query reputation system in production
            bid_price_uakt: price,
            total_bids_received: bids.len() as u32,
            selected_at: Some(current_timestamp()),
            is_trusted_provider: is_trusted,
        });

        self.save_workflow(workflow).await?;
        Ok(selected)
    }

    /// Step 6: Create lease.
    async fn step_create_lease(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        bid: &BidInfo,
    ) -> Result<()> {
        workflow.current_step = AkashWorkflowStep::LeaseCreate as i32;
        self.save_workflow(workflow).await?;

        tracing::info!("Creating lease with provider {}", bid.provider);

        // Build MsgCreateLease
        let msg = build_create_lease_msg(&bid.owner, bid.dseq, bid.gseq, bid.oseq, &bid.provider);

        let msg_any = msg_to_any(&msg, msg_types::MSG_CREATE_LEASE);

        let result = self
            .tx_lifecycle
            .sign_broadcast_wait(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                msg_any,
                DEFAULT_GAS_LIMIT,
                DEFAULT_GAS_PRICE,
                Some("ergors lease creation"),
            )
            .await?;

        if !result.is_success() {
            return Err(anyhow!(
                "Lease creation failed (code {}): {}",
                result.code,
                result.raw_log
            ));
        }

        tracing::info!(
            "Lease created: tx_hash={}, height={}",
            result.hash,
            result.height
        );

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
            runtime.lease_id = format!("{}/{}/{}/{}/{}", bid.owner, bid.dseq, bid.gseq, bid.oseq, bid.provider);
        }

        self.save_workflow(workflow).await?;
        Ok(())
    }

    /// Step 7: Send manifest.
    async fn step_send_manifest(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
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

        tracing::info!("Sending manifest to provider at {}", provider_uri);

        let sender = ManifestSender::new(&provider_uri);
        sender
            .send_manifest_from_sdl(
                &lease_info.owner,
                lease_info.dseq,
                lease_info.gseq,
                lease_info.oseq,
                &sdl.resolved_content,
            )
            .await?;

        tracing::info!("Manifest sent successfully");
        self.save_workflow(workflow).await?;
        Ok(())
    }

    /// Step 8: Retrieve endpoints.
    async fn step_retrieve_endpoints(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<Vec<AkashServiceEndpoint>> {
        workflow.current_step = AkashWorkflowStep::EndpointRetrieval as i32;
        self.save_workflow(workflow).await?;

        let lease_info = workflow
            .lease_id_info
            .as_ref()
            .ok_or_else(|| anyhow!("No lease info"))?;

        let provider_uri = format!("https://{}:8443", lease_info.provider);

        tracing::info!("Retrieving endpoints from provider at {}", provider_uri);

        // Wait a bit for services to start
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Poll for endpoints with retries
        let mut endpoints = HashMap::new();
        let mut attempts = 0;
        const MAX_ENDPOINT_ATTEMPTS: u32 = 5;

        while attempts < MAX_ENDPOINT_ATTEMPTS {
            attempts += 1;

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
                    for (name, ep) in eps {
                        tracing::info!("Discovered endpoint: {} -> {}", name, ep.external_uri);
                        endpoints.insert(name, ep);
                    }
                    break;
                }
                Ok(_) => {
                    tracing::info!(
                        "No endpoints yet (attempt {}/{}), waiting...",
                        attempts,
                        MAX_ENDPOINT_ATTEMPTS
                    );
                }
                Err(e) => {
                    tracing::warn!("Endpoint query failed (attempt {}): {}", attempts, e);
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
            tracing::warn!("No endpoints discovered - service may still be starting");
        }

        Ok(endpoint_infos)
    }

    /// Step 9: Save endpoints.
    async fn step_save_endpoints(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        endpoints: Vec<AkashServiceEndpoint>,
    ) -> Result<()> {
        // Store in workflow
        workflow.service_endpoints = endpoints.clone();

        // Also store in legacy endpoints map for backwards compatibility
        for ep in &endpoints {
            workflow.endpoints.insert(ep.service_name.clone(), ep.external_uri.clone());
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

            tracing::info!("Saved {} endpoints to storage key: {}", endpoints.len(), storage_key);
        }

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
    pub async fn close_deployment(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<()> {
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
        let msg_any = msg_to_any(&msg, msg_types::MSG_CLOSE_DEPLOYMENT);

        let result = self
            .tx_lifecycle
            .sign_broadcast_wait(
                &workflow.selected_key_name,
                workflow.hd_account_index,
                msg_any,
                DEFAULT_GAS_LIMIT,
                DEFAULT_GAS_PRICE,
                Some("ergors close deployment"),
            )
            .await?;

        if !result.is_success() {
            return Err(anyhow!(
                "Close deployment failed (code {}): {}",
                result.code,
                result.raw_log
            ));
        }

        tracing::info!("Deployment closed: tx_hash={}", result.hash);
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
