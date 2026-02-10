//! Chain-specific client modules.
//!
//! Each submodule provides chain-specific client logic:
//! - `cosmos/` - Generic Cosmos SDK client (Akash, Osmosis, Cosmos Hub, etc.)
//! - `eth/` - Ethereum JSON-RPC client

pub mod cosmos;
pub mod eth;
