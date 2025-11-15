pub mod auth;
pub mod call;
pub mod config;

pub mod init;
pub mod llm;
pub mod middleware;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;

// Re-export the macro for external use
use crate::{
    auth::AuthCmd, call::CallCmd, init::InitCmd, network::manager::PeerInfo, server::Server,
};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use cnidarium::Storage as CnidariumStorage;
use ho_std::error::HoResult;
use ho_std::{
    config::env::{default_home, init_env},
    constants::CONFIG_FILE_NAME,
    llm::LlmRouter,
    traits::HoConfigTrait,
    types::cw_ho::{network::v1::*, orchestration::v1::*},
};
use {
    commonware_cryptography::ed25519,
    commonware_p2p::authenticated,
    commonware_runtime::{
        tokio::{Config as RuntimeConfig, Context, Runner},
        Runner as _,
    },
};

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

// Define all wrapper types using the macro
define_wrapper!(CwHoConfig, HoConfig);
define_wrapper!(CwHoLlmRouterConfig, LlmRouterConfig);

/// Defines the storage used for this CwHo.
/// implemenations in ./storage.rs
pub struct CwHoStorage {
    cnidarium: CnidariumStorage,
}

/// Minimal network manager for cw-ho/ implementations in ./manager.rs
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
pub struct ErgorsAppState {
    pub storage: Arc<CwHoStorage>,
    pub llm_router: Arc<LlmRouter>,
    pub nm: Arc<tokio::sync::Mutex<CwHoNetworkManifold>>, // Network
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
    Start {},
    /// Generate a sample configuration file
    Init(InitCmd),
    /// register/revoke
    ManageAuth(AuthCmd),
    /// register/revoke
    Call(CallCmd),
}

pub fn start(cli: Cli) -> HoResult<()> {
    let path: Utf8PathBuf = cli.home.as_path().join(CONFIG_FILE_NAME);
    let config = CwHoConfig::load(&path)?;
    init_env(cli.home.as_path())?;
    // Create commonware runtime configuration
    let runtime_config = RuntimeConfig::default();
    let runner = Runner::new(runtime_config);
    // info!("  {}\n n", config.t);

    info!("");
    runner.start(|context| async move {
        let server = match Server::new(config.clone(), context).await {
            Ok(s) => s,
            Err(e) => {
                error!("❌ Failed to initialize server: {}", e);
                return;
            }
        };
        if let Err(e) = server.run().await {
            error!("❌ Server runtime error: {}", e);
        }
    });
    Ok(())
}
