//! Akash Deployment Integration Testing Suite
//!
//! This module provides a comprehensive testing framework for validating
//! the ERGORS Akash deployment workflow using Akash's official Kind-based
//! development environment.
//!
//! ## Components
//!
//! - [`AkashDevEnvironment`] - Docker/Kind cluster lifecycle management
//! - [`MockInferenceProvider`] - Simulated inference workload container
//! - [`TestWalletManager`] - Pre-funded test account management
//! - [`NetworkTopology`] - Multi-node ERGORS network simulation
//! - [`TestNetworkOrchestrator`] - Multi-node ERGORS instance orchestration
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ergors::deploy::testing::prelude::*;
//!
//! #[tokio::test]
//! async fn test_e2e_deployment_workflow() {
//!     // Start ERGORS node network
//!     let mut orchestrator = TestNetworkOrchestrator::new();
//!     orchestrator.init().await.unwrap();
//!     orchestrator.start_all().await.unwrap();
//!
//!     // Start Akash dev environment
//!     let env = AkashDevEnvironment::start().await.unwrap();
//!
//!     // Deploy mock inference provider through ERGORS workflow
//!     let coordinator = orchestrator.coordinator().await.unwrap();
//!     let executor = orchestrator.executors().await[0].clone();
//!
//!     // Executor requests grant from coordinator
//!     // Coordinator approves and provides deployment funds
//!     // Executor deploys to Akash via the workflow
//!
//!     orchestrator.cleanup().await.unwrap();
//! }
//! ```

pub mod environment;
pub mod mock_inference;
pub mod network;
pub mod orchestrator;
pub mod wallet;

pub mod prelude {
    pub use super::environment::*;
    pub use super::mock_inference::*;
    pub use super::network::*;
    pub use super::orchestrator::*;
    pub use super::wallet::*;
}

pub use prelude::*;
