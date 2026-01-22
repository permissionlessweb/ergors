//! Minimal networking implementation for ergors using commonware libraries
//!
//! This module provides a simplified networking layer that avoids the fractal
//! complexity of the previous implementation while maintaining the essential
//! tetrahedral topology.
//!
//! ## Network Channels
//!
//! - Channel 0: Discovery - Peer discovery and announcement
//! - Channel 1: Tasks - Task orchestration messages
//! - Channel 2: State - State synchronization
//! - Channel 3: Health - Health checks and heartbeats
//! - Channel 4: Key Sharing - Secure API key distribution

pub mod deploy;
pub mod key_sharing;
pub mod manager;
pub mod topology;

pub use key_sharing::{KeySharingHandler, KEY_SHARING_CHANNEL};
