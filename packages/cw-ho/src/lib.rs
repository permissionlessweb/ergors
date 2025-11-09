pub mod config;
pub mod error;
pub mod init;
pub mod llm;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;

// Re-export the macro for external use

use crate::init::InitCmd;
use crate::llm::ApiKeys;
use crate::network::{manager::PeerInfo, topology::NetworkTopology};
use crate::server::Server;
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use cnidarium::Storage as CnidariumStorage;
use commonware_cryptography::ed25519;
use commonware_p2p::authenticated;
use commonware_runtime::tokio::Context;
use commonware_runtime::tokio::{Config as RuntimeConfig, Runner};
use commonware_runtime::Runner as _;
use ho_std::config::env::default_home;
use ho_std::constants::CONFIG_FILE_NAME;
use ho_std::prelude::*;
use ho_std::traits::HoConfigTrait;
use reqwest::Client;
use tracing::{error, info};

use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};

use anyhow::Result;

// Define all wrapper types using the macro
define_wrapper!(CwHoConfig, HoConfig);
define_wrapper!(CwHoLlmRouterConfig, LlmRouterConfig);

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
    identity: NodeIdentity,
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
#[command(name = "ergors: cw-hoe", version = "0.1.0")]
#[command(about = "HOE: Helper Orchestration Engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// The home directory used to store configuration and data.
    #[clap(long, default_value_t = default_home(), env = "NODE_DATA_PATH")]
    pub home: Utf8PathBuf,

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
    Init(InitCmd),
    /// Check service health
    Health {
        /// API endpoint to check
        #[arg(long, default_value = "http://localhost:8080")]
        endpoint: String,
    },
}

pub fn start(cli: Cli, port: Option<u16>) -> Result<()> {
    info!("🚀 Starting CW-AGENT Minimal Prompt Capture Service");
    let path = cli.home.as_path().join(CONFIG_FILE_NAME);
    // Load configuration
    let config = CwHoConfig::load(&path)?;

    // Override port if provided
    let server_port = port.unwrap_or(config.identity().api_port.try_into().unwrap());
    info!(
        "🔌 Server will listen on port {}\n
        💾 Data directory: {}\n",
        server_port,
        config.storage().data_dir
    );

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
