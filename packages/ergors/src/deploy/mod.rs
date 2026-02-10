//! Deployment module.
//!
//! Organized into submodules:
//! - `akash/` - Akash Network deployment operations
//! - `bootstrap/` - Node bootstrap orchestration
//! - `traits` - Generic deployment provider abstraction
//! - `eth_*` - Ethereum integration (future)
//! - Provider stubs: docker, ec2, linux, phala (future)

// === Active submodules ===
pub mod akash;
pub mod bootstrap;
pub mod traits;

// === Bootstrap (old top-level files, kept for backward compat) ===
pub mod orchestrator;
pub mod state_machine;

// === Stubs / future providers ===
pub mod docker;
pub mod ec2;
pub mod linux;
pub mod phala;
pub mod state_ext;

// === Re-exports for backward compatibility ===
pub use orchestrator::{BootstrapOrchestrator, NodeBootstrapParams};
pub use bootstrap::receiver::BootstrapReceiver;
pub use state_machine::{BootstrapState, BootstrapStep, StepResult};
pub use traits::{DeploymentMetadata, DeploymentProvider};

// Re-export from akash for backward compat (used by server.rs, lib.rs, etc.)
pub use akash::deployer::AutomatedDeployer;
pub use akash::client::AkashClient;
pub use akash::cache_refresher;
pub use akash::climb_signer;
pub use akash::endpoint_manager;
pub use akash::grant_inbox;
pub use akash::granter;
pub use akash::authz;
pub use akash::sdl;
pub use akash::reputation;
pub use akash::requester;
pub use akash::deployment_builder;
pub use akash::manifest;
pub use akash::node_sdl;
pub use akash::workflow;
