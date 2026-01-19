//! CosmWasm-VM integration module
//!
//! Provides CosmWasm smart contract execution capabilities with Cnidarium storage integration:
//! - Contract upload, instantiation, execution, and queries
//! - Custom Backend implementation for cosmwasm-vm
//! - State management with configurable retention policies
//! - Integration with Cnidarium verifiable storage (JMT-based)

pub mod backend;
pub mod runtime;
pub mod state_ext;
pub mod state_keys;

pub use backend::WasmVmBackend;
pub use runtime::WasmRuntime;
pub use state_ext::{WasmVmCnidariumStateRead, WasmVmCnidariumStateWrite};
