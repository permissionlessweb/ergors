//! Bootstrap orchestration module.
//!
//! Handles the complete node bootstrap workflow:
//! - HTTP handlers for bootstrap API endpoints
//! - Orchestration of multi-step bootstrap process
//! - State machine for tracking progress
//! - Receiver for bootstrapped nodes

pub mod handlers;
pub mod orchestrator;
pub mod receiver;
pub mod state_machine;

// Re-exports for backward compatibility with server.rs
pub use handlers::{
    handle_bootstrap, handle_bootstrap_status, handle_delete_bootstrap_session,
    handle_list_bootstrap_sessions,
};
