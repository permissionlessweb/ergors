//! Mock Workflow Engine for Testing
//!
//! Wraps the real `DeploymentWorkflow<TestBackend>` state machine, providing
//! a proto-compatible interface for MockManagementClient while exercising
//! actual workflow logic for deployment steps (certificate through endpoints).
//!
//! ## Architecture
//!
//! Pre-workflow steps (key selection, grants) are handled directly by
//! `MockWorkflowEngine`. Deployment steps (certificate through endpoints)
//! delegate to the real `DeploymentWorkflow` state machine.
//!
//! ## Failure Injection
//!
//! Two systems handle failures:
//! - **Pre-workflow steps** (KeySelection, BalanceCheck, GrantRequest, etc.):
//!   Use `step_configs` HashMap for direct control.
//! - **Real workflow steps** (CertificateSetup, DeploymentCreate, etc.):
//!   Use `TestBackend::inject_failure()` to inject backend errors.
//!
//! Both support permanent and transient (N failures then succeed) modes.

use super::chain::MockCosmosChain;
use super::test_backend::{FailureConfig, MockProviderConfig, TestBackend, TestSigner};
use akash_deploy_rs::{
    AkashBackend, DeploymentState, DeploymentWorkflow, Step, StepResult, WorkflowConfig,
};
use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashProviderSelection, AkashRuntime, AkashWorkflowStatus,
    AkashWorkflowStep, ConfiguredSdl,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

// Test SDL constants
const TEST_SDL_CPU_UNITS: &str = "0.5";
const TEST_SDL_MEMORY_SIZE: &str = "512Mi";
const TEST_SDL_STORAGE_SIZE: &str = "512Mi";
const TEST_SDL_BID_AMOUNT: u64 = 1000; // uakt per block

/// Configuration for how a pre-workflow step should behave in tests.
#[derive(Debug, Clone, Default)]
struct StepConfig {
    should_fail: bool,
    error_message: Option<String>,
    fail_count: u32,
    current_failures: u32,
}

/// Workflow engine wrapping the real `DeploymentWorkflow<TestBackend>`.
///
/// Pre-workflow steps (key selection, balance check with grant redirect, grants)
/// are handled directly. Deployment steps (certificate setup through endpoint
/// retrieval) delegate to the real state machine via `TestBackend`.
pub struct MockWorkflowEngine {
    pub(crate) backend: TestBackend,
    signer: TestSigner,
    config: WorkflowConfig,
    /// Pre-workflow step failure configs
    step_configs: HashMap<AkashWorkflowStep, StepConfig>,
    /// Proto steps with permanent failure injection via TestBackend
    permanent_failures: HashSet<AkashWorkflowStep>,
    /// Default account address
    default_account: Option<String>,
}

impl MockWorkflowEngine {
    /// Create a new workflow engine backed by the given chain.
    pub fn new(chain: Arc<RwLock<MockCosmosChain>>) -> Self {
        let backend = TestBackend::new(chain);
        Self {
            backend,
            signer: TestSigner,
            config: WorkflowConfig {
                min_balance_uakt: 1000, // Low threshold for testing
                bid_wait_seconds: 0,    // No waiting in tests
                max_bid_wait_attempts: 3,
                max_endpoint_wait_attempts: 3,
                auto_select_cheapest_bid: true,
                trusted_providers: Vec::new(),
            },
            step_configs: HashMap::new(),
            permanent_failures: HashSet::new(),
            default_account: None,
        }
    }

    /// Set default account address.
    pub fn set_default_account(&mut self, address: impl Into<String>) {
        self.default_account = Some(address.into());
    }

    /// Configure a step to fail permanently.
    pub fn configure_step_failure(
        &mut self,
        step: AkashWorkflowStep,
        error_message: impl Into<String>,
    ) {
        let msg = error_message.into();
        if let Some(method) = Self::step_to_backend_method(step) {
            self.backend.inject_failure(
                method,
                FailureConfig {
                    message: msg,
                    remaining: None,
                },
            );
            self.permanent_failures.insert(step);
        } else {
            self.step_configs.insert(
                step,
                StepConfig {
                    should_fail: true,
                    error_message: Some(msg),
                    ..Default::default()
                },
            );
        }
    }

    /// Configure transient failures for a step.
    pub fn configure_transient_failures(&mut self, step: AkashWorkflowStep, fail_count: u32) {
        if let Some(method) = Self::step_to_backend_method(step) {
            self.backend.inject_failure(
                method,
                FailureConfig {
                    message: format!("Transient failure ({}x)", fail_count),
                    remaining: Some(fail_count),
                },
            );
        } else {
            self.step_configs.insert(
                step,
                StepConfig {
                    should_fail: false,
                    error_message: None,
                    fail_count,
                    current_failures: 0,
                },
            );
        }
    }

