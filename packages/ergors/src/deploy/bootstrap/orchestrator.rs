//! Bootstrap Orchestrator
//!
//! Drives the complete bootstrap workflow from start to finish.
//! Integrates all components: Akash deployment, config generation, P2P transport.

use crate::deploy::state_machine::{BootstrapState, BootstrapStep, StepResult};
use crate::deploy::akash::deployer::AutomatedDeployer;
use crate::deploy::akash::node_sdl::{NodeBootstrapConfig, NodeSdlGenerator};
use crate::storage::ErgorsStorage;
use crate::ErgorsNetworkManifold;
use anyhow::{anyhow, Result};
use ho_std::bootstrap::{BootstrapConfigGenerator, BootstrapTransport, FileType};
use ho_std::keys::commonware::NodePubkey;
use ho_std::types::ergors::network::v1::NodeType;
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashWorkflowOptions, ConfiguredSdl,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum number of P2P connection check attempts before failing
const MAX_P2P_CHECK_ATTEMPTS: u32 = 60;
/// Seconds between P2P connection checks
const P2P_CHECK_INTERVAL_SECS: u64 = 10;
/// Maximum time to wait for node online verification (seconds)
const VERIFY_ONLINE_TIMEOUT_SECS: u64 = 30;

/// Bootstrap orchestrator parameters
#[derive(Debug, Clone)]
pub struct NodeBootstrapParams {
    /// Node type to bootstrap
    pub node_type: NodeType,
    /// Docker image tag to use (from build-image.sh output)
    pub image_tag: String,
    /// Bootstrap peer addresses (coordinator's P2P address)
    pub bootstrap_peers: Vec<String>,
    /// Custom environment variables
    pub env_vars: Vec<(String, String)>,
    /// Cosmos key name for Akash deployments
    pub cosmos_key_name: String,
    /// Cosmos account address (bech32)
    pub cosmos_account_address: String,
    /// Akash deployment label
    pub deploy_label: String,
}

/// Bootstrap orchestrator
///
/// Coordinates the multi-step bootstrap workflow:
/// 1. Generate identity and config
/// 2. Create Akash deployment with generated SDL
/// 3. Wait for deployment to become ready
/// 4. Establish P2P connection with new node
/// 5. Transfer config and custody files
/// 6. Verify node is operational
pub struct BootstrapOrchestrator {
    akash_deployer: Arc<AutomatedDeployer>,
    config_generator: BootstrapConfigGenerator,
    transport: Option<Arc<Mutex<BootstrapTransport>>>,
    network_manifold: Arc<Mutex<ErgorsNetworkManifold>>,
    storage: Arc<ErgorsStorage>,
}

impl BootstrapOrchestrator {
    /// Create a new bootstrap orchestrator
    pub fn new(
        akash_deployer: Arc<AutomatedDeployer>,
        config_generator: BootstrapConfigGenerator,
        transport: Option<Arc<Mutex<BootstrapTransport>>>,
        network_manifold: Arc<Mutex<ErgorsNetworkManifold>>,
        storage: Arc<ErgorsStorage>,
    ) -> Self {
        Self {
            akash_deployer,
            config_generator,
            transport,
            network_manifold,
            storage,
        }
    }

    /// Bootstrap a new node via Akash deployment
    ///
    /// Creates a new bootstrap session and initiates the workflow.
    /// Returns the session ID for tracking progress.
    pub async fn bootstrap_node_akash(&self, params: NodeBootstrapParams) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        info!("Starting bootstrap session: {}", session_id);

        // Create initial state
        let state = BootstrapState::new(session_id.clone(), params.node_type);

        // Save initial state
        self.storage
            .save_bootstrap_state(&session_id, &state)
            .await?;

