//! CosmWasm runtime for contract execution with node-wide synchronization
//!
//! Provides high-level APIs for uploading, instantiating, executing, and querying
//! CosmWasm smart contracts with Cnidarium storage backend.
//!
//! Key Features:
//! - Node-wide state synchronization using RwLock
//! - Atomic state updates via Cnidarium's StateDelta
//! - Thread-safe cache using Arc<Cache>
//! - Proper read-write consistency guarantees

use crate::error::{HoError, HoResult};
use cnidarium::Storage;

#[cfg(feature = "cw")]
use {
    crate::wasm::backend::{
        CnidariumQuerier, CnidariumStorage, ContractInfoResponse, QuerierStateReader, WasmVmBackend,
    },
    crate::wasm::state_ext::{WasmVmCnidariumStateRead, WasmVmCnidariumStateWrite},
    cosmwasm_std::{Addr, BlockInfo, Coin, ContractInfo, Env, MessageInfo, Timestamp},
    cosmwasm_vm::{
        call_execute, call_instantiate, call_query, capabilities_from_csv, Backend, Cache,
        CacheOptions, InstanceOptions, Size,
    },
    sha2::{Digest, Sha256},
    std::path::PathBuf,
    std::sync::Arc,
    tracing::{debug, info, warn},
};

/// Default gas limits for contract operations
#[cfg(feature = "cw")]
pub const DEFAULT_INSTANTIATE_GAS: u64 = 100_000_000;
#[cfg(feature = "cw")]
pub const DEFAULT_EXECUTE_GAS: u64 = 50_000_000;
#[cfg(feature = "cw")]
pub const DEFAULT_QUERY_GAS: u64 = 10_000_000;

/// Default memory limit per contract instance (32 MB)
#[cfg(feature = "cw")]
pub const DEFAULT_MEMORY_LIMIT: Size = Size::mebi(32);

/// Memory cache size for compiled WASM modules (200 MB)
#[cfg(feature = "cw")]
pub const MEMORY_CACHE_SIZE: Size = Size::mebi(200);

/// Maximum WASM code size (800 KB)
#[cfg(feature = "cw")]
pub const MAX_WASM_CODE_SIZE: usize = 800 * 1024;

/// Default CosmWasm capabilities
#[cfg(feature = "cw")]
pub const DEFAULT_CAPABILITIES: &str = "iterator,staking,stargate,cosmwasm_1_1,cosmwasm_1_2,cosmwasm_1_3,cosmwasm_1_4,cosmwasm_2_0,cosmwasm_2_1,cosmwasm_2_2";

/// Type alias for our WASM cache with lifetime for the storage reference
#[cfg(feature = "cw")]
pub type WasmCache = Cache<WasmVmBackend, CnidariumStorage, CnidariumQuerier>;

/// WasmRuntime manages CosmWasm contract lifecycle with node-wide synchronization
///
/// This runtime provides node-wide state synchronization to ensure contract writes
/// are committed before subsequent reads. Each ERGORS node maintains its own isolated
/// VM instance with proper consistency guarantees.
#[cfg(feature = "cw")]
pub struct WasmRuntime {
    /// Cached WASM module cache for performance
    cache: Arc<WasmCache>,
    /// Node-wide synchronization for state consistency
    /// Write operations take exclusive lock, reads take shared lock
    state_lock: Arc<tokio::sync::RwLock<()>>,
    /// Default gas limit for instantiate operations
    instantiate_gas_limit: u64,
    /// Default gas limit for execute operations
    execute_gas_limit: u64,
    /// Default gas limit for query operations
    query_gas_limit: u64,
}

/// State reader for cross-contract queries using Cnidarium snapshots
///
/// This wraps a Cnidarium Storage and provides synchronous access to state
/// through the QuerierStateReader trait, enabling cross-contract queries
/// during contract execution.
#[cfg(feature = "cw")]
struct SnapshotStateReader {
    storage: Storage,
}

