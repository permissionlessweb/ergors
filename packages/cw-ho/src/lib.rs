pub mod auth;
pub mod call;
pub mod config;

pub mod init;
pub mod headstash;
pub mod middleware;
pub mod orchestrator;
pub mod network;
pub mod server;
pub mod storage;
pub mod traits;

// Re-export the macro for external use
use crate::{config::ErgorsConfig, network::manager::PeerInfo, storage::ErgorsStorage};

use ho_std::{llm::LlmRouter, types::ergors::network::v1::*};
use {
    commonware_cryptography::ed25519, commonware_p2p::authenticated,
    commonware_runtime::tokio::Context,
};

use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{mpsc, RwLock};

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
/// `r` = router\
/// `s` = storage\
/// `nm` = network manifold\
/// `t` = time\
/// `c` = variable config
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
