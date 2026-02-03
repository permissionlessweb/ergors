//! Mock Workflow Engine for Testing
//!
//! Implements the 17-step Akash deployment workflow state machine
//! for testing without real infrastructure.

use super::chain::MockCosmosChain;
use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashProviderSelection, AkashRuntime, AkashWorkflowStatus,
    AkashWorkflowStep, ConfiguredSdl,
};
use std::collections::HashMap;

/// Mock workflow engine for testing the 17-step deployment workflow.
///
/// Each step can be configured to succeed, fail, or require specific conditions.
pub struct MockWorkflowEngine {
    /// Step execution configurations for testing different scenarios
    step_configs: HashMap<AkashWorkflowStep, StepConfig>,
    /// Simulated provider addresses that will bid on deployments
    mock_providers: Vec<MockProviderConfig>,
    /// Default account to use when key_name not specified
    default_account: Option<String>,
}

/// Configuration for how a step should behave in tests.
#[derive(Debug, Clone, Default)]
pub struct StepConfig {
    /// Should this step fail?
    pub should_fail: bool,
    /// Custom error message if failing
    pub error_message: Option<String>,
    /// Number of retries before succeeding (simulates transient failures)
    pub fail_count: u32,
    /// Current failure count
    current_failures: u32,
}

/// Mock provider that will bid on deployments.
#[derive(Debug, Clone)]
pub struct MockProviderConfig {
    pub address: String,
    pub bid_price_uakt: u64,
    /// If true, will automatically bid on any deployment
    pub auto_bid: bool,
}

impl Default for MockWorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockWorkflowEngine {
    /// Create a new workflow engine.
    pub fn new() -> Self {
        Self {
            step_configs: HashMap::new(),
            mock_providers: vec![
                // Default provider for testing
                MockProviderConfig {
                    address: "akash1provider0testxyz".to_string(),
                    bid_price_uakt: 1000,
                    auto_bid: true,
                },
            ],
            default_account: None,
        }
    }

    /// Set default account address.
    pub fn set_default_account(&mut self, address: impl Into<String>) {
        self.default_account = Some(address.into());
    }

    /// Configure a step to fail.
    pub fn configure_step_failure(
        &mut self,
        step: AkashWorkflowStep,
        error_message: impl Into<String>,
    ) {
        self.step_configs.insert(
            step,
            StepConfig {
                should_fail: true,
                error_message: Some(error_message.into()),
                ..Default::default()
            },
        );
    }

