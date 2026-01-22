//! CosmWasm Backend implementation with Cnidarium storage
//!
//! This module provides a custom Backend for cosmwasm-vm that uses Cnidarium's
//! JMT-based verifiable storage for contract state persistence.

#[cfg(feature = "cw")]
use {
    crate::wasm::{
        state_ext::WasmStorageState, WasmVmCnidariumStateRead,
    },
    cosmwasm_std::{
        Api, Binary, ContractResult, SystemResult,
    },
    cosmwasm_vm::{BackendApi, BackendError, BackendResult, GasInfo, Querier, Storage},
};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Gas costs for storage operations (in CosmWasm gas units)
const GAS_COST_READ: u64 = 100;
const GAS_COST_WRITE: u64 = 200;
const GAS_COST_REMOVE: u64 = 150;
const GAS_COST_SCAN: u64 = 50;
const GAS_COST_NEXT: u64 = 25;

/// Default backend API implementation for address handling.\
/// Implements cosmwasmVm required [BackendApi] definition
#[derive(Clone)]
#[cfg(feature = "cw")]
pub struct WasmVmBackend;

#[cfg(feature = "cw")]
impl BackendApi for WasmVmBackend {
    fn addr_validate(&self, input: &str) -> BackendResult<()> {
        // Validate address format
        // Allow ergors prefixed addresses and other valid contract addresses
        if input.is_empty() {
            return (
                Err(BackendError::user_err("Address cannot be empty")),
                GasInfo::free(),
            );
        }

        if input.len() > 128 {
            return (
                Err(BackendError::user_err("Address too long (max 128 chars)")),
                GasInfo::free(),
            );
        }

        // Allow alphanumeric, underscore, and hyphen (common address chars)
        if !input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return (
                Err(BackendError::user_err(
                    "Address contains invalid characters",
                )),
                GasInfo::free(),
            );
        }

        (Ok(()), GasInfo::free())
    }

    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>> {
        // Validate first
        let (result, _) = self.addr_validate(human);
        if let Err(e) = result {
            return (Err(e), GasInfo::free());
        }

        // For ERGORS, canonical form is UTF-8 bytes of the human address
        // This allows round-tripping without loss of information
        (Ok(human.as_bytes().to_vec()), GasInfo::free())
    }

    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String> {
        // Convert canonical bytes back to human-readable string
        match String::from_utf8(canonical.to_vec()) {
            Ok(human) => {
                // Validate the result
                let (result, _) = self.addr_validate(&human);
                if let Err(e) = result {
                    return (Err(e), GasInfo::free());
                }
                (Ok(human), GasInfo::free())
            }
            Err(_) => (
                Err(BackendError::user_err(
                    "Invalid canonical address: not valid UTF-8",
                )),
                GasInfo::free(),
            ),
        }
    }
}

/// Iterator state for scanning storage keys
#[cfg(feature = "cw")]
struct ScanIterator {
    items: Vec<(Vec<u8>, Vec<u8>)>,
    position: usize,
}

/// Storage implementation backed by Cnidarium StateDelta
/// Uses trait objects for dynamic dispatch to work with any Cnidarium state implementation
#[cfg(feature = "cw")]
pub struct CnidariumStorage {
    contract_address: String,
    state: &'static mut dyn WasmStorageState,
    gas_used: AtomicU64,
    iterators: HashMap<u32, ScanIterator>,
    next_iterator_id: u32,
}

#[cfg(feature = "cw")]
impl CnidariumStorage {
    /// Create a new storage adapter for a contract
    ///
    /// Takes a mutable reference to state implementing WasmStorageState.
    /// Uses unsafe lifetime extension to satisfy CosmWasm VM requirements.
    ///
    /// # Safety
    /// The caller must ensure the storage instance does not outlive the state reference.
    ///
    /// # Example
    /// ```ignore
    /// let state_delta = StateDelta::new(storage.cs.latest_snapshot());
    /// let storage = unsafe { CnidariumStorage::new(contract_addr, &mut state_delta) };
    /// let backend = Backend { api, storage, querier };
    /// let instance = cache.get_instance(&checksum, backend, options)?;
    /// ```
    pub unsafe fn new(contract_address: String, state: &mut dyn WasmStorageState) -> Self {
        Self {
            contract_address,
            state: std::mem::transmute(state),
            gas_used: AtomicU64::new(0),
            iterators: HashMap::new(),
            next_iterator_id: 1,
        }
    }

