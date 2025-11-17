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

//! ERGORS Protocol Buffer Definitions
//!
//! This crate provides protocol buffer definitions for the ERGORS (CommonWare Host Orchestrator)
//!
//! 1. **Storage Layer** - LLM prompt/response storage, query operations, and health monitoring
//! 2. **Node, Networking, Communication Layer** - Node coordination, cosmic task management, network topology
//! 3. **Orchestration Layer** - Node coordination, cosmic task management, network topology
//! 4. **Deployment Layer** - Node coordination, cosmic task management, network topology
//! 5. **Authorization Client** - Node coordination, cosmic task management, network topology
//!
//! The proto definitions are organized following the sacred geometry principles embedded
//! in the ERGORS architecture, with fractal recursion, golden ratio scaling, and tetrahedral
//! network topologies.
//!
extern crate alloc;

// pub mod action;
pub mod config;
pub mod constants;
pub mod custody;
pub mod deploy;
pub mod error;
pub mod examples;
pub mod keys;
pub mod txhash;
pub mod llm;
pub mod network;
pub mod orchestrate;
pub mod python;
mod serde;
pub mod storage;
pub mod traits;
pub mod transports;
#[allow(deprecated, unused_imports, clippy::large_enum_variant)]
pub mod types;
pub mod utils;

use crate::llm::HoError;
// pub mod shared_impl;

pub type HoResult<T> = std::result::Result<T, HoError>;
