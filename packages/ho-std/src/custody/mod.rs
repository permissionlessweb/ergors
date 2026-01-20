//! Implementations of custody services responsible for signing transactions
//! and managing cryptographic keys.
//!
//! This module provides:
//! - [`soft_kms`] - Basic software key management system for policy-based authorization
//! - [`encrypted`] - Password-based encryption utilities (Argon2 + ChaCha20Poly1305)
//! - [`node_identity`] - Custody backends for node identity key management
//!
//! We also make use of this kms for storage, retrieval, and decryption of inference provider APIs.

#![deny(clippy::unwrap_used)]
// // Requires nightly.
// #![cfg_attr(docsrs, feature(doc_auto_cfg))]
// #[macro_use]
// extern crate serde_with;

mod client;
pub mod encrypted;
pub mod node_identity;
pub mod null_kms;
pub mod policy;
mod pre_auth;
mod request;
pub mod soft_kms;
// pub mod threshold;
mod terminal;

pub use client::CustodyClient;
pub use node_identity::{PasswordEncryptedCustody, PlaintextCustody};
pub use pre_auth::PreAuthorization;
pub use request::{
    AuthorizeRequest,
    //  AuthorizeValidatorDefinitionRequest, AuthorizeValidatorVoteRequest,
};
