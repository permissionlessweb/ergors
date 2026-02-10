//! Deployment cache refresh with lease validation and auto top-up.
//!
//! This module handles:
//! - Refreshing deployment cache from storage
//! - Verifying leases are still active on chain
//! - Auto-topping up deployments when escrow balance falls below threshold

use anyhow::Result;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::llm::deployment_cache::{DeploymentEndpoint, DeploymentProviderCache, ServiceEndpoint};
use ho_std::types::ergors::orch::v1::{
    AkashDeployConfig, AkashDeploymentWorkflow, AkashWorkflowStatus, CosmosKeyStore,
};
use prost::Name;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::messages::broadcast_akash_msg;
use super::types::LeaseState;
use super::client::AkashClient;
use super::deployment_builder::build_escrow_deposit_msg;
use crate::deploy::climb_signer::create_signing_client_with_failover;
use crate::storage::ErgorsStorage;

/// Default escrow balance threshold for auto top-up (20% of initial deposit).
/// When balance falls below this percentage, trigger a deposit.
pub const DEFAULT_TOP_UP_THRESHOLD_PERCENT: u64 = 20;

/// Default top-up amount in uakt (5 AKT).
pub const DEFAULT_TOP_UP_AMOUNT_UAKT: u64 = 5_000_000;

/// Result of a cache refresh operation.
#[derive(Debug, Default)]
pub struct RefreshResult {
    /// Number of active deployments loaded into cache
    pub active_count: usize,
    /// Number of workflows checked
    pub workflows_checked: usize,
    /// Number of workflows with inactive/closed leases (removed from cache)
    pub inactive_count: usize,
    /// Number of deployments with low escrow balance
    pub low_balance_count: usize,
    /// Deployments that were auto-topped up
    pub topped_up: Vec<String>,
    /// Errors encountered during refresh
    pub errors: Vec<String>,
}

/// Deployment cache refresher with chain verification.
pub struct DeploymentCacheRefresher {
    storage: Arc<ErgorsStorage>,
    cosmos: Arc<AkashClient>,
    cache: Arc<DeploymentProviderCache>,
    /// Threshold percentage for auto top-up (0-100)
    top_up_threshold_percent: u64,
    /// Amount to deposit when topping up
    top_up_amount_uakt: u64,
    /// Whether auto top-up is enabled
    auto_top_up_enabled: bool,
    /// Key manager for signing top-up transactions (required if auto_top_up_enabled)
    key_manager: Option<Arc<RwLock<EncryptedCosmosKeyManager>>>,
    /// Key store for signing top-up transactions
    key_store: Option<Arc<RwLock<CosmosKeyStore>>>,
    /// Akash config for chain info
    akash_config: Option<AkashDeployConfig>,
}

impl DeploymentCacheRefresher {
    /// Create a new cache refresher.
    pub fn new(
        storage: Arc<ErgorsStorage>,
        cosmos: Arc<AkashClient>,
        cache: Arc<DeploymentProviderCache>,
    ) -> Self {
        Self {
            storage,
            cosmos,
            cache,
            top_up_threshold_percent: DEFAULT_TOP_UP_THRESHOLD_PERCENT,
            top_up_amount_uakt: DEFAULT_TOP_UP_AMOUNT_UAKT,
            auto_top_up_enabled: false, // Disabled by default for safety
            key_manager: None,
            key_store: None,
            akash_config: None,
        }
    }

    /// Enable auto top-up with custom settings.
    pub fn with_auto_top_up(
        mut self,
        enabled: bool,
        threshold_percent: Option<u64>,
        amount_uakt: Option<u64>,
    ) -> Self {
        self.auto_top_up_enabled = enabled;
        if let Some(t) = threshold_percent {
            self.top_up_threshold_percent = t.min(100);
        }
        if let Some(a) = amount_uakt {
            self.top_up_amount_uakt = a;
        }
        self
    }

    /// Configure signing components for auto top-up transactions.
    /// Required if auto_top_up_enabled is true.
    pub fn with_signing_components(
        mut self,
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        akash_config: AkashDeployConfig,
    ) -> Self {
        self.key_manager = Some(key_manager);
        self.key_store = Some(key_store);
        self.akash_config = Some(akash_config);
        self
    }

