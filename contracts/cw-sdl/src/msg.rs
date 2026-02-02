use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Binary;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Message to instantiate the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The SDL template as a JSON string
    pub sdl_template: String,
    /// Default values for template variables (key-value pairs)
    pub variable_defaults: HashMap<String, String>,
    /// Optional label for this SDL template
    pub label: Option<String>,
    /// Optional admin address who can update the template
    pub admin: Option<String>,
}

/// Execute messages for the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Update the SDL template (admin only)
    UpdateTemplate {
        sdl_template: String,
    },
    /// Update variable defaults (admin only)
    UpdateDefaults {
        variable_defaults: HashMap<String, String>,
    },
    /// Update a single variable default (admin only)
    UpdateSingleDefault {
        key: String,
        value: String,
    },
    /// Remove a variable default (admin only)
    RemoveDefault {
        key: String,
    },
    /// Update the contract label (admin only)
    UpdateLabel {
        label: String,
    },
    /// Transfer admin privileges (admin only)
    TransferAdmin {
        new_admin: String,
    },
    /// Factory method to instantiate a new contract from the same code ID
    /// parent_results: deployment results from parent to merge into child's variable_defaults
    InstantiateNew {
        instantiate_msg: InstantiateMsg,
        label: String,
        parent_results: Option<HashMap<String, String>>,
    },
    /// Record a single deployment result (peer ID, endpoint, etc.)
    RecordDeploymentResult {
        key: String,
        value: String,
    },
    /// Record multiple deployment results in one call
    RecordDeploymentResults {
        results: HashMap<String, String>,
    },
}

/// Query messages for the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get the SDL template
    #[returns(TemplateResponse)]
    GetTemplate {},

    /// Get all variable defaults
    #[returns(DefaultsResponse)]
    GetDefaults {},

    /// Get a specific variable default
    #[returns(SingleDefaultResponse)]
    GetDefault { key: String },

    /// Get contract info (label, admin, etc.)
    #[returns(InfoResponse)]
    GetInfo {},

    /// List all variable keys
    #[returns(KeysResponse)]
    ListKeys {},

    /// Get the rendered SDL with provided values (falls back to defaults)
    #[returns(RenderedSdlResponse)]
    RenderSdl {
        variables: Option<HashMap<String, String>>,
    },

    /// Get the rendered SDL as parsed JSON with provided values (falls back to defaults)
    #[returns(RenderedJsonResponse)]
    GetRenderedJson {
        variables: Option<HashMap<String, String>>,
    },

    /// Get a specific deployment result
    #[returns(DeploymentResultResponse)]
    GetDeploymentResult { key: String },

    /// List all deployment results
    #[returns(DeploymentResultsResponse)]
    ListDeploymentResults {},

    /// List all child contracts created via factory
    #[returns(ChildContractsResponse)]
    ListChildContracts {},
}

/// Response for GetTemplate query
#[cw_serde]
pub struct TemplateResponse {
    pub sdl_template: String,
    pub template_json: JsonValue,
}

/// Response for GetDefaults query
#[cw_serde]
pub struct DefaultsResponse {
    pub defaults: HashMap<String, String>,
}

/// Response for GetDefault query
#[cw_serde]
pub struct SingleDefaultResponse {
    pub key: String,
    pub value: Option<String>,
}

/// Response for GetInfo query
#[cw_serde]
pub struct InfoResponse {
    pub label: Option<String>,
    pub admin: Option<String>,
    pub code_id: u64,
}

/// Response for ListKeys query
#[cw_serde]
pub struct KeysResponse {
    pub keys: Vec<String>,
}

/// Response for RenderSdl query
#[cw_serde]
pub struct RenderedSdlResponse {
    pub rendered_sdl: String,
    pub used_variables: HashMap<String, String>,
}

/// Response for InstantiateNew execution
#[cw_serde]
pub struct InstantiateNewResponse {
    pub contract_address: String,
}

/// Response for GetRenderedJson query
#[cw_serde]
pub struct RenderedJsonResponse {
    /// The SDL template rendered as a parsed JSON value with variables substituted
    pub sdl_json: JsonValue,
    /// Variables that were used in rendering
    pub used_variables: HashMap<String, String>,
}

/// Response for GetDeploymentResult query
#[cw_serde]
pub struct DeploymentResultResponse {
    pub key: String,
    pub value: Option<String>,
}

/// Response for ListDeploymentResults query
#[cw_serde]
pub struct DeploymentResultsResponse {
    pub results: HashMap<String, String>,
}

/// Response for ListChildContracts query
#[cw_serde]
pub struct ChildContractsResponse {
    pub contracts: HashMap<String, String>,
}
