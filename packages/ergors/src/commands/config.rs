//! Config command for programmatically managing ERGORS node configuration
//!
//! Provides `ergors config` subcommands for setting, getting, and listing
//! configuration values with type validation.

use anyhow::{anyhow, Result};
use camino::Utf8Path;

use ho_std::constants::CONFIG_FILE_NAME;
use ho_std::traits::HoConfigTrait;
use ho_std::types::ergors::{
    management::v1::{
        management_service_client::ManagementServiceClient, DeleteChainConfigRequest,
        GetChainConfigRequest, ListChainConfigsRequest, ListCliKeysRequest,
        RegisterCliKeyRequest, RevokeCliKeyRequest, SetChainConfigRequest,
    },
    network::v1::{ChannelConfig, NetworkConfig, NodeIdentity},
    orch::v1::{
        AkashDeployConfig, ContractConfig, ContractDeployment, CosmosChainConfig, CosmwasmConfig,
        LlmRouterConfig,
    },
    storage::v1::StorageConfig,
};

use crate::config::ErgorsConfig;

/// Config command for managing node configuration
#[derive(Debug, clap::Parser)]
pub struct ConfigCmd {
    #[clap(subcommand)]
    pub subcmd: ConfigSubCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum ConfigSubCmd {
    /// Set a configuration value (e.g., `config set network.listen_port 9090`)
    #[clap(display_order = 100)]
    Set {
        /// Dot-separated config path (e.g., "network.listen_port", "identity.host")
        key: String,
        /// Value to set (will be parsed based on key type)
        value: String,
    },

    /// Get a configuration value
    #[clap(display_order = 200)]
    Get {
        /// Dot-separated config path
        key: String,
    },

    /// List all configuration keys and their types
    #[clap(display_order = 300)]
    List {},

    /// Initialize a minimal valid configuration file
    #[clap(display_order = 400)]
    Init {
        /// Node type: coordinator, executor, referee, or development
        #[clap(long, default_value = "development")]
        node_type: String,

        /// gRPC/API port
        #[clap(long, default_value = "50051")]
        api_port: u32,

        /// P2P port
        #[clap(long, default_value = "26656")]
        p2p_port: u32,

        /// Enable CosmWasm and deploy SDL template contract on startup
        #[clap(long)]
        with_sdl_contract: bool,

        /// Path to SDL template registrar WASM file (required if --with-sdl-contract)
        #[clap(long)]
        sdl_wasm_path: Option<String>,
    },

    /// Configure a Cosmos SDK chain (stored in cnidarium)
    #[clap(display_order = 500)]
    SetChain {
        /// Chain ID (e.g., "akashnet-2", "local", "osmosis-1")
        chain_id: String,

        /// Human-readable chain name
        #[clap(long)]
        name: String,

        /// Bech32 address prefix (e.g., "akash", "osmo", "cosmos")
        #[clap(long)]
        prefix: String,

        /// Base denom (e.g., "uakt", "uosmo", "uatom")
        #[clap(long)]
        denom: String,

        /// RPC endpoints (comma-separated)
        #[clap(long)]
        rpc: String,

        /// gRPC endpoints (comma-separated)
        #[clap(long)]
        grpc: String,

        /// REST/LCD endpoints (comma-separated, optional)
        #[clap(long)]
        rest: Option<String>,

        /// Gas prices (e.g., "0.025uakt")
        #[clap(long, default_value = "0.025")]
        gas_prices: String,

        /// Gas adjustment multiplier
        #[clap(long, default_value = "1.5")]
        gas_adjustment: f64,

        /// Keyring backend (os, file, test)
        #[clap(long, default_value = "test")]
        keyring_backend: String,

        /// Default key name
        #[clap(long, default_value = "default")]
        default_key: String,
    },

    /// Get a Cosmos chain configuration
    #[clap(display_order = 600)]
    GetChain {
        /// Chain ID to retrieve
        chain_id: String,
    },

    /// List all configured Cosmos chains
    #[clap(display_order = 700)]
    ListChains {},

    /// Delete a Cosmos chain configuration
    #[clap(display_order = 800)]
    DeleteChain {
        /// Chain ID to delete
        chain_id: String,
    },

    /// Register an Ed25519 public key for authenticated remote CLI access
    #[clap(display_order = 900)]
    RegisterCliKey {
        /// Ed25519 public key (64 hex chars)
        pubkey_hex: String,
        /// Human-readable label for this key
        #[clap(long, default_value = "cli")]
        label: String,
    },

    /// Revoke an authorized CLI key
    #[clap(display_order = 1000)]
    RevokeCliKey {
        /// Ed25519 public key to revoke (64 hex chars)
        pubkey_hex: String,
    },

    /// List all authorized CLI keys
    #[clap(display_order = 1100)]
    ListCliKeys {},
}

impl ConfigCmd {
    pub fn exec(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        match &self.subcmd {
            ConfigSubCmd::Set { key, value } => self.set_config(home_dir, key, value),
            ConfigSubCmd::Get { key } => self.get_config(home_dir, key),
            ConfigSubCmd::List {} => self.list_config(home_dir, json),
            ConfigSubCmd::Init {
                node_type,
                api_port,
                p2p_port,
                with_sdl_contract,
                sdl_wasm_path,
            } => self.init_config(
                home_dir,
                node_type,
                *api_port,
                *p2p_port,
                *with_sdl_contract,
                sdl_wasm_path.as_deref(),
            ),
            ConfigSubCmd::SetChain {
                chain_id,
                name,
                prefix,
                denom,
                rpc,
                grpc,
                rest,
                gas_prices,
                gas_adjustment,
                keyring_backend,
                default_key,
            } => {
                tokio::runtime::Runtime::new()?.block_on(self.set_chain(
                    home_dir,
                    chain_id,
                    name,
                    prefix,
                    denom,
                    rpc,
                    grpc,
                    rest.as_deref(),
                    gas_prices,
                    *gas_adjustment,
                    keyring_backend,
                    default_key,
                ))
            }
            ConfigSubCmd::GetChain { chain_id } => {
                tokio::runtime::Runtime::new()?.block_on(self.get_chain(home_dir, chain_id))
            }
            ConfigSubCmd::ListChains {} => {
                tokio::runtime::Runtime::new()?.block_on(self.list_chains(home_dir, json))
            }
            ConfigSubCmd::DeleteChain { chain_id } => {
                tokio::runtime::Runtime::new()?.block_on(self.delete_chain(home_dir, chain_id, json))
            }
            ConfigSubCmd::RegisterCliKey { pubkey_hex, label } => {
                tokio::runtime::Runtime::new()?
                    .block_on(self.register_cli_key(home_dir, pubkey_hex, label, json))
            }
            ConfigSubCmd::RevokeCliKey { pubkey_hex } => {
                tokio::runtime::Runtime::new()?
                    .block_on(self.revoke_cli_key(home_dir, pubkey_hex, json))
            }
            ConfigSubCmd::ListCliKeys {} => {
                tokio::runtime::Runtime::new()?
                    .block_on(self.list_cli_keys(home_dir, json))
            }
        }
    }

