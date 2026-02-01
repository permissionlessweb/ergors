use cosmwasm_schema::{cw_serde, QueryResponses};

/// Message to instantiate the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The admin who can modify the whitelist (usually node operator key)
    pub admin: String,
    /// Optional description of this authenticator
    pub description: Option<String>,
    /// Optional list of addresses to whitelist initially
    pub initial_whitelist: Option<Vec<String>>,
    /// Whether to allow all addresses by default (open policy)
    /// If true, the whitelist acts as a blocklist instead
    pub default_allow: Option<bool>,
}

/// Execute messages for the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Add an address to the whitelist (admin only)
    AddAddress { address: String },
    /// Remove an address from the whitelist (admin only)
    RemoveAddress { address: String },
    /// Batch add addresses (admin only)
    BatchAdd { addresses: Vec<String> },
    /// Batch remove addresses (admin only)
    BatchRemove { addresses: Vec<String> },
    /// Update the description (admin only)
    UpdateDescription { description: String },
    /// Set the default policy (admin only)
    SetDefaultPolicy { allow: bool },
    /// Transfer admin privileges (admin only)
    TransferAdmin { new_admin: String },
}

/// Query messages for the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Check if an address is allowed (primary query for auth middleware)
    /// JSON: {"is_allowed": {"address": "ergors..."}}
    /// Response: {"allowed": true|false}
    #[returns(IsAllowedResponse)]
    IsAllowed { address: String },

    /// Get the admin address
    #[returns(AdminResponse)]
    GetAdmin {},

    /// List all whitelisted addresses
    #[returns(ListWhitelistResponse)]
    ListWhitelist {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    /// Get contract configuration
    #[returns(ConfigResponse)]
    GetConfig {},

    /// Check multiple addresses at once
    #[returns(BatchCheckResponse)]
    BatchCheck { addresses: Vec<String> },
}

/// Response for IsAllowed query
/// This is the primary response format for auth middleware
#[cw_serde]
pub struct IsAllowedResponse {
    pub allowed: bool,
}

/// Response for GetAdmin query
#[cw_serde]
pub struct AdminResponse {
    pub admin: String,
}

/// Response for ListWhitelist query
#[cw_serde]
pub struct ListWhitelistResponse {
    pub addresses: Vec<String>,
    pub total: u64,
}

/// Response for GetConfig query
#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub description: Option<String>,
    pub default_allow: bool,
    pub whitelist_count: u64,
}

/// Response for BatchCheck query
#[cw_serde]
pub struct BatchCheckResponse {
    pub results: Vec<AddressCheckResult>,
}

#[cw_serde]
pub struct AddressCheckResult {
    pub address: String,
    pub allowed: bool,
}
