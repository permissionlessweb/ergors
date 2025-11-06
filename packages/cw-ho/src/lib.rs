pub mod config;
pub mod error;
pub mod llm;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;

// Re-export the macro for external use

use crate::llm::ApiKeys;
use crate::network::{manager::PeerInfo, topology::NetworkTopology};
use crate::server::Server;
use clap::{Parser, Subcommand};
use cnidarium::Storage as CnidariumStorage;
use commonware_cryptography::{ed25519, Signer};
use commonware_p2p::{authenticated, Recipients};
use commonware_runtime::tokio::{Config as RuntimeConfig, Runner};
use commonware_runtime::Runner as _;
use commonware_runtime::{tokio::Context, Metrics, Spawner};
use ho_std::prelude::*;
use ho_std::traits::HoConfigTrait;
use reqwest::Client;
use tracing::{error, info};

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};

use anyhow::Result;

// Define all wrapper types using the macro
define_wrapper!(CwHoConfig, HoConfig);
define_wrapper!(CwHoLlmRouterConfig, LlmRouterConfig);
define_wrapper!(CwHoStorageConfig, StorageConfig);
define_wrapper!(CwHoNodeIdentity, NodeIdentity);

/// Defines the storage used for this CwHo.
/// implemenations in ./storage.rs
pub struct CwHoStorage {
    cnidarium: CnidariumStorage,
}

/// Defines the Llm router used for this CwHo
pub struct LlmRouter {
    client: Client,
    api_keys: ApiKeys,
    config: LlmRouterConfig,
}

/// Minimal network manager for cw-ho/
/// implementations in ./manager.rs
pub struct CwHoNetworkManifold {
    context: Context,
    /// Network running flag
    network_running: Arc<RwLock<bool>>,
    /// Channel senders for different message types
    channel_senders: HashMap<u8, authenticated::lookup::Sender<ed25519::PublicKey>>,
    /// Channel receivers for different message types
    channel_receivers: HashMap<u8, authenticated::lookup::Receiver<ed25519::PublicKey>>,
    /// Connected peers
    peers: Arc<RwLock<HashMap<ed25519::PublicKey, PeerInfo>>>,
    /// Network topology
    topology: Arc<RwLock<NetworkTopology>>,
    /// Event sender for network events
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    /// Event receiver
    event_rx: Option<mpsc::UnboundedReceiver<NetworkEvent>>,
    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
    /// Our node identity
    identity: CwHoNodeIdentity,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<CwHoStorage>,
    pub llm_router: Arc<LlmRouter>,
    pub network_manifold: Arc<tokio::sync::Mutex<CwHoNetworkManifold>>,
    pub start_time: Instant,
    pub config: CwHoConfig,
}

#[derive(Parser)]
#[command(name = "cw-ho", version = "0.1.0")]
#[command(about = "CW-HOE Minimal Helper Orchestration Engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the HTTP API server
    Start {
        /// HTTP server port (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Generate a sample configuration file
    Init {
        /// Output path for configuration file
        #[arg(short, long, default_value = "config.toml")]
        output: String,
    },
    /// Check service health
    Health {
        /// API endpoint to check
        #[arg(long, default_value = "http://localhost:8080")]
        endpoint: String,
    },
}

pub fn init(output: String) -> Result<()> {
    info!("📝 Generating sample configuration file: {}", output);
    let config: HoConfig = HoConfig::default();
    let toml_content = toml::to_string_pretty(&config)?;
    std::fs::write(&output, toml_content)?;
    info!("✅ Configuration template saved to {}", output);
    info!("📋 Edit the file and configure your LLM API keys");
    Ok(())
}

pub fn start(cli: Cli, port: Option<u16>) -> Result<()> {
    info!("🚀 Starting CW-AGENT Minimal Prompt Capture Service");

    // Load configuration
    let config = if std::path::Path::new(&cli.config).exists() {
        CwHoConfig::from_file(&cli.config)?
    } else {
        info!("⚠️  Config file not found, using defaults");
        CwHoConfig::default()
    };

    // Override port if provided
    let server_port = port.unwrap_or(config.identity().api_port.try_into().unwrap());
    info!("🔌 Server will listen on port {}", server_port);
    info!("💾 Data directory: {}", config.storage().data_dir);

    // Create commonware runtime configuration
    let runtime_config = RuntimeConfig::default();
    let runner = Runner::new(runtime_config);

    info!("🌐 Starting within commonware runtime context");
    runner.start(|context| async move {
        let server = match Server::new(config.clone(), context).await {
            Ok(s) => s,
            Err(e) => {
                error!("❌ Failed to initialize server: {}", e);
                return;
            }
        };
        if let Err(e) = server.run(server_port).await {
            error!("❌ Server runtime error: {}", e);
        }
    });
    Ok(())
}
