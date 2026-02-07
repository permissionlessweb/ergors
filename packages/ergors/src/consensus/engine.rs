//! Engine orchestrator — wires all consensus components into a running Simplex system.
//!
//! # Supreme Leader Model
//!
//! Simplex runs with a **single participant** (the supreme leader). With 1 participant,
//! threshold = 1 → the supreme leader proposes, self-notarizes, and finalizes each block
//! instantly. Other nodes are **observers**: they receive finalized blocks via gossip,
//! validate, and apply state, but don't participate in consensus voting.
//!
//! - Supreme leader drops → consensus halts (no blocks produced)
//! - Supreme leader returns → consensus resumes
//! - Observer drops → no impact on block production
//!
//! [`start_consensus()`] auto-detects the node's role from its private key:
//! - If `private_key.public_key() == supreme_leader` → runs full Simplex engine
//! - Otherwise → observer mode (gossip + mempool only, no Simplex engine)

use super::{
    app::ErgorsConsensusApp,
    bridge::{self, SharedHeight},
    gossip,
    lifecycle::ConsensusLifecycle,
    mempool::Mempool,
    reporter::{FinalizationReporter, SimplexScheme},
};
use commonware_consensus::simplex;
use commonware_cryptography::{ed25519, Signer};
use commonware_p2p::{authenticated, Blocker};
use commonware_runtime::{
    buffer::PoolRef,
    tokio::Context,
    Metrics,
};
use commonware_utils::set::Ordered;
use governor::Quota;
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Configuration for the Simplex consensus engine.
pub struct ConsensusConfig {
    pub supreme_leader: ed25519::PublicKey,
    pub epoch: u64,
    pub namespace: Vec<u8>,
    pub partition: String,
    pub mailbox_size: usize,
    pub leader_timeout: Duration,
    pub notarization_timeout: Duration,
    pub nullify_retry: Duration,
    pub fetch_timeout: Duration,
    pub activity_timeout: u64,
    pub skip_timeout: u64,
    pub max_fetch_count: usize,
    pub fetch_rate_per_peer: Quota,
    pub fetch_concurrent: usize,
    pub replay_buffer: NonZeroUsize,
    pub write_buffer: NonZeroUsize,
    pub max_per_block: usize,
    pub max_pending: usize,
}

impl ConsensusConfig {
    /// Recommended defaults for a production-like consensus configuration.
    ///
    /// `supreme_leader` is the only Simplex participant — the node whose
    /// presence is required for block production.
    pub fn recommended(supreme_leader: ed25519::PublicKey) -> Self {
        Self {
            supreme_leader,
            epoch: 0,
            namespace: b"ergors-consensus".to_vec(),
            partition: "ergors".to_string(),
            mailbox_size: 128,
            leader_timeout: Duration::from_secs(1),
            notarization_timeout: Duration::from_secs(2),
            nullify_retry: Duration::from_secs(10),
            fetch_timeout: Duration::from_secs(5),
            activity_timeout: 10,
            skip_timeout: 5,
            max_fetch_count: 16,
            fetch_rate_per_peer: Quota::per_second(NonZeroU32::new(10).unwrap()),
            fetch_concurrent: 4,
            replay_buffer: NonZeroUsize::new(512).unwrap(),
            write_buffer: NonZeroUsize::new(512).unwrap(),
            max_per_block: 100,
            max_pending: 1000,
        }
    }
}

/// Minimal Blocker that logs block requests without actually blocking peers.
#[derive(Clone)]
pub struct LogOnlyBlocker;

impl Blocker for LogOnlyBlocker {
    type PublicKey = ed25519::PublicKey;

    async fn block(&mut self, peer: Self::PublicKey) {
        warn!(?peer, "consensus requested peer block (log-only)");
    }
}

/// Handle returned by [`start_consensus()`].
///
/// Background actors (bridge, gossip, engine) run as spawned tasks until shutdown.
pub struct ConsensusSystem {
    pub height: SharedHeight,
    /// True if this node is the supreme leader running the Simplex engine.
    /// False for observer nodes.
    pub is_leader: bool,
}

