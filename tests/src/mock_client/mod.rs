//! Mock Client for ERGORS Engine Testing
//!
//! Provides mock implementations of the ManagementService client and supporting
//! infrastructure for testing Akash deployment workflows without real gRPC or
//! blockchain infrastructure.
//!
//! # Components
//!
//! - [`MockCosmosChain`]: Simulates blockchain state (balances, authz, feegrants)
//! - [`MockStorage`]: In-memory storage for sessions and workflows
//! - [`TestBackend`]: Implements `AkashBackend` trait backed by MockCosmosChain
//! - [`MockWorkflowEngine`]: Wraps real `DeploymentWorkflow<TestBackend>`
//! - [`MockManagementClient`]: Composes all components into a testable client
//!
//! # Example
//!
//! ```rust,ignore
//! use ergors_tests::mock_client::*;
//!
//! #[tokio::test]
//! async fn test_deployment_workflow() {
//!     let mut client = MockManagementClient::new();
//!
//!     // Fund accounts
//!     client.chain_mut().fund_account("akash1coord...", 1_000_000_000);
//!     client.chain_mut().fund_account("akash1exec...", 100_000);
//!
//!     // Create workflow
//!     let workflow = client.create_akash_deployment("test-session", "sdl...", None).await.unwrap();
//!     assert_eq!(workflow.current_step, AkashWorkflowStep::KeySelection as i32);
//! }
//! ```

mod chain;
mod storage;
pub mod test_backend;
mod types;
mod workflow;

pub use chain::MockCosmosChain;
pub use storage::MockStorage;
pub use test_backend::TestBackend;
pub use types::*;
pub use workflow::MockWorkflowEngine;

use anyhow::{anyhow, Result};
use ho_std::types::ergors::management::v1::FractalSession;
use ho_std::types::ergors::network::v1::{NodeIdentity, NodeType};
use ho_std::types::ergors::orch::v1::{AkashDeploymentWorkflow, AkashWorkflowStatus};
use ho_std::utils::IdGenerator;
use std::sync::{Arc, RwLock};

/// Mock implementation of ManagementServiceClient for testing.
///
/// Does not use gRPC - all operations are in-memory. Provides controllable
/// behavior for testing edge cases and failure scenarios.
pub struct MockManagementClient {
    chain: Arc<RwLock<MockCosmosChain>>,
    storage: MockStorage,
    workflow_engine: MockWorkflowEngine,
    node_identity: NodeIdentity,
    node_type: NodeType,

    // Test control knobs
    simulated_latency_ms: u64,
    error_injection: Option<String>,
}

impl Default for MockManagementClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockManagementClient {
    /// Create a new mock client with default configuration.
    pub fn new() -> Self {
        let chain = Arc::new(RwLock::new(MockCosmosChain::new()));
        Self {
            chain: Arc::clone(&chain),
            storage: MockStorage::new(),
            workflow_engine: MockWorkflowEngine::new(chain),
            node_identity: NodeIdentity {
                host: "127.0.0.1".to_string(),
                p2p_port: 26656,
                api_port: 50051,
                user: "mock".to_string(),
                os: 0,
                ssh_port: 22,
                node_type: "development".to_string(),
                public_key: Some(vec![0u8; 32]),
                bech32_address: None,
            },
            node_type: NodeType::Development,
            simulated_latency_ms: 0,
            error_injection: None,
        }
    }

    /// Create client configured as coordinator.
    pub fn as_coordinator() -> Self {
        let mut client = Self::new();
        client.node_type = NodeType::Coordinator;
        client.node_identity.node_type = "coordinator".to_string();
        client
    }

    /// Create client configured as executor.
    pub fn as_executor() -> Self {
        let mut client = Self::new();
        client.node_type = NodeType::Executor;
        client.node_identity.node_type = "executor".to_string();
        client
    }

