//! CLI command implementations for gRPC-based operations
//!
//! These commands require the daemon to be running and communicate via gRPC.
//! For local operations (without daemon), use the direct command modules
//! (e.g., config_cmd, keys, init).

pub mod bootstrap;
pub mod call;
pub mod config;
pub mod deploy;
pub mod gateway;
pub mod init;
pub mod rag;
pub mod sentinel;
pub mod workspace;
use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;

use crate::client::{
    format_engine_state, format_uptime, ManagementClient, NodeTypeProto as NodeType,
};

/// CLI context passed to command executors
pub struct CliContext {
    pub home: camino::Utf8PathBuf,
    pub grpc_addr: String,
    pub json: bool,
}

pub use bootstrap::BootstrapCmd;
pub use deploy::DeployCmd;
pub use gateway::GatewayCmd;
pub use rag::RagCmd;
pub use workspace::WorkspaceCmd;

// ============ Engine Commands ============

#[derive(Subcommand)]
pub enum EngineCmd {
    /// Start the engine daemon (spawns ergors process)
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,

        /// gRPC management port
        #[arg(long, default_value = "50051")]
        grpc_port: u16,
    },
    /// Stop the running engine
    Stop {
        /// Force immediate shutdown
        #[arg(short, long)]
        force: bool,
    },
    /// Show engine status
    Status,
    /// Restart the engine
    Restart {
        /// Force immediate shutdown before restart
        #[arg(short, long)]
        force: bool,
    },
}

impl EngineCmd {
    pub async fn execute(&self, ctx: &CliContext, client: Result<ManagementClient>) -> Result<()> {
        match self {
            EngineCmd::Start {
                foreground,
                grpc_port,
            } => {
                // Check if engine is already running
                if let Ok(mut c) = ManagementClient::connect(&ctx.grpc_addr).await {
                    if c.get_status().await.is_ok() {
                        println!("Engine is already running at {}", ctx.grpc_addr);
                        return Ok(());
                    }
                }

                // Find the ergors binary (same directory as this CLI binary, or in PATH)
                let ergors_bin = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("ergors")))
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| std::path::PathBuf::from("ergors"));

                if *foreground {
                    println!("Starting engine in foreground mode...");
                    // Execute ergors start (replaces current process on Unix)
                    let status = std::process::Command::new(&ergors_bin)
                        .arg("start")
                        .arg("--grpc-port")
                        .arg(grpc_port.to_string())
                        .status()?;

                    if !status.success() {
                        anyhow::bail!("Engine exited with status: {}", status);
                    }
                } else {
                    println!("Starting engine daemon...");

                    // Spawn ergors as a background daemon
                    let child = std::process::Command::new(&ergors_bin)
                        .arg("start")
                        .arg("--grpc-port")
                        .arg(grpc_port.to_string())
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;

                    println!("Engine started (PID: {})", child.id());

                    // Wait a moment for the server to start
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    // Verify it's running
                    match ManagementClient::connect(&ctx.grpc_addr).await {
                        Ok(mut c) => {
                            if c.get_status().await.is_ok() {
                                println!("Engine is running. Use 'ergors status' for details.");
                            } else {
                                println!("Engine started but not responding yet. Check logs.");
                            }
                        }
                        Err(_) => {
                            println!("Engine started but gRPC not ready yet. Check logs.");
                        }
                    }
                }
                Ok(())
            }
            EngineCmd::Stop { force } => {
                let mut client = client?;
                let result = client.shutdown(*force).await?;

                if result.success {
                    println!("Engine shutdown initiated");
                } else {
                    println!("Shutdown failed: {}", result.message);
                }
                Ok(())
            }
            EngineCmd::Status => {
                match client {
                    Ok(mut c) => {
                        let status = c.get_status().await?;

                        if ctx.json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "version": status.version,
                                    "state": format_engine_state(status.state),
                                    "uptime_seconds": status.uptime_seconds,
                                    "storage_status": status.storage_status,
                                    "network_status": status.network_status,
                                    "connected_peers": status.connected_peers,
                                    "total_requests": status.total_requests_handled,
                                }))?
                            );
                        } else {
                            println!("ERGORS Engine Status");
                            println!("====================");
                            println!("Version:         {}", status.version);
                            println!("State:           {}", format_engine_state(status.state));
                            println!("Uptime:          {}", format_uptime(status.uptime_seconds));
                            println!("Storage:         {}", status.storage_status);
                            println!("Network:         {}", status.network_status);
                            println!("Connected Peers: {}", status.connected_peers);
                            println!("Total Requests:  {}", status.total_requests_handled);
                        }
                    }
                    Err(_) => {
                        if ctx.json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "state": "stopped",
                                    "error": "Cannot connect to engine"
                                }))?
                            );
                        } else {
                            println!("Engine Status: NOT RUNNING");
                            println!("Cannot connect to engine at {}", ctx.grpc_addr);
                        }
                    }
                }
                Ok(())
            }
            EngineCmd::Restart { force } => {
                let mut client = client?;
                println!("Restarting engine...");
                let _ = client.shutdown(*force).await;
                // Wait a moment then start again
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                // TODO: spawn ergors start
                println!("Engine restart initiated");
                Ok(())
            }
        }
    }
}

