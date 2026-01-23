pub mod auth;
pub mod bootstrap;
pub mod call;
pub mod config;
pub mod config_cmd;
#[cfg(feature = "cw")]
pub mod contracts;
pub mod daemon;
pub mod deploy;
pub mod distribution;
pub mod git;
pub mod grpc;
pub mod headstash;
pub mod init;
pub mod middleware;
pub mod network;
pub mod open_responses;
pub mod orchestrator;
pub mod proxy;
pub mod server;
pub mod session;
pub mod storage;
pub mod traits;

#[cfg(feature = "cw")]
pub mod cosmwasm;
#[cfg(feature = "cw")]
use ho_std::wasm::WasmRuntime;

// Re-export the macro for external use
use crate::{config::ErgorsConfig, network::manager::PeerInfo, storage::ErgorsStorage};
use ho_std::{llm::LlmRouter, types::ergors::network::v1::*};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{mpsc, RwLock};
use {
    commonware_cryptography::ed25519, commonware_p2p::authenticated,
    commonware_runtime::tokio::Context,
};

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
/// `c` = variable config\
/// `wasm` = WASM runtime (optional)
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
    /// wasm = WASM runtime (when cw feature is enabled)
    #[cfg(feature = "cw")]
    pub wasm: Arc<WasmRuntime>,
}

impl ErgorsAppState {
    fn new(
        r: Arc<LlmRouter>,
        s: Arc<ErgorsStorage>,
        nm: Arc<tokio::sync::Mutex<ErgorsNetworkManifold>>,
        t: Instant,
        c: ErgorsConfig,
        #[cfg(feature = "cw")] wasm: Arc<WasmRuntime>,
    ) -> Self {
        Self {
            r,
            s,
            nm,
            t,
            c,
            #[cfg(feature = "cw")]
            wasm,
        }
    }
}