    /// Get total gas used
    pub fn gas_used(&self) -> u64 {
        self.gas_used.load(Ordering::Relaxed)
    }

    /// Add gas cost
    fn add_gas(&self, amount: u64) {
        self.gas_used.fetch_add(amount, Ordering::Relaxed);
    }

    /// Reset gas counter
    pub fn reset_gas(&self) {
        self.gas_used.store(0, Ordering::Relaxed);
    }
}

#[cfg(feature = "cw")]
impl Storage for CnidariumStorage {
    fn get(&self, key: &[u8]) -> BackendResult<Option<Vec<u8>>> {
        self.add_gas(GAS_COST_READ);

        // Bridge async to sync using block_in_place
        let result = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let value_result = tokio::task::block_in_place(|| {
                    handle.block_on(
                        self.state
                            .get_contract_state_dyn(&self.contract_address, key),
                    )
                });
                match value_result {
                    Ok(value) => (Ok(value), GasInfo::new(GAS_COST_READ, self.gas_used())),
                    Err(e) => (
                        Err(BackendError::unknown(format!("Storage read error: {}", e))),
                        GasInfo::new(GAS_COST_READ, self.gas_used()),
                    ),
                }
            }
            Err(_) => (
                Err(BackendError::unknown("No async runtime available")),
                GasInfo::new(GAS_COST_READ, self.gas_used()),
            ),
        };
        result
    }

    fn set(&mut self, key: &[u8], value: &[u8]) -> BackendResult<()> {
        self.add_gas(GAS_COST_WRITE);

        self.state
            .put_contract_state_dyn(&self.contract_address, key, value.to_vec());

        (Ok(()), GasInfo::new(GAS_COST_WRITE, self.gas_used()))
    }

    fn remove(&mut self, key: &[u8]) -> BackendResult<()> {
        self.add_gas(GAS_COST_REMOVE);

        self.state
            .delete_contract_state_dyn(&self.contract_address, key);

        (Ok(()), GasInfo::new(GAS_COST_REMOVE, self.gas_used()))
    }

    fn scan(
        &mut self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: cosmwasm_std::Order,
    ) -> BackendResult<u32> {
        self.add_gas(GAS_COST_SCAN);

        let contract_addr = self.contract_address.clone();

        // Get all contract state
        let all_state = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(self.state.get_all_contract_state_dyn(&contract_addr))
            }),
            Err(_) => {
                return (
                    Err(BackendError::unknown("No async runtime available")),
                    GasInfo::new(GAS_COST_SCAN, self.gas_used()),
                )
            }
        };

        let all_state: Vec<(Vec<u8>, Vec<u8>)> = match all_state {
            Ok(state) => state,
            Err(e) => {
                return (
                    Err(BackendError::unknown(format!("Scan error: {}", e))),
                    GasInfo::new(GAS_COST_SCAN, self.gas_used()),
                )
            }
        };

        // Filter by range
        let mut filtered: Vec<(Vec<u8>, Vec<u8>)> = all_state
            .into_iter()
            .filter(|(k, _)| {
                let after_start = start.is_none_or(|s| k.as_slice() >= s);
                let before_end = end.is_none_or(|e| k.as_slice() < e);
                after_start && before_end
            })
            .collect();

        // Sort based on order
        if order == cosmwasm_std::Order::Descending {
            filtered.sort_by(|a, b| b.0.cmp(&a.0));
        } else {
            filtered.sort_by(|a, b| a.0.cmp(&b.0));
        }

        // Create iterator
        let iterator = ScanIterator {
            items: filtered,
            position: 0,
        };

        let id = self.next_iterator_id;
        self.next_iterator_id += 1;

        self.iterators.insert(id, iterator);

        (Ok(id), GasInfo::new(GAS_COST_SCAN, self.gas_used()))
    }

    fn next(&mut self, iterator_id: u32) -> BackendResult<Option<(Vec<u8>, Vec<u8>)>> {
        self.add_gas(GAS_COST_NEXT);

        match self.iterators.get_mut(&iterator_id) {
            Some(iterator) => {
                if iterator.position < iterator.items.len() {
                    let item = iterator.items[iterator.position].clone();
                    iterator.position += 1;
                    (Ok(Some(item)), GasInfo::new(GAS_COST_NEXT, self.gas_used()))
                } else {
                    self.iterators.remove(&iterator_id);
                    (Ok(None), GasInfo::new(GAS_COST_NEXT, self.gas_used()))
                }
            }
            None => (
                Err(BackendError::unknown(format!(
                    "Iterator {} not found",
                    iterator_id
                ))),
                GasInfo::new(GAS_COST_NEXT, self.gas_used()),
            ),
        }
    }
}

