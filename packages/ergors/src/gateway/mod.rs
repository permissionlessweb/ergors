//! Gateway module for communication interfaces.
//!
//! Provides modular support for different communication platforms
//! (Discord, Nostr, Element, etc.) to interact with the ERGORS engine.

pub mod crypto;
pub mod manager;

#[cfg(feature = "discord")]
pub mod discord;

pub use crypto::{decrypt_gateway_secret, encrypt_gateway_secret, GATEWAY_SECRET_ENCRYPTION_METHOD};
pub use manager::GatewayManager;
