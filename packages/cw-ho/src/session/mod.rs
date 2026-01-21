//! Session module for fractal session management
//!
//! This module provides the core session management functionality for ERGORS nodes,
//! implementing a fractal (self-similar) session hierarchy with parent/child relationships
//! and cross-node coordination.
//!
//! ## Submodules
//!
//! - `manager` - Core SessionManager implementation
//! - `orchestrator` - Integration helpers for CosmicOrchestrator

pub mod manager;
pub mod orchestrator;

pub use manager::{SessionManager, SessionManagerConfig};
pub use orchestrator::OrchestratorSessionHelper;
