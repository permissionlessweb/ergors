//! Route definitions and configuration for CW-HO servers
//!
//! This module provides a centralized way to define and manage API routes
//! with support for authentication middleware and nested router patterns.

pub mod auth;
pub mod config;

pub use auth::{auth_middleware, AuthConfig, AuthError};
pub use config::{configure_routes, routes, Route};

// Re-export the macro
pub use crate::create_routes;
