//! Mock-specific types for testing.
//!
//! These types complement the proto-generated types from ho-std for test scenarios.

/// Grant acceptance mode for coordinator behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrantAcceptanceMode {
    /// Automatically approve all grant requests
    AcceptAll,
    /// Automatically reject all grant requests
    RejectAll,
    /// Only approve requests from whitelisted pubkeys
    Whitelist,
    /// Require manual approval (default)
    #[default]
    Manual,
}

/// Status of a grant request in the mock system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantRequestStatus {
    /// Request created, awaiting granter decision
    Pending,
    /// Granter approved, grants being created
    Approved,
    /// Grants created and confirmed on mock chain
    Confirmed,
    /// Granter rejected the request
    Rejected,
    /// Requester cancelled
    Cancelled,
    /// Request expired before approval
    Expired,
}

/// State of a grant request for testing.
#[derive(Debug, Clone)]
pub struct GrantRequestState {
    pub request_id: String,
    pub granter_address: String,
    pub grantee_address: String,
    pub msg_types: Vec<String>,
    pub spend_limit_uakt: u64,
    pub duration_seconds: u64,
    pub status: GrantRequestStatus,
    pub created_at_unix: u64,
    pub rejection_reason: Option<String>,
}

/// Authz grant record in mock chain.
#[derive(Debug, Clone)]
pub struct MockAuthzGrant {
    pub granter: String,
    pub grantee: String,
    pub msg_type_url: String,
    pub expiration_unix: u64,
    pub active: bool,
}

/// Feegrant allowance record in mock chain.
#[derive(Debug, Clone)]
pub struct MockFeegrant {
    pub granter: String,
    pub grantee: String,
    pub spend_limit_uakt: u64,
    pub spent_uakt: u64,
    pub expiration_unix: u64,
    pub allowed_messages: Vec<String>,
    pub active: bool,
}

/// Mock account in the simulated chain.
#[derive(Debug, Clone)]
pub struct MockAccount {
    pub address: String,
    pub balances: std::collections::HashMap<String, u64>,
    pub pubkey: Vec<u8>,
}

impl MockAccount {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            balances: std::collections::HashMap::new(),
            pubkey: Vec::new(),
        }
    }

    pub fn with_balance(mut self, denom: impl Into<String>, amount: u64) -> Self {
        self.balances.insert(denom.into(), amount);
        self
    }
}

/// Simulated Akash deployment info.
#[derive(Debug, Clone)]
pub struct MockDeployment {
    pub dseq: u64,
    pub owner: String,
    pub state: DeploymentState,
    pub created_at_unix: u64,
}

/// Simulated deployment state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentState {
    Open,
    Active,
    Closed,
}

/// Simulated bid info.
#[derive(Debug, Clone)]
pub struct MockBid {
    pub dseq: u64,
    pub provider: String,
    pub price_uakt: u64,
    pub state: BidState,
}

/// Simulated bid state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidState {
    Open,
    Matched,
    Closed,
}

/// Simulated lease info.
#[derive(Debug, Clone)]
pub struct MockLease {
    pub dseq: u64,
    pub provider: String,
    pub owner: String,
    pub price_uakt: u64,
    pub state: LeaseState,
}

/// Simulated lease state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    Active,
    Closed,
}

/// Akash message type URLs for authz grants.
pub mod msg_types {
    pub const MSG_CREATE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgCreateDeployment";
    pub const MSG_UPDATE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgUpdateDeployment";
    pub const MSG_CLOSE_DEPLOYMENT: &str = "/akash.deployment.v1beta3.MsgCloseDeployment";
    pub const MSG_CREATE_LEASE: &str = "/akash.market.v1beta4.MsgCreateLease";
    pub const MSG_CLOSE_BID: &str = "/akash.market.v1beta4.MsgCloseBid";

    /// Default message types for deployment workflow.
    pub fn deployment_msg_types() -> Vec<String> {
        vec![
            MSG_CREATE_DEPLOYMENT.to_string(),
            MSG_UPDATE_DEPLOYMENT.to_string(),
            MSG_CLOSE_DEPLOYMENT.to_string(),
            MSG_CREATE_LEASE.to_string(),
        ]
    }
}
