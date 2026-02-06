//! ERGORS Integration Test Suite
//!
//! This is a modular, extensible test suite for the ERGORS system.
//! It provides comprehensive coverage of all core components and their interactions.
//!
//! ## Test Modules
//!
//! - `common`: Shared test utilities, fixtures, and assertions
//! - `mock_client`: Mock ManagementServiceClient for unit testing
//! - `network`: Network topology, messaging, and peer discovery tests
//! - `storage`: Storage persistence, snapshots, and query tests
//! - `llm`: LLM provider routing, cost tracking, and model selection tests
//! - `orchestration`: Cosmic task, fractal recursion, and golden ratio tests
//! - `session`: Session hierarchy, snapshots, and coordination tests
//! - `custody`: Custody signing and backend tests
//! - `git`: Git repository and workspace tests
//! - `wasm`: CosmWasm contract lifecycle tests
//! - `integration`: End-to-end integration scenarios
//!
//! ## Running Tests
//!
//! ```bash
//! # All tests
//! cargo test -p ergors-tests --all-features
//!
//! # Component tests only
//! cargo test -p ergors-tests network::
//!
//! # Mock client tests
//! cargo test -p ergors-tests mock_client::
//!
//! # Integration tests
//! cargo test -p ergors-tests --features integration
//!
//! # E2E tests (requires infrastructure)
//! cargo test -p ergors-tests --features e2e
//! ```

// Common test utilities
mod common;

// Mock client for testing without real infrastructure
pub mod mock_client;

// Component test modules
#[cfg(test)]
mod config;
// #[cfg(test)]
// mod custody;
#[cfg(test)]
pub mod git;
#[cfg(test)]
pub mod llm;
#[cfg(test)]
pub mod network;
#[cfg(test)]
pub mod orchestration;
#[cfg(test)]
pub mod session;
#[cfg(test)]
pub mod storage;
#[cfg(test)]
pub mod wasm;

// // Integration test modules
// #[cfg(test)]
// pub mod integration;