    /// Initialize a minimal valid configuration
    fn init_config(
        &self,
        home_dir: &Utf8Path,
        node_type: &str,
        api_port: u32,
        p2p_port: u32,
        with_sdl_contract: bool,
        sdl_wasm_path: Option<&str>,
    ) -> Result<()> {
        let config_path = home_dir.join(CONFIG_FILE_NAME);

        // Map node_type string to enum value
        let node_type_int = match node_type.to_lowercase().as_str() {
            "coordinator" => 1,
            "executor" => 2,
            "referee" => 3,
            "development" | "dev" => 4,
            _ => {
                return Err(anyhow!(
                    "Invalid node_type '{}'. Use: coordinator, executor, referee, development",
                    node_type
                ))
            }
        };

        // Capitalize for identity.node_type string
        let node_type_cap = match node_type.to_lowercase().as_str() {
            "coordinator" => "Coordinator",
            "executor" => "Executor",
            "referee" => "Referee",
            _ => "Development",
        };

        // Create storage directory
        let data_dir = home_dir.join("data");
        std::fs::create_dir_all(&data_dir)?;

        // Build minimal valid config
        let config = crate::config::ErgorsConfig(ho_std::types::ergors::orch::v1::HoConfig {
            home: home_dir.to_string(),
            network: Some(NetworkConfig {
                node_type: node_type_int,
                bootstrap_peers: vec![],
                known_peers: vec![],
                listen_port: p2p_port,
                listen_address: "0.0.0.0".to_string(),
                connection_timeout_ms: 5000,
                enable_discovery: false,
                limits: None,
                channels: Some(ChannelConfig {
                    discovery_buffer: 100,
                    task_buffer: 100,
                    state_buffer: 100,
                    health_buffer: 50,
                    key_sharing_buffer: 50,
                }),
            }),
            identity: Some(NodeIdentity {
                host: "127.0.0.1".to_string(),
                p2p_port,
                api_port,
                user: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
                os: 1, // Linux
                ssh_port: 22,
                node_type: node_type_cap.to_string(),
                public_key: None,
                bech32_address: None,
            }),
            storage: Some(StorageConfig {
                data_dir: data_dir.to_string(),
                max_size_mb: 1024,
                enable_compression: false,
            }),
            llm: Some(LlmRouterConfig {
                api_keys_file: "".to_string(),
                entities: vec![],
                default_strategy: 0,
                timeout_seconds: 30,
                max_retries: 3,
                default_entity: 0,
            }),
            custody: None,
            cosmwasm: Some(self.build_cosmwasm_config(
                home_dir,
                with_sdl_contract,
                sdl_wasm_path,
            )?),
            akash: None,
        });

        config.save(&config_path)?;
        println!("Created config: {}", config_path);
        println!("  node_type: {} ({})", node_type_cap, node_type_int);
        println!("  api_port: {}", api_port);
        println!("  p2p_port: {}", p2p_port);
        println!("  data_dir: {}", data_dir);
        println!("  cosmwasm.enabled: {}", with_sdl_contract);
        if with_sdl_contract {
            println!("  sdl_contract: cw-sdl (will deploy on startup)");
        }

        Ok(())
    }