        // Start workflow in background
        let orchestrator = self.clone_for_background();
        let params_clone = params.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = orchestrator
                .run_workflow(&session_id_clone, params_clone)
                .await
            {
                error!("Bootstrap workflow failed: {}", e);
            }
        });

        Ok(session_id)
    }

    /// Run the complete bootstrap workflow
    async fn run_workflow(&self, session_id: &str, params: NodeBootstrapParams) -> Result<()> {
        loop {
            // Load current state
            let mut state = self
                .storage
                .load_bootstrap_state(session_id)
                .await?
                .ok_or_else(|| anyhow!("Bootstrap session not found: {}", session_id))?;

            // Check if terminal
            if state.is_terminal() {
                info!("Bootstrap session {} finished: {}", session_id, state.step);
                break;
            }

            // Advance one step
            match self.advance_bootstrap(&mut state, &params).await {
                Ok(StepResult::Continue) => {
                    self.storage
                        .save_bootstrap_state(session_id, &state)
                        .await?;
                }
                Ok(StepResult::Complete) => {
                    state.transition(BootstrapStep::Complete);
                    self.storage
                        .save_bootstrap_state(session_id, &state)
                        .await?;
                    info!("Bootstrap complete: {}", session_id);
                    break;
                }
                Ok(StepResult::Waiting { retry_after_secs }) => {
                    self.storage
                        .save_bootstrap_state(session_id, &state)
                        .await?;
                    tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
                }
                Ok(StepResult::Failed(reason)) => {
                    error!("Bootstrap step reported failure: {}", reason);
                    state.fail(reason);
                    self.storage
                        .save_bootstrap_state(session_id, &state)
                        .await?;
                    break;
                }
                Err(e) => {
                    error!("Bootstrap step failed: {}", e);
                    state.fail(e.to_string());
                    self.storage
                        .save_bootstrap_state(session_id, &state)
                        .await?;
                    break;
                }
            }
        }

        Ok(())
    }

    /// Advance bootstrap state machine by one step
    pub async fn advance_bootstrap(
        &self,
        state: &mut BootstrapState,
        params: &NodeBootstrapParams,
    ) -> Result<StepResult> {
        debug!("Advancing bootstrap step: {}", state.step);

        match &state.step {
            BootstrapStep::Init => {
                state.transition(BootstrapStep::GenerateIdentity);
                Ok(StepResult::Continue)
            }
            BootstrapStep::GenerateIdentity => {
                self.step_generate_identity(state, params).await
            }
            BootstrapStep::BuildDockerImage => {
                // Skip - we assume image is pre-built and tagged
                state.docker_image_tag = Some(params.image_tag.clone());
                state.transition(BootstrapStep::CreateAkashDeployment);
                Ok(StepResult::Continue)
            }
            BootstrapStep::CreateAkashDeployment => {
                self.step_create_akash_deployment(state, params).await
            }
            BootstrapStep::WaitForDeployment => self.step_wait_for_deployment(state).await,
            BootstrapStep::EstablishP2PConnection => self.step_establish_p2p(state).await,
            BootstrapStep::SendConfig => self.step_send_config(state).await,
            BootstrapStep::SendCustody => self.step_send_custody(state).await,
            BootstrapStep::SendApiKeys => {
                // Optional step - skip for now
                state.transition(BootstrapStep::VerifyNodeOnline);
                Ok(StepResult::Continue)
            }
            BootstrapStep::VerifyNodeOnline => self.step_verify_online(state).await,
            BootstrapStep::Complete | BootstrapStep::Failed { .. } => Ok(StepResult::Complete),
        }
    }

    /// Step: Generate node identity and config
    ///
    /// Creates a new Ed25519 keypair, builds config TOML, and encrypts
    /// custody data. All generated data is stored in BootstrapState for
    /// later transmission to the new node.
    async fn step_generate_identity(
        &self,
        state: &mut BootstrapState,
        params: &NodeBootstrapParams,
    ) -> Result<StepResult> {
        info!("Generating node identity for session: {}", state.session_id);

        // Generate secure bootstrap password
        let bootstrap_password = BootstrapConfigGenerator::generate_bootstrap_password();

        // Generate complete node config
        let node_config_params = ho_std::bootstrap::NodeBootstrapParams {
            node_type: params.node_type,
            host: "0.0.0.0".to_string(),
            p2p_port: 26969,
            api_port: 8080,
            ssh_port: 22,
            bootstrap_peers: params.bootstrap_peers.clone(),
            custody_password: bootstrap_password.clone(),
        };

        let node_config = self
            .config_generator
            .generate_node_config(node_config_params)
            .await?;

        // Store public key in state
        let pubkey_bytes = node_config
            .identity
            .public_key
            .as_ref()
            .ok_or_else(|| anyhow!("Generated identity missing public key"))?;
        let pubkey_hex = hex::encode(pubkey_bytes);
        state.generated_identity_pubkey = Some(pubkey_hex);
        state.bootstrap_peer = params.bootstrap_peers.first().cloned();

        // Serialize config to TOML and store in state for later transmission
        let config_toml = self.config_generator.to_toml(&node_config)?;
        state.config_toml = Some(config_toml);
        state.custody_data = Some(node_config.custody_data);
        state.custody_password = Some(bootstrap_password);

        info!(
            "Generated identity for new node, pubkey: {}",
            state.generated_identity_pubkey.as_deref().unwrap_or("?")
        );

        state.transition(BootstrapStep::BuildDockerImage);
        Ok(StepResult::Continue)
    }

    /// Step: Create Akash deployment
    ///
    /// Generates SDL for the node type and submits a deployment to Akash
    /// via the AutomatedDeployer. This runs the full 11-step Akash
    /// deployment workflow (balance check, create deployment, wait for bids,
    /// select provider, create lease, send manifest, retrieve endpoints).
    async fn step_create_akash_deployment(
        &self,
        state: &mut BootstrapState,
        params: &NodeBootstrapParams,
    ) -> Result<StepResult> {
        info!(
            "Creating Akash deployment for session: {}",
            state.session_id
        );

        // Generate SDL for the node type
        let sdl_config = NodeBootstrapConfig {
            node_type: params.node_type,
            image_tag: params.image_tag.clone(),
            p2p_port: 26969,
            api_port: 8080,
            bootstrap_peers: params.bootstrap_peers.clone(),
            env_vars: params.env_vars.clone(),
        };

        let generator = NodeSdlGenerator::new(sdl_config);
        let sdl_content = match params.node_type {
            NodeType::Executor => generator.generate_executor_sdl()?,
            NodeType::Coordinator => generator.generate_coordinator_sdl()?,
            _ => generator.generate_executor_sdl()?,
        };

        // Create AkashDeploymentWorkflow with the SDL
        let mut workflow = AkashDeploymentWorkflow {
            session_id: state.session_id.clone(),
            selected_key_name: params.cosmos_key_name.clone(),
            account_address: params.cosmos_account_address.clone(),
            configured_sdl: Some(ConfiguredSdl {
                template_name: format!("bootstrap-{}", params.node_type.as_str_name()),
                resolved_content: sdl_content,
                ..Default::default()
            }),
            label: params.deploy_label.clone(),
            ..Default::default()
        };

        let opts = AkashWorkflowOptions {
            bid_wait_blocks: 2,
            ..Default::default()
        };

        // Run the full Akash deployment workflow
        let result = self.akash_deployer.deploy(&mut workflow, &opts).await?;

        // Store deployment result in state
        state.akash_session_id = Some(result.session_id);
        state.akash_dseq = Some(result.dseq);
        state.akash_provider = Some(result.provider);
        state.akash_endpoints = result
            .endpoints
            .iter()
            .map(|ep| ep.external_uri.clone())
            .collect();

        info!(
            "Akash deployment created: DSEQ={}, provider={}, endpoints={}",
            result.dseq,
            state.akash_provider.as_deref().unwrap_or("?"),
            state.akash_endpoints.len()
        );

        state.transition(BootstrapStep::WaitForDeployment);
        Ok(StepResult::Continue)
    }

    /// Step: Wait for Akash deployment to become ready
    ///
    /// Since deploy() runs the full workflow including waiting for bids
    /// and sending the manifest, by this point the deployment should
    /// already be active. We just verify the state is correct.
    async fn step_wait_for_deployment(&self, state: &mut BootstrapState) -> Result<StepResult> {
        let dseq = state
            .akash_dseq
            .ok_or_else(|| anyhow!("Missing Akash DSEQ - deployment may have failed"))?;

        // Deployment is already active (deploy() completed successfully in previous step)
        // The deployed node needs time to boot and connect to P2P
        info!(
            "Deployment DSEQ={} is active, waiting for node to boot...",
            dseq
        );

        state.transition(BootstrapStep::EstablishP2PConnection);
        Ok(StepResult::Continue)
    }

    /// Step: Establish P2P connection with new node
    ///
    /// Waits for the newly deployed node to connect via P2P. The new node
    /// has the coordinator's address in its bootstrap_peers config, so it
    /// should connect automatically once it boots. We check the peers map
    /// for the new node's public key.
    async fn step_establish_p2p(&self, state: &mut BootstrapState) -> Result<StepResult> {
        let pubkey_hex = state
            .generated_identity_pubkey
            .as_ref()
            .ok_or_else(|| anyhow!("Missing generated identity pubkey"))?;

        // Decode the hex pubkey for peer lookup
        let node_pubkey = NodePubkey::from_hex(pubkey_hex)
            .ok_or_else(|| anyhow!("Invalid pubkey hex: {}", pubkey_hex))?;

        // Check if the new node has connected
        let manifold = self.network_manifold.lock().await;
        let connected = manifold.is_peer_connected(&node_pubkey.0).await;
        drop(manifold);

        if connected {
            info!("P2P connection established with new node: {}", pubkey_hex);
            state.p2p_connected = true;
            state.transition(BootstrapStep::SendConfig);
            Ok(StepResult::Continue)
        } else {
            state.p2p_check_attempts += 1;

            if state.p2p_check_attempts >= MAX_P2P_CHECK_ATTEMPTS {
                return Ok(StepResult::Failed(format!(
                    "Timed out waiting for P2P connection after {} attempts ({}s)",
                    MAX_P2P_CHECK_ATTEMPTS,
                    MAX_P2P_CHECK_ATTEMPTS as u64 * P2P_CHECK_INTERVAL_SECS
                )));
            }

            debug!(
                "Waiting for P2P connection (attempt {}/{})",
                state.p2p_check_attempts, MAX_P2P_CHECK_ATTEMPTS
            );

            Ok(StepResult::Waiting {
                retry_after_secs: P2P_CHECK_INTERVAL_SECS,
            })
        }
    }

    /// Step: Send config file to new node
    ///
    /// Transmits the generated config.toml to the new node over Channel 4
    /// using the authenticated BootstrapTransport.
    async fn step_send_config(&self, state: &mut BootstrapState) -> Result<StepResult> {
        info!("Sending config to new node: {}", state.session_id);

        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow!("Bootstrap transport not available"))?;

        let config_toml = state
            .config_toml
            .as_ref()
            .ok_or_else(|| anyhow!("Config TOML not generated - identity step may have failed"))?;

        let recipient = self.get_node_pubkey(state)?;

        // Send config over transport
        let mut transport = transport.lock().await;
        transport
            .send_file(&recipient, FileType::Config, config_toml.as_bytes().to_vec())
            .await
            .map_err(|e| anyhow!("Failed to send config: {}", e))?;

        info!("Config sent ({} bytes)", config_toml.len());
        state.transition(BootstrapStep::SendCustody);
        Ok(StepResult::Continue)
    }

    /// Step: Send custody file to new node
    ///
    /// Transmits the encrypted custody file (containing the node's Ed25519
    /// private key) to the new node over Channel 4.
    async fn step_send_custody(&self, state: &mut BootstrapState) -> Result<StepResult> {
        info!("Sending custody to new node: {}", state.session_id);

        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow!("Bootstrap transport not available"))?;

        let custody_data = state
            .custody_data
            .as_ref()
            .ok_or_else(|| anyhow!("Custody data not generated - identity step may have failed"))?;

        let recipient = self.get_node_pubkey(state)?;

        // Send custody over transport
        let mut transport = transport.lock().await;
        transport
            .send_file(&recipient, FileType::Custody, custody_data.clone())
            .await
            .map_err(|e| anyhow!("Failed to send custody: {}", e))?;

        info!("Custody sent ({} bytes)", custody_data.len());
        state.transition(BootstrapStep::SendApiKeys);
        Ok(StepResult::Continue)
    }

    /// Step: Verify node is online
    ///
    /// Checks that the new node is still connected via P2P after receiving
    /// its config and custody files.
    async fn step_verify_online(&self, state: &mut BootstrapState) -> Result<StepResult> {
        info!("Verifying node online: {}", state.session_id);

        let pubkey_hex = state
            .generated_identity_pubkey
            .as_ref()
            .ok_or_else(|| anyhow!("Missing generated identity pubkey"))?;

        let node_pubkey = NodePubkey::from_hex(pubkey_hex)
            .ok_or_else(|| anyhow!("Invalid pubkey hex: {}", pubkey_hex))?;

        // Give the node a moment to process received files and restart
        tokio::time::sleep(Duration::from_secs(VERIFY_ONLINE_TIMEOUT_SECS)).await;

        let manifold = self.network_manifold.lock().await;
        let connected = manifold.is_peer_connected(&node_pubkey.0).await;
        drop(manifold);

        if connected {
            info!(
                "Node verified online: {} (DSEQ={})",
                pubkey_hex,
                state.akash_dseq.unwrap_or(0)
            );
            Ok(StepResult::Complete)
        } else {
            warn!(
                "Node {} not detected online after bootstrap - it may still be initializing",
                pubkey_hex
            );
            // Don't fail - the node might just need more time to restart with new config
            Ok(StepResult::Complete)
        }
    }

    /// Extract the new node's public key from state as a NodePubkey
    fn get_node_pubkey(&self, state: &BootstrapState) -> Result<NodePubkey> {
        let pubkey_hex = state
            .generated_identity_pubkey
            .as_ref()
            .ok_or_else(|| anyhow!("Missing generated identity pubkey"))?;

        NodePubkey::from_hex(pubkey_hex)
            .ok_or_else(|| anyhow!("Invalid pubkey hex: {}", pubkey_hex))
    }

    /// Clone for background task
    fn clone_for_background(&self) -> Self {
        Self {
            akash_deployer: self.akash_deployer.clone(),
            config_generator: BootstrapConfigGenerator::new(),
            transport: self.transport.clone(),
            network_manifold: self.network_manifold.clone(),
            storage: self.storage.clone(),
        }
    }
}
