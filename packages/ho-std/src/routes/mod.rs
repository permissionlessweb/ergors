//! Route definitions and configuration for CW-HO servers
//!
//! This module provides a centralized way to define and manage API routes
//! with support for authentication middleware and proto-based type/value patterns.
//!
//! Routes are defined using proto-generated request/response types for standardized
//! transport layer communication.

pub mod auth;
pub mod config;

pub use auth::{AuthError, AuthLayer};
pub use config::{RouteDefinition, RouteRegistry};

// Re-export the macro
pub use crate::define_routes;
