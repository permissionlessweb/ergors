//! Contract Manager for CosmWasm contract lifecycle management
//!
//! Provides automated contract deployment during node startup and a unified
//! interface for contract operations. This module handles:
//!
//! - Automatic contract deployment during coordinator node startup
//! - Contract existence checks (skip deployment if already exists)
//! - Named contract address resolution
//! - Contract upload, instantiation, execution, and queries

#[cfg(feature = "cw")]
mod manager;

#[cfg(feature = "cw")]
pub use manager::ContractManager;

/// Contract deployment error types
#[derive(Debug)]
pub enum ContractError {
    NotDeployed(String),
    UploadFailed(String),
    InstantiationFailed(String),
    ExecutionFailed(String),
    QueryFailed(String),
    Storage(String),
    Serialization(String),
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::NotDeployed(name) => write!(f, "Contract '{}' not deployed", name),
            ContractError::UploadFailed(msg) => write!(f, "Contract upload failed: {}", msg),
            ContractError::InstantiationFailed(msg) => {
                write!(f, "Contract instantiation failed: {}", msg)
            }
            ContractError::ExecutionFailed(msg) => write!(f, "Contract execution failed: {}", msg),
            ContractError::QueryFailed(msg) => write!(f, "Contract query failed: {}", msg),
            ContractError::Storage(msg) => write!(f, "Storage error: {}", msg),
            ContractError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for ContractError {}

impl From<ContractError> for ho_std::llm::HoError {
    fn from(err: ContractError) -> Self {
        ho_std::llm::HoError::Cfg(err.to_string())
    }
}