/// Querier for cross-contract queries
///
/// Handles query requests from within contract execution, supporting:
/// - WasmQuery::Raw - Read contract state directly
/// - WasmQuery::ContractInfo - Read contract metadata
/// - WasmQuery::Smart - Execute query on another contract (returns error for now)
/// - BankQuery - Query account balances (returns empty for now)
#[cfg(feature = "cw")]
pub struct CnidariumQuerier {
    /// Snapshot of state for read-only queries
    /// Using a channel-based approach to communicate with async context
    state_reader: Option<std::sync::Arc<dyn QuerierStateReader>>,
}

/// Trait for state reading in querier context
/// This allows the querier to access state without holding direct references
#[cfg(feature = "cw")]
pub trait QuerierStateReader: Send + Sync {
    /// Get contract state value
    fn get_contract_state(&self, contract_address: &str, key: &[u8]) -> Option<Vec<u8>>;

    /// Get contract info
    fn get_contract_info(&self, contract_address: &str) -> Option<ContractInfoResponse>;
}

/// Response type for contract info queries
#[cfg(feature = "cw")]
#[derive(Clone, Debug)]
pub struct ContractInfoResponse {
    pub code_id: u64,
    pub creator: String,
    pub admin: Option<String>,
}

#[cfg(feature = "cw")]
impl Clone for CnidariumQuerier {
    fn clone(&self) -> Self {
        Self {
            state_reader: self.state_reader.clone(),
        }
    }
}

#[cfg(feature = "cw")]
impl CnidariumQuerier {
    /// Create a new querier without state access (for cache initialization)
    pub fn new() -> Self {
        Self { state_reader: None }
    }

    /// Create a querier with state reader for actual queries
    pub fn with_state_reader(reader: std::sync::Arc<dyn QuerierStateReader>) -> Self {
        Self {
            state_reader: Some(reader),
        }
    }
}

#[cfg(feature = "cw")]
impl Default for CnidariumQuerier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "cw")]
impl Querier for CnidariumQuerier {
    fn query_raw(
        &self,
        request: &[u8],
        _gas_limit: u64,
    ) -> (
        std::result::Result<SystemResult<ContractResult<cosmwasm_std::Binary>>, BackendError>,
        cosmwasm_vm::GasInfo,
    ) {
        use cosmwasm_std::{from_json, to_json_binary, Coin, QueryRequest};

        // Parse the query request
        let query_request: QueryRequest<cosmwasm_std::Empty> = match from_json(request) {
            Ok(req) => req,
            Err(e) => {
                return (
                    Ok(SystemResult::Err(
                        cosmwasm_std::SystemError::InvalidRequest {
                            error: format!("Failed to parse query request: {}", e),
                            request: Binary::from(request.to_vec()),
                        },
                    )),
                    GasInfo::new(100, 100),
                );
            }
        };

        // Handle different query types
        let result = match query_request {
            QueryRequest::Wasm(wasm_query) => self.handle_wasm_query(wasm_query),

            QueryRequest::Bank(_) => {
                // Return empty balance response for bank queries
                // Bank module not implemented - return empty response
                #[derive(serde::Serialize)]
                struct EmptyBalanceResp {
                    amount: Vec<Coin>,
                }
                let response = EmptyBalanceResp { amount: vec![] };
                match to_json_binary(&response) {
                    Ok(binary) => Ok(SystemResult::Ok(ContractResult::Ok(binary))),
                    Err(e) => Ok(SystemResult::Err(
                        cosmwasm_std::SystemError::InvalidResponse {
                            error: e.to_string(),
                            response: Binary::default(),
                        },
                    )),
                }
            }

            QueryRequest::Staking(_) => Ok(SystemResult::Err(
                cosmwasm_std::SystemError::UnsupportedRequest {
                    kind: "Staking queries not supported".to_string(),
                },
            )),

            QueryRequest::Custom(_) => Ok(SystemResult::Err(
                cosmwasm_std::SystemError::UnsupportedRequest {
                    kind: "Custom queries not supported".to_string(),
                },
            )),

            _ => Ok(SystemResult::Err(
                cosmwasm_std::SystemError::UnsupportedRequest {
                    kind: "Unknown query type".to_string(),
                },
            )),
        };

        (result, GasInfo::new(1000, 1000))
    }
}

