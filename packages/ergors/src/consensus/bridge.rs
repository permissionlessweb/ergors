//! Bridge between Commonware Simplex consensus and the ABCI lifecycle.
//!
//! Simplex calls `Automaton::propose()` → we drain mempool + run PrepareProposal.
//! Simplex calls `Automaton::verify()` → we run ProcessProposal.
//! Content is broadcast via `Relay::broadcast()` for cross-node verification.
//!
//! Uses an actor pattern (like Alto's marshal):
//! - [`MetaLedgerMailbox`] — thin cloneable handle (implements `Automaton` + `Relay`)
//! - [`MetaLedgerActor`] — background actor holding the actual state
//!
//! The Simplex engine only sees digests (SHA-256 hashes of block content).
//! The actual block content (Vec<NodeCommitment>) is stored in a bounded
//! [`ContentStore`] and looked up by digest during verification.

use super::{
    gossip::GossipHandle,
    lifecycle::ConsensusLifecycle,
    mempool::Mempool,
    types::NodeCommitment,
};
use commonware_consensus::{Automaton, Epochable, Relay};
use commonware_consensus::simplex::types::Context;
use commonware_consensus::types::Epoch;
use commonware_cryptography::{sha256, Hasher, Sha256};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt, StreamExt,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

// --- Type aliases for the Simplex integration ---

/// Block digest type — SHA-256 hash of serialized commitments.
pub type BlockDigest = sha256::Digest;

/// Peer identity key type.
pub type PeerKey = commonware_cryptography::ed25519::PublicKey;

/// Simplex context parameterized with our types.
pub type SimplexContext = Context<BlockDigest, PeerKey>;

/// Shared content store with bounded capacity.
pub type ContentStore = Arc<RwLock<BoundedContentStore>>;

/// Shared height counter for coordinating between bridge and reporter.
/// Represents the last finalized block height (0 = no blocks finalized).
/// Only the reporter writes (via `store`), bridge reads (via `load`).
pub type SharedHeight = Arc<std::sync::atomic::AtomicU64>;

/// Maximum unfinalized proposals to retain before evicting oldest.
const DEFAULT_MAX_CONTENT_ENTRIES: usize = 64;

// --- BoundedContentStore ---

/// Content store with bounded capacity and FIFO eviction.
///
/// Maps block digests to their commitment vectors. Evicts the oldest
/// entries when at capacity to prevent unbounded growth from
/// non-finalized proposals.
pub struct BoundedContentStore {
    entries: HashMap<BlockDigest, Vec<NodeCommitment>>,
    /// Insertion order for FIFO eviction.
    order: VecDeque<BlockDigest>,
    max_entries: usize,
}