    /// Build CosmWasm configuration with optional SDL contract
    fn build_cosmwasm_config(
        &self,
        home_dir: &Utf8Path,
        with_sdl_contract: bool,
        sdl_wasm_path: Option<&str>,
    ) -> Result<CosmwasmConfig> {
        let cache_dir = home_dir.join("wasm_cache").to_string();

        if !with_sdl_contract {
            return Ok(CosmwasmConfig {
                enabled: false,
                cache_dir,
                memory_limit: 33554432, // 32MB
                gas_limits: None,
                initial_contracts: vec![],
            });
        }

        // Resolve SDL WASM path
        let wasm_path = match sdl_wasm_path {
            Some(path) => path.to_string(),
            None => {
                // Default to looking in home_dir or contracts/artifacts
                let default_path = home_dir.join("cw_sdl.wasm");
                if default_path.exists() {
                    default_path.to_string()
                } else {
                    return Err(anyhow!(
                        "SDL contract WASM not found. Use --sdl-wasm-path to specify location."
                    ));
                }
            }
        };

        // Create SDL template registrar contract deployment
        let sdl_contract = ContractDeployment {
            name: "cw-sdl".to_string(),
            wasm_path,
            wasm_bytes: vec![],
            label: "SDL Template Store".to_string(),
            // Empty init_msg creates contract with no template (can be updated later)
            init_msg: r#"{"sdl_template":"{}","variable_defaults":{}}"#.to_string(),
            admin: "".to_string(), // Will use node identity
            required: true,        // Fail startup if deployment fails
            deploy_on_node_types: vec![
                "Coordinator".to_string(),
                "coordinator".to_string(),
            ],
            config: Some(ContractConfig {
                skip_if_exists: true,
                migration: None,
                metadata: std::collections::HashMap::new(),
            }),
        };

        Ok(CosmwasmConfig {
            enabled: true,
            cache_dir,
            memory_limit: 33554432, // 32MB
            gas_limits: None,
            initial_contracts: vec![sdl_contract],
        })
    }

    /// Set a configuration value
    fn set_config(&self, home_dir: &Utf8Path, key: &str, value: &str) -> Result<()> {
        let config_path = home_dir.join(CONFIG_FILE_NAME);
        let mut config = ErgorsConfig::load(&config_path)?;

        // Parse the dotted key path
        let parts: Vec<&str> = key.split('.').collect();

        self.set_by_path(&mut config, &parts, value)?;

        // Validate before saving
        config.validate()?;

        config.save(&config_path)?;
        println!("Set {} = {}", key, value);

        Ok(())
    }

