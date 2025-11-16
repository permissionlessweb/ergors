//! # ERGORS Trait System
//!
//! This module provides traits used for the ERGORS engine.
//!
//! ## Architecture Overview
//!
//! ### Key Components
//!
//! - [`config`] - Configuration management and validation traits
//! - [`file_ops`] - Utilities for interacting with files
//! - [`llm`] - LLM provider routing, prompt handling, and response processing traits
//! - [`network`] - Network node identity, topology, and message handling traits
//! - [`orchestrator`] - Cosmic task management and fractal orchestration traits
//! - [`storage`] - Data persistence, querying, and snapshot management traits
//!
//! ## Usage Pattern
//! 
//! ### TRAITS -> STRUCTS -> CONFIGS -> IMPLEMENTATIONS
//! 

pub mod config;
pub mod custody;
mod domain;
pub mod file_ops;
pub mod fractals;
pub mod llm;
pub mod message;
pub mod network;
pub mod orchestrator;
pub mod storage;

// Re-export all traits for convenience
pub use config::*;
pub use custody::*;
pub use domain::*;
pub use fractals::*;
pub use llm::*;
pub use message::*;
pub use network::*;
pub use orchestrator::*;
pub use storage::*;
