//! CosmWasm Backend implementation with Cnidarium storage
//!
//! This module provides a custom Backend for cosmwasm-vm that uses Cnidarium's
//! JMT-based verifiable storage for contract state persistence.

#[cfg(feature = "cw")]
use {
    crate::wasm::{
        state_ext::WasmStorageState, WasmVmCnidariumStateRead, WasmVmCnidariumStateWrite,
    },
    cosmwasm_std::{
        Addr, Api, Binary, CanonicalAddr, ContractResult, QuerierResult, RecoverPubkeyError,
        StdError, StdResult, SystemResult, VerificationError,
    },
    cosmwasm_vm::{BackendApi, BackendError, BackendResult, GasInfo, Querier, Storage},
};

use crate::error::{HoError, HoResult};
use cnidarium::{StateRead, StateWrite};
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
        todo!()
    }

    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>> {
        todo!()
    }

    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String> {
        todo!()
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
                            .get_contract_state_dyn(&self.contract_address, &key.to_vec()),
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
                let after_start = start.map_or(true, |s| k.as_slice() >= s);
                let before_end = end.map_or(true, |e| k.as_slice() < e);
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
#[derive(Clone)]
#[cfg(feature = "cw")]
pub struct CnidariumQuerier;

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
        todo!()
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
