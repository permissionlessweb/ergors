pub mod auth;
pub mod bootstrap;
pub mod call;
pub mod client;
pub mod commands;
pub mod config;
pub mod config_cmd;
pub mod keys;
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
pub mod rag;
pub mod server;
pub mod session;
pub mod storage;
pub mod traits;

#[cfg(feature = "cw")]
pub mod cosmwasm;
#[cfg(feature = "cw")]
use ho_std::wasm::WasmRuntime;

// Re-export the macro for external use
use crate::{
    config::ErgorsConfig,
    deploy::{
        automated::AutomatedDeployer, certificate::CertificateManager,
        cosmos_client::CosmosClient, signer::TxSigner, tx_lifecycle::TxLifecycle,
    },
    network::manager::PeerInfo,
    proxy::ProxyRouter,
    storage::ErgorsStorage,
};
use ho_std::{
    keys::encrypted_cosmos::EncryptedCosmosKeyManager,
    llm::LlmRouter,
    types::ergors::{network::v1::*, orch::v1::CosmosKeyStore},
};
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

/// Akash deployment context containing all components needed for automated deployments.
///
/// This is initialized lazily when Akash config is present and a key store exists.
#[derive(Clone)]
pub struct AkashDeploymentContext {
    /// CosmosClient for chain queries
    pub cosmos: Arc<CosmosClient>,
    /// Transaction signer
    pub signer: Arc<TxSigner>,
    /// Transaction lifecycle manager
    pub tx_lifecycle: Arc<TxLifecycle>,
    /// Certificate manager
    pub cert_manager: Arc<CertificateManager>,
    /// Key manager (unlocked with password)
    pub key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    /// Key store
    pub key_store: Arc<RwLock<CosmosKeyStore>>,
}

impl AkashDeploymentContext {
    /// Create automated deployer from this context.
    pub fn create_deployer(&self, storage: Arc<ErgorsStorage>) -> AutomatedDeployer {
        AutomatedDeployer::new(
            storage,
            self.cosmos.clone(),
            self.cert_manager.clone(),
            self.tx_lifecycle.clone(),
            self.signer.clone(),
        )
    }
}

/// `r` = router\
/// `s` = storage\
/// `nm` = network manifold\
/// `t` = time\
/// `c` = variable config\
/// `akash` = Akash deployment context (optional)\
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
    /// pr = proxy router (dynamic routing to upstream providers)
    pub pr: Arc<RwLock<ProxyRouter>>,
    /// akash = Akash deployment context (when Akash config + key store present)
    pub akash: Option<AkashDeploymentContext>,
    /// wasm = WASM runtime (when cw feature is enabled)
    #[cfg(feature = "cw")]
    pub wasm: Arc<WasmRuntime>,
}

impl ErgorsAppState {
    pub fn new(
        r: Arc<LlmRouter>,
        s: Arc<ErgorsStorage>,
        nm: Arc<tokio::sync::Mutex<ErgorsNetworkManifold>>,
        t: Instant,
        c: ErgorsConfig,
        pr: Arc<RwLock<ProxyRouter>>,
        akash: Option<AkashDeploymentContext>,
        #[cfg(feature = "cw")] wasm: Arc<WasmRuntime>,
    ) -> Self {
        Self {
            r,
            s,
            nm,
            t,
            c,
            pr,
            akash,
            #[cfg(feature = "cw")]
            wasm,
        }
    }
}