// ============ Node Commands ============

#[derive(Subcommand)]
pub enum NodeCmd {
    /// Show node identity
    Info {
        /// Address prefix for bech32 encoding (e.g., "ergors", "akash", "cosmos")
        /// Allows viewing the node's Ed25519 address with any prefix
        #[arg(long, default_value = "ergors")]
        prefix: String,
        /// Show addresses for all common prefixes
        #[arg(long)]
        all_prefixes: bool,
    },
    /// Generate new node identity
    Generate {
        /// Node type: coordinator, executor, referee, development
        #[arg(long, default_value = "development")]
        node_type: String,
    },
    /// Export node identity
    Export {
        /// Export only public key
        #[arg(long)]
        public_only: bool,
    },
    /// Get cosmos address for a stored key (any chain)
    Address {
        /// Key name (uses default key if not specified)
        #[arg(short = 'k', long)]
        key_name: Option<String>,
        /// Address prefix (bech32 hrp) - e.g., "akash", "cosmos", "osmo"
        #[arg(short = 'p', long, default_value = "akash")]
        prefix: String,
        /// Coin type (BIP-44) - e.g., 118 for Cosmos, 330 for Terra, 60 for ETH
        #[arg(short = 'c', long, default_value = "118")]
        coin_type: u32,
        /// Account index (HD derivation)
        #[arg(short = 'i', long, default_value = "0")]
        account_index: u32,
    },
}

