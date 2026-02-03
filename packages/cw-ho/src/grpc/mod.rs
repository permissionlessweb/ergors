//! gRPC server for CLI-to-engine management
//!
//! Implements the ManagementService for engine control and monitoring.

pub mod auth;
pub mod doc_loader;
pub mod management;
pub mod rlm_docs;

pub use auth::{create_auth_interceptor, simple_auth_interceptor, TokenStore};
pub use doc_loader::load_documents_by_prefix;
pub use management::{start_grpc_server, ManagementServiceImpl};
pub use rlm_docs::RlmDocService;
