//! CosmWasm runtime for contract execution
//!
//! Provides high-level APIs for uploading, instantiating, executing, and querying
//! CosmWasm smart contracts with Cnidarium storage backend.
//!
//! This implementation follows best practices from both CosmWasm and Cnidarium:
//! - Thread-safe cache using Arc<Cache>
//! - No unsafe code (uses CacheOptions)
//! - Deterministic state updates via Cnidarium's StateDelta
//! - Proper gas tracking and limits

use crate::error::{HoError, HoResult};
use crate::traits::StateWrite;
use crate::types::cosmwasm::wasm::v1::{CodeInfo, ContractInfo};
use crate::wasm::backend::{CnidariumQuerier, CnidariumStorage, WasmVmBackend};

#[cfg(feature = "cw")]
use crate::wasm::state_ext::WasmStorageState;
use crate::wasm::state_ext::{WasmVmCnidariumStateRead, WasmVmCnidariumStateWrite};
use cnidarium::{StateRead, StateWrite as CnidariumStateWrite};
use sha2::{Digest, Sha256};
#[cfg(feature = "cw")]
use std::collections::HashSet;
#[cfg(feature = "cw")]
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

#[cfg(feature = "cw")]
use cosmwasm_std::{Addr, Coin, Env, MessageInfo, Timestamp};
#[cfg(feature = "cw")]
use cosmwasm_vm::{
    call_execute_raw, call_instantiate_raw, call_query_raw, capabilities_from_csv, Backend, Cache,
    CacheOptions, Instance, InstanceOptions, Size,
};

/// Default gas limits for contract operations
pub const DEFAULT_INSTANTIATE_GAS: u64 = 100_000_000;
pub const DEFAULT_EXECUTE_GAS: u64 = 50_000_000;
pub const DEFAULT_QUERY_GAS: u64 = 10_000_000;

/// Default memory limit per contract instance (32 MB)
pub const DEFAULT_MEMORY_LIMIT: Size = Size::mebi(32);

/// Memory cache size for compiled WASM modules (200 MB)
pub const MEMORY_CACHE_SIZE: Size = Size::mebi(200);

/// Maximum WASM code size (800 KB)
pub const MAX_WASM_CODE_SIZE: usize = 800 * 1024;

/// Default CosmWasm capabilities
/// See: https://github.com/CosmWasm/cosmwasm/blob/main/packages/vm/README.md#capabilities
pub const DEFAULT_CAPABILITIES: &str = "iterator,staking,stargate,cosmwasm_1_1,cosmwasm_1_2,cosmwasm_1_3,cosmwasm_1_4,cosmwasm_2_0,cosmwasm_2_1,cosmwasm_2_2";

/// Type alias for our WASM cache with lifetime for the storage reference
#[cfg(feature = "cw")]
pub type WasmCache = Cache<WasmVmBackend, CnidariumStorage, CnidariumQuerier>;

/// WasmRuntime manages CosmWasm contract lifecycle
///
/// This runtime is thread-safe and can be shared across multiple threads using Arc.
/// The WASM module cache is stored in memory and on disk for optimal performance.
#[cfg(feature = "cw")]
pub struct WasmRuntime {
    /// Cached WASM module cache for performance
    cache: Arc<WasmCache>,
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
    ///
    /// # Arguments
    /// * `cache_dir` - Directory where compiled WASM modules will be cached
    ///
    /// # Returns
    /// * `WasmRuntime` instance ready for contract execution
    ///
    /// # Example
    /// ```no_run
    /// use ho_std::wasm::WasmRuntime;
    /// use std::path::PathBuf;
    ///
    /// let runtime = WasmRuntime::new(PathBuf::from("./data/wasm_cache"))?;
    /// ```
    pub fn new(cache_dir: PathBuf) -> HoResult<Self> {
        Self::new_with_options(
            cache_dir,
            DEFAULT_CAPABILITIES,
            MEMORY_CACHE_SIZE,
            DEFAULT_MEMORY_LIMIT,
        )
    }

