// #![doc = include_str!("../README.md")]
#![doc(html_logo_url = "")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    clippy::derive_partial_eq_without_eq
)]
#![forbid(unsafe_code)]
#![warn(trivial_casts, trivial_numeric_casts, unused_import_braces)]
#![cfg_attr(not(feature = "std"), no_std)]

//! CW-HO Protocol Buffer Definitions
//!
//! This crate provides protocol buffer definitions for the CW-HO (CommonWare Host Orchestrator)
//!
//! 1. **Storage Layer** - LLM prompt/response storage, query operations, and health monitoring
//! 2. **Node, Networking, Communication Layer** - Node coordination, cosmic task management, network topology
//! 3. **Orchestration Layer** - Node coordination, cosmic task management, network topology
//! 4. TODO: **Debugging / Developer APIs** - logging help, historical reference
//!
//! The proto definitions are organized following the sacred geometry principles embedded
//! in the CW-HO architecture, with fractal recursion, golden ratio scaling, and tetrahedral
//! network topologies.
//!

pub mod commonware;
pub mod config;
pub mod constants;
pub mod deploy;
pub mod error;
pub mod examples;
pub mod llm;
pub mod network;
pub mod orchestrate;
pub mod prelude;
pub mod python;
pub mod routes;
pub mod transports;
pub mod utils;

// pub mod shared_impl;
// pub use types::server::NodeType;

extern crate alloc;

/// The version (commit hash) of the Cosmos SDK used when generating this library.
pub const GO_BITSONG_VERSION: &str = include_str!("types/CW_HO_COMMIT");

mod serde;
pub mod shim;
pub mod traits;
#[allow(deprecated, unused_imports, clippy::large_enum_variant)]
pub mod types;
