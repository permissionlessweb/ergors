//! Akash-specific types for deployments, bids, leases, and escrow accounts.

/// Deployment information from Akash chain.
#[derive(Debug, Clone)]
pub struct DeploymentInfo {
    pub owner: String,
    pub dseq: u64,
    pub state: DeploymentState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentState {
    Invalid,
    Active,
    Closed,
}

/// Bid information from Akash market.
#[derive(Debug, Clone)]
pub struct BidInfo {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
    pub bseq: u32,
    pub price_denom: String,
    pub price_amount: String,
    pub state: BidState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BidState {
    Invalid,
    Open,
    Active,
    Lost,
    Closed,
}

/// Lease information from Akash market.
#[derive(Debug, Clone)]
pub struct LeaseInfo {
    pub owner: String,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
    pub provider: String,
    pub state: LeaseState,
    pub price_denom: String,
    pub price_amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseState {
    Invalid,
    Active,
    InsufficientFunds,
    Closed,
}

/// Escrow account information for deployments.
#[derive(Debug, Clone)]
pub struct EscrowAccountInfo {
    pub owner: String,
    pub dseq: u64,
    pub state: EscrowState,
    pub balances: Vec<EscrowBalance>,
    pub total_uakt: u64,
}

/// Escrow balance entry.
#[derive(Debug, Clone)]
pub struct EscrowBalance {
    pub denom: String,
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscrowState {
    Invalid,
    Open,
    Closed,
    Overdrawn,
}

/// Provider information from Akash provider registry.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub owner: String,
    pub host_uri: String,
    pub email: String,
    pub website: String,
}
