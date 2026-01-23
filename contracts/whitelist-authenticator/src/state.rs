use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

/// Contract configuration
pub const CONFIG: Item<Config> = Item::new("config");

/// Whitelist map (address -> bool)
pub const WHITELIST: Map<&str, bool> = Map::new("whitelist");

#[cosmwasm_schema::cw_serde]
pub struct Config {
    /// Admin who can modify the whitelist
    pub admin: Addr,
    /// Optional description of this authenticator
    pub description: Option<String>,
    /// Whether to allow all addresses by default
    /// If true: whitelist acts as blocklist (default allow, whitelist = blocked)
    /// If false: whitelist acts as allowlist (default deny, whitelist = allowed)
    pub default_allow: bool,
}