    /// Set value by dotted path with type validation
    fn set_by_path(&self, config: &mut ErgorsConfig, path: &[&str], value: &str) -> Result<()> {
        match path {
            // === home ===
            ["home"] => {
                config.0.home = value.to_string();
            }

            // === identity fields ===
            ["identity", "host"] => {
                self.ensure_identity(config)?.host = value.to_string();
            }
            ["identity", "p2p_port"] => {
                self.ensure_identity(config)?.p2p_port = value.parse()?;
            }
            ["identity", "api_port"] => {
                self.ensure_identity(config)?.api_port = value.parse()?;
            }
            ["identity", "user"] => {
                self.ensure_identity(config)?.user = value.to_string();
            }
            ["identity", "os"] => {
                self.ensure_identity(config)?.os = value.parse()?;
            }
            ["identity", "ssh_port"] => {
                self.ensure_identity(config)?.ssh_port = value.parse()?;
            }
            ["identity", "node_type"] => {
                self.ensure_identity(config)?.node_type = value.to_string();
            }

            // === network fields ===
            ["network", "node_type"] => {
                self.ensure_network(config)?.node_type = value.parse()?;
            }
            ["network", "listen_port"] => {
                self.ensure_network(config)?.listen_port = value.parse()?;
            }
            ["network", "listen_address"] => {
                self.ensure_network(config)?.listen_address = value.to_string();
            }
            ["network", "connection_timeout_ms"] => {
                self.ensure_network(config)?.connection_timeout_ms = value.parse()?;
            }
            ["network", "enable_discovery"] => {
                self.ensure_network(config)?.enable_discovery = value.parse()?;
            }

            // === storage fields ===
            ["storage", "data_dir"] => {
                self.ensure_storage(config)?.data_dir = value.to_string();
            }
            ["storage", "max_size_mb"] => {
                self.ensure_storage(config)?.max_size_mb = value.parse()?;
            }
            ["storage", "enable_compression"] => {
                self.ensure_storage(config)?.enable_compression = value.parse()?;
            }

            // === llm fields ===
            ["llm", "api_keys_file"] => {
                self.ensure_llm(config)?.api_keys_file = value.to_string();
            }
            ["llm", "timeout_seconds"] => {
                self.ensure_llm(config)?.timeout_seconds = value.parse()?;
            }
            ["llm", "max_retries"] => {
                self.ensure_llm(config)?.max_retries = value.parse()?;
            }
            ["llm", "default_strategy"] => {
                self.ensure_llm(config)?.default_strategy = value.parse()?;
            }

            // === cosmwasm fields ===
            ["cosmwasm", "enabled"] => {
                self.ensure_cosmwasm(config)?.enabled = value.parse()?;
            }
            ["cosmwasm", "cache_dir"] => {
                self.ensure_cosmwasm(config)?.cache_dir = value.to_string();
            }
            ["cosmwasm", "memory_limit"] => {
                self.ensure_cosmwasm(config)?.memory_limit = value.parse()?;
            }

            // === akash fields ===
            ["akash", "chain_id"] => {
                self.ensure_akash(config)?.chain_id = value.to_string();
            }
            ["akash", "gas_prices"] => {
                self.ensure_akash(config)?.gas_prices = value.to_string();
            }
            ["akash", "gas_adjustment"] => {
                self.ensure_akash(config)?.gas_adjustment = value.parse()?;
            }
            ["akash", "keyring_backend"] => {
                self.ensure_akash(config)?.keyring_backend = value.to_string();
            }
            ["akash", "default_key_name"] => {
                self.ensure_akash(config)?.default_key_name = value.to_string();
            }
            ["akash", "rpc_endpoints"] => {
                // Parse comma-separated endpoints
                self.ensure_akash(config)?.rpc_endpoints = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            ["akash", "grpc_endpoints"] => {
                // Parse comma-separated endpoints
                self.ensure_akash(config)?.grpc_endpoints = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            ["akash", "rest_endpoints"] => {
                // Parse comma-separated endpoints
                self.ensure_akash(config)?.rest_endpoints = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            ["akash", "max_retries_per_endpoint"] => {
                self.ensure_akash(config)?.max_retries_per_endpoint = value.parse()?;
            }
            ["akash", "max_total_retries"] => {
                self.ensure_akash(config)?.max_total_retries = value.parse()?;
            }
            ["akash", "connection_timeout_seconds"] => {
                self.ensure_akash(config)?.connection_timeout_seconds = value.parse()?;
            }

            _ => {
                return Err(anyhow!(
                    "Unknown config key: '{}'. Run 'ergors config list' to see available keys.",
                    path.join(".")
                ))
            }
        }
        Ok(())
    }

    /// Ensure network config exists, create default if missing
    fn ensure_network<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut NetworkConfig> {
        if config.0.network.is_none() {
            config.0.network = Some(NetworkConfig::default());
        }
        config
            .0
            .network
            .as_mut()
            .ok_or_else(|| anyhow!("network config not found"))
    }

    /// Ensure identity config exists
    fn ensure_identity<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut NodeIdentity> {
        if config.0.identity.is_none() {
            config.0.identity = Some(NodeIdentity::default());
        }
        config
            .0
            .identity
            .as_mut()
            .ok_or_else(|| anyhow!("identity config not found"))
    }

    /// Ensure storage config exists
    fn ensure_storage<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut StorageConfig> {
        if config.0.storage.is_none() {
            config.0.storage = Some(StorageConfig::default());
        }
        config
            .0
            .storage
            .as_mut()
            .ok_or_else(|| anyhow!("storage config not found"))
    }

    /// Ensure llm config exists
    fn ensure_llm<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut LlmRouterConfig> {
        if config.0.llm.is_none() {
            config.0.llm = Some(LlmRouterConfig::default());
        }
        config
            .0
            .llm
            .as_mut()
            .ok_or_else(|| anyhow!("llm config not found"))
    }

    /// Ensure cosmwasm config exists
    fn ensure_cosmwasm<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut CosmwasmConfig> {
        if config.0.cosmwasm.is_none() {
            config.0.cosmwasm = Some(CosmwasmConfig::default());
        }
        config
            .0
            .cosmwasm
            .as_mut()
            .ok_or_else(|| anyhow!("cosmwasm config not found"))
    }

    /// Ensure akash config exists
    fn ensure_akash<'a>(&self, config: &'a mut ErgorsConfig) -> Result<&'a mut AkashDeployConfig> {
        if config.0.akash.is_none() {
            config.0.akash = Some(ErgorsConfig::default_akash_config());
        }
        config
            .0
            .akash
            .as_mut()
            .ok_or_else(|| anyhow!("akash config not found"))
    }

    /// Get a configuration value
    fn get_config(&self, home_dir: &Utf8Path, key: &str) -> Result<()> {
        let config_path = home_dir.join(CONFIG_FILE_NAME);
        let config = ErgorsConfig::load(&config_path)?;

        let parts: Vec<&str> = key.split('.').collect();
        let value = self.get_by_path(&config, &parts)?;

        println!("{} = {}", key, value);
        Ok(())
    }

    /// Get value by dotted path
    fn get_by_path(&self, config: &ErgorsConfig, path: &[&str]) -> Result<String> {
        match path {
            ["home"] => Ok(config.0.home.clone()),

            // identity
            ["identity", "host"] => Ok(config
                .0
                .identity
                .as_ref()
                .map(|i| i.host.clone())
                .unwrap_or_default()),
            ["identity", "p2p_port"] => Ok(config
                .0
                .identity
                .as_ref()
                .map(|i| i.p2p_port.to_string())
                .unwrap_or_default()),
            ["identity", "api_port"] => Ok(config
                .0
                .identity
                .as_ref()
                .map(|i| i.api_port.to_string())
                .unwrap_or_default()),
            ["identity", "node_type"] => Ok(config
                .0
                .identity
                .as_ref()
                .map(|i| i.node_type.clone())
                .unwrap_or_default()),

            // network
            ["network", "node_type"] => Ok(config
                .0
                .network
                .as_ref()
                .map(|n| n.node_type.to_string())
                .unwrap_or_default()),
            ["network", "listen_port"] => Ok(config
                .0
                .network
                .as_ref()
                .map(|n| n.listen_port.to_string())
                .unwrap_or_default()),
            ["network", "listen_address"] => Ok(config
                .0
                .network
                .as_ref()
                .map(|n| n.listen_address.clone())
                .unwrap_or_default()),

            // storage
            ["storage", "data_dir"] => Ok(config
                .0
                .storage
                .as_ref()
                .map(|s| s.data_dir.clone())
                .unwrap_or_default()),
            ["storage", "max_size_mb"] => Ok(config
                .0
                .storage
                .as_ref()
                .map(|s| s.max_size_mb.to_string())
                .unwrap_or_default()),

            // cosmwasm
            ["cosmwasm", "enabled"] => Ok(config
                .0
                .cosmwasm
                .as_ref()
                .map(|c| c.enabled.to_string())
                .unwrap_or_default()),

            // akash
            ["akash", "chain_id"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.chain_id.clone())
                .unwrap_or_default()),
            ["akash", "gas_prices"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.gas_prices.clone())
                .unwrap_or_default()),
            ["akash", "gas_adjustment"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.gas_adjustment.to_string())
                .unwrap_or_default()),
            ["akash", "keyring_backend"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.keyring_backend.clone())
                .unwrap_or_default()),
            ["akash", "default_key_name"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.default_key_name.clone())
                .unwrap_or_default()),
            ["akash", "rpc_endpoints"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.rpc_endpoints.join(","))
                .unwrap_or_default()),
            ["akash", "grpc_endpoints"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.grpc_endpoints.join(","))
                .unwrap_or_default()),
            ["akash", "rest_endpoints"] => Ok(config
                .0
                .akash
                .as_ref()
                .map(|a| a.rest_endpoints.join(","))
                .unwrap_or_default()),

            _ => Err(anyhow!("Unknown config key: '{}'", path.join("."))),
        }
    }

    /// List configuration — loads and displays the actual config file
    fn list_config(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        let config_path = home_dir.join(CONFIG_FILE_NAME);

        if !config_path.as_std_path().exists() {
            if json {
                println!("{{}}");
            } else {
                println!("No configuration file found at {}", config_path);
                println!("Run 'ergors config init' to create one.");
            }
            return Ok(());
        }

        let config = ErgorsConfig::load(&config_path)?;

        if json {
            // ErgorsConfig derives Serialize (via toml), convert via TOML intermediary
            let toml_str = toml::to_string_pretty(&config)?;
            let table: toml::Table = toml_str.parse()?;
            println!("{}", serde_json::to_string_pretty(&table)?);
        } else {
            println!("ERGORS Configuration ({})", config_path);
            println!("==========================================");
            println!();

            println!("home = \"{}\"", config.0.home);

            if let Some(id) = &config.0.identity {
                println!();
                println!("[identity]");
                println!("  host       = \"{}\"", id.host);
                println!("  p2p_port   = {}", id.p2p_port);
                println!("  api_port   = {}", id.api_port);
                println!("  node_type  = \"{}\"", id.node_type);
                println!("  user       = \"{}\"", id.user);
            }

            if let Some(net) = &config.0.network {
                println!();
                println!("[network]");
                println!("  node_type         = {}", net.node_type);
                println!("  listen_address    = \"{}\"", net.listen_address);
                println!("  listen_port       = {}", net.listen_port);
                println!("  enable_discovery  = {}", net.enable_discovery);
                println!("  timeout_ms        = {}", net.connection_timeout_ms);
            }

            if let Some(st) = &config.0.storage {
                println!();
                println!("[storage]");
                println!("  data_dir          = \"{}\"", st.data_dir);
                println!("  max_size_mb       = {}", st.max_size_mb);
                println!("  compression       = {}", st.enable_compression);
            }

            if let Some(llm) = &config.0.llm {
                println!();
                println!("[llm]");
                println!("  timeout_seconds   = {}", llm.timeout_seconds);
                println!("  max_retries       = {}", llm.max_retries);
                println!("  default_strategy  = {}", llm.default_strategy);
            }

            if let Some(cw) = &config.0.cosmwasm {
                println!();
                println!("[cosmwasm]");
                println!("  enabled           = {}", cw.enabled);
                println!("  cache_dir         = \"{}\"", cw.cache_dir);
                println!("  memory_limit      = {}", cw.memory_limit);
            }

            if let Some(ak) = &config.0.akash {
                println!();
                println!("[akash]");
                println!("  chain_id          = \"{}\"", ak.chain_id);
                println!("  gas_prices        = \"{}\"", ak.gas_prices);
                println!("  gas_adjustment    = {}", ak.gas_adjustment);
                println!("  keyring_backend   = \"{}\"", ak.keyring_backend);
                println!("  default_key_name  = \"{}\"", ak.default_key_name);
                if !ak.rpc_endpoints.is_empty() {
                    println!("  rpc_endpoints     = {}", ak.rpc_endpoints.join(", "));
                }
                if !ak.grpc_endpoints.is_empty() {
                    println!("  grpc_endpoints    = {}", ak.grpc_endpoints.join(", "));
                }
            }
        }

        Ok(())
    }

    // === Chain Config Commands (using gRPC to cnidarium) ===

    /// Get gRPC endpoint from config
    fn get_grpc_endpoint(&self, home_dir: &Utf8Path) -> Result<String> {
        let config_path = home_dir.join(CONFIG_FILE_NAME);
        let config = ErgorsConfig::load(&config_path)?;

        let host = config
            .0
            .identity
            .as_ref()
            .map(|i| i.host.as_str())
            .unwrap_or("127.0.0.1");

        // Check for gRPC port environment variable first
        let port = std::env::var("ERGORS_GRPC_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .or_else(|| {
                // Fall back to api_port + 1 (gRPC port convention)
                config.0.identity.as_ref().and_then(|i| {
                    u16::try_from(i.api_port + 1).ok()
                })
            })
            .unwrap_or(50051); // Default gRPC port

        Ok(format!("http://{}:{}", host, port))
    }

    /// Normalize endpoint URL - adds http:// scheme if missing
    fn normalize_endpoint(endpoint: &str) -> String {
        let trimmed = endpoint.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            format!("http://{}", trimmed)
        }
    }

    /// Parse comma-separated endpoints and normalize them
    fn parse_endpoints(input: &str) -> Vec<String> {
        input
            .split(',')
            .map(Self::normalize_endpoint)
            .collect()
    }

    /// Configure a Cosmos SDK chain
    async fn set_chain(
        &self,
        home_dir: &Utf8Path,
        chain_id: &str,
        name: &str,
        prefix: &str,
        denom: &str,
        rpc: &str,
        grpc: &str,
        rest: Option<&str>,
        gas_prices: &str,
        gas_adjustment: f64,
        keyring_backend: &str,
        default_key: &str,
    ) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let config = CosmosChainConfig {
            chain_id: chain_id.to_string(),
            chain_name: name.to_string(),
            bech32_prefix: prefix.to_string(),
            denom: denom.to_string(),
            gas_prices: gas_prices.to_string(),
            gas_adjustment,
            rpc_endpoints: Self::parse_endpoints(rpc),
            grpc_endpoints: Self::parse_endpoints(grpc),
            rest_endpoints: rest.map(Self::parse_endpoints).unwrap_or_default(),
            max_retries_per_endpoint: 3,
            max_total_retries: 10,
            connection_timeout_seconds: 30,
            keyring_backend: keyring_backend.to_string(),
            default_key_name: default_key.to_string(),
            features: vec![],
            trusted_addresses: vec![],
            updated_at: chrono::Utc::now().timestamp(),
            updated_by: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        };

        let request = tonic::Request::new(SetChainConfigRequest {
            config: Some(config),
        });

        let response = client.set_chain_config(request).await?;
        let result = response.into_inner();

        if result.success {
            println!("{}", result.message);
        } else {
            return Err(anyhow!("Failed to set chain config: {}", result.message));
        }

        Ok(())
    }

    /// Get a Cosmos chain configuration
    async fn get_chain(&self, home_dir: &Utf8Path, chain_id: &str) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(GetChainConfigRequest {
            chain_id: chain_id.to_string(),
        });

        let response = client.get_chain_config(request).await?;
        let result = response.into_inner();

        if let Some(config) = result.config {
            println!("Chain: {} ({})", config.chain_name, config.chain_id);
            println!("  Prefix:       {}", config.bech32_prefix);
            println!("  Denom:        {}", config.denom);
            println!("  Gas:          {} (adj: {})", config.gas_prices, config.gas_adjustment);
            println!("  Keyring:      {}", config.keyring_backend);
            println!("  Default Key:  {}", config.default_key_name);
            println!("  RPC:          {}", config.rpc_endpoints.join(", "));
            println!("  gRPC:         {}", config.grpc_endpoints.join(", "));
            if !config.rest_endpoints.is_empty() {
                println!("  REST:         {}", config.rest_endpoints.join(", "));
            }
            if config.updated_at > 0 {
                let dt = chrono::DateTime::from_timestamp(config.updated_at, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                println!("  Updated:      {} by {}", dt, config.updated_by);
            }
        } else {
            return Err(anyhow!("Chain '{}' not found", chain_id));
        }

        Ok(())
    }

    /// List all configured Cosmos chains
    async fn list_chains(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(ListChainConfigsRequest {});

        let response = client.list_chain_configs(request).await?;
        let result = response.into_inner();

        if json {
            use super::responses::{ChainListResponse, ChainSummary};
            let resp = ChainListResponse {
                chains: result
                    .chains
                    .iter()
                    .map(|c| ChainSummary {
                        chain_id: c.chain_id.clone(),
                        chain_name: c.chain_name.clone(),
                        bech32_prefix: c.bech32_prefix.clone(),
                        denom: c.denom.clone(),
                        rpc_endpoint: c.rpc_endpoints.first().cloned().unwrap_or_default(),
                    })
                    .collect(),
            };
            println!("{}", serde_json::to_string_pretty(&resp)?);
        } else if result.chains.is_empty() {
            println!("No chains configured.");
            println!();
            println!("Use 'ergors config set-chain' to configure a chain:");
            println!("  ergors config set-chain local \\");
            println!("    --name \"Akash Local\" \\");
            println!("    --prefix akash \\");
            println!("    --denom uakt \\");
            println!("    --rpc http://localhost:26657 \\");
            println!("    --grpc http://localhost:9090");
        } else {
            println!("Configured Cosmos chains:");
            println!();
            for config in result.chains {
                println!("  {} ({})", config.chain_name, config.chain_id);
                println!("    Prefix: {}, Denom: {}", config.bech32_prefix, config.denom);
                println!("    RPC: {}", config.rpc_endpoints.first().unwrap_or(&"none".to_string()));
            }
        }

        Ok(())
    }

    /// Delete a Cosmos chain configuration (password-protected)
    async fn delete_chain(&self, home_dir: &Utf8Path, chain_id: &str, json: bool) -> Result<()> {
        // Require password confirmation before deletion
        let _password = crate::keys::get_password(false)?;

        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(DeleteChainConfigRequest {
            chain_id: chain_id.to_string(),
        });

        let response = client.delete_chain_config(request).await?;
        let result = response.into_inner();

        if result.success {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "deleted": chain_id,
                        "message": result.message,
                    }))?
                );
            } else {
                println!("{}", result.message);
            }
        } else {
            return Err(anyhow!("Failed to delete chain config: {}", result.message));
        }

        Ok(())
    }

    // ============ CLI Key Management ============

    async fn register_cli_key(
        &self,
        home_dir: &Utf8Path,
        pubkey_hex: &str,
        label: &str,
        json: bool,
    ) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(RegisterCliKeyRequest {
            public_key_hex: pubkey_hex.to_string(),
            label: label.to_string(),
        });

        let response = client.register_cli_key(request).await?;
        let result = response.into_inner();

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&super::responses::OperationResponse {
                    success: result.success,
                    message: result.message,
                })?
            );
        } else if result.success {
            println!("{}", result.message);
        } else {
            return Err(anyhow!("Failed to register CLI key: {}", result.message));
        }

        Ok(())
    }

    async fn revoke_cli_key(
        &self,
        home_dir: &Utf8Path,
        pubkey_hex: &str,
        json: bool,
    ) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(RevokeCliKeyRequest {
            public_key_hex: pubkey_hex.to_string(),
        });

        let response = client.revoke_cli_key(request).await?;
        let result = response.into_inner();

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&super::responses::OperationResponse {
                    success: result.success,
                    message: result.message,
                })?
            );
        } else if result.success {
            println!("{}", result.message);
        } else {
            return Err(anyhow!("Failed to revoke CLI key: {}", result.message));
        }

        Ok(())
    }

    async fn list_cli_keys(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        let endpoint = self.get_grpc_endpoint(home_dir)?;
        let mut client = ManagementServiceClient::connect(endpoint).await?;

        let request = tonic::Request::new(ListCliKeysRequest {});

        let response = client.list_cli_keys(request).await?;
        let result = response.into_inner();

        if json {
            let keys: Vec<super::responses::CliKeyEntryJson> = result
                .keys
                .iter()
                .map(|k| super::responses::CliKeyEntryJson {
                    public_key_hex: k.public_key_hex.clone(),
                    label: k.label.clone(),
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&super::responses::CliKeyListJsonResponse { keys })?
            );
        } else if result.keys.is_empty() {
            println!("No authorized CLI keys registered.");
        } else {
            println!("Authorized CLI Keys:");
            println!("{:<66} {}", "PUBLIC KEY", "LABEL");
            println!("{}", "-".repeat(80));
            for key in &result.keys {
                println!("{:<66} {}", key.public_key_hex, key.label);
            }
        }

        Ok(())
    }
}