#[cfg(feature = "cw")]
impl SnapshotStateReader {
    fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

#[cfg(feature = "cw")]
impl QuerierStateReader for SnapshotStateReader {
    fn get_contract_state(&self, contract_address: &str, key: &[u8]) -> Option<Vec<u8>> {
        // Bridge async to sync for cross-contract queries
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let snapshot = self.storage.latest_snapshot();
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        snapshot
                            .get_contract_state(contract_address, key)
                            .await
                            .ok()
                            .flatten()
                    })
                })
            }
            Err(_) => None,
        }
    }

    fn get_contract_info(&self, contract_address: &str) -> Option<ContractInfoResponse> {
        // Bridge async to sync for cross-contract queries
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let snapshot = self.storage.latest_snapshot();
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        match snapshot.get_wasm_contract_info(contract_address).await {
                            Ok(Some(info)) => Some(ContractInfoResponse {
                                code_id: info.code_id,
                                creator: info.creator,
                                admin: if info.admin.is_empty() {
                                    None
                                } else {
                                    Some(info.admin)
                                },
                            }),
                            _ => None,
                        }
                    })
                })
            }
            Err(_) => None,
        }
    }
}

#[cfg(feature = "cw")]
impl WasmRuntime {
    /// Create a new WasmRuntime with default configuration
    pub fn new(cache_dir: PathBuf) -> HoResult<Self> {
        Self::new_with_options(
            cache_dir,
            DEFAULT_CAPABILITIES,
            MEMORY_CACHE_SIZE,
            DEFAULT_MEMORY_LIMIT,
        )
    }