impl NodeCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            NodeCmd::Info {
                prefix,
                all_prefixes,
            } => {
                use ho_std::keys::cosmos::cosmos_address_from_ed25519_pubkey;

                let identity = client.get_node_identity().await?;

                // Derive addresses with different prefixes if requested
                let addresses = if *all_prefixes {
                    // Common Cosmos ecosystem prefixes
                    let prefixes = ["ergors", "akash", "cosmos", "osmo", "juno", "stars"];
                    prefixes
                        .iter()
                        .filter_map(|p| {
                            identity.public_key.as_ref().and_then(|pk| {
                                cosmos_address_from_ed25519_pubkey(pk, p)
                                    .ok()
                                    .map(|addr| (p.to_string(), addr))
                            })
                        })
                        .collect::<Vec<_>>()
                } else if prefix != "ergors" {
                    // Derive address with custom prefix
                    identity
                        .public_key
                        .as_ref()
                        .and_then(|pk| {
                            cosmos_address_from_ed25519_pubkey(pk, prefix)
                                .ok()
                                .map(|addr| vec![(prefix.clone(), addr)])
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                if ctx.json {
                    let mut json = serde_json::json!({
                        "host": identity.host,
                        "node_type": identity.node_type,
                        "p2p_port": identity.p2p_port,
                        "api_port": identity.api_port,
                        "bech32_address": identity.bech32_address,
                    });
                    if !addresses.is_empty() {
                        json["addresses"] = serde_json::json!(addresses
                            .iter()
                            .map(|(p, a)| { serde_json::json!({"prefix": p, "address": a}) })
                            .collect::<Vec<_>>());
                    }
                    println!("{}", serde_json::to_string_pretty(&json)?);
                } else {
                    println!("Node Identity");
                    println!("=============");
                    println!("Host:      {}", identity.host);
                    println!("Type:      {}", identity.node_type);
                    println!("P2P Port:  {}", identity.p2p_port);
                    println!("API Port:  {}", identity.api_port);
                    if let Some(pk) = &identity.public_key {
                        println!("Public Key: {}", hex::encode(pk));
                    }
                    if let Some(addr) = &identity.bech32_address {
                        println!("Address (ergors): {}", addr);
                    }

                    // Show additional addresses
                    if !addresses.is_empty() {
                        println!("\nAdditional Prefixes:");
                        for (p, addr) in addresses {
                            println!("  {:<8}: {}", p, addr);
                        }
                    }
                }
                Ok(())
            }
            NodeCmd::Generate { node_type } => {
                let nt = match node_type.to_lowercase().as_str() {
                    "coordinator" => NodeType::Coordinator,
                    "executor" => NodeType::Executor,
                    "referee" => NodeType::Referee,
                    "development" => NodeType::Development,
                    _ => NodeType::Development,
                };

                let (public_key, node_id, mnemonic) = client.generate_node_identity(nt).await?;

                println!("Generated New Node Identity");
                println!("===========================");
                println!("Node ID:    {}", node_id);
                println!("Public Key: {}", hex::encode(&public_key));
                println!();
                println!("IMPORTANT: Save your mnemonic phrase securely!");
                println!("Mnemonic:   {}", mnemonic);

                Ok(())
            }
            NodeCmd::Export { public_only } => {
                let identity = client.get_node_identity().await?;

                if *public_only {
                    if let Some(pk) = &identity.public_key {
                        println!("{}", hex::encode(pk));
                    }
                    if let Some(addr) = &identity.bech32_address {
                        println!("Address:    {}", addr);
                    }
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "host": identity.host,
                            "node_type": identity.node_type,
                            "public_key": identity.public_key.as_ref().map(hex::encode),
                        }))?
                    );
                }
                Ok(())
            }
            NodeCmd::Address {
                key_name,
                prefix,
                coin_type,
                account_index,
            } => {
                let response = client
                    .get_key_address(
                        key_name.as_deref().unwrap_or(""),
                        prefix,
                        *coin_type,
                        *account_index,
                    )
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "address": response.address,
                            "public_key": hex::encode(&response.public_key),
                            "hd_path": response.hd_path,
                            "key_name": response.key_name,
                            "address_prefix": response.address_prefix,
                            "coin_type": response.coin_type,
                        }))?
                    );
                } else {
                    println!("Cosmos Address");
                    println!("==============");
                    println!("Address:    {}", response.address);
                    println!("Key Name:   {}", response.key_name);
                    println!("HD Path:    {}", response.hd_path);
                    println!("Prefix:     {}", response.address_prefix);
                    println!("Coin Type:  {}", response.coin_type);
                    println!("Public Key: {}", hex::encode(&response.public_key));
                }
                Ok(())
            }
        }
    }
}

// ============ Remote Config Commands (via gRPC) ============

#[derive(Subcommand)]
pub enum RemoteConfigCmd {
    /// Show full configuration (from running daemon)
    Show,
    /// Get a specific config value (from running daemon)
    Get {
        /// Config key (dot-separated path)
        key: String,
    },
    /// Set a config value (on running daemon)
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
}

