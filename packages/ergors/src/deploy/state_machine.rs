//! Bootstrap State Machine
//!
//! Tracks the multi-step bootstrap workflow through all stages.
//! Provides state persistence so failed bootstraps can be retried or cleaned up.

use chrono::{DateTime, Utc};
use ho_std::types::ergors::network::v1::NodeType;
use serde::{Deserialize, Serialize};

/// Bootstrap workflow step
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BootstrapStep {
    /// Initial state - not started
    Init,
    /// Generating Ed25519 identity for new node
    GenerateIdentity,
    /// Building Docker image (optional, may use pre-built)
    BuildDockerImage,
    /// Creating Akash deployment
    CreateAkashDeployment,
    /// Waiting for Akash deployment to become ready
    WaitForDeployment,
    /// Establishing P2P connection with new node
    EstablishP2PConnection,
    /// Sending config.toml file
    SendConfig,
    /// Sending encrypted custody file
    SendCustody,
    /// Sending API keys (optional)
    SendApiKeys,
    /// Verifying node is online and functional
    VerifyNodeOnline,
    /// Bootstrap completed successfully
    Complete,
    /// Bootstrap failed
    Failed { reason: String },
}

impl std::fmt::Display for BootstrapStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init => write!(f, "Initializing"),
            Self::GenerateIdentity => write!(f, "Generating identity"),
            Self::BuildDockerImage => write!(f, "Building Docker image"),
            Self::CreateAkashDeployment => write!(f, "Creating Akash deployment"),
            Self::WaitForDeployment => write!(f, "Waiting for deployment"),
            Self::EstablishP2PConnection => write!(f, "Establishing P2P connection"),
            Self::SendConfig => write!(f, "Sending configuration"),
            Self::SendCustody => write!(f, "Sending custody file"),
            Self::SendApiKeys => write!(f, "Sending API keys"),
            Self::VerifyNodeOnline => write!(f, "Verifying node online"),
            Self::Complete => write!(f, "Complete"),
            Self::Failed { reason } => write!(f, "Failed: {}", reason),
        }
    }
}

/// Complete bootstrap state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    /// Unique session ID
    pub session_id: String,
    /// Current workflow step
    pub step: BootstrapStep,
    /// Target node type being bootstrapped
    pub target_node_type: NodeType,
    /// Docker image tag (if using Docker deployment)
    pub docker_image_tag: Option<String>,
    /// Generated node identity (public key hex)
    pub generated_identity_pubkey: Option<String>,
    /// Generated config TOML content for the new node
    pub config_toml: Option<String>,
    /// Generated encrypted custody data for the new node
    pub custody_data: Option<Vec<u8>>,
    /// Bootstrap password used for custody encryption
    pub custody_password: Option<String>,
    /// Akash deployment session ID
    pub akash_session_id: Option<String>,
    /// Akash deployment DSEQ
    pub akash_dseq: Option<u64>,
    /// Akash provider address
    pub akash_provider: Option<String>,
    /// Service endpoints from Akash deployment
    pub akash_endpoints: Vec<String>,
    /// P2P connection established flag
    pub p2p_connected: bool,
    /// Bootstrap peer address (coordinator's address)
    pub bootstrap_peer: Option<String>,
    /// Number of P2P connection check attempts
    pub p2p_check_attempts: u32,
    /// Error messages from failed steps
    pub errors: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl BootstrapState {
    /// Create a new bootstrap state
    pub fn new(session_id: String, target_node_type: NodeType) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            step: BootstrapStep::Init,
            target_node_type,
            docker_image_tag: None,
            generated_identity_pubkey: None,
            config_toml: None,
            custody_data: None,
            custody_password: None,
            akash_session_id: None,
            akash_dseq: None,
            akash_provider: None,
            akash_endpoints: Vec::new(),
            p2p_connected: false,
            bootstrap_peer: None,
            p2p_check_attempts: 0,
            errors: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Transition to a new step
    pub fn transition(&mut self, next: BootstrapStep) {
        self.step = next;
        self.updated_at = Utc::now();
    }

    /// Add an error message
    pub fn add_error(&mut self, error: String) {
        self.errors.push(format!(
            "[{}] {}",
            self.updated_at.format("%Y-%m-%d %H:%M:%S"),
            error
        ));
        self.updated_at = Utc::now();
    }

    /// Mark as failed with reason
    pub fn fail(&mut self, reason: String) {
        self.add_error(reason.clone());
        self.step = BootstrapStep::Failed { reason };
    }

    /// Check if bootstrap is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.step,
            BootstrapStep::Complete | BootstrapStep::Failed { .. }
        )
    }

    /// Check if bootstrap is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.step, BootstrapStep::Complete)
    }

    /// Check if bootstrap failed
    pub fn is_failed(&self) -> bool {
        matches!(self.step, BootstrapStep::Failed { .. })
    }

    /// Get a human-readable status string
    pub fn status_string(&self) -> String {
        format!("{}", self.step)
    }
}

/// Result of advancing one step in the state machine
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Step completed, ready for next step
    Continue,
    /// Step completed, bootstrap finished
    Complete,
    /// Step failed
    Failed(String),
    /// Step requires waiting (e.g., for deployment)
    Waiting { retry_after_secs: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let state = BootstrapState::new("test-123".to_string(), NodeType::Executor);

        assert_eq!(state.session_id, "test-123");
        assert_eq!(state.step, BootstrapStep::Init);
        assert_eq!(state.target_node_type, NodeType::Executor);
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_transition() {
        let mut state = BootstrapState::new("test".to_string(), NodeType::Executor);
        let initial_time = state.updated_at;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        state.transition(BootstrapStep::GenerateIdentity);

        assert_eq!(state.step, BootstrapStep::GenerateIdentity);
        assert!(state.updated_at > initial_time);
    }

    #[test]
    fn test_fail() {
        let mut state = BootstrapState::new("test".to_string(), NodeType::Executor);
        state.fail("Something went wrong".to_string());

        assert!(state.is_failed());
        assert!(state.is_terminal());
        assert_eq!(state.errors.len(), 1);
    }

    #[test]
    fn test_complete() {
        let mut state = BootstrapState::new("test".to_string(), NodeType::Executor);
        state.transition(BootstrapStep::Complete);

        assert!(state.is_complete());
        assert!(state.is_terminal());
    }

    #[test]
    fn test_add_error() {
        let mut state = BootstrapState::new("test".to_string(), NodeType::Executor);
        state.add_error("Error 1".to_string());
        state.add_error("Error 2".to_string());

        assert_eq!(state.errors.len(), 2);
        assert!(state.errors[0].contains("Error 1"));
        assert!(state.errors[1].contains("Error 2"));
    }
}