    /// Get mutable access to chain for test setup.
    ///
    /// Returns a write guard that derefs to `&mut MockCosmosChain`.
    /// The guard is dropped at the end of the expression, releasing the lock.
    pub fn chain_mut(&self) -> std::sync::RwLockWriteGuard<'_, MockCosmosChain> {
        self.chain.write().unwrap()
    }

    /// Get read access to chain for assertions.
    ///
    /// Returns a read guard that derefs to `&MockCosmosChain`.
    pub fn chain(&self) -> std::sync::RwLockReadGuard<'_, MockCosmosChain> {
        self.chain.read().unwrap()
    }

    /// Get mutable reference to storage.
    pub fn storage_mut(&mut self) -> &mut MockStorage {
        &mut self.storage
    }

    /// Get reference to storage for assertions.
    pub fn storage(&self) -> &MockStorage {
        &self.storage
    }

    /// Get reference to workflow engine for assertions.
    pub fn workflow_engine(&self) -> &MockWorkflowEngine {
        &self.workflow_engine
    }

    /// Get mutable reference to workflow engine for configuration.
    pub fn workflow_engine_mut(&mut self) -> &mut MockWorkflowEngine {
        &mut self.workflow_engine
    }

    /// Inject an error to be returned on next operation.
    pub fn inject_error(&mut self, error: impl Into<String>) {
        self.error_injection = Some(error.into());
    }

    /// Clear any injected error.
    pub fn clear_error(&mut self) {
        self.error_injection = None;
    }

    /// Set simulated latency for operations.
    pub fn set_latency(&mut self, ms: u64) {
        self.simulated_latency_ms = ms;
    }

    // Check for injected error and simulate latency
    async fn check_injection(&mut self) -> Result<()> {
        if self.simulated_latency_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.simulated_latency_ms)).await;
        }
        if let Some(err) = self.error_injection.take() {
            return Err(anyhow!("{}", err));
        }
        Ok(())
    }

    // =========================================================================
    // ManagementService RPC implementations
    // =========================================================================

    /// Get node identity.
    pub async fn get_node_identity(&mut self) -> Result<NodeIdentity> {
        self.check_injection().await?;
        Ok(self.node_identity.clone())
    }

    /// Create a new Akash deployment workflow.
    pub async fn create_akash_deployment(
        &mut self,
        session_id: impl Into<String>,
        sdl_content: impl Into<String>,
        key_name: Option<String>,
    ) -> Result<AkashDeploymentWorkflow> {
        self.check_injection().await?;

        let session_id = session_id.into();
        let sdl = sdl_content.into();

        // Create workflow in engine
        let workflow = self
            .workflow_engine
            .create_workflow(session_id.clone(), sdl, key_name);

        // Store it
        self.storage.put_workflow(workflow.clone());

        Ok(workflow)
    }

    /// Advance workflow to next step.
    pub async fn advance_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        self.check_injection().await?;

        let workflow = self
            .storage
            .get_workflow(session_id)
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?
            .clone();

        // Execute the step via the real workflow engine
        let updated = self
            .workflow_engine
            .advance_workflow(workflow)
            .await?;

        // Update storage
        self.storage.put_workflow(updated.clone());

        Ok(updated)
    }

    /// Get workflow by session ID.
    pub async fn get_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        self.check_injection().await?;

        self.storage
            .get_workflow(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))
    }

    /// List all workflows with optional status filter.
    pub async fn list_akash_deployments(
        &mut self,
        status_filter: Option<AkashWorkflowStatus>,
    ) -> Result<Vec<AkashDeploymentWorkflow>> {
        self.check_injection().await?;

        let workflows = self.storage.list_workflows();
        match status_filter {
            Some(status) => Ok(workflows
                .into_iter()
                .filter(|w| w.status == status as i32)
                .collect()),
            None => Ok(workflows),
        }
    }

    /// Cancel a workflow.
    pub async fn cancel_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<AkashDeploymentWorkflow> {
        self.check_injection().await?;

        let mut workflow = self
            .storage
            .get_workflow(session_id)
            .ok_or_else(|| anyhow!("Workflow not found: {}", session_id))?
            .clone();

        workflow.status = AkashWorkflowStatus::Cancelled as i32;
        self.storage.put_workflow(workflow.clone());

        Ok(workflow)
    }

    /// Create a new session.
    pub async fn create_session(&mut self, name: impl Into<String>) -> Result<FractalSession> {
        self.check_injection().await?;

        let session = self.storage.create_session(name.into());
        Ok(session)
    }

    /// Get session by ID.
    pub async fn get_session(&mut self, session_id: &str) -> Result<FractalSession> {
        self.check_injection().await?;

        self.storage
            .get_session(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("Session not found: {}", session_id))
    }

    /// Request authz/feegrant from a granter.
    pub async fn request_grant(
        &mut self,
        granter_address: &str,
        grantee_address: &str,
        msg_types: Vec<String>,
        spend_limit_uakt: u64,
        duration_seconds: u64,
    ) -> Result<GrantRequestState> {
        self.check_injection().await?;

        let request = GrantRequestState {
            request_id: IdGenerator::new_uuid_string(),
            granter_address: granter_address.to_string(),
            grantee_address: grantee_address.to_string(),
            msg_types,
            spend_limit_uakt,
            duration_seconds,
            status: GrantRequestStatus::Pending,
            created_at_unix: chrono::Utc::now().timestamp() as u64,
            rejection_reason: None,
        };

        self.storage.add_grant_request(request.clone());
        Ok(request)
    }

    /// Approve a pending grant request.
    pub async fn approve_grant(&mut self, request_id: &str) -> Result<GrantRequestState> {
        self.check_injection().await?;

        let mut request = self
            .storage
            .get_grant_request(request_id)
            .ok_or_else(|| anyhow!("Grant request not found: {}", request_id))?
            .clone();

        if request.status != GrantRequestStatus::Pending {
            return Err(anyhow!(
                "Grant request not pending: {:?}",
                request.status
            ));
        }

        // Create the grants on chain
        {
            let mut chain = self.chain.write().unwrap();
            for msg_type in &request.msg_types {
                chain.grant_authz(
                    &request.granter_address,
                    &request.grantee_address,
                    msg_type,
                    request.duration_seconds,
                )?;
            }

            if request.spend_limit_uakt > 0 {
                chain.create_feegrant(
                    &request.granter_address,
                    &request.grantee_address,
                    request.spend_limit_uakt,
                    request.duration_seconds,
                )?;
            }
        }

        request.status = GrantRequestStatus::Confirmed;
        self.storage.update_grant_request(request.clone());

        Ok(request)
    }

    /// Reject a pending grant request.
    pub async fn reject_grant(
        &mut self,
        request_id: &str,
        reason: impl Into<String>,
    ) -> Result<GrantRequestState> {
        self.check_injection().await?;

        let mut request = self
            .storage
            .get_grant_request(request_id)
            .ok_or_else(|| anyhow!("Grant request not found: {}", request_id))?
            .clone();

        if request.status != GrantRequestStatus::Pending {
            return Err(anyhow!(
                "Grant request not pending: {:?}",
                request.status
            ));
        }

        request.status = GrantRequestStatus::Rejected;
        request.rejection_reason = Some(reason.into());
        self.storage.update_grant_request(request.clone());

        Ok(request)
    }

    /// Query balance for an address.
    pub async fn query_balance(&mut self, address: &str, denom: &str) -> Result<u64> {
        self.check_injection().await?;
        Ok(self.chain.read().unwrap().get_balance(address, denom))
    }
}

