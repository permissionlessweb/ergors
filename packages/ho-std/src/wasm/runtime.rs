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
    crate::wasm::backend::{CnidariumQuerier, CnidariumStorage, WasmVmBackend},
    cosmwasm_std::{Addr, Coin, Env, MessageInfo, Timestamp},
    cosmwasm_vm::{capabilities_from_csv, Backend, Cache, CacheOptions, InstanceOptions, Size},
    sha2::{Digest, Sha256},
    std::path::PathBuf,
    std::sync::Arc,
    tracing::{debug, info},
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
        delta.put_raw(next_id_key.to_string(), (code_id + 1).to_le_bytes().to_vec());

        // Commit changes to storage
        state.commit(delta).await.map_err(|e| {
            HoError::Storage(format!("Failed to store WASM code: {}", e))
        })?;

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
        _state: &Storage,
        contract_address: String,
        _msg: Vec<u8>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>> {
        // Acquire shared read lock for concurrent queries
        let _node_lock = self.state_lock.read().await;

        // TODO: Implement full contract querying
        debug!("Contract {} queried with node-wide consistency (latest state)", contract_address);

        // Placeholder response - would be actual contract query result
        Ok(cosmwasm_std::ContractResult::Ok(cosmwasm_std::Binary::default()))
    }

    /// Instantiate a contract (placeholder for now)
    pub async fn instantiate_contract(
        &self,
        _state: &Storage,
        code_id: u64,
        creator: String,
        _admin: Option<String>,
        label: String,
        _msg: Vec<u8>,
        _funds: Vec<Coin>,
        node_id: &str,
    ) -> HoResult<(String, cosmwasm_std::ContractResult<cosmwasm_std::Response>)> {
        // Acquire node-wide exclusive lock for state consistency
        let _node_lock = self.state_lock.write().await;

        // Generate contract address
        let contract_address = self.generate_contract_address(code_id, &creator, &label, node_id)?;

        debug!("Contract {} instantiated with node-wide synchronization", contract_address);

        // Placeholder response
        Ok((contract_address, cosmwasm_std::ContractResult::Ok(cosmwasm_std::Response::default())))
    }

    /// Execute a contract with node-wide state consistency
    ///
    /// Uses exclusive write lock to ensure all state changes are atomic
    /// and immediately visible to subsequent operations.
    pub async fn execute_contract(
        &self,
        _state: &Storage,
        contract_address: String,
        _sender: String,
        _msg: Vec<u8>,
        _funds: Vec<Coin>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Response>> {
        // Acquire node-wide exclusive lock for state consistency
        let _node_lock = self.state_lock.write().await;

        // TODO: Implement full contract execution
        // 1. Load contract info from state
        // 2. Load contract code from cache
        // 3. Create execution environment
        // 4. Execute contract with message
        // 5. Apply state changes via StateDelta
        // 6. Commit to storage

        debug!("Contract {} executed with node-wide synchronization", contract_address);

        // Placeholder response
        Ok(cosmwasm_std::ContractResult::Ok(cosmwasm_std::Response::default()))
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