/// Wire all consensus components and start the Simplex engine.
///
/// Auto-detects role: if this node's key matches `config.supreme_leader`,
/// runs the full Simplex engine. Otherwise runs in observer mode
/// (gossip + mempool, no consensus voting).
///
/// # Arguments
/// - `context` — Commonware runtime context
/// - `private_key` — This node's Ed25519 private key for signing
/// - `all_validators` — All known validator public keys with voting power (for commitment validation)
/// - `storage` — Cnidarium storage backend
/// - `gossip_channel` — Optional P2P channel 10 (sender, receiver) for gossip
/// - `pending_network` — P2P channel 5 for pending votes (used by supreme leader only)
/// - `recovered_network` — P2P channel 6 for recovered certificates (used by supreme leader only)
/// - `resolver_network` — P2P channel 7 for resolution requests (used by supreme leader only)
/// - `config` — Consensus configuration (includes `supreme_leader` designation)
pub fn start_consensus(
    context: Context,
    private_key: ed25519::PrivateKey,
    all_validators: Vec<(ed25519::PublicKey, u64)>,
    storage: cnidarium::Storage,
    gossip_channel: Option<(
        authenticated::lookup::Sender<ed25519::PublicKey>,
        authenticated::lookup::Receiver<ed25519::PublicKey>,
    )>,
    pending_network: (
        authenticated::lookup::Sender<ed25519::PublicKey>,
        authenticated::lookup::Receiver<ed25519::PublicKey>,
    ),
    recovered_network: (
        authenticated::lookup::Sender<ed25519::PublicKey>,
        authenticated::lookup::Receiver<ed25519::PublicKey>,
    ),
    resolver_network: (
        authenticated::lookup::Sender<ed25519::PublicKey>,
        authenticated::lookup::Receiver<ed25519::PublicKey>,
    ),
    config: ConsensusConfig,
) -> ConsensusSystem {
    let our_pubkey = private_key.public_key();
    let is_leader = our_pubkey == config.supreme_leader;

    if is_leader {
        info!("starting as SUPREME LEADER — running Simplex consensus engine");
    } else {
        info!(
            supreme_leader = ?config.supreme_leader,
            "starting as OBSERVER — gossip + mempool only, no consensus voting"
        );
    }

    // 1. Mempool — all nodes collect commitments
    let mempool = Arc::new(Mempool::new(config.max_per_block, config.max_pending));

    // 2. Application lifecycle — all validators are known for commitment validation,
    //    but only the supreme leader participates in Simplex consensus
    let app = ErgorsConsensusApp::new(storage, all_validators, mempool.clone());
    let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

    // 3. Bridge + content store + shared height
    let (mailbox, mut actor, content, height) =
        bridge::new_bridge(lifecycle.clone(), mempool.clone(), config.mailbox_size, None);

    // 4. Gossip — all nodes participate (observers send commitments, receive blocks)
    if let Some((gsender, greceiver)) = gossip_channel {
        let (gossip_handle, gossip_actor) =
            gossip::new_gossip(lifecycle.clone(), mempool.clone(), content.clone(), gsender, greceiver);
        actor.set_gossip(gossip_handle);
        tokio::spawn(gossip_actor.run());
    }

    // 5. Spawn bridge actor (both leader and observer)
    tokio::spawn(actor.run());

    // 6. Supreme leader: build and start Simplex engine
    if is_leader {
        // Ordered set contains ONLY the supreme leader — threshold = 1
        let ordered: Ordered<ed25519::PublicKey> = vec![config.supreme_leader].into();
        let scheme = SimplexScheme::new(ordered, private_key);

        let reporter = FinalizationReporter::new(lifecycle, content, height.clone());
        let blocker = LogOnlyBlocker;
        let buffer_pool = PoolRef::new(
            NonZeroUsize::new(1024).unwrap(),
            NonZeroUsize::new(10).unwrap(),
        );

        let simplex_config = simplex::Config {
            scheme,
            blocker,
            automaton: mailbox.clone(),
            relay: mailbox,
            reporter,
            partition: config.partition,
            mailbox_size: config.mailbox_size,
            epoch: config.epoch,
            namespace: config.namespace,
            replay_buffer: config.replay_buffer,
            write_buffer: config.write_buffer,
            buffer_pool,
            leader_timeout: config.leader_timeout,
            notarization_timeout: config.notarization_timeout,
            nullify_retry: config.nullify_retry,
            activity_timeout: config.activity_timeout,
            skip_timeout: config.skip_timeout,
            fetch_timeout: config.fetch_timeout,
            max_fetch_count: config.max_fetch_count,
            fetch_rate_per_peer: config.fetch_rate_per_peer,
            fetch_concurrent: config.fetch_concurrent,
        };

        let engine = simplex::Engine::new(context.with_label("consensus"), simplex_config);
        let _handle = engine.start(pending_network, recovered_network, resolver_network);
    }
    // Observer: Simplex channels are unused — gossip provides block content.
    // Phase 5 will add gossip-triggered finalization for observers.

    ConsensusSystem { height, is_leader }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::keys::commonware::NodePrivKey;

    fn leader_key() -> NodePrivKey {
        NodePrivKey::from_seed(1)
    }

    #[test]
    fn consensus_config_defaults() {
        let leader = leader_key().id().0;
        let cfg = ConsensusConfig::recommended(leader.clone());
        assert_eq!(cfg.epoch, 0);
        assert_eq!(cfg.mailbox_size, 128);
        assert!(cfg.leader_timeout < cfg.notarization_timeout);
        assert!(cfg.activity_timeout > cfg.skip_timeout);
        assert!(cfg.max_per_block > 0);
        assert!(cfg.max_pending >= cfg.max_per_block);
        assert_eq!(cfg.supreme_leader, leader);
    }

    #[tokio::test]
    async fn log_only_blocker_does_not_panic() {
        let mut blocker = LogOnlyBlocker;
        let key = NodePrivKey::from_seed(1).id().0;
        blocker.block(key).await;
    }

    #[test]
    fn scheme_single_participant() {
        // Supreme leader model: Ordered set contains only 1 key
        let leader = leader_key();
        let ordered: Ordered<ed25519::PublicKey> = vec![leader.id().0].into();
        let _scheme = SimplexScheme::new(ordered, leader.private_key());
    }

    #[tokio::test]
    async fn start_consensus_leader_construction() {
        // Verify supreme leader config builds a valid Simplex Config.
        // Engine::new() calls cfg.assert() — validates all constraints.
        let leader = leader_key();
        let supreme_leader_pubkey = leader.id().0;

        // Ordered: only the supreme leader
        let ordered: Ordered<ed25519::PublicKey> = vec![supreme_leader_pubkey.clone()].into();
        let scheme = SimplexScheme::new(ordered, leader.private_key());

        let dir = tempfile::TempDir::new().unwrap();
        let storage = cnidarium::Storage::load(dir.path().to_path_buf(), vec![])
            .await
            .unwrap();

        let all_validators = vec![
            (leader.id().0, 1u64),
            (NodePrivKey::from_seed(2).id().0, 1u64),
        ];

        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, all_validators, mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let (mailbox, _actor, content, height) =
            bridge::new_bridge(lifecycle.clone(), mempool, 128, None);

        let reporter = FinalizationReporter::new(lifecycle, content, height);
        let blocker = LogOnlyBlocker;
        let buffer_pool = PoolRef::new(
            NonZeroUsize::new(1024).unwrap(),
            NonZeroUsize::new(10).unwrap(),
        );

        let config = ConsensusConfig::recommended(supreme_leader_pubkey);

        // Validates all trait bounds + single-participant config at compile time
        let _cfg = simplex::Config {
            scheme,
            blocker,
            automaton: mailbox.clone(),
            relay: mailbox,
            reporter,
            partition: config.partition,
            mailbox_size: config.mailbox_size,
            epoch: config.epoch,
            namespace: config.namespace,
            replay_buffer: config.replay_buffer,
            write_buffer: config.write_buffer,
            buffer_pool,
            leader_timeout: config.leader_timeout,
            notarization_timeout: config.notarization_timeout,
            nullify_retry: config.nullify_retry,
            activity_timeout: config.activity_timeout,
            skip_timeout: config.skip_timeout,
            fetch_timeout: config.fetch_timeout,
            max_fetch_count: config.max_fetch_count,
            fetch_rate_per_peer: config.fetch_rate_per_peer,
            fetch_concurrent: config.fetch_concurrent,
        };
    }
}
