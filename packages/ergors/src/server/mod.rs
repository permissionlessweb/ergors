//! Server module.
//!
//! Contains the HTTP/gRPC server, daemon lifecycle, network management,
//! and session management.

mod server;
pub mod daemon;
pub mod network;
pub mod session_manager;
pub mod storage;

// Re-export server internals at this level
pub use server::*;
