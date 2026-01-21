//! gRPC server for CLI-to-engine management
//!
//! Implements the ManagementService for engine control and monitoring.

pub mod auth;
pub mod management;

pub use auth::{create_auth_interceptor, simple_auth_interceptor, TokenStore};
pub use management::{start_grpc_server, ManagementServiceImpl};
