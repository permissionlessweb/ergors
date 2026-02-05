// #![doc = include_str!("../README.md")]
#![doc(html_logo_url = "")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    clippy::derive_partial_eq_without_eq
)]
// #![forbid(unsafe_code)]
#![warn(trivial_casts, trivial_numeric_casts, unused_import_braces)]
#![cfg_attr(not(feature = "std"), no_std)]

mod serde;

// Re-export commonly used prost traits
pub use prost::{Message, Name};

pub mod config;
pub mod constants;
pub mod error;
pub mod git;
pub mod languages;
pub mod llm;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;
pub mod transports;
pub mod utils;

#[cfg(feature = "cw")]
pub mod wasm;
// pub mod examples;

#[allow(deprecated, unused_imports, clippy::large_enum_variant)]
pub mod types;

// pub mod action;
// pub mod view;
pub mod bootstrap;
pub mod custody;
pub mod ephemeral;
pub mod keys;
pub mod secret_sharing;
// pub mod wallet;
// pub mod txhash;

use crate::llm::HoError;
// pub mod shared_impl;

pub type HoResult<T> = std::result::Result<T, HoError>;
