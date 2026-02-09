//! Contract-based authentication middleware for ERGORS endpoints.
//!
//! This module provides:
//! - CLI commands for user authentication management
//! - Tower middleware layer that integrates with CosmWasm contracts for
//!   flexible, programmable authentication for API endpoints.
//!
//! ## How it works
//!
//! 1. When a request arrives at a protected endpoint, the middleware checks if
//!    a custom authenticator contract is registered for that endpoint.
//!
//! 2. If a contract is registered, it queries the contract with the caller's
//!    address to determine authorization.
//!
//! 3. If no contract is registered, it falls back to the standard Ed25519
//!    signature-based authentication.
//!
//! ## Contract Query Format
//!
//! The middleware expects contracts to implement this query interface:
//!
//! ```json
//! {"is_allowed": {"address": "ergors{node_id}_{pubkey_hash}"}}
//! ```
//!
//! Expected response:
//!
//! ```json
//! {"allowed": true|false}
//! ```

pub mod handlers;
pub mod middleware;
pub mod operation_recorder;
pub use operation_recorder::record_operation;
pub mod grpc;


pub use handlers::{
    handle_check_authorization, handle_delete_authenticator, handle_list_authenticators,
    handle_register_authenticator,
};
pub use middleware::contract_auth_middleware;

use serde::{Deserialize, Serialize};
use anyhow::Result;
use camino::Utf8Path;

// ============================
// CLI Commands for Auth
// ============================

/// CLI command for user authentication management
#[derive(Debug, clap::Parser)]
pub struct AuthCmd {
    #[clap(subcommand)]
    pub subcmd: AuthTopSubCmd,
    /// base-64 encoded json of authentication structure
    #[clap(display_order = 200)]
    pub auth: String,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum AuthTopSubCmd {
    /// register a user key pair for permissioned api access
    #[clap(display_order = 100)]
    Register {},
    /// revoke a user key pair for permissioned api access
    #[clap(display_order = 200)]
    Revoke {},
}

impl AuthCmd {
    pub fn exec(&self, _home_dir: &Utf8Path) -> Result<()> {
        match self.subcmd.clone() {
            AuthTopSubCmd::Register {} => {}
            AuthTopSubCmd::Revoke {} => {}
        };
        Ok(())
    }
}

// ============================
// Contract Query/Response Types
// ============================

/// Query message sent to authenticator contracts
#[derive(Serialize)]
pub struct IsAllowedQuery {
    pub is_allowed: IsAllowedQueryInner,
}

#[derive(Serialize)]
pub struct IsAllowedQueryInner {
    pub address: String,
}

/// Response from authenticator contracts
#[derive(Deserialize)]
pub struct IsAllowedResponse {
    pub allowed: bool,
}

impl IsAllowedQuery {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            is_allowed: IsAllowedQueryInner {
                address: address.into(),
            },
        }
    }
}
