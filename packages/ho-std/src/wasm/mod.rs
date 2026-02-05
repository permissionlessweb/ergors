//! CosmWasm-VM integration module
//!
//! Provides CosmWasm smart contract execution capabilities with Cnidarium storage integration:
//! - Contract upload, instantiation, execution, and queries
//! - Custom Backend implementation for cosmwasm-vm
//! - State management with configurable retention policies
//! - Integration with Cnidarium verifiable storage (JMT-based)

pub mod backend;
pub mod event_router;
pub mod runtime;
pub mod state_ext;
pub mod state_keys;

#[cfg(not(feature = "cw"))]
pub use backend::WasmVmBackend;
#[cfg(feature = "cw")]
pub use backend::{
    CnidariumQuerier, CnidariumStorage, ContractInfoResponse, QuerierStateReader, WasmVmBackend,
};
pub use event_router::{parse_engine_actions, parse_response_attributes, ActionResult, EngineAction};
pub use runtime::WasmRuntime;
pub use state_ext::{WasmVmCnidariumStateRead, WasmVmCnidariumStateWrite};
