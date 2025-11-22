pub mod auth;
pub mod call;
pub mod config;

pub mod init;
pub mod middleware;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;
// Re-export the macro for external use
use crate::{
    auth::AuthCmd, call::CallCmd, init::InitCmd, network::manager::PeerInfo,
    server::Server as CwHoServer, storage::ErgorsStorage,
};

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use ho_std::{
    config::env::{default_home, init_env},
    constants::CONFIG_FILE_NAME,
    error::HoResult,
    llm::LlmRouter,
    traits::HoConfigTrait,
    types::ergors::{network::v1::*, orch::v1::*},
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
use tracing::error;

// Define all wrapper types using the macro
define_wrapper!(ErgorsConfig, HoConfig);
define_wrapper!(CwHoLlmRouterConfig, LlmRouterConfig);

/// Minimal network manager for ergors/ implementations in ./manager.rs
pub struct ErgorsNetworkManifold {
    context: Context,
    /// Network running flag
    up: Arc<RwLock<bool>>,
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
/// r = router
/// s = storage
/// nm = network manifold
/// t = time
/// c = variable config
#[derive(Clone)]
pub struct ErgorsAppState {
    /// r = router
    pub r: Arc<LlmRouter>,
    /// s = storage
    pub s: Arc<ErgorsStorage>,
    /// nm = network manifold
    pub nm: Arc<tokio::sync::Mutex<ErgorsNetworkManifold>>,
    /// t = time
    pub t: Instant,
    /// c = variable config
    pub c: ErgorsConfig,
}

impl ErgorsAppState {
    fn new(
        r: Arc<LlmRouter>,
        s: Arc<ErgorsStorage>,
        nm: Arc<tokio::sync::Mutex<ErgorsNetworkManifold>>,
        t: Instant,
        c: ErgorsConfig,
    ) -> Self {
        Self { r, s, nm, t, c }
    }
}

#[derive(Parser)]
#[command(name = "ergors", version = "0.1.0")]
#[command(about = "Ergors: Ergodic Recursive Systems")]
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
    let p: Utf8PathBuf = cli.home.as_path().join(CONFIG_FILE_NAME);
    let c = ErgorsConfig::load(&p)?;
    init_env(cli.home.as_path())?;

    // Create commonware runtime configuration
    Runner::new(RuntimeConfig::default()).start(|context| async move {
        let s = match CwHoServer::new(c.clone(), context).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to start server: {}", e);
                return;
            }
        };
        if let Err(e) = s.run().await {
            println!("{:#?}", c.clone());
            error!("Ergors Error: {}", e);
            error!("Ergors Error: {:#?}", e.backtrace());
        }
    });
    Ok(())
}
