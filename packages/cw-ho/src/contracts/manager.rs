//! Contract Manager implementation
//!
//! Handles contract lifecycle including deployment during startup,
//! named contract resolution, and contract operations.

use super::ContractError;
use crate::config::{ContractDeployment, ErgorsConfig};
use crate::storage::ErgorsStorage;
use ho_std::error::HoResult;
use ho_std::traits::HoConfigTrait;
use ho_std::wasm::WasmRuntime;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Storage keys for contract metadata
const CONTRACT_PREFIX: &str = "contracts";

/// Contract Manager for deploying and interacting with CosmWasm contracts
///
/// Provides:
/// - Automatic contract deployment on coordinator startup
/// - Contract existence checks (skip deployment if already exists)
/// - Named contract address resolution
/// - Unified interface for contract operations
pub struct ContractManager {
    /// Storage for contract metadata
    storage: Arc<ErgorsStorage>,
    /// WASM runtime for contract execution
    wasm_runtime: Arc<WasmRuntime>,
    /// Node identity for signing
    node_id: String,
}

impl ContractManager {
    /// Create a new ContractManager
    ///
    /// # Arguments
    /// * `storage` - Cnidarium-backed storage
    /// * `wasm_runtime` - CosmWasm VM runtime
    /// * `node_id` - Node identifier for contract address generation
    pub fn new(
        storage: Arc<ErgorsStorage>,
        wasm_runtime: Arc<WasmRuntime>,
        node_id: String,
    ) -> Self {
        Self {
            storage,
            wasm_runtime,
            node_id,
        }
    }

    /// Check if a contract exists in storage by name
    pub async fn contract_exists(&self, name: &str) -> HoResult<bool> {
        let key = format!("{}/{}/address", CONTRACT_PREFIX, name);
        let snapshot = self.storage.cs.latest_snapshot();

        use cnidarium::StateRead;
        Ok(snapshot.get_raw(&key).await?.is_some())
    }

