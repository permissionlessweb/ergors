use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Contract configuration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Optional label for this SDL template
    pub label: Option<String>,
    /// Optional admin address who can update the template
    pub admin: Option<Addr>,
}

/// The SDL template stored as a string
pub const SDL_TEMPLATE: Item<String> = Item::new("sdl_template");

/// Contract configuration
pub const CONFIG: Item<Config> = Item::new("config");

/// Variable defaults stored as key-value pairs
pub const VARIABLE_DEFAULTS: Map<&str, String> = Map::new("variable_defaults");

/// Deployment results from Akash deployments (peer IDs, endpoints, etc.)
/// Stores key-value pairs like {"NODE_A_PEER_ID": "abc123@1.2.3.4:26656"}
pub const DEPLOYMENT_RESULTS: Map<&str, String> = Map::new("deployment_results");

/// Child contracts created via factory pattern, tracked by label
pub const CHILD_CONTRACTS: Map<&str, Addr> = Map::new("child_contracts");