impl RemoteConfigCmd {
    pub async fn execute(&self, _ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            RemoteConfigCmd::Show => {
                let config = client.get_config().await?;
                let content = String::from_utf8_lossy(&config.data);
                println!("{}", content);
                Ok(())
            }
            RemoteConfigCmd::Get { key } => {
                let config = client.get_config().await?;
                let content = String::from_utf8_lossy(&config.data);

                // Parse TOML and extract key
                if let Ok(table) = content.parse::<toml::Table>() {
                    let parts: Vec<&str> = key.split('.').collect();
                    let mut current: &toml::Value = &toml::Value::Table(table);

                    for part in parts {
                        if let Some(v) = current.get(part) {
                            current = v;
                        } else {
                            println!("Key not found: {}", key);
                            return Ok(());
                        }
                    }

                    println!("{}", current);
                }
                Ok(())
            }
            RemoteConfigCmd::Set { key, value } => {
                let result = client.update_config(key, value).await?;

                if result.success {
                    println!("Configuration updated: {} = {}", key, value);
                } else {
                    println!("Failed to update config: {}", result.message);
                }
                Ok(())
            }
        }
    }
}

// ============ Network Commands ============

#[derive(Subcommand)]
pub enum NetworkCmd {
    /// List connected peers
    Peers,
    /// Add a bootstrap peer
    Add {
        /// Peer address (host:port)
        address: String,
    },
    /// Remove a peer
    Remove {
        /// Node ID to remove
        node_id: String,
    },
    /// Show network topology
    Topology,
}

impl NetworkCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            NetworkCmd::Peers | NetworkCmd::Topology => {
                let topology = client.get_network_topology().await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "nodes": topology.nodes,
                            "connections": topology.connections,
                        }))?
                    );
                } else {
                    println!("Network Topology");
                    println!("================");
                    println!("Nodes: {}", topology.nodes.len());

                    for node in &topology.nodes {
                        let status = if node.online { "online" } else { "offline" };
                        println!("  {} ({}) - {}", node.node_id, node.node_type, status);
                    }

                    println!("\nConnections: {}", topology.connections.len());
                    for conn in &topology.connections {
                        println!("  {} <-> {}", conn.from_node_id, conn.to_node_id);
                    }
                }
                Ok(())
            }
            NetworkCmd::Add { address } => {
                let result = client.add_peer(address).await?;

                if result.success {
                    println!("Peer added: {}", address);
                } else {
                    println!("Failed to add peer: {}", result.message);
                }
                Ok(())
            }
            NetworkCmd::Remove { node_id } => {
                let result = client.remove_peer(node_id).await?;

                if result.success {
                    println!("Peer removed: {}", node_id);
                } else {
                    println!("Failed to remove peer: {}", result.message);
                }
                Ok(())
            }
        }
    }
}

// ============ Provider Commands ============

#[derive(Subcommand)]
pub enum ProviderCmd {
    /// List configured providers
    List,
    /// Add/configure a provider
    Add {
        /// Provider name (openai, anthropic, etc.)
        name: String,
        /// API key (will prompt if not provided)
        #[arg(long)]
        api_key: Option<String>,
        /// Set as default provider
        #[arg(long)]
        default: bool,
    },
    /// Test provider connectivity
    Test {
        /// Provider name (tests all if omitted)
        name: Option<String>,
    },
    /// Set default provider
    Default {
        /// Provider name
        name: String,
    },
}