#[cfg(feature = "cw")]
impl CnidariumQuerier {
    fn handle_wasm_query(
        &self,
        wasm_query: cosmwasm_std::WasmQuery,
    ) -> std::result::Result<SystemResult<ContractResult<Binary>>, BackendError> {
        use cosmwasm_std::{to_json_binary, WasmQuery};

        match wasm_query {
            WasmQuery::Raw { contract_addr, key } => {
                // Read raw contract state
                match &self.state_reader {
                    Some(reader) => {
                        let value = reader.get_contract_state(&contract_addr, key.as_slice());
                        let binary = Binary::from(value.unwrap_or_default());
                        Ok(SystemResult::Ok(ContractResult::Ok(binary)))
                    }
                    None => Ok(SystemResult::Err(
                        cosmwasm_std::SystemError::UnsupportedRequest {
                            kind: "State reader not available".to_string(),
                        },
                    )),
                }
            }

            WasmQuery::ContractInfo { contract_addr } => {
                // Return contract info
                match &self.state_reader {
                    Some(reader) => {
                        match reader.get_contract_info(&contract_addr) {
                            Some(info) => {
                                // Build response manually as ContractInfoResponse::new takes 6 args
                                #[derive(serde::Serialize)]
                                struct ContractInfoResp {
                                    code_id: u64,
                                    creator: String,
                                    admin: Option<String>,
                                    pinned: bool,
                                    ibc_port: Option<String>,
                                    ibc2_port: Option<String>,
                                }
                                let response = ContractInfoResp {
                                    code_id: info.code_id,
                                    creator: info.creator.clone(),
                                    admin: info.admin.clone(),
                                    pinned: false,
                                    ibc_port: None,
                                    ibc2_port: None,
                                };
                                match to_json_binary(&response) {
                                    Ok(binary) => Ok(SystemResult::Ok(ContractResult::Ok(binary))),
                                    Err(e) => Ok(SystemResult::Err(
                                        cosmwasm_std::SystemError::InvalidResponse {
                                            error: e.to_string(),
                                            response: Binary::default(),
                                        },
                                    )),
                                }
                            }
                            None => Ok(SystemResult::Err(
                                cosmwasm_std::SystemError::NoSuchContract {
                                    addr: contract_addr,
                                },
                            )),
                        }
                    }
                    None => Ok(SystemResult::Err(
                        cosmwasm_std::SystemError::UnsupportedRequest {
                            kind: "State reader not available".to_string(),
                        },
                    )),
                }
            }

            WasmQuery::Smart {
                contract_addr,
                msg: _,
            } => {
                // Smart queries require recursive VM execution
                // For now, return an error indicating this is not yet supported
                Ok(SystemResult::Err(
                    cosmwasm_std::SystemError::UnsupportedRequest {
                        kind: format!(
                            "Cross-contract smart queries not yet supported (target: {})",
                            contract_addr
                        ),
                    },
                ))
            }

            _ => Ok(SystemResult::Err(
                cosmwasm_std::SystemError::UnsupportedRequest {
                    kind: "Unknown WASM query type".to_string(),
                },
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "cw")]
    fn test_backend_api_address_conversion() {
        let api = WasmVmBackend;

        let human = "ergors1test";
        let canonical = api.addr_canonicalize(human).0.unwrap();
        let back_to_human = api.addr_humanize(&canonical).0.unwrap();

        assert_eq!(human, back_to_human);
    }

    #[test]
    #[cfg(feature = "cw")]
    fn test_backend_api_validation() {
        let api = WasmVmBackend;

        assert!(api.addr_validate("ergors1test").0.is_ok());
        assert!(api.addr_validate("contract-123").0.is_ok());
        assert!(api.addr_validate("").0.is_err());
        assert!(api.addr_validate(&"x".repeat(256)).0.is_err());
    }
}