    /// Get contract address by name
    pub async fn get_contract_address(&self, name: &str) -> HoResult<Option<String>> {
        let key = format!("{}/{}/address", CONTRACT_PREFIX, name);
        let snapshot = self.storage.cs.latest_snapshot();

        use cnidarium::StateRead;
        match snapshot.get_raw(&key).await? {
            Some(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).to_string())),
            None => Ok(None),
        }
    }

    /// Get contract code_id by name
    pub async fn get_contract_code_id(&self, name: &str) -> HoResult<Option<u64>> {
        let key = format!("{}/{}/code_id", CONTRACT_PREFIX, name);
        let snapshot = self.storage.cs.latest_snapshot();

        use cnidarium::StateRead;
        match snapshot.get_raw(&key).await? {
            Some(bytes) => {
                if bytes.len() >= 8 {
                    let code_id = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                    Ok(Some(code_id))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Upload contract WASM code
    ///
    /// Stores the WASM bytecode and associates it with a name for later reference.
    ///
    /// # Arguments
    /// * `wasm_bytes` - Raw WASM bytecode
    /// * `name` - Name to associate with this contract code
    ///
    /// # Returns
    /// The code_id assigned to this WASM code
    pub async fn upload_contract(&self, wasm_bytes: &[u8], name: &str) -> HoResult<u64> {
        // Store code via WasmRuntime
        let (code_id, _checksum) = self
            .wasm_runtime
            .store_code(&self.storage.cs, wasm_bytes.to_vec(), self.node_id.clone())
            .await
            .map_err(|e| ContractError::UploadFailed(e.to_string()))?;

        // Store code_id reference for this contract name
        let key = format!("{}/{}/code_id", CONTRACT_PREFIX, name);
        let mut delta = cnidarium::StateDelta::new(self.storage.cs.latest_snapshot());

        use cnidarium::StateWrite;
        delta.put_raw(key, code_id.to_le_bytes().to_vec());

        self.storage.cs.commit(delta).await.map_err(|e| {
            ContractError::Storage(format!("Failed to store code_id reference: {}", e))
        })?;

        info!("Uploaded contract '{}' with code_id: {}", name, code_id);
        Ok(code_id)
    }

    /// Instantiate contract from code_id
    ///
    /// Creates a new contract instance and stores its address for later reference.
    ///
    /// # Arguments
    /// * `code_id` - Code ID from upload_contract
    /// * `name` - Name to associate with this contract instance
    /// * `init_msg` - Instantiation message (will be JSON serialized)
    ///
    /// # Returns
    /// The contract address
    pub async fn instantiate_contract<T: Serialize>(
        &self,
        code_id: u64,
        name: &str,
        init_msg: &T,
    ) -> HoResult<String> {
        let msg_bytes = serde_json::to_vec(init_msg)
            .map_err(|e| ContractError::Serialization(e.to_string()))?;

        let (address, response) = self
            .wasm_runtime
            .instantiate_contract(
                &self.storage.cs,
                code_id,
                self.node_id.clone(), // sender/creator
                None,                 // no admin
                name.to_string(),     // label
                msg_bytes,
                vec![], // no funds
                &self.node_id,
            )
            .await
            .map_err(|e| ContractError::InstantiationFailed(e.to_string()))?;

        // Check if instantiation succeeded
        if let Err(err) = response.into_result() {
            return Err(ContractError::InstantiationFailed(err).into());
        }

        // Store contract address for this name
        let key = format!("{}/{}/address", CONTRACT_PREFIX, name);
        let mut delta = cnidarium::StateDelta::new(self.storage.cs.latest_snapshot());

        use cnidarium::StateWrite;
        delta.put_raw(key, address.as_bytes().to_vec());

        self.storage.cs.commit(delta).await.map_err(|e| {
            ContractError::Storage(format!("Failed to store contract address: {}", e))
        })?;

        info!("Instantiated contract '{}' at: {}", name, address);
        Ok(address)
    }

    /// Execute a contract method by name
    ///
    /// # Arguments
    /// * `name` - Contract name (must be previously instantiated)
    /// * `msg` - Execute message (will be JSON serialized)
    pub async fn execute_contract<T: Serialize>(&self, name: &str, msg: &T) -> HoResult<()> {
        let address = self
            .get_contract_address(name)
            .await?
            .ok_or_else(|| ContractError::NotDeployed(name.to_string()))?;

        let msg_bytes =
            serde_json::to_vec(msg).map_err(|e| ContractError::Serialization(e.to_string()))?;

        let response = self
            .wasm_runtime
            .execute_contract(
                &self.storage.cs,
                address.clone(),
                self.node_id.clone(),
                msg_bytes,
                vec![],
            )
            .await
            .map_err(|e| ContractError::ExecutionFailed(e.to_string()))?;

        // Check if execution succeeded
        if let Err(err) = response.into_result() {
            return Err(ContractError::ExecutionFailed(err).into());
        }

        debug!("Executed contract '{}' successfully", name);
        Ok(())
    }

    /// Query a contract by name
    ///
    /// # Arguments
    /// * `name` - Contract name (must be previously instantiated)
    /// * `msg` - Query message (will be JSON serialized)
    ///
    /// # Returns
    /// Deserialized query response
    pub async fn query_contract<T: Serialize, R: DeserializeOwned>(
        &self,
        name: &str,
        msg: &T,
    ) -> HoResult<R> {
        let address = self
            .get_contract_address(name)
            .await?
            .ok_or_else(|| ContractError::NotDeployed(name.to_string()))?;

        let msg_bytes =
            serde_json::to_vec(msg).map_err(|e| ContractError::Serialization(e.to_string()))?;

        let result = self
            .wasm_runtime
            .query_contract(&self.storage.cs, address, msg_bytes)
            .await
            .map_err(|e| ContractError::QueryFailed(e.to_string()))?;

        // Extract the binary result
        let binary = result.into_result().map_err(ContractError::QueryFailed)?;

        // Deserialize the response
        serde_json::from_slice(&binary)
            .map_err(|e| ContractError::Serialization(e.to_string()).into())
    }

    /// Deploy required contracts based on configuration
    ///
    /// This method:
    /// 1. Checks if CosmWasm is enabled
    /// 2. Checks node type against deployment requirements
    /// 3. Iterates through initial_contracts from config
    /// 4. Deploys each contract that matches the node type and doesn't already exist
    ///
    /// # Arguments
    /// * `config` - Node configuration
    pub async fn deploy_required_contracts(&self, config: &ErgorsConfig) -> HoResult<()> {
        // Check if CosmWasm is enabled
        if !config.cosmwasm_enabled() {
            debug!("CosmWasm disabled, skipping contract deployment");
            return Ok(());
        }

        let node_type = &config.identity().node_type;
        let initial_contracts = config.initial_contracts();

        if initial_contracts.is_empty() {
            debug!("No initial contracts configured for deployment");
            return Ok(());
        }

        info!(
            "Processing {} configured contracts for deployment",
            initial_contracts.len()
        );

        for contract in initial_contracts {
            // Check if this contract should be deployed on this node type
            if !self.should_deploy_on_node_type(&contract, node_type) {
                debug!(
                    "Skipping contract '{}': not configured for node_type={}",
                    contract.name, node_type
                );
                continue;
            }

            // Deploy the contract
            match self.deploy_contract_from_config(config, &contract).await {
                Ok(address) => {
                    info!(
                        "Successfully deployed contract '{}' at: {}",
                        contract.name, address
                    );
                }
                Err(e) => {
                    if contract.required {
                        error!(
                            "Failed to deploy required contract '{}': {}",
                            contract.name, e
                        );
                        return Err(e);
                    } else {
                        warn!(
                            "Failed to deploy optional contract '{}': {}",
                            contract.name, e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a contract should be deployed on this node type
    fn should_deploy_on_node_type(&self, contract: &ContractDeployment, node_type: &str) -> bool {
        // If deploy_on_node_types is empty, only deploy on coordinators (default)
        if contract.deploy_on_node_types.is_empty() {
            return node_type == "NODE_TYPE_COORDINATOR" || node_type == "coordinator";
        }

        // Check if current node type is in the list
        contract.deploy_on_node_types.iter().any(|t| {
            t == node_type
                || (t == "coordinator" && node_type == "NODE_TYPE_COORDINATOR")
                || (t == "NODE_TYPE_COORDINATOR" && node_type == "coordinator")
        })
    }

    /// Deploy a contract from configuration
    async fn deploy_contract_from_config(
        &self,
        config: &ErgorsConfig,
        contract: &ContractDeployment,
    ) -> HoResult<String> {
        // Check if contract already exists
        let skip_if_exists = contract
            .config
            .as_ref()
            .map(|c| c.skip_if_exists)
            .unwrap_or(true); // Default to skipping if exists

        if skip_if_exists {
            if let Some(address) = self.get_contract_address(&contract.name).await? {
                info!(
                    "Contract '{}' already deployed at {}, skipping",
                    contract.name, address
                );
                return Ok(address);
            }
        }

        // Load WASM bytes
        let wasm_bytes = self.load_wasm_bytes(config, contract).await?;

        // Upload the contract
        let code_id = self.upload_contract(&wasm_bytes, &contract.name).await?;

        // Parse the init message
        let init_msg_bytes = if contract.init_msg.is_empty() {
            b"{}".to_vec() // Empty JSON object as default
        } else {
            contract.init_msg.as_bytes().to_vec()
        };

        // Get label (use name if label not specified)
        let label = if contract.label.is_empty() {
            contract.name.clone()
        } else {
            contract.label.clone()
        };

        // Get admin (use coordinator's node_id if not specified)
        let admin = if contract.admin.is_empty() {
            Some(self.node_id.clone())
        } else if contract.admin == "none" {
            None
        } else {
            Some(contract.admin.clone())
        };

        // Instantiate the contract
        let (address, response) = self
            .wasm_runtime
            .instantiate_contract(
                &self.storage.cs,
                code_id,
                self.node_id.clone(),
                admin,
                label,
                init_msg_bytes,
                vec![],
                &self.node_id,
            )
            .await
            .map_err(|e| ContractError::InstantiationFailed(e.to_string()))?;

        // Check if instantiation succeeded
        if let Err(err) = response.into_result() {
            return Err(ContractError::InstantiationFailed(err).into());
        }

        // Store contract address for this name
        let key = format!("{}/{}/address", CONTRACT_PREFIX, contract.name);
        let mut delta = cnidarium::StateDelta::new(self.storage.cs.latest_snapshot());

        use cnidarium::StateWrite;
        delta.put_raw(key, address.as_bytes().to_vec());

        self.storage.cs.commit(delta).await.map_err(|e| {
            ContractError::Storage(format!("Failed to store contract address: {}", e))
        })?;

        // Automatically register SDL template contracts
        if contract.name.contains("sdl") && contract.name.contains("template") {
            info!(
                "Automatically registering SDL template contract: {}",
                contract.name
            );
            // Use the original label from config or name
            let sdl_label = if contract.label.is_empty() {
                Some(contract.name.clone())
            } else {
                Some(contract.label.clone())
            };
            if let Err(e) = self
                .storage
                .register_sdl_template_contract(&address, sdl_label, code_id)
                .await
            {
                warn!("Failed to register SDL template contract: {}", e);
            }
        }

        Ok(address)
    }

    /// Load WASM bytes from file path or embedded bytes
    async fn load_wasm_bytes(
        &self,
        config: &ErgorsConfig,
        contract: &ContractDeployment,
    ) -> HoResult<Vec<u8>> {
        // Prefer embedded bytes if available
        if !contract.wasm_bytes.is_empty() {
            return Ok(contract.wasm_bytes.clone());
        }

        // Load from file path
        if contract.wasm_path.is_empty() {
            return Err(ContractError::UploadFailed(format!(
                "Contract '{}' has no wasm_path or wasm_bytes specified",
                contract.name
            ))
            .into());
        }

        let wasm_path = config.resolve_wasm_path(&contract.wasm_path);
        std::fs::read(wasm_path.as_std_path()).map_err(|e| {
            ContractError::UploadFailed(format!("Failed to read WASM file '{}': {}", wasm_path, e))
                .into()
        })
    }

    /// Deploy the identity registry contract with provided WASM binary
    ///
    /// This is called explicitly when the contract binary is available.
    ///
    /// # Arguments
    /// * `wasm_bytes` - Identity registry WASM bytecode
    /// * `coordinator_pubkey` - Coordinator's public key for contract admin
    /// * `providers` - Initial provider configurations
    pub async fn deploy_identity_registry(
        &self,
        wasm_bytes: &[u8],
        coordinator_pubkey: Vec<u8>,
        providers: Vec<ProviderConfig>,
    ) -> HoResult<String> {
        // Check if already deployed
        if let Some(address) = self.get_contract_address("identity_registry").await? {
            info!("Identity registry already deployed at: {}", address);
            return Ok(address);
        }

        // Upload the contract
        let code_id = self
            .upload_contract(wasm_bytes, "identity_registry")
            .await?;

        // Prepare instantiation message
        let init_msg = IdentityRegistryInstantiateMsg {
            coordinator: coordinator_pubkey,
            providers,
        };

        // Instantiate the contract
        let address = self
            .instantiate_contract(code_id, "identity_registry", &init_msg)
            .await?;

        info!("Deployed identity_registry contract at: {}", address);
        Ok(address)
    }
}

/// Instantiation message for the identity registry contract
#[derive(Debug, Clone, Serialize)]
pub struct IdentityRegistryInstantiateMsg {
    /// Coordinator's public key (has admin rights)
    pub coordinator: Vec<u8>,
    /// Initial provider configurations
    pub providers: Vec<ProviderConfig>,
}

/// Provider configuration for the identity registry
#[derive(Debug, Clone, Serialize)]
pub struct ProviderConfig {
    /// Provider name (e.g., "anthropic", "openai")
    pub name: String,
    /// Ownership type: "shared" or "local"
    pub ownership: String,
    /// For shared providers: threshold for Shamir reconstruction
    pub threshold: Option<u32>,
    /// For shared providers: total shares to generate
    pub total_shares: Option<u32>,
}

/// Instantiation message for the auth_registry_updater contract
#[derive(Debug, Clone, Serialize)]
pub struct AuthRegistryUpdaterInstantiateMsg {
    /// Coordinator's public key (has admin rights)
    pub coordinator: String,
    /// Initial list of addresses authorized to update the registry
    pub initial_authorized: Option<Vec<String>>,
}

/// Instantiation message for the whitelist_authenticator contract
#[derive(Debug, Clone, Serialize)]
pub struct WhitelistAuthenticatorInstantiateMsg {
    /// Admin who can modify the whitelist
    pub admin: String,
    /// Optional description of this authenticator
    pub description: Option<String>,
    /// Initial addresses to whitelist
    pub initial_whitelist: Option<Vec<String>>,
    /// Whether to allow all addresses by default (open policy)
    pub default_allow: Option<bool>,
}

impl ContractManager {
    /// Deploy the auth registry updater contract
    ///
    /// This contract controls who can update the authenticator registry.
    /// It's automatically deployed on coordinator nodes during startup.
    ///
    /// # Arguments
    /// * `wasm_bytes` - Auth registry updater WASM bytecode
    /// * `coordinator_address` - Coordinator's address for admin rights
    pub async fn deploy_auth_registry_updater(
        &self,
        wasm_bytes: &[u8],
        coordinator_address: String,
    ) -> HoResult<String> {
        // Check if already deployed
        if let Some(address) = self.get_contract_address("auth_registry_updater").await? {
            info!("Auth registry updater already deployed at: {}", address);
            return Ok(address);
        }

        // Upload the contract
        let code_id = self
            .upload_contract(wasm_bytes, "auth_registry_updater")
            .await?;

        // Prepare instantiation message
        let init_msg = AuthRegistryUpdaterInstantiateMsg {
            coordinator: coordinator_address,
            initial_authorized: None,
        };

        // Instantiate the contract
        let address = self
            .instantiate_contract(code_id, "auth_registry_updater", &init_msg)
            .await?;

        info!("Deployed auth_registry_updater contract at: {}", address);
        Ok(address)
    }

    /// Deploy a whitelist authenticator contract for a specific endpoint
    ///
    /// # Arguments
    /// * `wasm_bytes` - Whitelist authenticator WASM bytecode
    /// * `endpoint_name` - Name/identifier for this authenticator
    /// * `admin_address` - Admin who can modify the whitelist
    /// * `description` - Description of what this authenticator protects
    /// * `initial_whitelist` - Initial addresses to whitelist
    pub async fn deploy_whitelist_authenticator(
        &self,
        wasm_bytes: &[u8],
        endpoint_name: &str,
        admin_address: String,
        description: Option<String>,
        initial_whitelist: Option<Vec<String>>,
    ) -> HoResult<String> {
        let contract_name = format!("whitelist_auth_{}", endpoint_name);

        // Check if already deployed
        if let Some(address) = self.get_contract_address(&contract_name).await? {
            info!(
                "Whitelist authenticator '{}' already deployed at: {}",
                contract_name, address
            );
            return Ok(address);
        }

        // Upload the contract (reuse code if same bytecode already uploaded)
        let code_id = self.upload_contract(wasm_bytes, &contract_name).await?;

        // Prepare instantiation message
        let init_msg = WhitelistAuthenticatorInstantiateMsg {
            admin: admin_address,
            description,
            initial_whitelist,
            default_allow: Some(false), // Default to deny mode (allowlist)
        };

        // Instantiate the contract
        let address = self
            .instantiate_contract(code_id, &contract_name, &init_msg)
            .await?;

        info!(
            "Deployed whitelist_authenticator '{}' at: {}",
            contract_name, address
        );
        Ok(address)
    }
}
