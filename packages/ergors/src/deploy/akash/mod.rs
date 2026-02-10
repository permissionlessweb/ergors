//! Akash Network deployment module.
//!
//! All Akash-specific deployment logic lives here:
//! - Chain queries (deployments, bids, leases, escrow, certificates)
//! - SDL generation and deployment building
//! - Manifest management and provider communication
//! - Automated deployment orchestration
//! - Escrow cache management
//! - Authorization and grant management
//! - Provider reputation scoring
//! - Endpoint management with failover

pub mod api_client;
pub mod authz;
pub mod cache_refresher;
pub mod climb_signer;
pub mod client;
pub mod deployer;
pub mod deployment_builder;
pub mod endpoint_manager;
pub mod grant_inbox;
pub mod granter;
pub mod manifest;
pub mod messages;
pub mod node_sdl;
pub mod reputation;
pub mod requester;
pub mod sdl;
pub mod types;
pub mod workflow;

// Re-exports for convenience
pub use client::AkashClient;
pub use deployer::AutomatedDeployer;
pub use messages::{broadcast_akash_msg, broadcast_akash_msgs, msg_types};
pub use types::*;