    /// Clear step configuration.
    pub fn clear_step_config(&mut self, step: AkashWorkflowStep) {
        if let Some(method) = Self::step_to_backend_method(step) {
            self.backend.clear_failure(method);
            self.permanent_failures.remove(&step);
        } else {
            self.step_configs.remove(&step);
        }
    }

    /// Add a mock provider.
    pub fn add_mock_provider(&mut self, address: impl Into<String>, bid_price_uakt: u64) {
        let addr = address.into();
        self.backend.add_provider(MockProviderConfig {
            host_uri: format!("https://{}.test:8443", addr),
            address: addr,
            bid_price_uakt,
            auto_bid: true,
        });
    }

    /// Create a new proto workflow.
    pub fn create_workflow(
        &self,
        session_id: String,
        sdl_content: String,
        key_name: Option<String>,
    ) -> AkashDeploymentWorkflow {
        let now = chrono::Utc::now();

        AkashDeploymentWorkflow {
            session_id,
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            selected_key_name: key_name.unwrap_or_else(|| "default".to_string()),
            account_address: self.default_account.clone().unwrap_or_default(),
            hd_account_index: 0,
            authz_grants: Vec::new(),
            feegrants: Vec::new(),
            configured_sdl: Some(ConfiguredSdl {
                resolved_content: sdl_content,
                ..Default::default()
            }),
            deployment: None,
            provider: None,
            endpoints: HashMap::new(),
            test_results: Vec::new(),
            last_error: String::new(),
            retry_count: 0,
            created_at: Some(pbjson_types::Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            updated_at: None,
            completed_at: None,
            chain_id: "akashnet-2".to_string(),
            node_endpoint: "http://localhost:26657".to_string(),
            max_retries: 3,
            timeout_seconds: 300,
            grant_request: None,
            request_grant_from: Vec::new(),
            grant_duration_seconds: 86400,
            label: String::new(),
            model_name: String::new(),
            grant_spend_limit_uakt: 5_000_000,
            grant_purpose: String::new(),
            available_bids: Vec::new(),
            certificate: None,
            encrypted_cert_private_key: vec![],
            lease_id_info: None,
            options: None,
            service_endpoints: Vec::new(),
        }
    }

    /// Advance workflow to next step.
    pub async fn advance_workflow(
        &mut self,
        mut workflow: AkashDeploymentWorkflow,
    ) -> Result<AkashDeploymentWorkflow> {
        let current = AkashWorkflowStep::try_from(workflow.current_step)
            .map_err(|_| anyhow!("Invalid workflow step: {}", workflow.current_step))?;

        // Check pre-workflow step configs for failures
        {
            if let Some(config) = self.step_configs.get_mut(&current) {
                if config.should_fail {
                    workflow.status = AkashWorkflowStatus::Failed as i32;
                    workflow.last_error = config
                        .error_message
                        .clone()
                        .unwrap_or_else(|| format!("Step {:?} configured to fail", current));
                    workflow.current_step = AkashWorkflowStep::Failed as i32;
                    return Ok(workflow);
                }

                if config.current_failures < config.fail_count {
                    config.current_failures += 1;
                    workflow.retry_count += 1;
                    workflow.last_error = format!(
                        "Transient failure {}/{}",
                        config.current_failures, config.fail_count
                    );
                    return Ok(workflow);
                }
            }
        }

        workflow.status = AkashWorkflowStatus::Running as i32;

        match current {
            // Pre-workflow steps (mock-handled)
            AkashWorkflowStep::ConnectivityCheck => {
                workflow.current_step = AkashWorkflowStep::KeySelection as i32;
            }
            AkashWorkflowStep::KeySelection => {
                self.handle_key_selection(&mut workflow);
            }
            AkashWorkflowStep::BalanceCheck => {
                self.handle_balance_check(&mut workflow)?;
            }
            AkashWorkflowStep::GrantRequest => {
                workflow.current_step = AkashWorkflowStep::GrantWait as i32;
            }
            AkashWorkflowStep::GrantWait => {
                workflow.current_step = AkashWorkflowStep::SdlConfiguration as i32;
            }
            AkashWorkflowStep::SdlConfiguration => {
                self.handle_sdl_configuration(&mut workflow).await?;
            }

            // Real workflow steps
            AkashWorkflowStep::CertificateSetup
            | AkashWorkflowStep::DeploymentCreate
            | AkashWorkflowStep::BidWait
            | AkashWorkflowStep::ProviderSelection
            | AkashWorkflowStep::LeaseCreate
            | AkashWorkflowStep::ManifestSend
            | AkashWorkflowStep::EndpointRetrieval => {
                self.advance_real_workflow(&mut workflow).await?;
            }

            AkashWorkflowStep::Complete => {}
            AkashWorkflowStep::Failed | AkashWorkflowStep::Unspecified => {
                return Err(anyhow!("Cannot advance from step {:?}", current));
            }
        }

        // Update timestamp
        let now = chrono::Utc::now();
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        });