/// Mock network of coordinator and executor nodes for grant workflow testing.
pub struct MockNodeNetwork {
    coordinator: MockManagementClient,
    executors: Vec<MockManagementClient>,
    grant_acceptance_mode: GrantAcceptanceMode,
    whitelisted_pubkeys: Vec<Vec<u8>>,
}

impl Default for MockNodeNetwork {
    fn default() -> Self {
        Self::new(1)
    }
}

impl MockNodeNetwork {
    /// Create a network with 1 coordinator and N executors.
    pub fn new(executor_count: usize) -> Self {
        let coordinator = MockManagementClient::as_coordinator();
        let executors = (0..executor_count)
            .map(|_| MockManagementClient::as_executor())
            .collect();

        Self {
            coordinator,
            executors,
            grant_acceptance_mode: GrantAcceptanceMode::Manual,
            whitelisted_pubkeys: Vec::new(),
        }
    }

    /// Get mutable reference to coordinator.
    pub fn coordinator_mut(&mut self) -> &mut MockManagementClient {
        &mut self.coordinator
    }

    /// Get reference to coordinator.
    pub fn coordinator(&self) -> &MockManagementClient {
        &self.coordinator
    }

    /// Get mutable reference to executor by index.
    pub fn executor_mut(&mut self, index: usize) -> Option<&mut MockManagementClient> {
        self.executors.get_mut(index)
    }

