//! State extension traits for WASM storage operations
//!
//! Provides convenient methods for reading and writing WASM data to Cnidarium storage.
//! - [WasmVmCnidariumStateRead]
//! - [WasmVmCnidariumStateWrite]

use crate::error::{HoError, HoResult};
use crate::types::ergors::cosmwasm::wasm::v1::{CodeInfo, ContractInfo, Model};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use futures::StreamExt;
use prost::Message;

use super::state_keys::*;

/// Object-safe trait for WASM state operations
/// Avoids associated types and generic methods that prevent trait objects
#[async_trait::async_trait]
pub trait WasmStorageState: std::fmt::Debug + Send + 'static {
    async fn get_contract_state_dyn(
        &self,
        contract_address: &str,
        key: &[u8],
    ) -> HoResult<Option<Vec<u8>>>;

    fn put_contract_state_dyn(&mut self, contract_address: &str, key: &[u8], value: Vec<u8>);

    fn delete_contract_state_dyn(&mut self, contract_address: &str, key: &[u8]);

    async fn get_all_contract_state_dyn(
        &self,
        contract_address: &str,
    ) -> HoResult<Vec<(Vec<u8>, Vec<u8>)>>;
}

/// Blanket implementation: any type with StateRead + StateWrite can be used as WASM storage
#[async_trait::async_trait]
impl<T> WasmStorageState for T
where
    T: StateRead + StateWrite + std::fmt::Debug + Send + 'static,
{
    async fn get_contract_state_dyn(
        &self,
        contract_address: &str,
        key: &[u8],
    ) -> HoResult<Option<Vec<u8>>> {
        self.get_contract_state(contract_address, key).await
    }

    fn put_contract_state_dyn(&mut self, contract_address: &str, key: &[u8], value: Vec<u8>) {
        self.put_contract_state(contract_address, key, value)
    }

    fn delete_contract_state_dyn(&mut self, contract_address: &str, key: &[u8]) {
        self.delete_contract_state(contract_address, key)
    }

    async fn get_all_contract_state_dyn(
        &self,
        contract_address: &str,
    ) -> HoResult<Vec<(Vec<u8>, Vec<u8>)>> {
        self.get_all_contract_state(contract_address).await
    }
}

/// Extension trait for reading WASM state from Cnidarium storage
#[async_trait]
pub trait WasmVmCnidariumStateRead: cnidarium::StateRead {
    /// Get WASM code bytes by code ID
    async fn get_wasm_code(&self, code_id: u64) -> HoResult<Option<Vec<u8>>> {
        let key = wasm_code_key(code_id);
        self.get(&key)
            .await
            .map_err(|e| HoError::Storage(format!("Failed to get WASM code {}: {}", code_id, e)))
    }