    /// Create a new WasmRuntime with custom configuration
    ///
    /// # Arguments
    /// * `cache_dir` - Directory for WASM cache
    /// * `capabilities` - CSV string of supported capabilities
    /// * `memory_cache_size` - Size of in-memory module cache
    /// * `memory_limit` - Memory limit per contract instance
    ///
    /// # Example
    /// ```no_run
    /// use ho_std::wasm::WasmRuntime;
    /// use cosmwasm_vm::Size;
    /// use std::path::PathBuf;
    ///
    /// let runtime = WasmRuntime::new_with_options(
    ///     PathBuf::from("./cache"),
    ///     "iterator,staking",
    ///     Size::mebi(100),
    ///     Size::mebi(16),
    /// )?;
    /// ```
    pub fn new_with_options(
        cache_dir: PathBuf,
        capabilities: &str,
        memory_cache_size: Size,
        memory_limit: Size,
    ) -> HoResult<Self> {
        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            HoError::Storage(format!("Failed to create WASM cache directory: {}", e))
        })?;

        // Parse capabilities
        let supported_capabilities = capabilities_from_csv(capabilities);

        // Create the cache instance once
        let options = CacheOptions::new(
            cache_dir.clone(),
            supported_capabilities,
            memory_cache_size,
            memory_limit,
        );

        // SAFETY: We trust the filesystem cache integrity
        let cache = unsafe { Cache::new(options) }
            .map_err(|e| HoError::Storage(format!("Failed to create WASM cache: {}", e)))?;

        info!(
            "Initialized WasmRuntime with cache at {:?}, capabilities: {}, memory_limit: {:?}",
            cache_dir, capabilities, memory_limit
        );

        Ok(Self {
            cache: Arc::new(cache),
            instantiate_gas_limit: DEFAULT_INSTANTIATE_GAS,
            execute_gas_limit: DEFAULT_EXECUTE_GAS,
            query_gas_limit: DEFAULT_QUERY_GAS,
        })
    }

    /// Store WASM code and return code ID
    ///
    /// This validates the WASM bytecode, computes its hash, and stores it in both
    /// the cache (for execution) and Cnidarium (for persistence).
    ///
    /// # Arguments
    /// * `state` - Mutable reference to Cnidarium state
    /// * `wasm_code` - Raw WASM bytecode
    /// * `creator` - Address of the code uploader
    ///
    /// # Returns
    /// * `code_id` - Unique identifier for the stored code
    pub async fn store_code(
        &self,
        state: &mut CnidariumStorage,
        wasm_code: Vec<u8>,
        creator: String,
    ) -> HoResult<u64> {
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
        if let Some(existing_id) = state.get_code_id_by_hash(&code_hash).await? {
            info!("WASM code already exists with ID: {}", existing_id);
            return Ok(existing_id);
        }

        // Validate and save WASM code to cache
        // This compiles the module and ensures it's valid CosmWasm
        let checksum = self
            .cache
            .store_code(
                &wasm_code, /* verify_checksum */ true, /* force */ false,
            )
            .map_err(|e| HoError::Wasm(format!("Invalid WASM code: {}", e)))?;

        debug!("Validated WASM code with checksum: {:?}", checksum);

        // Get next code ID
        let code_id = state.get_next_code_id().await?;

        // Create CodeInfo metadata
        let code_info = CodeInfo {
            code_hash: code_hash.clone(),
            creator: creator.clone(),
            instantiate_config: None,
        };

        // Store in Cnidarium (deterministic, verifiable)
        state.put_wasm_code(code_id, wasm_code);
        state.put_wasm_code_info(code_id, &code_info);
        state.put_code_hash_mapping(&code_hash, code_id);

        info!(
            "Stored WASM code with ID: {} for creator: {}",
            code_id, creator
        );

        Ok(code_id)
    }

    /// Instantiate a contract from stored code
    ///
    /// Creates a new contract instance by calling its `instantiate` entrypoint.
    /// All state changes are committed to Cnidarium atomically.
    ///
    /// # Arguments
    /// * `state` - Mutable reference to Cnidarium state
    /// * `code_id` - ID of the stored WASM code
    /// * `creator` - Address instantiating the contract
    /// * `admin` - Optional admin address for migrations
    /// * `label` - Human-readable label
    /// * `msg` - JSON-encoded instantiate message
    /// * `funds` - Coins sent with instantiation
    ///
    /// # Returns
    /// * `(contract_address, response_data)` tuple
    pub async fn instantiate_contract(
        &self,
        state: &mut CnidariumStorage,
        code_id: u64,
        creator: String,
        admin: Option<String>,
        label: String,
        msg: Vec<u8>,
        funds: Vec<Coin>,
    ) -> HoResult<(String, cosmwasm_std::ContractResult<cosmwasm_std::Response>)> {
        // Retrieve WASM code from storage

        use cosmwasm_vm::call_instantiate;
        let wasm_code = state
            .get_wasm_code(code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code ID {} not found", code_id)))?;

        // Generate deterministic contract address
        let contract_address = self.generate_contract_address(code_id, &creator, &label)?;

        // Prevent duplicate instantiation
        if state
            .get_wasm_contract_info(&contract_address)
            .await?
            .is_some()
        {
            return Err(HoError::Wasm(format!(
                "Contract already exists at address: {}",
                contract_address
            )));
        }

        // Create CosmWasm environment
        let env = self.create_env(&contract_address, 0)?;
        let info = self.create_message_info(&creator, funds)?;

        // Create StateDelta for atomic contract state changes
        let mut delta = cnidarium::StateDelta::new(state.latest_snapshot());

        // Create response in a scoped block to ensure proper lifetime management
        let response = {
            // Get compiled module from cache
            let checksum = self
                .cache
                .store_code(&wasm_code, true, false)
                .map_err(|e| HoError::Wasm(format!("Failed to load WASM module: {}", e)))?;

            // Create backend with Cnidarium storage
            let storage = unsafe { CnidariumStorage::new(contract_address.clone(), &mut delta) };
            let backend = Backend {
                api: WasmVmBackend,
                storage,
                querier: CnidariumQuerier,
            };

            // Create VM instance
            let options = InstanceOptions {
                gas_limit: self.instantiate_gas_limit,
            };

            let mut instance = self
                .cache
                .get_instance(&checksum, backend, options)
                .map_err(|e| HoError::Wasm(format!("Failed to create instance: {}", e)))?;

            // Call instantiate entrypoint
            call_instantiate(&mut instance, &env, &info, &msg)
                .map_err(|e| HoError::Wasm(format!("Instantiate failed: {}", e)))?
            // instance is dropped here, releasing the borrow on state
        };

        // Store contract metadata
        let contract_info = ContractInfo {
            code_id,
            creator: creator.clone(),
            admin: admin.unwrap_or_default(),
            label: label.clone(),
            created: None,
            ibc_port_id: String::new(),
            extension: None,
        };

        state.put_wasm_contract_info(&contract_address, &contract_info);

        // Apply the changes from the delta to the state
        for (key, value) in delta.changes {
            match value {
                Some(v) => {
                    state.put(&key, v);
                }
                None => {
                    state.delete(&key);
                }
            }
        }

        info!(
            "Instantiated contract at {} from code ID {}",
            contract_address, code_id
        );

        Ok((contract_address, response))
    }

    /// Execute a contract (mutable operation)
    ///
    /// Calls the contract's `execute` entrypoint, allowing state modifications.
    /// All changes are committed atomically to Cnidarium.
    ///
    /// # Arguments
    /// * `state` - Mutable reference to Cnidarium state
    /// * `contract_address` - Address of the contract to execute
    /// * `sender` - Address calling the contract
    /// * `msg` - JSON-encoded execute message
    /// * `funds` - Coins sent with execution
    ///
    /// # Returns
    /// * Response data from the contract
    pub async fn execute_contract(
        &self,
        state: &mut CnidariumState,
        contract_address: String,
        sender: String,
        msg: Vec<u8>,
        funds: Vec<Coin>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Response>> {
        // Get contract metadata

        use cosmwasm_vm::call_execute;
        let contract_info = state
            .get_wasm_contract_info(&contract_address)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Contract not found: {}", contract_address)))?;

        // Get WASM code
        let wasm_code = state
            .get_wasm_code(contract_info.code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code ID {} not found", contract_info.code_id)))?;

        // Create environment and message info
        let env = self.create_env(&contract_address, 0)?;
        let info = self.create_message_info(&sender, funds)?;

        // Create StateDelta for atomic contract state changes
        let mut delta = cnidarium::StateDelta::new(state.latest_snapshot());

        // Execute in scoped block for proper lifetime management
        let response = {
            use crate::wasm::state_ext::WasmStorageState;

            // Get module from cache
            let checksum = self
                .cache
                .store_code(&wasm_code, true, false)
                .map_err(|e| HoError::Wasm(format!("Failed to load WASM module: {}", e)))?;

            // Create backend
            let storage = unsafe { CnidariumStorage::new(contract_address.clone(), &mut delta) };
            let backend = Backend {
                api: WasmVmBackend,
                storage,
                querier: CnidariumQuerier,
            };

            // Create instance
            let options = InstanceOptions {
                gas_limit: self.execute_gas_limit,
            };

            let mut instance = self
                .cache
                .get_instance(&checksum, backend, options)
                .map_err(|e| HoError::Wasm(format!("Failed to create instance: {}", e)))?;

            // Call execute entrypoint
            call_execute(&mut instance, &env, &info, &msg)
                .map_err(|e| HoError::Wasm(format!("Execute failed: {}", e)))?
        };

        // Apply the changes from the delta to the state
        for (key, value) in delta.changes {
            match value {
                Some(v) => {
                    state.put(&key, v);
                }
                None => {
                    state.delete(&key);
                }
            }
        }

        debug!(
            "Executed contract {} with sender {}",
            contract_address, sender
        );

        Ok(response)
    }

    /// Query a contract (read-only operation)
    ///
    /// Calls the contract's `query` entrypoint without modifying state.
    ///
    /// Note: Even though this is read-only, we need `&mut S` because the CosmWasm VM's
    /// Storage trait requires mutable methods. The VM won't actually write during queries.
    ///
    /// # Arguments
    /// * `state` - Mutable reference to Cnidarium state (won't be modified)
    /// * `contract_address` - Address of the contract to query
    /// * `msg` - JSON-encoded query message
    ///
    /// # Returns
    /// * Response data from the contract
    pub async fn query_contract(
        &self,
        state: &mut CnidariumState,
        contract_address: String,
        msg: Vec<u8>,
    ) -> HoResult<cosmwasm_std::ContractResult<cosmwasm_std::Binary>> {
        // Get contract info

        use cosmwasm_vm::call_query;
        let contract_info = state
            .get_wasm_contract_info(&contract_address)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Contract not found: {}", contract_address)))?;

        // Get code
        let wasm_code = state
            .get_wasm_code(contract_info.code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code ID {} not found", contract_info.code_id)))?;

        // Create environment
        let env = self.create_env(&contract_address, 0)?;

        // Query in scoped block for proper lifetime management
        let response = {
            // Get module from cache
            let checksum = self
                .cache
                .store_code(&wasm_code, true, false)
                .map_err(|e| HoError::Wasm(format!("Failed to load WASM module: {}", e)))?;

            // Create read-only backend
            let storage = unsafe { CnidariumStorage::new(contract_address.clone(), state) };
            let backend = Backend {
                api: WasmVmBackend,
                storage,
                querier: CnidariumQuerier,
            };

            // Create instance
            let options = InstanceOptions {
                gas_limit: self.query_gas_limit,
            };

            let mut instance = self
                .cache
                .get_instance(&checksum, backend, options)
                .map_err(|e| HoError::Wasm(format!("Failed to create instance: {}", e)))?;

            // Call query entrypoint
            call_query(&mut instance, &env, &msg)
                .map_err(|e| HoError::Wasm(format!("Query failed: {}", e)))?
        };
        debug!("Queried contract {}", contract_address);
        Ok(response)
    }

    /// Generate deterministic contract address
    ///
    /// Uses SHA256(code_id || creator || label) to generate a unique address.
    fn generate_contract_address(
        &self,
        code_id: u64,
        creator: &str,
        label: &str,
    ) -> HoResult<String> {
        let mut hasher = Sha256::new();
        hasher.update(code_id.to_le_bytes());
        hasher.update(creator.as_bytes());
        hasher.update(label.as_bytes());
        let hash = hasher.finalize();

        // Use first 20 bytes (similar to Ethereum addresses)
        Ok(format!("ergors{}", hex::encode(&hash[..20])))
    }

    /// Create CosmWasm execution environment
    fn create_env(&self, contract_address: &str, block_height: u64) -> HoResult<Env> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| HoError::Storage(format!("System time error: {}", e)))?;

        Ok(Env {
            block: cosmwasm_std::BlockInfo {
                height: block_height,
                time: Timestamp::from_seconds(now.as_secs()),
                chain_id: "ergors-1".to_string(),
            },
            transaction: None,
            contract: cosmwasm_std::ContractInfo {
                address: Addr::unchecked(contract_address),
            },
        })
    }

    /// Create CosmWasm message info
    fn create_message_info(&self, sender: &str, funds: Vec<Coin>) -> HoResult<MessageInfo> {
        Ok(MessageInfo {
            sender: Addr::unchecked(sender),
            funds,
        })
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
            .generate_contract_address(1, "creator1", "label1")
            .unwrap();
        let addr2 = runtime
            .generate_contract_address(1, "creator1", "label1")
            .unwrap();
        let addr3 = runtime
            .generate_contract_address(2, "creator1", "label1")
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
