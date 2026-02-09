//! Typed CLI response structs for consistent, machine-readable output.
//!
//! These structs replace ad-hoc `serde_json::json!()` calls to give CLI
//! consumers a stable contract they can deserialize against.

use serde::Serialize;
use std::collections::HashMap;

// ============ Status ============

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub state: String,
    pub uptime_seconds: u64,
    pub storage_status: String,
    pub network_status: String,
    pub connected_peers: u32,
    pub total_requests: u64,
}

#[derive(Debug, Serialize)]
pub struct StatusStoppedResponse {
    pub state: &'static str,
    pub error: String,
}

// ============ Config: List-Chains ============

#[derive(Debug, Serialize)]
pub struct ChainListResponse {
    pub chains: Vec<ChainSummary>,
}

#[derive(Debug, Serialize)]
pub struct ChainSummary {
    pub chain_id: String,
    pub chain_name: String,
    pub bech32_prefix: String,
    pub denom: String,
    pub rpc_endpoint: String,
}

// ============ Keys ============

#[derive(Debug, Serialize)]
pub struct KeyListResponse {
    pub keys: Vec<KeyEntry>,
}

#[derive(Debug, Serialize)]
pub struct KeyEntry {
    pub label: String,
    pub address: String,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
pub struct KeyImportResponse {
    pub label: String,
    pub address: String,
    pub is_default: bool,
}

// ============ Deploy: Shared Types ============

/// Used by Cancel, CloseLease, CloseDeployment, UpdateDeployment,
/// AddProvider, RemoveProvider, ApproveGrant, RevokeGrant, ConfigureProxy
#[derive(Debug, Serialize)]
pub struct OperationResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct WorkflowSummary {
    pub session_id: String,
    pub status: String,
    pub current_step: String,
    pub account_address: String,
}

#[derive(Debug, Serialize)]
pub struct EndpointEntry {
    pub service: String,
    pub uri: String,
    pub internal_port: u32,
    pub external_port: u32,
    pub protocol: String,
}

#[derive(Debug, Serialize)]
pub struct LeaseEntry {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
    pub state: String,
}

// ============ Deploy: Command-Specific Types ============

/// deploy create (run path) + deploy run
#[derive(Debug, Serialize)]
pub struct DeployRunResponse {
    pub session_id: String,
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_required: Option<String>,
}

/// deploy create (fallback when no run happens)
#[derive(Debug, Serialize)]
pub struct DeployCreateResponse {
    pub session_id: String,
    pub status: i32,
    pub current_step: i32,
    pub account_address: String,
    pub chain_id: String,
    pub node_endpoint: String,
}

/// deploy list
#[derive(Debug, Serialize)]
pub struct DeployListResponse {
    pub workflows: Vec<WorkflowSummary>,
    pub total_count: u32,
}

/// deploy get
#[derive(Debug, Serialize)]
pub struct DeployGetResponse {
    pub session_id: String,
    pub status: String,
    pub current_step: String,
    pub account_address: String,
    pub chain_id: String,
    pub node_endpoint: String,
    pub selected_key_name: String,
    pub last_error: String,
    pub retry_count: u32,
}

/// deploy info
#[derive(Debug, Serialize)]
pub struct DeployInfoResponse {
    pub session_id: String,
    pub status: String,
    pub current_step: String,
    pub account_address: String,
    pub chain_id: String,
    pub node_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<DeploymentRuntime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease: Option<LeaseEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<EndpointEntry>>,
}

#[derive(Debug, Serialize)]
pub struct DeploymentRuntime {
    pub dseq: u64,
    pub provider: String,
    pub lease_id: String,
}

/// deploy bids
#[derive(Debug, Serialize)]
pub struct DeployBidsResponse {
    pub bids: Vec<BidEntry>,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct BidEntry {
    pub provider: String,
    pub price_uakt: u64,
}

/// deploy select
#[derive(Debug, Serialize)]
pub struct DeploySelectResponse {
    pub success: bool,
    pub provider: String,
    pub price_uakt: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// deploy endpoints
#[derive(Debug, Serialize)]
pub struct DeployEndpointsResponse {
    pub session_id: String,
    pub endpoints: Vec<EndpointEntry>,
    pub total: usize,
}

/// deploy set-endpoints
#[derive(Debug, Serialize)]
pub struct DeploySetEndpointsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// deploy request-grant
#[derive(Debug, Serialize)]
pub struct DeployGrantRequestResponse {
    pub success: bool,
    pub message: String,
    pub request_id: String,
}

/// deploy list-grants
#[derive(Debug, Serialize)]
pub struct DeployGrantListResponse {
    pub requests: Vec<GrantRequestEntry>,
}

#[derive(Debug, Serialize)]
pub struct GrantRequestEntry {
    pub request_id: String,
    pub granter: String,
    pub grantee: String,
    pub msg_types: Vec<String>,
    pub allowance: u64,
    pub status: String,
}

/// deploy query-balance
#[derive(Debug, Serialize)]
pub struct DeployBalanceResponse {
    pub address: String,
    pub denom: String,
    pub amount: String,
}

/// deploy topup-escrow
#[derive(Debug, Serialize)]
pub struct DeployTopupResponse {
    pub success: bool,
    pub message: String,
    pub amount_uakt: u64,
}

/// deploy status
#[derive(Debug, Serialize)]
pub struct DeployStatusResponse {
    pub deployment_status: String,
    pub balance_remaining_uakt: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease: Option<LeaseEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<EndpointEntry>>,
}

// ============ CLI Key Management ============

#[derive(Debug, Serialize)]
pub struct CliKeyListJsonResponse {
    pub keys: Vec<CliKeyEntryJson>,
}

#[derive(Debug, Serialize)]
pub struct CliKeyEntryJson {
    pub public_key_hex: String,
    pub label: String,
}

/// deploy trusted-providers
#[derive(Debug, Serialize)]
pub struct TrustedProvidersResponse {
    pub providers: Vec<TrustedProviderEntry>,
}

#[derive(Debug, Serialize)]
pub struct TrustedProviderEntry {
    pub address: String,
    pub label: String,
}