        Ok(workflow)
    }

    // =========================================================================
    // Pre-workflow step handlers
    // =========================================================================

    fn handle_key_selection(&self, workflow: &mut AkashDeploymentWorkflow) {
        if workflow.account_address.is_empty() {
            workflow.account_address = self
                .default_account
                .clone()
                .unwrap_or_else(|| "akash1testaccount0xyz".to_string());
        }

        // Ensure account exists in chain
        self.backend.create_account(&workflow.account_address);

        workflow.current_step = AkashWorkflowStep::BalanceCheck as i32;
    }

    fn handle_balance_check(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        let balance = self.backend.get_balance(&workflow.account_address, "uakt");

        if balance < self.config.min_balance_uakt {
            if !workflow.request_grant_from.is_empty() {
                workflow.current_step = AkashWorkflowStep::GrantRequest as i32;
            } else {
                return Err(anyhow!(
                    "Insufficient balance: {} uakt (need at least {} for testing)",
                    balance,
                    self.config.min_balance_uakt
                ));
            }
        } else {
            workflow.current_step = AkashWorkflowStep::SdlConfiguration as i32;
        }

        Ok(())
    }

    async fn handle_sdl_configuration(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<()> {
        // Validate SDL exists
        let sdl = workflow
            .configured_sdl
            .as_ref()
            .ok_or_else(|| anyhow!("No SDL configured"))?;
        if sdl.resolved_content.is_empty() {
            return Err(anyhow!("SDL content is empty"));
        }

        // Create DeploymentState starting at EnsureCertificate.
        // Init (SDL validation) and CheckBalance were already handled
        // by the mock pre-workflow steps above.
        let sdl_content = sdl.resolved_content.clone();
        let mut state = DeploymentState::new(&workflow.session_id, &workflow.account_address)
            .with_sdl(sdl_content);
        state.transition(Step::EnsureCertificate);

        self.backend
            .save_state(&workflow.session_id, &state)
            .await
            .map_err(|e| anyhow!("{}", e))?;

        workflow.current_step = AkashWorkflowStep::CertificateSetup as i32;
        Ok(())
    }

    // =========================================================================
    // Real workflow step advancement
    // =========================================================================

    async fn advance_real_workflow(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
    ) -> Result<()> {
        let session_id = workflow.session_id.clone();

        // Load the internal deployment state
        let mut state = self
            .backend
            .load_state(&session_id)
            .await
            .map_err(|e| anyhow!("{}", e))?
            .ok_or_else(|| anyhow!("Internal deployment state not found for {}", session_id))?;

        // Advance via the real workflow engine
        let wf = DeploymentWorkflow::new(&self.backend, &self.signer, self.config.clone());
        let result = wf.advance(&mut state).await;

        let current_proto = AkashWorkflowStep::try_from(workflow.current_step)
            .map_err(|_| anyhow!("Invalid step"))?;

        match result {
            Ok(StepResult::Continue) => {
                let new_proto = Self::state_step_to_proto(&state.step);
                workflow.current_step = new_proto as i32;
                Self::sync_proto_from_state(workflow, &state);
            }
            Ok(StepResult::Complete) => {
                Self::sync_proto_from_state(workflow, &state);
                workflow.current_step = AkashWorkflowStep::Complete as i32;
                workflow.status = AkashWorkflowStatus::Completed as i32;
                let now = chrono::Utc::now();
                workflow.completed_at = Some(pbjson_types::Timestamp {
                    seconds: now.timestamp(),
                    nanos: now.timestamp_subsec_nanos() as i32,
                });
            }
            Ok(StepResult::Failed(reason)) => {
                workflow.current_step = AkashWorkflowStep::Failed as i32;
                workflow.status = AkashWorkflowStatus::Failed as i32;
                workflow.last_error = reason;
            }
            Ok(StepResult::NeedsInput(_)) => {
                // With auto_select_cheapest_bid=true this shouldn't happen
            }
            Err(e) => {
                // Backend error — check if permanent or transient
                if self.permanent_failures.contains(&current_proto) {
                    workflow.current_step = AkashWorkflowStep::Failed as i32;
                    workflow.status = AkashWorkflowStatus::Failed as i32;
                    workflow.last_error = e.to_string();
                } else {
                    // Transient — stay on same step for retry
                    workflow.retry_count += 1;
                    workflow.last_error = e.to_string();
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // Conversion helpers
    // =========================================================================

    /// Map proto step to backend method name for failure injection.
    fn step_to_backend_method(step: AkashWorkflowStep) -> Option<&'static str> {
        match step {
            AkashWorkflowStep::CertificateSetup => Some("broadcast_create_certificate"),
            AkashWorkflowStep::DeploymentCreate => Some("broadcast_create_deployment"),
            AkashWorkflowStep::BidWait => Some("query_bids"),
            AkashWorkflowStep::LeaseCreate => Some("broadcast_create_lease"),
            AkashWorkflowStep::ManifestSend => Some("send_manifest"),
            AkashWorkflowStep::EndpointRetrieval => Some("query_provider_status"),
            _ => None,
        }
    }

    /// Map internal Step to proto AkashWorkflowStep.
    fn state_step_to_proto(step: &Step) -> AkashWorkflowStep {
        match step {
            Step::Init => AkashWorkflowStep::SdlConfiguration,
            Step::CheckBalance => AkashWorkflowStep::BalanceCheck,
            Step::EnsureCertificate => AkashWorkflowStep::CertificateSetup,
            Step::CreateDeployment => AkashWorkflowStep::DeploymentCreate,
            Step::WaitForBids { .. } => AkashWorkflowStep::BidWait,
            Step::SelectProvider => AkashWorkflowStep::ProviderSelection,
            Step::CreateLease => AkashWorkflowStep::LeaseCreate,
            Step::SendManifest => AkashWorkflowStep::ManifestSend,
            Step::WaitForEndpoints { .. } => AkashWorkflowStep::EndpointRetrieval,
            Step::Complete => AkashWorkflowStep::Complete,
            Step::Failed { .. } => AkashWorkflowStep::Failed,
        }
    }

    /// Sync proto workflow fields from internal DeploymentState.
    fn sync_proto_from_state(workflow: &mut AkashDeploymentWorkflow, state: &DeploymentState) {
        if let Some(dseq) = state.dseq {
            workflow.deployment = Some(AkashRuntime {
                deployment_sequence: dseq.to_string(),
                ..Default::default()
            });
        }

        if let Some(provider) = &state.selected_provider {
            let price = state
                .bids
                .iter()
                .find(|b| &b.provider == provider)
                .map(|b| b.price_uakt)
                .unwrap_or(0);
            workflow.provider = Some(AkashProviderSelection {
                provider_address: provider.clone(),
                bid_price_uakt: price,
                ..Default::default()
            });
        }

        for ep in &state.endpoints {
            workflow
                .endpoints
                .insert(ep.service.clone(), format!("{}:{}", ep.uri, ep.port));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sdl() -> String {
        format!(
            r#"---
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: {cpu}
        memory:
          size: {mem}
        storage:
          size: {storage}
  placement:
    default:
      pricing:
        web:
          denom: uakt
          amount: {bid}
deployment:
  web:
    default:
      profile: web
      count: 1
"#,
            cpu = TEST_SDL_CPU_UNITS,
            mem = TEST_SDL_MEMORY_SIZE,
            storage = TEST_SDL_STORAGE_SIZE,
            bid = TEST_SDL_BID_AMOUNT
        )
    }

    fn make_engine() -> (MockWorkflowEngine, Arc<RwLock<MockCosmosChain>>) {
        let chain = Arc::new(RwLock::new(MockCosmosChain::new()));
        let engine = MockWorkflowEngine::new(Arc::clone(&chain));
        (engine, chain)
    }

    #[test]
    fn test_workflow_creation() {
        let (engine, _chain) = make_engine();
        let workflow = engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        assert_eq!(workflow.session_id, "test-session");
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::KeySelection as i32
        );
        assert_eq!(workflow.status, AkashWorkflowStatus::Pending as i32);
    }

    #[tokio::test]
    async fn test_workflow_advance_key_selection() {
        let (mut engine, _chain) = make_engine();
        engine.set_default_account("akash1testaccount");

        let workflow = engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        let advanced = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(
            advanced.current_step,
            AkashWorkflowStep::BalanceCheck as i32
        );
        assert_eq!(advanced.account_address, "akash1testaccount");
    }

    #[tokio::test]
    async fn test_full_workflow_happy_path() {
        let (mut engine, chain) = make_engine();
        engine.set_default_account("akash1owner");
        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Run through all steps
        let max_iterations = 20;
        let mut iterations = 0;

        while workflow.current_step != AkashWorkflowStep::Complete as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
            && iterations < max_iterations
        {
            workflow = engine.advance_workflow(workflow).await.unwrap();
            iterations += 1;
        }

        assert_eq!(workflow.current_step, AkashWorkflowStep::Complete as i32);
        assert_eq!(workflow.status, AkashWorkflowStatus::Completed as i32);
        assert!(workflow.completed_at.is_some());
        assert!(!workflow.endpoints.is_empty());
        // Verify real workflow created chain state
        assert!(workflow.deployment.is_some());
        assert!(workflow.provider.is_some());
    }

    #[tokio::test]
    async fn test_workflow_step_failure() {
        let (mut engine, chain) = make_engine();
        engine.set_default_account("akash1owner");
        engine
            .configure_step_failure(AkashWorkflowStep::SdlConfiguration, "SDL validation failed");

        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Advance until failure or completion
        let max_iterations = 20;
        let mut iterations = 0;

        while workflow.current_step != AkashWorkflowStep::Complete as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
            && iterations < max_iterations
        {
            workflow = engine.advance_workflow(workflow).await.unwrap();
            iterations += 1;
        }

        assert_eq!(workflow.current_step, AkashWorkflowStep::Failed as i32);
        assert_eq!(workflow.status, AkashWorkflowStatus::Failed as i32);
        assert!(workflow.last_error.contains("SDL validation failed"));
    }

    #[tokio::test]
    async fn test_transient_failure_recovery() {
        let (mut engine, chain) = make_engine();
        engine.set_default_account("akash1owner");
        engine.configure_transient_failures(AkashWorkflowStep::DeploymentCreate, 2);

        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Advance to DeploymentCreate
        while workflow.current_step != AkashWorkflowStep::DeploymentCreate as i32 {
            workflow = engine.advance_workflow(workflow).await.unwrap();
        }

        // First attempt - should fail transiently
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(workflow.retry_count, 1);
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::DeploymentCreate as i32
        );

        // Second attempt - should fail transiently
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(workflow.retry_count, 2);

        // Third attempt - should succeed
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(workflow.current_step, AkashWorkflowStep::BidWait as i32);
    }

    #[tokio::test]
    async fn test_insufficient_balance_triggers_grant_flow() {
        let (mut engine, _chain) = make_engine();
        engine.set_default_account("akash1executor");
        // Don't fund account - should trigger grant request

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Set up grant request target
        workflow.request_grant_from = vec![1, 2, 3]; // Dummy pubkey

        // Advance through key selection
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::BalanceCheck as i32
        );

        // Balance check should redirect to grant request
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::GrantRequest as i32
        );
    }

    #[tokio::test]
    async fn test_real_workflow_creates_chain_state() {
        let (mut engine, chain) = make_engine();
        engine.set_default_account("akash1owner");
        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Run to completion
        let max_iterations = 20;
        let mut iterations = 0;
        while workflow.current_step != AkashWorkflowStep::Complete as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
            && iterations < max_iterations
        {
            workflow = engine.advance_workflow(workflow).await.unwrap();
            iterations += 1;
        }

        assert_eq!(workflow.current_step, AkashWorkflowStep::Complete as i32);

        // Verify chain state was mutated by the real workflow
        let chain_r = chain.read().unwrap();
        let dseq: u64 = workflow
            .deployment
            .as_ref()
            .unwrap()
            .deployment_sequence
            .parse()
            .unwrap();
        assert!(chain_r.get_deployment(dseq).is_some());
        assert!(chain_r.get_lease(dseq).is_some());
    }

    #[tokio::test]
    async fn test_permanent_failure_on_real_step() {
        let (mut engine, chain) = make_engine();
        engine.set_default_account("akash1owner");
        engine.configure_step_failure(
            AkashWorkflowStep::DeploymentCreate,
            "deployment creation blocked",
        );

        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Advance to DeploymentCreate
        while workflow.current_step != AkashWorkflowStep::DeploymentCreate as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
        {
            workflow = engine.advance_workflow(workflow).await.unwrap();
        }

        // Should reach DeploymentCreate then fail
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::DeploymentCreate as i32
        );
        workflow = engine.advance_workflow(workflow).await.unwrap();
        assert_eq!(workflow.current_step, AkashWorkflowStep::Failed as i32);
        assert!(workflow
            .last_error
            .contains("deployment creation blocked"));
    }
}
