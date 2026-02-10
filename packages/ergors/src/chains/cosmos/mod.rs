//! Generic Cosmos SDK client library.
//!
//! Provides chain-agnostic operations for any Cosmos SDK blockchain.
//! Use this for Akash, Osmosis, Cosmos Hub, Juno, or any custom Cosmos chain.
//!
//! # Example
//!
//! ```no_run
//! use ergors::chains::cosmos::{ChainConfig, CosmosBaseClient};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create a client for Akash
//! let config = ChainConfig::akash();
//! let client = CosmosBaseClient::new(config)?;
//!
//! // Query balance
//! let balance = client.query_balance("akash1...", "uakt").await?;
//! # Ok(())
//! # }
//! ```

pub mod broadcaster;
pub mod client;
pub mod signer;
pub mod tx_lifecycle;
pub mod types;

// Re-exports
pub use broadcaster::{CosmosBroadcaster, BroadcastResponse, TxResponse};
pub use client::CosmosBaseClient;
pub use signer::{CosmosSigner, msg_to_any};
pub use types::ChainConfig;
