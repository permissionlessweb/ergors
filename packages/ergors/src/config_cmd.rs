//! Config command for programmatically managing ERGORS node configuration
//!
//! Provides `ergors config` subcommands for setting, getting, and listing
//! configuration values with type validation.

use anyhow::{anyhow, Result};
use camino::Utf8Path;

use ho_std::constants::CONFIG_FILE_NAME;
use ho_std::traits::HoConfigTrait;
use ho_std::types::ergors::{
    network::v1::{ChannelConfig, NetworkConfig, NodeIdentity},
    orch::v1::{ContractConfig, ContractDeployment, CosmwasmConfig, LlmRouterConfig},
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
}

impl ConfigCmd {
    pub fn exec(&self, home_dir: &Utf8Path) -> Result<()> {
        match &self.subcmd {
            ConfigSubCmd::Set { key, value } => self.set_config(home_dir, key, value),
            ConfigSubCmd::Get { key } => self.get_config(home_dir, key),
            ConfigSubCmd::List {} => self.list_config(),
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

            _ => Err(anyhow!("Unknown config key: '{}'", path.join("."))),
        }
    }

    /// List all available configuration keys
    fn list_config(&self) -> Result<()> {
        println!("Available configuration keys:");
        println!();
        println!("home                           (string)  - Home directory path");
        println!();
        println!("[identity]");
        println!("  identity.host                (string)  - Node hostname/IP");
        println!("  identity.p2p_port            (u32)     - P2P listening port");
        println!("  identity.api_port            (u32)     - API/gRPC port");
        println!("  identity.user                (string)  - Username");
        println!("  identity.os                  (i32)     - OS type (1=Linux, 2=MacOS, 3=Windows)");
        println!("  identity.ssh_port            (u32)     - SSH port");
        println!(
            "  identity.node_type           (string)  - Coordinator, Executor, Referee, Development"
        );
        println!();
        println!("[network]");
        println!(
            "  network.node_type            (i32)     - 1=Coordinator, 2=Executor, 3=Referee, 4=Development"
        );
        println!("  network.listen_port          (u32)     - P2P listening port");
        println!("  network.listen_address       (string)  - Bind address (e.g., 0.0.0.0)");
        println!("  network.connection_timeout_ms (u32)    - Connection timeout in ms");
        println!("  network.enable_discovery     (bool)    - Enable peer discovery");
        println!();
        println!("[storage]");
        println!("  storage.data_dir             (string)  - Data directory path");
        println!("  storage.max_size_mb          (u32)     - Maximum storage size in MB");
        println!("  storage.enable_compression   (bool)    - Enable data compression");
        println!();
        println!("[llm]");
        println!("  llm.api_keys_file            (string)  - Path to API keys file");
        println!("  llm.timeout_seconds          (u64)     - Request timeout");
        println!("  llm.max_retries              (u32)     - Maximum retry attempts");
        println!("  llm.default_strategy         (i32)     - Model selection strategy");
        println!();
        println!("[cosmwasm]");
        println!("  cosmwasm.enabled             (bool)    - Enable CosmWasm VM");
        println!("  cosmwasm.cache_dir           (string)  - WASM cache directory");
        println!("  cosmwasm.memory_limit        (u64)     - Memory limit in bytes");
        println!();
        println!("Usage:");
        println!("  ergors config init --node-type executor --api-port 50051 --p2p-port 26656");
        println!("  ergors config set network.listen_port 9090");
        println!("  ergors config get identity.node_type");

        Ok(())
    }
}