    /// Create a new WasmRuntime with custom configuration
    pub fn new_with_options(
        cache_dir: PathBuf,
        capabilities: &str,
        memory_cache_size: Size,
        memory_limit: Size,
    ) -> HoResult<Self> {
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            HoError::Storage(format!("Failed to create WASM cache directory: {}", e))
        })?;

        let supported_capabilities = capabilities_from_csv(capabilities);
        let options = CacheOptions::new(
            cache_dir.clone(),
            supported_capabilities,
            memory_cache_size,
            memory_limit,
        );

        let cache = unsafe { Cache::new(options) }
            .map_err(|e| HoError::Storage(format!("Failed to create WASM cache: {}", e)))?;

        info!(
            "Initialized WasmRuntime with node-wide synchronization, cache at {:?}",
            cache_dir
        );

        Ok(Self {
            cache: Arc::new(cache),
            state_lock: Arc::new(tokio::sync::RwLock::new(())),
            instantiate_gas_limit: DEFAULT_INSTANTIATE_GAS,
            execute_gas_limit: DEFAULT_EXECUTE_GAS,
            query_gas_limit: DEFAULT_QUERY_GAS,
        })
    }

    /// Store WASM code with node-wide state synchronization
    ///
    /// This validates the WASM bytecode, computes its hash, and stores it in both
    /// the cache (for execution) and Cnidarium (for persistence). Uses exclusive
    /// lock to ensure code storage operations are atomic.
    ///
    /// # Arguments
    /// * `state` - Mutable reference to Cnidarium storage that implements WasmVmCnidariumStateRead/Write
    /// * `wasm_code` - Raw WASM bytecode
    /// * `creator` - Address of the code uploader
    ///
    /// # Returns
    /// * `code_id` - Unique identifier for the stored code
    pub async fn store_code(
        &self,
        state: &cnidarium::Storage,
        wasm_code: Vec<u8>,
        creator: String,
    ) -> HoResult<u64> {
        use cnidarium::StateRead;
        use cnidarium::StateWrite;

        // Acquire node-wide exclusive lock for global state consistency
        let _node_lock = self.state_lock.write().await;

        // Create StateDelta for atomic code storage
        let mut delta = cnidarium::StateDelta::new(state.latest_snapshot());

        // Validate code size
        if wasm_code.len() > MAX_WASM_CODE_SIZE {
            return Err(HoError::Storage(format!(
                "WASM code size {} exceeds maximum {}",
                wasm_code.len(),
                MAX_WASM_CODE_SIZE
            )));
        }

        // Compute code hash
        let code_hash = Sha256::digest(&wasm_code).to_vec();

        // Check if code already exists (deduplication)
        let hash_key = format!("wasm/code_hash/{}", hex::encode(&code_hash));
        if let Some(existing_id_bytes) = delta.get_raw(&hash_key).await? {
            let existing_id = u64::from_le_bytes(
                existing_id_bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| HoError::Storage("Invalid code_id bytes".to_string()))?,
            );
            info!("WASM code already exists with ID: {}", existing_id);
            return Ok(existing_id);
        }

        // Validate and save WASM code to cache
        let checksum = self
            .cache
            .store_code(&wasm_code, true, false)
            .map_err(|e| HoError::Wasm(format!("Invalid WASM code: {}", e)))?;

        debug!("Validated WASM code with checksum: {:?}", checksum);

        // Get next code ID
        let next_id_key = "wasm/next_code_id";
        let code_id = match delta.get_raw(next_id_key).await? {
            Some(bytes) => {
                u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| HoError::Storage("Invalid next_code_id bytes".to_string()))?,
                ) + 1
            }
            None => 1,
        };

        // Store code and metadata
        let code_key = format!("wasm/code/{}", code_id);
        delta.put_raw(code_key, wasm_code);

        let hash_key = format!("wasm/code_hash/{}", hex::encode(&code_hash));
        delta.put_raw(hash_key, code_id.to_le_bytes().to_vec());

        // Update next code ID
        delta.put_raw(
            next_id_key.to_string(),
            (code_id + 1).to_le_bytes().to_vec(),
        );

        // Commit changes to storage
        state
            .commit(delta)
            .await
            .map_err(|e| HoError::Storage(format!("Failed to store WASM code: {}", e)))?;

        info!(
            "Stored WASM code with ID: {} for creator: {}",
            code_id, creator
        );

        Ok(code_id)
    }

    /// Query a contract with node-wide consistency guarantee
    ///
    /// Uses shared read lock to allow concurrent queries while ensuring
    /// queries always see the latest committed state.
    pub async fn query_contract(
        &self,
        state: &Storage,
        contract_address: String,
        msg: Vec<u8>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>> {
        // Acquire shared read lock for concurrent queries
        let _node_lock = self.state_lock.read().await;

        let snapshot = state.latest_snapshot();

        // Load contract info to get code_id
        let contract_info = snapshot
            .get_wasm_contract_info(&contract_address)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Contract not found: {}", contract_address)))?;

        let code_id = contract_info.code_id;

        // Load WASM code
        let wasm_code = snapshot
            .get_wasm_code(code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code not found for code_id: {}", code_id)))?;

        // Save WASM to cache filesystem so get_instance can find it
        let checksum = self
            .cache
            .store_code(&wasm_code, true, true)
            .map_err(|e| HoError::Wasm(format!("Failed to save WASM to cache: {}", e)))?;

        // Create state reader for querier
        let state_reader = Arc::new(SnapshotStateReader::new(state.clone()));

        // Create backend components
        let api = WasmVmBackend;
        let querier = CnidariumQuerier::with_state_reader(state_reader);

        // For queries, we use a dummy storage since queries should not write
        let mut dummy_delta = cnidarium::StateDelta::new(snapshot.clone());
        let storage = unsafe { CnidariumStorage::new(contract_address.clone(), &mut dummy_delta) };

        let backend = Backend {
            api,
            storage,
            querier,
        };

        // Create instance options
        let options = InstanceOptions {
            gas_limit: self.query_gas_limit,
        };

        // Get VM instance
        let mut instance = self
            .cache
            .get_instance(&checksum, backend, options)
            .map_err(|e| HoError::Wasm(format!("Failed to create VM instance: {}", e)))?;

        // Create query environment
        let env = self.create_env(&contract_address);

        // Execute query using cosmwasm_vm::call_query
        let result = call_query(&mut instance, &env, &msg)
            .map_err(|e| HoError::Wasm(format!("Query execution failed: {}", e)))?;

        debug!(
            "Contract {} queried with node-wide consistency (latest state)",
            contract_address
        );

        Ok(result)
    }

    /// Instantiate a contract with full VM execution
    pub async fn instantiate_contract(
        &self,
        state: &Storage,
        code_id: u64,
        creator: String,
        admin: Option<String>,
        label: String,
        msg: Vec<u8>,
        funds: Vec<Coin>,
        node_id: &str,
    ) -> HoResult<(String, cosmwasm_std::ContractResult<cosmwasm_std::Response>)> {
        // Acquire node-wide exclusive lock for state consistency
        let _node_lock = self.state_lock.write().await;

        // Generate contract address
        let contract_address =
            self.generate_contract_address(code_id, &creator, &label, node_id)?;

        // Create StateDelta for atomic state updates
        let snapshot = state.latest_snapshot();
        let mut delta = cnidarium::StateDelta::new(snapshot.clone());

        // Load WASM code
        let wasm_code = delta
            .get_wasm_code(code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code not found for code_id: {}", code_id)))?;

        // Save WASM to cache filesystem so get_instance can find it
        // Note: save_wasm persists the source, while store_code only caches compiled modules
        let checksum = self
            .cache
            .store_code(&wasm_code, true, true)
            .map_err(|e| HoError::Wasm(format!("Failed to save WASM to cache: {}", e)))?;

        // Create state reader for querier
        let state_reader = Arc::new(SnapshotStateReader::new(state.clone()));

        // Create backend components
        let api = WasmVmBackend;
        let querier = CnidariumQuerier::with_state_reader(state_reader);

        // Create storage for this contract
        let storage = unsafe { CnidariumStorage::new(contract_address.clone(), &mut delta) };

        let backend = Backend {
            api,
            storage,
            querier,
        };

        // Create instance options
        let options = InstanceOptions {
            gas_limit: self.instantiate_gas_limit,
        };

        // Get VM instance
        let mut instance = self
            .cache
            .get_instance(&checksum, backend, options)
            .map_err(|e| HoError::Wasm(format!("Failed to create VM instance: {}", e)))?;

        // Create environment and message info
        let env = self.create_env(&contract_address);
        let info = self.create_message_info(&creator, funds);

        // Execute instantiate using cosmwasm_vm::call_instantiate
        let result = call_instantiate(&mut instance, &env, &info, &msg)
            .map_err(|e| HoError::Wasm(format!("Instantiate execution failed: {}", e)))?;

        // If instantiate succeeded, store contract info and commit state
        if result.is_ok() {
            // Store contract info
            let contract_info = crate::wasm::state_ext::ContractInfo {
                code_id,
                creator: creator.clone(),
                admin: admin.unwrap_or_default(),
                label: label.clone(),
                created: None,
                ibc_port_id: String::new(),
                extension: None,
            };
            delta.put_wasm_contract_info(&contract_address, &contract_info);

            // Commit state changes
            state
                .commit(delta)
                .await
                .map_err(|e| HoError::Storage(format!("Failed to commit contract state: {}", e)))?;

            info!(
                "Contract {} instantiated with code_id {} by {}",
                contract_address, code_id, creator
            );
        } else {
            warn!(
                "Contract instantiation failed for {}: {:?}",
                contract_address, result
            );
        }

        Ok((contract_address, result))
    }

    /// Execute a contract with node-wide state consistency
    ///
    /// Uses exclusive write lock to ensure all state changes are atomic
    /// and immediately visible to subsequent operations.
    pub async fn execute_contract(
        &self,
        state: &Storage,
        contract_address: String,
        sender: String,
        msg: Vec<u8>,
        funds: Vec<Coin>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Response>> {
        // Acquire node-wide exclusive lock for state consistency
        let _node_lock = self.state_lock.write().await;

        // Create StateDelta for atomic state updates
        let snapshot = state.latest_snapshot();
        let mut delta = cnidarium::StateDelta::new(snapshot.clone());

        // Load contract info to get code_id
        let contract_info = delta
            .get_wasm_contract_info(&contract_address)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Contract not found: {}", contract_address)))?;

        let code_id = contract_info.code_id;

        // Load WASM code
        let wasm_code = delta
            .get_wasm_code(code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code not found for code_id: {}", code_id)))?;

        // Save WASM to cache filesystem so get_instance can find it
        let checksum = self
            .cache
            .store_code(&wasm_code, true, true)
            .map_err(|e| HoError::Wasm(format!("Failed to save WASM to cache: {}", e)))?;

        // Create state reader for querier
        let state_reader = Arc::new(SnapshotStateReader::new(state.clone()));

        // Create backend components
        let api = WasmVmBackend;
        let querier = CnidariumQuerier::with_state_reader(state_reader);

        // Create storage for this contract
        let storage = unsafe { CnidariumStorage::new(contract_address.clone(), &mut delta) };

        let backend = Backend {
            api,
            storage,
            querier,
        };

        // Create instance options
        let options = InstanceOptions {
            gas_limit: self.execute_gas_limit,
        };

        // Get VM instance
        let mut instance = self
            .cache
            .get_instance(&checksum, backend, options)
            .map_err(|e| HoError::Wasm(format!("Failed to create VM instance: {}", e)))?;

        // Create environment and message info
        let env = self.create_env(&contract_address);
        let info = self.create_message_info(&sender, funds);

        // Execute contract using cosmwasm_vm::call_execute
        let result = call_execute(&mut instance, &env, &info, &msg)
            .map_err(|e| HoError::Wasm(format!("Execute failed: {}", e)))?;

        // If execution succeeded, commit state changes
        if result.is_ok() {
            state
                .commit(delta)
                .await
                .map_err(|e| HoError::Storage(format!("Failed to commit contract state: {}", e)))?;

            debug!(
                "Contract {} executed successfully by {}",
                contract_address, sender
            );
        } else {
            warn!(
                "Contract execution failed for {}: {:?}",
                contract_address, result
            );
        }

        Ok(result)
    }

    /// Create an Env struct for contract execution
    fn create_env(&self, contract_address: &str) -> Env {
        Env {
            block: BlockInfo {
                height: 1,
                time: Timestamp::from_seconds(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                ),
                chain_id: "ergors-1".to_string(),
            },
            transaction: None, // TransactionInfo is non-exhaustive, use None for simplicity
            contract: ContractInfo {
                address: Addr::unchecked(contract_address),
            },
        }
    }

    /// Create a MessageInfo struct for contract execution
    fn create_message_info(&self, sender: &str, funds: Vec<Coin>) -> MessageInfo {
        MessageInfo {
            sender: Addr::unchecked(sender),
            funds,
        }
    }

    /// Generate deterministic contract address with node isolation
    fn generate_contract_address(
        &self,
        code_id: u64,
        creator: &str,
        label: &str,
        node_id: &str,
    ) -> HoResult<String> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(node_id.as_bytes());
        hasher.update(code_id.to_le_bytes());
        hasher.update(creator.as_bytes());
        hasher.update(label.as_bytes());
        let hash = hasher.finalize();

        // Use first 20 bytes with node prefix for collision resistance
        Ok(format!("ergors{}_{}", node_id, hex::encode(&hash[..20])))
    }
}

// Stub implementation when 'cw' feature is not enabled
#[cfg(not(feature = "cw"))]
pub struct WasmRuntime;

#[cfg(not(feature = "cw"))]
impl WasmRuntime {
    pub fn new(_cache_dir: PathBuf) -> HoResult<Self> {
        Err(HoError::Storage(
            "CosmWasm support not enabled. Enable 'cw' feature.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "cw")]
    fn test_contract_address_generation() {
        let runtime = WasmRuntime::new(PathBuf::from("/tmp/wasm_test")).unwrap();

        let addr1 = runtime
            .generate_contract_address(1, "creator1", "label1", "node123")
            .unwrap();
        let addr2 = runtime
            .generate_contract_address(1, "creator1", "label1", "node123")
            .unwrap();
        let addr3 = runtime
            .generate_contract_address(2, "creator1", "label1", "node123")
            .unwrap();

        // Same inputs should generate same address (determinism)
        assert_eq!(addr1, addr2);

        // Different inputs should generate different addresses
        assert_ne!(addr1, addr3);
    }

    #[test]
    #[cfg(feature = "cw")]
    fn test_runtime_creation() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let runtime = WasmRuntime::new(temp_dir.path().to_path_buf());

        assert!(runtime.is_ok());
    }
}
