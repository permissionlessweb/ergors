use cosmwasm_schema::{cw_serde, QueryResponses};

/// Message to instantiate the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The coordinator's public key (hex encoded) - automatically authorized
    pub coordinator: String,
    /// Optional list of additional addresses to authorize initially
    pub initial_authorized: Option<Vec<String>>,
}

/// Execute messages for the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Add an address to the authorized list (admin only)
    AddAuthorized { address: String },
    /// Remove an address from the authorized list (admin only)
    RemoveAuthorized { address: String },
    /// Transfer admin privileges (admin only)
    TransferAdmin { new_admin: String },
}

/// Query messages for the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Check if an address is authorized to update the registry
    /// This is the primary query used by the auth middleware
    #[returns(IsAuthorizedResponse)]
    IsAuthorized { address: String },

    /// Get the admin address
    #[returns(AdminResponse)]
    GetAdmin {},

    /// List all authorized addresses
    #[returns(ListAuthorizedResponse)]
    ListAuthorized {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    /// Get contract info
    #[returns(InfoResponse)]
    GetInfo {},
}

/// Response for IsAuthorized query
/// JSON format: {"authorized": true|false}
#[cw_serde]
pub struct IsAuthorizedResponse {
    pub authorized: bool,
}

/// Response for GetAdmin query
#[cw_serde]
pub struct AdminResponse {
    pub admin: String,
}

/// Response for ListAuthorized query
#[cw_serde]
pub struct ListAuthorizedResponse {
    pub addresses: Vec<String>,
}

/// Response for GetInfo query
#[cw_serde]
pub struct InfoResponse {
    pub admin: String,
    pub authorized_count: u64,
    pub contract_address: String,
}