    /// Refresh the deployment cache with chain verification.
    ///
    /// This method:
    /// 1. Lists all workflows from storage
    /// 2. Filters to completed deployments with labels
    /// 3. Verifies lease is still active on chain
    /// 4. Checks escrow balance and optionally triggers top-up
    /// 5. Updates the cache with verified active deployments
    pub async fn refresh(&self) -> RefreshResult {
        let mut result = RefreshResult::default();

        // Get all workflows from storage
        let all_workflows = match self.storage.list_akash_workflows().await {
            Ok(w) => w,
            Err(e) => {
                result.errors.push(format!("Failed to list workflows: {}", e));
                return result;
            }
        };

        // Filter to only completed workflows with labels (candidates for cache refresh)
        let workflows: Vec<_> = all_workflows
            .into_iter()
            .filter(|w| {
                !w.label.is_empty() && w.status == AkashWorkflowStatus::Completed as i32
            })
            .collect();

        result.workflows_checked = workflows.len();
        if !workflows.is_empty() {
            tracing::debug!("Checking {} active workflows for cache refresh", workflows.len());
        }

        // Process each completed workflow with a label
        for workflow in workflows {
            // Skip workflows without endpoints
            // (already filtered for label and completed status above)

            // Skip workflows without endpoints
            if workflow.service_endpoints.is_empty() {
                continue;
            }

            // Verify lease is still active on chain
            match self.verify_lease_active(&workflow).await {
                Ok(true) => {
                    // Lease is active, add to cache
                    if let Err(e) = self.add_to_cache(&workflow).await {
                        result.errors.push(format!(
                            "Failed to add {} to cache: {}",
                            workflow.label, e
                        ));
                    } else {
                        result.active_count += 1;

                        // Check escrow balance
                        if let Err(e) = self.check_and_top_up(&workflow, &mut result).await {
                            result.errors.push(format!(
                                "Escrow check failed for {}: {}",
                                workflow.label, e
                            ));
                        }
                    }
                }
                Ok(false) => {
                    // Lease is not active, remove from cache if present
                    result.inactive_count += 1;
                    if let Err(e) = self.cache.remove_deployment(&workflow.label).await {
                        tracing::warn!(
                            "Failed to remove inactive deployment {} from cache: {}",
                            workflow.label,
                            e
                        );
                    }
                    tracing::info!(
                        "Deployment '{}' lease is no longer active, removed from cache",
                        workflow.label
                    );
                }
                Err(e) => {
                    result.errors.push(format!(
                        "Failed to verify lease for {}: {}",
                        workflow.label, e
                    ));
                }
            }
        }

        if result.active_count > 0 || result.inactive_count > 0 {
            tracing::info!(
                "Cache refresh: {} active, {} inactive, {} low balance",
                result.active_count,
                result.inactive_count,
                result.low_balance_count
            );
        }

        result
    }

    /// Verify that the workflow's lease is still active on chain.
    async fn verify_lease_active(&self, workflow: &AkashDeploymentWorkflow) -> Result<bool> {
        // Extract lease info from workflow
        let lease_id = workflow.lease_id_info.as_ref();

        // Need lease_id info to query
        let (dseq, gseq, oseq, provider) = match lease_id {
            Some(lid) => {
                // AkashLeaseIdInfo has typed fields (u64, u32, u32, String)
                (lid.dseq, lid.gseq, lid.oseq, &lid.provider)
            }
            None => {
                // Can't verify without lease info, assume active
                tracing::debug!(
                    "Workflow {} missing lease info, assuming active",
                    workflow.session_id
                );
                return Ok(true);
            }
        };

        if dseq == 0 || provider.is_empty() {
            return Ok(true); // Can't query, assume active
        }

        // Query lease status from chain
        let lease = self
            .cosmos
            .query_lease(&workflow.account_address, dseq, gseq, oseq, provider)
            .await?;

        match lease.state {
            LeaseState::Active => Ok(true),
            LeaseState::InsufficientFunds => {
                tracing::warn!(
                    "Deployment '{}' has insufficient funds in escrow",
                    workflow.label
                );
                Ok(true) // Still technically active, but needs attention
            }
            LeaseState::Closed | LeaseState::Invalid => Ok(false),
        }
    }

    /// Add a verified workflow to the cache.
    async fn add_to_cache(&self, workflow: &AkashDeploymentWorkflow) -> Result<()> {
        let endpoints: Vec<ServiceEndpoint> = workflow
            .service_endpoints
            .iter()
            .map(|ep| ServiceEndpoint {
                service_name: ep.service_name.clone(),
                external_uri: ep.external_uri.clone(),
                port: ep.external_port as u16,
            })
            .collect();

        let dseq = workflow
            .deployment
            .as_ref()
            .and_then(|d| d.deployment_sequence.parse::<u64>().ok())
            .unwrap_or(0);

        // Resolve model_name: prefer workflow-level, then first endpoint's model_name
        let model_name = if !workflow.model_name.is_empty() {
            workflow.model_name.clone()
        } else {
            workflow
                .service_endpoints
                .first()
                .map(|ep| ep.model_name.clone())
                .unwrap_or_default()
        };

        let endpoint = DeploymentEndpoint {
            session_id: workflow.session_id.clone(),
            label: workflow.label.clone(),
            model_name,
            endpoints,
            owner: workflow.account_address.clone(),
            dseq,
        };

        let mut cache = self.cache.cache.write().await;
        cache.insert(workflow.label.clone(), endpoint);

        Ok(())
    }