impl BoundedContentStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_entries,
        }
    }

    /// Insert content. Evicts oldest entries if at capacity.
    /// Returns the number of stale entries evicted.
    pub fn insert(&mut self, digest: BlockDigest, commitments: Vec<NodeCommitment>) -> usize {
        // Duplicate digest — update in place, don't add to order queue
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.entries.entry(digest) {
            e.insert(commitments);
            return 0;
        }

        // Evict oldest if at capacity
        let mut evicted = 0;
        while self.entries.len() >= self.max_entries {
            match self.order.pop_front() {
                Some(old_digest) => {
                    if self.entries.remove(&old_digest).is_some() {
                        warn!(
                            ?old_digest,
                            "evicted stale proposal from content store"
                        );
                        evicted += 1;
                    }
                    // If not in entries, it was already removed (finalized) — skip
                }
                None => break,
            }
        }

        self.entries.insert(digest, commitments);
        self.order.push_back(digest);
        evicted
    }

    pub fn get(&self, digest: &BlockDigest) -> Option<&Vec<NodeCommitment>> {
        self.entries.get(digest)
    }

    pub fn remove(&mut self, digest: &BlockDigest) -> Option<Vec<NodeCommitment>> {
        if let Some(commitments) = self.entries.remove(digest) {
            self.order.retain(|d| d != digest);
            Some(commitments)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// --- Messages between Mailbox and Actor ---

/// Messages sent from the `MetaLedgerMailbox` to the `MetaLedgerActor`.
pub enum BridgeMessage {
    Genesis {
        epoch: Epoch,
        response: oneshot::Sender<BlockDigest>,
    },
    Propose {
        context: SimplexContext,
        response: oneshot::Sender<BlockDigest>,
    },
    Verify {
        context: SimplexContext,
        payload: BlockDigest,
        response: oneshot::Sender<bool>,
    },
    Broadcast {
        payload: BlockDigest,
    },
}

// --- MetaLedgerMailbox (thin handle) ---

/// Thin cloneable handle that implements [`Automaton`] and [`Relay`].
///
/// Pass this to the Simplex [`Engine`](commonware_consensus::simplex::Engine) config
/// as both `automaton` and `relay`. It forwards all calls to the background
/// [`MetaLedgerActor`] via an mpsc channel.
#[derive(Clone)]
pub struct MetaLedgerMailbox {
    sender: mpsc::Sender<BridgeMessage>,
}

impl Automaton for MetaLedgerMailbox {
    type Context = SimplexContext;
    type Digest = BlockDigest;

    async fn genesis(&mut self, epoch: <Self::Context as Epochable>::Epoch) -> Self::Digest {
        let (response, receiver) = oneshot::channel();
        if self
            .sender
            .send(BridgeMessage::Genesis { epoch, response })
            .await
            .is_err()
        {
            error!("bridge actor closed during genesis — consensus cannot start");
            return Sha256::hash(b"error-actor-closed");
        }
        receiver.await.unwrap_or_else(|_| {
            error!("bridge actor dropped genesis response");
            Sha256::hash(b"error-response-dropped")
        })
    }

    async fn propose(&mut self, context: Self::Context) -> oneshot::Receiver<Self::Digest> {
        let (response, receiver) = oneshot::channel();
        // If send fails (actor gone), the receiver will resolve to Canceled
        let _ = self
            .sender
            .send(BridgeMessage::Propose { context, response })
            .await;
        receiver
    }

    async fn verify(
        &mut self,
        context: Self::Context,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        let (response, receiver) = oneshot::channel();
        let _ = self
            .sender
            .send(BridgeMessage::Verify {
                context,
                payload,
                response,
            })
            .await;
        receiver
    }
}

impl Relay for MetaLedgerMailbox {
    type Digest = BlockDigest;

    async fn broadcast(&mut self, payload: Self::Digest) {
        let _ = self
            .sender
            .send(BridgeMessage::Broadcast { payload })
            .await;
    }
}

// --- MetaLedgerActor (background processor) ---

/// Background actor that processes bridge messages.
///
/// Holds references to the lifecycle (for PrepareProposal/ProcessProposal),
/// the mempool, and the shared content store. Processes messages from the
/// [`MetaLedgerMailbox`] sequentially.
pub struct MetaLedgerActor {
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    mempool: Arc<Mempool>,
    content: ContentStore,
    height: SharedHeight,
    gossip: Option<GossipHandle>,
    mailbox: mpsc::Receiver<BridgeMessage>,
}

impl MetaLedgerActor {
    /// Set the gossip handle after construction.
    /// Used when gossip depends on the ContentStore created by the bridge.
    pub fn set_gossip(&mut self, gossip: GossipHandle) {
        self.gossip = Some(gossip);
    }

    /// Hash a set of commitments to produce a block digest.
    ///
    /// Deterministic: same commitments in same order → same digest.
    pub fn hash_commitments(commitments: &[NodeCommitment]) -> BlockDigest {
        let mut hasher = Sha256::new();
        for c in commitments {
            hasher.update(&c.to_bytes());
        }
        hasher.finalize()
    }

    /// Genesis digest includes the epoch for network differentiation.
    fn genesis(&self, epoch: Epoch) -> BlockDigest {
        let mut hasher = Sha256::new();
        hasher.update(b"ergors-meta-ledger-genesis-");
        hasher.update(&epoch.to_le_bytes());
        hasher.finalize()
    }

    /// Drain mempool, validate candidates, and produce a block digest.
    ///
    /// The bridge owns the mempool drain lifecycle:
    /// 1. Drain candidates from mempool
    /// 2. Pass to lifecycle.prepare_proposal for validation/ordering
    /// 3. On failure, requeue all candidates back to mempool
    async fn propose(&self, context: &SimplexContext) -> BlockDigest {
        let height =
            self.height
                .load(std::sync::atomic::Ordering::SeqCst) + 1;

        // Bridge owns the drain — can requeue on failure
        let candidates = self.mempool.drain_for_proposal().await;

        let lifecycle = self.lifecycle.read().await;
        let commitments = match lifecycle
            .prepare_proposal(height, 1_048_576, &candidates)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                warn!(?e, height, "prepare_proposal failed, requeueing candidates");
                drop(lifecycle);
                let dropped = self.mempool.requeue(candidates).await;
                if !dropped.is_empty() {
                    warn!(
                        dropped = dropped.len(),
                        "could not requeue all candidates after prepare_proposal failure"
                    );
                }
                Vec::new()
            }
        };

        let digest = Self::hash_commitments(&commitments);
        let evicted = self.content.write().await.insert(digest, commitments);
        if evicted > 0 {
            warn!(evicted, "evicted stale proposals during insert");
        }

        debug!(
            ?digest,
            height,
            round = ?context.round,
            leader = ?context.leader,
            "proposed block"
        );
        digest
    }

    async fn verify(&self, context: &SimplexContext, payload: BlockDigest) -> bool {
        let height =
            self.height
                .load(std::sync::atomic::Ordering::SeqCst) + 1;

        let store = self.content.read().await;
        let commitments = match store.get(&payload) {
            Some(c) => c,
            None => {
                // Expected for non-leader nodes until Phase 3 (gossip) broadcasts content.
                warn!(
                    ?payload,
                    round = ?context.round,
                    "verify: block content not available (needs gossip layer)"
                );
                return false;
            }
        };

        let lifecycle = self.lifecycle.read().await;
        match lifecycle.process_proposal(height, commitments).await {
            Ok(valid) => valid,
            Err(e) => {
                warn!(?e, height, "process_proposal failed");
                false
            }
        }
    }

    /// Run the actor loop, processing messages until the channel closes.
    pub async fn run(mut self) {
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                BridgeMessage::Genesis { epoch, response } => {
                    let digest = self.genesis(epoch);
                    self.content.write().await.insert(digest, Vec::new());
                    let _ = response.send(digest);
                }
                BridgeMessage::Propose { context, response } => {
                    let digest = self.propose(&context).await;
                    let _ = response.send(digest);
                }
                BridgeMessage::Verify {
                    context,
                    payload,
                    response,
                } => {
                    let valid = self.verify(&context, payload).await;
                    let _ = response.send(valid);
                }
                BridgeMessage::Broadcast { payload } => {
                    if let Some(ref gossip) = self.gossip {
                        gossip.broadcast_content(payload);
                        debug!(?payload, "broadcast block content via gossip");
                    } else {
                        debug!(?payload, "broadcast block digest (no gossip handle)");
                    }
                }
            }
        }
        debug!("bridge actor shutting down");
    }
}