    /// Get reference to executor by index.
    pub fn executor(&self, index: usize) -> Option<&MockManagementClient> {
        self.executors.get(index)
    }

    /// Set grant acceptance mode.
    pub fn set_grant_acceptance_mode(&mut self, mode: GrantAcceptanceMode) {
        self.grant_acceptance_mode = mode;
    }

    /// Add a pubkey to whitelist.
    pub fn whitelist_pubkey(&mut self, pubkey: Vec<u8>) {
        self.whitelisted_pubkeys.push(pubkey);
    }

    /// Process pending grant requests according to acceptance mode.
    pub async fn process_grants(&mut self) -> Result<Vec<GrantRequestState>> {
        let pending: Vec<_> = self
            .coordinator
            .storage()
            .list_grant_requests()
            .into_iter()
            .filter(|r| r.status == GrantRequestStatus::Pending)
            .collect();

        let mut processed = Vec::new();

        for request in pending {
            let result = match self.grant_acceptance_mode {
                GrantAcceptanceMode::AcceptAll => {
                    self.coordinator.approve_grant(&request.request_id).await
                }
                GrantAcceptanceMode::RejectAll => {
                    self.coordinator
                        .reject_grant(&request.request_id, "Auto-reject mode")
                        .await
                }
                GrantAcceptanceMode::Whitelist => {
                    if !self.whitelisted_pubkeys.is_empty() {
                        self.coordinator.approve_grant(&request.request_id).await
                    } else {
                        self.coordinator
                            .reject_grant(&request.request_id, "Not whitelisted")
                            .await
                    }
                }
                GrantAcceptanceMode::Manual => {
                    continue;
                }
            };

            if let Ok(updated) = result {
                processed.push(updated);
            }
        }

        Ok(processed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_client_creation() {
        let client = MockManagementClient::new();
        assert_eq!(client.node_type, NodeType::Development);
    }

    #[tokio::test]
    async fn test_coordinator_client() {
        let client = MockManagementClient::as_coordinator();
        assert_eq!(client.node_type, NodeType::Coordinator);
    }

    #[tokio::test]
    async fn test_executor_client() {
        let client = MockManagementClient::as_executor();
        assert_eq!(client.node_type, NodeType::Executor);
    }

    #[tokio::test]
    async fn test_error_injection() {
        let mut client = MockManagementClient::new();
        client.inject_error("test error");

        let result = client.get_node_identity().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));

        // Error should be cleared after use
        let result = client.get_node_identity().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_network_creation() {
        let network = MockNodeNetwork::new(3);
        assert_eq!(network.coordinator().node_type, NodeType::Coordinator);
        assert_eq!(network.executors.len(), 3);
        for exec in &network.executors {
            assert_eq!(exec.node_type, NodeType::Executor);
        }
    }
}