    /// Configure transient failures for a step.
    pub fn configure_transient_failures(&mut self, step: AkashWorkflowStep, fail_count: u32) {
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

    /// Clear step configuration.
    pub fn clear_step_config(&mut self, step: AkashWorkflowStep) {
        self.step_configs.remove(&step);
    }

    /// Add a mock provider.
    pub fn add_mock_provider(&mut self, address: impl Into<String>, bid_price_uakt: u64) {
        self.mock_providers.push(MockProviderConfig {
            address: address.into(),
            bid_price_uakt,
            auto_bid: true,
        });
    }

    /// Create a new workflow.
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
    pub fn advance_workflow(
        &mut self,
        mut workflow: AkashDeploymentWorkflow,
        chain: &mut MockCosmosChain,
    ) -> Result<AkashDeploymentWorkflow> {
        let current = AkashWorkflowStep::try_from(workflow.current_step)
            .map_err(|_| anyhow!("Invalid workflow step: {}", workflow.current_step))?;

        // Check for configured failures
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

        // Execute the step
        workflow.status = AkashWorkflowStatus::Running as i32;

        match current {
            AkashWorkflowStep::ConnectivityCheck => {
                // Connectivity check passes in mock - just advance
                workflow.current_step = AkashWorkflowStep::KeySelection as i32;
            }
            AkashWorkflowStep::KeySelection => self.execute_key_selection(&mut workflow, chain)?,
            AkashWorkflowStep::BalanceCheck => self.execute_balance_check(&mut workflow, chain)?,
            AkashWorkflowStep::GrantRequest => self.execute_grant_request(&mut workflow)?,
            AkashWorkflowStep::GrantWait => self.execute_grant_wait(&mut workflow, chain)?,
            AkashWorkflowStep::AuthzSetup => self.execute_authz_setup(&mut workflow, chain)?,
            AkashWorkflowStep::FeegrantSetup => {
                self.execute_feegrant_setup(&mut workflow, chain)?
            }
            AkashWorkflowStep::SdlConfiguration => self.execute_sdl_configuration(&mut workflow)?,
            AkashWorkflowStep::CertificateSetup => self.execute_certificate_setup(&mut workflow)?,
            AkashWorkflowStep::DeploymentCreate => {
                self.execute_deployment_create(&mut workflow, chain)?
            }
            AkashWorkflowStep::BidWait => self.execute_bid_wait(&mut workflow, chain)?,
            AkashWorkflowStep::ProviderSelection => {
                self.execute_provider_selection(&mut workflow, chain)?
            }
            AkashWorkflowStep::LeaseCreate => self.execute_lease_create(&mut workflow, chain)?,
            AkashWorkflowStep::ManifestSend => self.execute_manifest_send(&mut workflow)?,
            AkashWorkflowStep::EndpointRetrieval => {
                self.execute_endpoint_retrieval(&mut workflow)?
            }
            AkashWorkflowStep::EndpointTesting => self.execute_endpoint_testing(&mut workflow)?,
            AkashWorkflowStep::Complete => {
                // Already complete, nothing to do
            }
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
    // Step Implementations
    // =========================================================================

    fn execute_key_selection(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &mut MockCosmosChain,
    ) -> Result<()> {
        // Use default account if not set
        if workflow.account_address.is_empty() {
            workflow.account_address = self
                .default_account
                .clone()
                .unwrap_or_else(|| "akash1testaccount0xyz".to_string());
        }

        // Ensure account exists in chain
        chain.create_account(&workflow.account_address);

        workflow.current_step = AkashWorkflowStep::BalanceCheck as i32;
        Ok(())
    }

    fn execute_balance_check(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &MockCosmosChain,
    ) -> Result<()> {
        let balance = chain.get_balance(&workflow.account_address, "uakt");

        // Require minimum balance (100 AKT = 100_000_000 uakt for deployment)
        // In mock, we're lenient - just need something
        if balance < 1000 {
            // Check if we should request grants
            if !workflow.request_grant_from.is_empty() {
                workflow.current_step = AkashWorkflowStep::GrantRequest as i32;
            } else {
                return Err(anyhow!(
                    "Insufficient balance: {} uakt (need at least 1000 for testing)",
                    balance
                ));
            }
        } else {
            // Skip grant steps if we have balance
            workflow.current_step = AkashWorkflowStep::SdlConfiguration as i32;
        }

        Ok(())
    }

    fn execute_grant_request(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // In mock, just transition to wait state
        // Real implementation would send request to coordinator
        workflow.current_step = AkashWorkflowStep::GrantWait as i32;
        Ok(())
    }

    fn execute_grant_wait(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &MockCosmosChain,
    ) -> Result<()> {
        // Check if grants exist on chain
        // For testing, we assume they've been set up externally or skip
        let _has_authz = !workflow.authz_grants.is_empty()
            || chain
                .query_authz_grants(&workflow.account_address, &workflow.account_address)
                .is_empty();

        // Just proceed for mock
        workflow.current_step = AkashWorkflowStep::AuthzSetup as i32;
        Ok(())
    }

    fn execute_authz_setup(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        _chain: &mut MockCosmosChain,
    ) -> Result<()> {
        // In mock, authz is either already set up or we skip
        workflow.current_step = AkashWorkflowStep::FeegrantSetup as i32;
        Ok(())
    }

    fn execute_feegrant_setup(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        _chain: &mut MockCosmosChain,
    ) -> Result<()> {
        // In mock, feegrant is either already set up or we skip
        workflow.current_step = AkashWorkflowStep::SdlConfiguration as i32;
        Ok(())
    }

    fn execute_sdl_configuration(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // Validate SDL exists
        if workflow.configured_sdl.is_none() {
            return Err(anyhow!("No SDL configured"));
        }

        let sdl = workflow.configured_sdl.as_ref().unwrap();
        if sdl.resolved_content.is_empty() {
            return Err(anyhow!("SDL content is empty"));
        }

        workflow.current_step = AkashWorkflowStep::CertificateSetup as i32;
        Ok(())
    }

    fn execute_certificate_setup(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // In mock, skip certificate setup
        workflow.current_step = AkashWorkflowStep::DeploymentCreate as i32;
        Ok(())
    }

    fn execute_deployment_create(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &mut MockCosmosChain,
    ) -> Result<()> {
        // Create deployment on chain
        let deployment = chain.create_deployment(&workflow.account_address)?;

        workflow.deployment = Some(AkashRuntime {
            deployment_sequence: deployment.dseq.to_string(),
            ..Default::default()
        });

        workflow.current_step = AkashWorkflowStep::BidWait as i32;
        Ok(())
    }

    fn execute_bid_wait(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &mut MockCosmosChain,
    ) -> Result<()> {
        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment found"))?;

        // Parse dseq from string
        let dseq: u64 = deployment
            .deployment_sequence
            .parse()
            .map_err(|_| anyhow!("Invalid deployment sequence"))?;

        // Auto-submit bids from mock providers
        for provider in &self.mock_providers {
            if provider.auto_bid {
                let _ = chain.submit_bid(dseq, &provider.address, provider.bid_price_uakt);
            }
        }

        // Check for bids
        let bids = chain.query_bids(dseq);
        if bids.is_empty() {
            return Err(anyhow!("No bids received"));
        }

        workflow.current_step = AkashWorkflowStep::ProviderSelection as i32;
        Ok(())
    }

    fn execute_provider_selection(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &MockCosmosChain,
    ) -> Result<()> {
        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment found"))?;

        // Parse dseq from string
        let dseq: u64 = deployment
            .deployment_sequence
            .parse()
            .map_err(|_| anyhow!("Invalid deployment sequence"))?;

        // Select cheapest bid
        let bids = chain.query_bids(dseq);
        let best_bid = bids
            .iter()
            .min_by_key(|b| b.price_uakt)
            .ok_or_else(|| anyhow!("No bids available"))?;

        workflow.provider = Some(AkashProviderSelection {
            provider_address: best_bid.provider.clone(),
            bid_price_uakt: best_bid.price_uakt,
            ..Default::default()
        });

        workflow.current_step = AkashWorkflowStep::LeaseCreate as i32;
        Ok(())
    }

    fn execute_lease_create(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        chain: &mut MockCosmosChain,
    ) -> Result<()> {
        let deployment = workflow
            .deployment
            .as_ref()
            .ok_or_else(|| anyhow!("No deployment found"))?;

        // Parse dseq from string
        let dseq: u64 = deployment
            .deployment_sequence
            .parse()
            .map_err(|_| anyhow!("Invalid deployment sequence"))?;

        let provider = workflow
            .provider
            .as_ref()
            .ok_or_else(|| anyhow!("No provider selected"))?;

        // Create lease
        let _lease = chain.create_lease(dseq, &provider.provider_address)?;

        workflow.current_step = AkashWorkflowStep::ManifestSend as i32;
        Ok(())
    }

    fn execute_manifest_send(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // In mock, skip manifest send
        workflow.current_step = AkashWorkflowStep::EndpointRetrieval as i32;
        Ok(())
    }

    fn execute_endpoint_retrieval(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // Generate mock endpoints
        workflow.endpoints.insert(
            "http".to_string(),
            "http://mock-endpoint.akash.network:80".to_string(),
        );
        workflow.endpoints.insert(
            "https".to_string(),
            "https://mock-endpoint.akash.network:443".to_string(),
        );

        workflow.current_step = AkashWorkflowStep::EndpointTesting as i32;
        Ok(())
    }

    fn execute_endpoint_testing(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
        // In mock, endpoints always pass testing
        workflow.current_step = AkashWorkflowStep::Complete as i32;
        workflow.status = AkashWorkflowStatus::Completed as i32;

        let now = chrono::Utc::now();
        workflow.completed_at = Some(pbjson_types::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sdl() -> String {
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
          units: 0.5
        memory:
          size: 512Mi
        storage:
          size: 512Mi
  placement:
    default:
      pricing:
        web:
          denom: uakt
          amount: 1000
deployment:
  web:
    default:
      profile: web
      count: 1
"#
        .to_string()
    }

    #[test]
    fn test_workflow_creation() {
        let engine = MockWorkflowEngine::new();
        let workflow = engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        assert_eq!(workflow.session_id, "test-session");
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::KeySelection as i32
        );
        assert_eq!(workflow.status, AkashWorkflowStatus::Pending as i32);
    }

    #[test]
    fn test_workflow_advance_key_selection() {
        let mut engine = MockWorkflowEngine::new();
        engine.set_default_account("akash1testaccount");

        let mut chain = MockCosmosChain::new();

        let workflow = engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        let advanced = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(
            advanced.current_step,
            AkashWorkflowStep::BalanceCheck as i32
        );
        assert_eq!(advanced.account_address, "akash1testaccount");
    }

    #[test]
    fn test_full_workflow_happy_path() {
        let mut engine = MockWorkflowEngine::new();
        engine.set_default_account("akash1owner");

        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1owner", 1_000_000_000); // 1000 AKT

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Run through all steps
        let max_iterations = 20;
        let mut iterations = 0;

        while workflow.current_step != AkashWorkflowStep::Complete as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
            && iterations < max_iterations
        {
            workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
            iterations += 1;
        }

        assert_eq!(workflow.current_step, AkashWorkflowStep::Complete as i32);
        assert_eq!(workflow.status, AkashWorkflowStatus::Completed as i32);
        assert!(workflow.completed_at.is_some());
        assert!(!workflow.endpoints.is_empty());
    }

    #[test]
    fn test_workflow_step_failure() {
        let mut engine = MockWorkflowEngine::new();
        engine.set_default_account("akash1owner");
        engine.configure_step_failure(AkashWorkflowStep::SdlConfiguration, "SDL validation failed");

        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Advance until failure or completion
        let max_iterations = 20;
        let mut iterations = 0;

        while workflow.current_step != AkashWorkflowStep::Complete as i32
            && workflow.current_step != AkashWorkflowStep::Failed as i32
            && iterations < max_iterations
        {
            workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
            iterations += 1;
        }

        assert_eq!(workflow.current_step, AkashWorkflowStep::Failed as i32);
        assert_eq!(workflow.status, AkashWorkflowStatus::Failed as i32);
        assert!(workflow.last_error.contains("SDL validation failed"));
    }

    #[test]
    fn test_transient_failure_recovery() {
        let mut engine = MockWorkflowEngine::new();
        engine.set_default_account("akash1owner");
        engine.configure_transient_failures(AkashWorkflowStep::DeploymentCreate, 2);

        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1owner", 1_000_000_000);

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Advance to DeploymentCreate
        while workflow.current_step != AkashWorkflowStep::DeploymentCreate as i32 {
            workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        }

        // First attempt - should fail transiently
        workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(workflow.retry_count, 1);
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::DeploymentCreate as i32
        );

        // Second attempt - should fail transiently
        workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(workflow.retry_count, 2);

        // Third attempt - should succeed
        workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(workflow.current_step, AkashWorkflowStep::BidWait as i32);
    }

    #[test]
    fn test_insufficient_balance_triggers_grant_flow() {
        let mut engine = MockWorkflowEngine::new();
        engine.set_default_account("akash1executor");

        let mut chain = MockCosmosChain::new();
        // Don't fund account - should trigger grant request

        let mut workflow =
            engine.create_workflow("test-session".to_string(), create_test_sdl(), None);

        // Set up grant request target
        workflow.request_grant_from = vec![1, 2, 3]; // Dummy pubkey

        // Advance through key selection
        workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::BalanceCheck as i32
        );

        // Balance check should redirect to grant request
        workflow = engine.advance_workflow(workflow, &mut chain).unwrap();
        assert_eq!(
            workflow.current_step,
            AkashWorkflowStep::GrantRequest as i32
        );
    }
}