    /// Get code metadata (CodeInfo) by code ID
    async fn get_wasm_code_info(&self, code_id: u64) -> HoResult<Option<CodeInfo>> {
        let key = wasm_code_info_key(code_id);
        match self.get(&key).await? {
            Some(bytes) => {
                let msg = CodeInfo::decode(&*bytes).map_err(|e| HoError::Storage(format!("Failed to decode proto at key {}: {}", key, e)))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Get code ID by code hash
    async fn get_code_id_by_hash(&self, hash: &[u8]) -> HoResult<Option<u64>> {
        let key = wasm_code_hash_key(hash);
        match self.get(&key).await? {
            Some(bytes) => {
                let id = u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| HoError::Storage("Invalid code_id bytes".to_string()))?,
                );
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Get contract info by address
    async fn get_wasm_contract_info(&self, address: &str) -> HoResult<Option<ContractInfo>> {
        let key = wasm_contract_key(address);
        match self.get(&key).await? {
            Some(bytes) => {
                let msg = ContractInfo::decode(&*bytes).map_err(|e| HoError::Storage(format!("Failed to decode proto at key {}: {}", key, e)))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Get contract state value by key
    async fn get_contract_state(
        &self,
        contract_address: &str,
        state_key: &[u8],
    ) -> HoResult<Option<Vec<u8>>> {
        let key = wasm_contract_state_key(contract_address, state_key);
        self.get(&key).await.map_err(|e| {
            HoError::Storage(format!(
                "Failed to get contract state for {}: {}",
                contract_address, e
            ))
        })
    }

    /// Get all contract state keys with their values for a given contract
    async fn get_all_contract_state(
        &self,
        contract_address: &str,
    ) -> HoResult<Vec<(Vec<u8>, Vec<u8>)>> {
        use futures::pin_mut;

        let prefix = wasm_contract_state_prefix(contract_address);
        let state_stream = self.prefix(&prefix);
        pin_mut!(state_stream);
        let mut results = Vec::new();

        while let Some(entry_result) = state_stream.next().await {
            match entry_result {
                Ok((key, value)) => {
                    // Extract the actual state key from the full storage key
                    let key_str = String::from_utf8_lossy(key.as_bytes());
                    if let Some(state_key_hex) = key_str.strip_prefix(&prefix) {
                        if let Ok(state_key) = hex::decode(state_key_hex) {
                            results.push((state_key, value));
                        }
                    }
                }
                Err(e) => {
                    return Err(HoError::Storage(format!(
                        "Failed to read contract state stream: {}",
                        e
                    )));
                }
            }
        }

        Ok(results)
    }

    /// List all contract addresses instantiated from a specific code ID
    async fn get_contracts_by_code(&self, code_id: u64) -> HoResult<Vec<String>> {
        use futures::pin_mut;

        let prefix = wasm_contracts_by_code_prefix(code_id);
        let contracts_stream = self.prefix(&prefix);
        pin_mut!(contracts_stream);
        let mut addresses = Vec::new();

        while let Some(entry_result) = contracts_stream.next().await {
            match entry_result {
                Ok((_, value)) => {
                    if let Ok(address) = String::from_utf8(value) {
                        addresses.push(address);
                    }
                }
                Err(e) => {
                    return Err(HoError::Storage(format!(
                        "Failed to read contracts by code stream: {}",
                        e
                    )));
                }
            }
        }

        Ok(addresses)
    }

    /// Get the next code ID to be assigned
    async fn get_next_code_id(&self) -> HoResult<u64> {
        let key = wasm_config_key();
        match self.get(&key).await? {
            Some(data) => {
                // Parse config to get next_code_id
                // For now, just return 1 as default
                // TODO: Implement proper config storage
                Ok(1)
            }
            None => Ok(1), // Start from 1 if no config exists
        }
    }
}

impl<T: cnidarium::StateRead + ?Sized> WasmVmCnidariumStateRead for T {}

/// Extension trait for writing WASM state to Cnidarium storage
pub trait WasmVmCnidariumStateWrite: cnidarium::StateWrite {
    /// Store WASM code bytes
    fn put_wasm_code(&mut self, code_id: u64, code: Vec<u8>) {
        let key = wasm_code_key(code_id);
        self.put(key, code);
    }

    /// Store code metadata (CodeInfo)
    fn put_wasm_code_info(&mut self, code_id: u64, code_info: &CodeInfo) {
        let key = wasm_code_info_key(code_id);
        let mut buf = Vec::new();
        code_info.encode(&mut buf).expect("proto encoding should not fail");
        self.put(key, buf);
    }

    /// Store code hash to code ID mapping
    fn put_code_hash_mapping(&mut self, hash: &[u8], code_id: u64) {
        let key = wasm_code_hash_key(hash);
        self.put(key, code_id.to_le_bytes().to_vec());
    }

    /// Store contract info
    fn put_wasm_contract_info(&mut self, address: &str, contract_info: &ContractInfo) {
        let key = wasm_contract_key(address);
        let mut buf = Vec::new();
        contract_info.encode(&mut buf).expect("proto encoding should not fail");
        self.put(key, buf);
    }

    /// Store contract address in code ID index
    fn put_contract_by_code_index(&mut self, code_id: u64, idx: u64, address: String) {
        let key = wasm_contract_by_code_key(code_id, idx);
        self.put(key, address.into_bytes());
    }

    /// Store contract state value
    fn put_contract_state(&mut self, contract_address: &str, state_key: &[u8], value: Vec<u8>) {
        let key = wasm_contract_state_key(contract_address, state_key);
        self.put(key, value);
    }

    /// Delete contract state value
    fn delete_contract_state(&mut self, contract_address: &str, state_key: &[u8]) {
        let key = wasm_contract_state_key(contract_address, state_key);
        self.delete(key);
    }

    /// Store multiple contract state values at once (for instantiate/migrate)
    fn put_contract_state_batch(&mut self, contract_address: &str, state: Vec<Model>) {
        for model in state {
            self.put_contract_state(contract_address, &model.key, model.value);
        }
    }

    /// Delete all contract state for a given contract (for cleanup)
    fn delete_all_contract_state(&mut self, contract_address: &str) {
        // Note: This requires iteration which is async, so this is a marker
        // In practice, you'd need to call this from an async context with proper cleanup
        let prefix = wasm_contract_state_prefix(contract_address);
        self.nonverifiable_delete(prefix.as_bytes().into());
    }
}

impl<T: cnidarium::StateWrite + ?Sized> WasmVmCnidariumStateWrite for T {}

// Helper trait for proto encoding/decoding
#[async_trait]
trait ProtoExt: StateRead {
    async fn get_proto<M: Message + Default>(&self, key: &str) -> HoResult<Option<M>> {
        match self.get_raw(key).await? {
            Some(bytes) => {
                let msg = M::decode(&*bytes).map_err(|e| {
                    HoError::Storage(format!("Failed to decode proto at key {}: {}", key, e))
                })?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl<T: StateRead + ?Sized> ProtoExt for T {}

trait ProtoWriteExt: StateWrite {
    fn put_proto<M: Message>(&mut self, key: String, value: M) {
        let mut buf = Vec::new();
        value
            .encode(&mut buf)
            .expect("proto encoding should not fail");
        self.put_raw(key, buf);
    }
}

impl<T: StateWrite + ?Sized> ProtoWriteExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require mocking StateRead/StateWrite implementations
    // For now, we rely on integration tests with actual Cnidarium storage
}
