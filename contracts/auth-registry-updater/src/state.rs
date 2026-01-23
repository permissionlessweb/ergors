use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

/// Contract configuration
pub const CONFIG: Item<Config> = Item::new("config");

/// Map of authorized addresses (address -> bool)
/// Using a Map allows efficient lookup and iteration
pub const AUTHORIZED: Map<&str, bool> = Map::new("authorized");

#[cosmwasm_schema::cw_serde]
pub struct Config {
    /// Admin who can modify the authorized list
    pub admin: Addr,
}