impl ProviderCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            ProviderCmd::List => {
                let list = client.list_providers().await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "providers": list.providers,
                            "default": list.default_provider,
                        }))?
                    );
                } else {
                    println!("LLM Providers");
                    println!("=============");
                    println!("Default: {}", list.default_provider);
                    println!();

                    for provider in &list.providers {
                        let status = if provider.enabled {
                            if provider.configured {
                                "configured"
                            } else {
                                "not configured"
                            }
                        } else {
                            "disabled"
                        };
                        println!("  {} - {}", provider.name, status);
                    }
                }
                Ok(())
            }
            ProviderCmd::Add {
                name,
                api_key,
                default,
            } => {
                let key = match api_key {
                    Some(k) => k.clone(),
                    None => {
                        // Prompt for API key
                        print!("Enter API key for {}: ", name);
                        use std::io::{self, Write};
                        io::stdout().flush()?;
                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                        input.trim().to_string()
                    }
                };

                let result = client.configure_provider(name, &key, *default).await?;

                if result.success {
                    println!("Provider {} configured", name);
                    if *default {
                        println!("Set as default provider");
                    }
                } else {
                    println!("Failed to configure provider: {}", result.message);
                }
                Ok(())
            }
            ProviderCmd::Test { name } => {
                match name {
                    Some(n) => {
                        let result = client.test_provider(n).await?;

                        if result.success {
                            println!("{}: OK ({}ms)", n, result.latency_ms);
                        } else {
                            println!("{}: FAILED - {}", n, result.error_message);
                        }
                    }
                    None => {
                        // Test all providers
                        let list = client.list_providers().await?;

                        for provider in &list.providers {
                            if provider.configured {
                                let result = client.test_provider(&provider.name).await?;

                                if result.success {
                                    println!("{}: OK ({}ms)", provider.name, result.latency_ms);
                                } else {
                                    println!(
                                        "{}: FAILED - {}",
                                        provider.name, result.error_message
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            ProviderCmd::Default { name } => {
                let result = client.configure_provider(name, "", true).await?;

                if result.success {
                    println!("Default provider set to: {}", name);
                } else {
                    println!("Failed to set default: {}", result.message);
                }
                Ok(())
            }
        }
    }
}

// ============ SDL Template Commands ============

#[derive(Subcommand)]
pub enum SdlCmd {
    /// List deployed SDL template contracts
    List,
    /// Get SDL template from contract
    GetTemplate {
        /// Contract address
        contract_address: String,
    },
    /// Get variable defaults from contract
    GetDefaults {
        /// Contract address
        contract_address: String,
    },
    /// Render SDL template with variables
    Render {
        /// Contract address
        contract_address: String,
        /// Variable values (key=value pairs)
        #[arg(short = 'v', long = "var", value_parser = parse_key_val)]
        vars: Vec<(String, String)>,
    },
}

impl SdlCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            SdlCmd::List => {
                // Query list of SDL template contracts from engine
                let list = client.list_sdl_templates().await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "templates": list.templates,
                        }))?
                    );
                } else {
                    println!("SDL Template Contracts");
                    println!("======================");
                    for template in &list.templates {
                        println!(
                            "  {} - {}",
                            template.contract_address,
                            template.label.as_ref().unwrap_or(&"(no label)".to_string())
                        );
                    }
                }
                Ok(())
            }
            SdlCmd::GetTemplate { contract_address } => {
                let template = client.get_sdl_template(contract_address).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "sdl_template": template.sdl_template,
                            "template_json": template.template_json,
                        }))?
                    );
                } else {
                    println!("SDL Template from {}", contract_address);
                    println!("==========================================");
                    println!("{}", template.sdl_template);
                }
                Ok(())
            }
            SdlCmd::GetDefaults { contract_address } => {
                let defaults = client.get_sdl_defaults(contract_address).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "defaults": defaults.defaults,
                        }))?
                    );
                } else {
                    println!("Variable Defaults from {}", contract_address);
                    println!("==========================================");
                    for (key, value) in &defaults.defaults {
                        println!("  {} = {}", key, value);
                    }
                }
                Ok(())
            }
            SdlCmd::Render {
                contract_address,
                vars,
            } => {
                let variables: HashMap<String, String> = vars.iter().cloned().collect();
                let rendered = client
                    .render_sdl_template(contract_address, variables)
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "rendered_sdl": rendered.rendered_sdl,
                            "used_variables": rendered.used_variables,
                        }))?
                    );
                } else {
                    println!("Rendered SDL from {}", contract_address);
                    println!("==========================================");
                    println!("{}", rendered.rendered_sdl);
                    println!("\nUsed Variables:");
                    for (key, value) in &rendered.used_variables {
                        println!("  {} = {}", key, value);
                    }
                }
                Ok(())
            }
        }
    }
}

/// Parse a single key-value pair
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
