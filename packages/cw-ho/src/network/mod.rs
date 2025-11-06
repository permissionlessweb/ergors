//! Minimal networking implementation for cw-ho using commonware libraries
//!
//! This module provides a simplified networking layer that avoids the fractal
//! complexity of the previous implementation while maintaining the essential
//! tetrahedral topology.

pub mod config;
pub mod manager;
pub mod topology;

pub use topology::NetworkTopology;