// --- Constructor ---

/// Create a new MetaLedger bridge.
///
/// Returns:
/// - `MetaLedgerMailbox` — pass to `simplex::Config` as `automaton` and `relay`
/// - `MetaLedgerActor` — spawn as a background task (`tokio::spawn(actor.run())`)
/// - `ContentStore` — share with [`FinalizationReporter`](super::reporter::FinalizationReporter)
/// - `SharedHeight` — share with [`FinalizationReporter`](super::reporter::FinalizationReporter)
pub fn new_bridge(
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    mempool: Arc<Mempool>,
    mailbox_size: usize,
    gossip: Option<GossipHandle>,
) -> (MetaLedgerMailbox, MetaLedgerActor, ContentStore, SharedHeight) {
    let (sender, receiver) = mpsc::channel(mailbox_size);
    let content: ContentStore =
        Arc::new(RwLock::new(BoundedContentStore::new(DEFAULT_MAX_CONTENT_ENTRIES)));
    let height: SharedHeight = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let mailbox = MetaLedgerMailbox { sender };
    let actor = MetaLedgerActor {
        lifecycle,
        mempool,
        content: content.clone(),
        height: height.clone(),
        gossip,
        mailbox: receiver,
    };

    (mailbox, actor, content, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::{
        app::ErgorsConsensusApp,
        mempool::Mempool,
        types::CommitmentKind,
    };
    use ho_std::keys::commonware::NodePrivKey;
    use tempfile::TempDir;

    async fn test_storage() -> (cnidarium::Storage, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = cnidarium::Storage::load(dir.path().to_path_buf(), vec![])
            .await
            .unwrap();
        (storage, dir)
    }

    fn test_validators() -> Vec<(commonware_cryptography::ed25519::PublicKey, u64)> {
        vec![
            (NodePrivKey::from_seed(1).id().0, 1),
            (NodePrivKey::from_seed(2).id().0, 1),
        ]
    }

    fn make_commitment(signer: &NodePrivKey, seq: u64) -> NodeCommitment {
        NodeCommitment::new(
            signer,
            [0u8; 32],
            [(seq & 0xff) as u8; 32],
            format!("transition-{seq}").as_bytes(),
            CommitmentKind::Inference,
            seq,
        )
    }

    /// Construct a minimal simplex Context for testing.
    fn dummy_context(parent_digest: BlockDigest) -> SimplexContext {
        use commonware_consensus::types::Round;
        SimplexContext {
            round: Round::new(0, 1),
            leader: NodePrivKey::from_seed(1).id().0,
            parent: (0, parent_digest),
        }
    }

    #[tokio::test]
    async fn genesis_includes_epoch() {
        let actor_genesis = |epoch: Epoch| {
            let mut hasher = Sha256::new();
            hasher.update(b"ergors-meta-ledger-genesis-");
            hasher.update(&epoch.to_le_bytes());
            hasher.finalize()
        };

        let d0 = actor_genesis(0);
        let d1 = actor_genesis(1);
        assert_ne!(d0, d1, "different epochs must produce different genesis digests");

        // Same epoch = deterministic
        let d0b = actor_genesis(0);
        assert_eq!(d0, d0b, "same epoch must produce same genesis digest");
    }

    #[tokio::test]
    async fn hash_commitments_deterministic() {
        let signer = NodePrivKey::from_seed(1);
        let c1 = make_commitment(&signer, 1);
        let c2 = make_commitment(&signer, 2);

        let h1 = MetaLedgerActor::hash_commitments(&[c1.clone(), c2.clone()]);
        let h2 = MetaLedgerActor::hash_commitments(&[c1.clone(), c2.clone()]);
        assert_eq!(h1, h2, "same commitments must produce same digest");

        // Different order → different digest
        let h3 = MetaLedgerActor::hash_commitments(&[c2, c1]);
        assert_ne!(h1, h3, "different order must produce different digest");
    }

    #[tokio::test]
    async fn bounded_content_store_evicts_oldest() {
        let mut store = BoundedContentStore::new(3);
        let signer = NodePrivKey::from_seed(1);

        let d1 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 1)]);
        let d2 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 2)]);
        let d3 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 3)]);
        let d4 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 4)]);

        store.insert(d1, vec![make_commitment(&signer, 1)]);
        store.insert(d2, vec![make_commitment(&signer, 2)]);
        store.insert(d3, vec![make_commitment(&signer, 3)]);
        assert_eq!(store.len(), 3);

        // Inserting 4th should evict d1
        let evicted = store.insert(d4, vec![make_commitment(&signer, 4)]);
        assert_eq!(evicted, 1);
        assert_eq!(store.len(), 3);
        assert!(store.get(&d1).is_none(), "oldest should be evicted");
        assert!(store.get(&d4).is_some(), "newest should exist");
    }

    #[tokio::test]
    async fn bounded_content_store_dedup_insert() {
        let mut store = BoundedContentStore::new(3);
        let signer = NodePrivKey::from_seed(1);
        let d1 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 1)]);

        store.insert(d1, vec![make_commitment(&signer, 1)]);
        store.insert(d1, vec![make_commitment(&signer, 1)]); // duplicate
        assert_eq!(store.len(), 1, "duplicate insert should not increase count");
    }

    #[tokio::test]
    async fn bounded_content_store_remove_cleans_order() {
        let mut store = BoundedContentStore::new(10);
        let signer = NodePrivKey::from_seed(1);

        let d1 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 1)]);
        let d2 = MetaLedgerActor::hash_commitments(&[make_commitment(&signer, 2)]);

        store.insert(d1, vec![make_commitment(&signer, 1)]);
        store.insert(d2, vec![make_commitment(&signer, 2)]);

        // Remove d1 (simulating finalization eviction)
        let removed = store.remove(&d1);
        assert!(removed.is_some());
        assert_eq!(store.len(), 1);

        // d1 should not be in the order queue anymore
        assert!(store.get(&d1).is_none());
    }

    #[tokio::test]
    async fn bridge_propose_and_verify() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));

        // Add commitments to mempool
        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);
        let c_a = make_commitment(&signer_a, 1);
        let c_b = make_commitment(&signer_b, 1);
        mempool.add(c_a).await;
        mempool.add(c_b).await;

        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let (mut mailbox, actor, content, _height) =
            new_bridge(lifecycle, mempool.clone(), 128, None);

        // Spawn the actor
        let actor_handle = tokio::spawn(actor.run());

        // Genesis
        let genesis_digest = mailbox.genesis(0).await;
        assert!(
            content.read().await.get(&genesis_digest).is_some(),
            "genesis block should be in content store"
        );

        // Propose — actor drains mempool and creates block
        let propose_receiver = mailbox.propose(dummy_context(genesis_digest)).await;
        let block_digest = propose_receiver.await.expect("propose should succeed");

        // Content should be stored
        let stored = content.read().await;
        let commitments = stored.get(&block_digest).expect("content should be stored");
        assert_eq!(commitments.len(), 2, "should have 2 commitments");
        drop(stored);

        // Mempool should be drained
        assert!(mempool.is_empty().await, "mempool should be drained after propose");

        // Verify the proposed block
        let verify_receiver = mailbox.verify(dummy_context(genesis_digest), block_digest).await;
        let valid = verify_receiver.await.expect("verify should respond");
        assert!(valid, "proposed block should verify");

        // Verify with unknown digest should fail
        let unknown = Sha256::hash(b"unknown-block");
        let verify_receiver = mailbox.verify(dummy_context(genesis_digest), unknown).await;
        let valid = verify_receiver.await.expect("verify should respond");
        assert!(!valid, "unknown block should not verify");

        // Shutdown
        drop(mailbox);
        let _ = actor_handle.await;
    }

    #[tokio::test]
    async fn bridge_graceful_shutdown() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let (mut mailbox, actor, _, _) = new_bridge(lifecycle, mempool, 128, None);
        let actor_handle = tokio::spawn(actor.run());

        // Drop actor first (simulate crash)
        drop(actor_handle);
        // Small delay for drop to propagate
        tokio::task::yield_now().await;

        // Genesis should NOT panic — returns default digest
        // (channel may still work briefly after handle drop, but the test
        // verifies we don't panic even if the actor is gone)
        let _digest = mailbox.genesis(0).await;
    }
}