    /// Check escrow balance and optionally trigger top-up.
    async fn check_and_top_up(
        &self,
        workflow: &AkashDeploymentWorkflow,
        result: &mut RefreshResult,
    ) -> Result<()> {
        let dseq = workflow
            .deployment
            .as_ref()
            .and_then(|d| d.deployment_sequence.parse::<u64>().ok())
            .unwrap_or(0);

        if dseq == 0 {
            return Ok(());
        }

        // Query escrow account
        let escrow = self
            .cosmos
            .query_deployment_escrow(&workflow.account_address, dseq)
            .await?;

        if let Some(escrow_info) = escrow {
            // Use default deposit amount as baseline (5 AKT)
            // In the future, we could track initial deposits per deployment
            let initial_deposit = super::deployment_builder::DEFAULT_DEPOSIT_UAKT;

            // Calculate threshold
            let threshold = (initial_deposit * self.top_up_threshold_percent) / 100;

            if escrow_info.total_uakt < threshold {
                result.low_balance_count += 1;
                tracing::warn!(
                    "Deployment '{}' escrow balance low: {} uakt (threshold: {} uakt)",
                    workflow.label,
                    escrow_info.total_uakt,
                    threshold
                );

                if self.auto_top_up_enabled {
                    match self.execute_top_up(workflow, dseq).await {
                        Ok(tx_hash) => {
                            tracing::info!(
                                "Auto topped up deployment '{}' with {} uakt (tx: {})",
                                workflow.label,
                                self.top_up_amount_uakt,
                                tx_hash
                            );
                            result.topped_up.push(workflow.label.clone());
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to auto top-up deployment '{}': {}",
                                workflow.label,
                                e
                            );
                            result.errors.push(format!(
                                "Top-up failed for {}: {}",
                                workflow.label, e
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute the actual top-up transaction.
    async fn execute_top_up(
        &self,
        workflow: &AkashDeploymentWorkflow,
        dseq: u64,
    ) -> Result<String> {
        // Verify we have signing components
        let key_manager = self.key_manager.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Key manager not configured for auto top-up")
        })?;
        let key_store = self.key_store.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Key store not configured for auto top-up")
        })?;
        let akash_config = self.akash_config.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Akash config not configured for auto top-up")
        })?;

        // Build the deposit message
        let msg = build_escrow_deposit_msg(
            &workflow.account_address, // signer = owner
            &workflow.account_address, // owner
            dseq,
            self.top_up_amount_uakt,
        )?;

        // Create signing client
        let client = create_signing_client_with_failover(
            key_manager.clone(),
            key_store.clone(),
            &workflow.selected_key_name,
            workflow.hd_account_index,
            akash_config,
        )
        .await?;

        // Broadcast the transaction
        let type_url = ho_std::types::ergors::akash::escrow::v1::MsgAccountDeposit::type_url();
        let tx_resp = broadcast_akash_msg(
            &client,
            &type_url,
            &msg,
            "ergors auto top-up",
        )
        .await?;

        Ok(tx_resp.txhash)
    }
}

/// Refresh deployment cache for a specific owner address.
///
/// This is a simpler version that queries active leases for an owner
/// without requiring individual workflow lookup.
pub async fn refresh_cache_for_owner(
    storage: &Arc<ErgorsStorage>,
    cosmos: &Arc<AkashClient>,
    cache: &DeploymentProviderCache,
    owner: &str,
) -> Result<usize> {
    // Query all active leases for this owner from chain
    let active_leases = cosmos.query_active_leases(owner).await?;
    let active_dseqs: std::collections::HashSet<u64> =
        active_leases.iter().map(|l| l.dseq).collect();

    tracing::debug!(
        "Found {} active leases for owner {}",
        active_dseqs.len(),
        owner
    );

    // Get all workflows from storage
    let workflows = storage.list_akash_workflows().await?;

    let mut loaded = 0;

    for workflow in workflows {
        // Skip workflows not owned by this address
        if workflow.account_address != owner {
            continue;
        }

        // Skip workflows without labels
        if workflow.label.is_empty() {
            continue;
        }

        // Skip non-completed workflows
        if workflow.status != AkashWorkflowStatus::Completed as i32 {
            continue;
        }

        // Get dseq from workflow
        let dseq = workflow
            .deployment
            .as_ref()
            .and_then(|d| d.deployment_sequence.parse::<u64>().ok())
            .unwrap_or(0);

        // Check if this deployment has an active lease
        if !active_dseqs.contains(&dseq) {
            // Lease not active, remove from cache
            let _ = cache.remove_deployment(&workflow.label).await;
            continue;
        }

        // Add to cache
        if cache.add_deployment(&workflow).await.is_ok() {
            loaded += 1;
        }
    }

    Ok(loaded)
}

#[cfg(test)]
mod tests {
    

    #[test]
    fn test_threshold_calculation() {
        // 20% of 5_000_000 = 1_000_000
        let initial = 5_000_000u64;
        let threshold_percent = 20u64;
        let threshold = (initial * threshold_percent) / 100;
        assert_eq!(threshold, 1_000_000);
    }
}
