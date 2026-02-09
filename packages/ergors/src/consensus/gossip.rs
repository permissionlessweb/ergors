//! Gossip layer for mempool convergence and block content relay.
//!
//! Closes the Phase 2 gap: non-leader nodes can now receive block content
//! from the leader (via P2P channel 10) and verify proposals.
//!
//! Two message types flow over the gossip channel:
//! - **CommitmentGossip** — individual NodeCommitment pushed to peers
//! - **BlockContent** — Vec<NodeCommitment> keyed by digest, from leader to validators
//!
//! Uses the same actor pattern as bridge.rs:
//! - [`GossipHandle`] — thin cloneable handle for fire-and-forget commands
//! - [`ConsensusGossipActor`] — background task multiplexing P2P + local commands

use super::{
    bridge::{BlockDigest, ContentStore, MetaLedgerActor},
    lifecycle::ConsensusLifecycle,
    mempool::Mempool,
    types::NodeCommitment,
};
use bytes::Bytes;
use commonware_cryptography::{ed25519, sha256};
use commonware_p2p::{authenticated, Recipients, Sender as P2pSender, Receiver as P2pReceiver};
use futures::{
    channel::mpsc,
    SinkExt, StreamExt,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

// --- Wire format ---

/// Gossip message envelope.
pub enum GossipMessage {
    /// A single commitment pushed to peers for mempool convergence.
    CommitmentGossip(NodeCommitment),
    /// Block content (Vec<NodeCommitment>) keyed by digest, from leader to validators.
    BlockContent {
        digest: BlockDigest,
        commitments: Vec<NodeCommitment>,
    },
}

/// Message type tags for the wire format.
const TAG_COMMITMENT: u8 = 0x00;
const TAG_BLOCK_CONTENT: u8 = 0x01;

/// Size of a single NodeCommitment in wire format.
const COMMITMENT_SIZE: usize = 201;

/// Encode a gossip message to bytes.
pub fn encode_gossip_message(msg: &GossipMessage) -> Vec<u8> {
    match msg {
        GossipMessage::CommitmentGossip(commitment) => {
            let mut buf = Vec::with_capacity(1 + COMMITMENT_SIZE);
            buf.push(TAG_COMMITMENT);
            buf.extend_from_slice(&commitment.to_bytes());
            buf
        }
        GossipMessage::BlockContent { digest, commitments } => {
            use commonware_codec::Encode;
            let encoded_digest = digest.encode();
            let count = commitments.len() as u32;
            let mut buf = Vec::with_capacity(1 + 32 + 4 + COMMITMENT_SIZE * commitments.len());
            buf.push(TAG_BLOCK_CONTENT);
            buf.extend_from_slice(&encoded_digest);
            buf.extend_from_slice(&count.to_le_bytes());
            for c in commitments {
                buf.extend_from_slice(&c.to_bytes());
            }
            buf
        }
    }
}

/// Decode a gossip message from bytes. Returns None on any malformed input.
pub fn decode_gossip_message(bytes: &[u8]) -> Option<GossipMessage> {
    if bytes.is_empty() {
        return None;
    }

    match bytes[0] {
        TAG_COMMITMENT => {
            if bytes.len() != 1 + COMMITMENT_SIZE {
                return None;
            }
            let commitment = NodeCommitment::from_bytes(&bytes[1..])?;
            Some(GossipMessage::CommitmentGossip(commitment))
        }
        TAG_BLOCK_CONTENT => {
            // Minimum: tag(1) + digest(32) + count(4) = 37
            if bytes.len() < 37 {
                return None;
            }
            use commonware_codec::DecodeExt;
            let digest = sha256::Digest::decode(&bytes[1..33]).ok()?;
            let count = u32::from_le_bytes(bytes[33..37].try_into().ok()?) as usize;

            let expected_len = 37 + COMMITMENT_SIZE * count;
            if bytes.len() != expected_len {
                return None;
            }

            let mut commitments = Vec::with_capacity(count);
            for i in 0..count {
                let start = 37 + i * COMMITMENT_SIZE;
                let end = start + COMMITMENT_SIZE;
                let c = NodeCommitment::from_bytes(&bytes[start..end])?;
                commitments.push(c);
            }

            Some(GossipMessage::BlockContent { digest, commitments })
        }
        _ => None,
    }
}

// --- Processing functions (testable without P2P) ---

/// Process an inbound commitment gossip message.
///
/// Validates the commitment (signature + check_tx) and adds to mempool.
/// Returns true if the commitment was accepted.
pub async fn process_commitment_gossip(
    lifecycle: &Arc<RwLock<dyn ConsensusLifecycle>>,
    mempool: &Arc<Mempool>,
    _peer: &ed25519::PublicKey,
    commitment: NodeCommitment,
) -> bool {
    // Fast reject: signature check
    if !commitment.verify() {
        warn!("gossip: rejected commitment with invalid signature");
        return false;
    }

    // Full validation via lifecycle
    let lc = lifecycle.read().await;
    if let Err(e) = lc.check_tx(&commitment).await {
        debug!(?e, "gossip: rejected commitment via check_tx");
        return false;
    }
    drop(lc);

    // Add to mempool (handles dedup)
    mempool.add(commitment).await
}

/// Process an inbound block content message.
///
/// Recomputes the digest from the commitments to prevent spoofing,
/// then stores in the content store.
/// Returns true if the content was accepted.
pub async fn process_block_content(
    content: &ContentStore,
    digest: BlockDigest,
    commitments: Vec<NodeCommitment>,
) -> bool {
    // Recompute digest — never trust the claimed digest
    let computed = MetaLedgerActor::hash_commitments(&commitments);
    if computed != digest {
        warn!(
            claimed = ?digest,
            computed = ?computed,
            "gossip: rejected block content with mismatched digest"
        );
        return false;
    }

    content.write().await.insert(digest, commitments);
    true
}

// --- Commands from GossipHandle to Actor ---

enum GossipCommand {
    BroadcastCommitment(NodeCommitment),
    BroadcastContent(BlockDigest),
}

// --- GossipHandle (thin, Clone) ---

/// Thin cloneable handle for sending gossip commands.
///
/// All methods are fire-and-forget — they log on failure but never block.
#[derive(Clone)]
pub struct GossipHandle {
    cmd_tx: mpsc::UnboundedSender<GossipCommand>,
}

impl GossipHandle {
    /// Broadcast a commitment to all peers for mempool convergence.
    pub fn broadcast_commitment(&self, commitment: NodeCommitment) {
        let _ = self.cmd_tx.unbounded_send(GossipCommand::BroadcastCommitment(commitment));
    }

    /// Broadcast block content (by digest) to all peers.
    /// The actor looks up the content from the shared ContentStore.
    pub fn broadcast_content(&self, digest: BlockDigest) {
        let _ = self.cmd_tx.unbounded_send(GossipCommand::BroadcastContent(digest));
    }
}

// --- ConsensusGossipActor ---

/// Background actor that multiplexes P2P inbound messages with local commands.
pub struct ConsensusGossipActor {
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    mempool: Arc<Mempool>,
    content: ContentStore,
    sender: authenticated::lookup::Sender<ed25519::PublicKey>,
    receiver: authenticated::lookup::Receiver<ed25519::PublicKey>,
    cmd_rx: mpsc::UnboundedReceiver<GossipCommand>,
}

impl ConsensusGossipActor {
    /// Run the actor loop until all handles are dropped.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                // Inbound P2P messages
                result = P2pReceiver::recv(&mut self.receiver) => {
                    match result {
                        Ok((peer, bytes)) => {
                            self.handle_inbound(&peer, &bytes).await;
                        }
                        Err(e) => {
                            warn!(?e, "gossip P2P receiver error");
                            break;
                        }
                    }
                }

                // Local commands from GossipHandle
                cmd = self.cmd_rx.next() => {
                    match cmd {
                        Some(command) => self.handle_command(command).await,
                        None => {
                            debug!("gossip actor shutting down (all handles dropped)");
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_inbound(&self, peer: &ed25519::PublicKey, bytes: &Bytes) {
        let msg = match decode_gossip_message(bytes) {
            Some(m) => m,
            None => {
                warn!("gossip: received malformed message, ignoring");
                return;
            }
        };

        match msg {
            GossipMessage::CommitmentGossip(commitment) => {
                let accepted = process_commitment_gossip(
                    &self.lifecycle,
                    &self.mempool,
                    peer,
                    commitment,
                )
                .await;
                debug!(accepted, "gossip: processed commitment from peer");
            }
            GossipMessage::BlockContent { digest, commitments } => {
                let accepted =
                    process_block_content(&self.content, digest, commitments).await;
                debug!(accepted, ?digest, "gossip: processed block content from peer");
            }
        }
    }

    async fn handle_command(&mut self, cmd: GossipCommand) {
        match cmd {
            GossipCommand::BroadcastCommitment(commitment) => {
                let msg = GossipMessage::CommitmentGossip(commitment);
                let bytes = Bytes::from(encode_gossip_message(&msg));
                if let Err(e) = self.sender.send(Recipients::All, bytes, false).await {
                    warn!(?e, "gossip: failed to broadcast commitment");
                }
            }
            GossipCommand::BroadcastContent(digest) => {
                let store = self.content.read().await;
                let commitments = match store.get(&digest) {
                    Some(c) => c.clone(),
                    None => {
                        warn!(?digest, "gossip: content not found for broadcast");
                        return;
                    }
                };
                drop(store);

                let msg = GossipMessage::BlockContent { digest, commitments };
                let bytes = Bytes::from(encode_gossip_message(&msg));
                if let Err(e) = self.sender.send(Recipients::All, bytes, false).await {
                    warn!(?e, "gossip: failed to broadcast block content");
                }
            }
        }
    }
}

// --- Constructor ---

/// Create a new gossip actor and handle.
///
/// Returns:
/// - `GossipHandle` — pass to bridge for broadcasting
/// - `ConsensusGossipActor` — spawn as background task (`tokio::spawn(actor.run())`)
pub fn new_gossip(
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    mempool: Arc<Mempool>,
    content: ContentStore,
    sender: authenticated::lookup::Sender<ed25519::PublicKey>,
    receiver: authenticated::lookup::Receiver<ed25519::PublicKey>,
) -> (GossipHandle, ConsensusGossipActor) {
    let (cmd_tx, cmd_rx) = mpsc::unbounded();

    let handle = GossipHandle { cmd_tx };
    let actor = ConsensusGossipActor {
        lifecycle,
        mempool,
        content,
        sender,
        receiver,
        cmd_rx,
    };

    (handle, actor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::{
        app::ErgorsConsensusApp,
        bridge::{BoundedContentStore, MetaLedgerActor},
        mempool::Mempool,
        types::CommitmentKind,
    };
    use ho_std::keys::commonware::NodePrivKey;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn test_signer(seed: u64) -> NodePrivKey {
        NodePrivKey::from_seed(seed)
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

    fn test_validators() -> Vec<(ed25519::PublicKey, u64)> {
        vec![
            (test_signer(1).id().0, 1),
            (test_signer(2).id().0, 1),
        ]
    }

    async fn test_storage() -> (cnidarium::Storage, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = cnidarium::Storage::load(dir.path().to_path_buf(), vec![])
            .await
            .unwrap();
        (storage, dir)
    }

    // --- Wire format tests ---

    #[test]
    fn wire_roundtrip_commitment() {
        let signer = test_signer(1);
        let commitment = make_commitment(&signer, 1);

        let msg = GossipMessage::CommitmentGossip(commitment.clone());
        let encoded = encode_gossip_message(&msg);
        assert_eq!(encoded.len(), 1 + COMMITMENT_SIZE);
        assert_eq!(encoded[0], TAG_COMMITMENT);

        let decoded = decode_gossip_message(&encoded).expect("should decode");
        match decoded {
            GossipMessage::CommitmentGossip(c) => {
                assert_eq!(c.sequence, commitment.sequence);
                assert!(c.verify());
            }
            _ => panic!("expected CommitmentGossip"),
        }
    }

    #[test]
    fn wire_roundtrip_block_content() {
        let signer_a = test_signer(1);
        let signer_b = test_signer(2);
        let commitments = vec![
            make_commitment(&signer_a, 1),
            make_commitment(&signer_b, 1),
        ];
        let digest = MetaLedgerActor::hash_commitments(&commitments);

        let msg = GossipMessage::BlockContent {
            digest,
            commitments: commitments.clone(),
        };
        let encoded = encode_gossip_message(&msg);
        assert_eq!(encoded.len(), 37 + COMMITMENT_SIZE * 2);
        assert_eq!(encoded[0], TAG_BLOCK_CONTENT);

        let decoded = decode_gossip_message(&encoded).expect("should decode");
        match decoded {
            GossipMessage::BlockContent { digest: d, commitments: cs } => {
                assert_eq!(d, digest);
                assert_eq!(cs.len(), 2);
                assert!(cs[0].verify());
                assert!(cs[1].verify());
            }
            _ => panic!("expected BlockContent"),
        }
    }

    #[test]
    fn wire_reject_empty() {
        assert!(decode_gossip_message(&[]).is_none());
    }

    #[test]
    fn wire_reject_unknown_tag() {
        assert!(decode_gossip_message(&[0xFF, 0x00]).is_none());
    }

    #[test]
    fn wire_reject_truncated_commitment() {
        // Valid tag but too short
        let mut bytes = vec![TAG_COMMITMENT];
        bytes.extend_from_slice(&[0u8; 100]); // 100 < 201
        assert!(decode_gossip_message(&bytes).is_none());
    }

    #[test]
    fn wire_reject_wrong_count() {
        let signer = test_signer(1);
        let commitments = vec![make_commitment(&signer, 1)];
        let digest = MetaLedgerActor::hash_commitments(&commitments);

        let msg = GossipMessage::BlockContent {
            digest,
            commitments,
        };
        let mut encoded = encode_gossip_message(&msg);

        // Corrupt the count field (bytes 33..37) to claim 2 commitments instead of 1
        encoded[33] = 2;
        encoded[34] = 0;
        encoded[35] = 0;
        encoded[36] = 0;

        assert!(
            decode_gossip_message(&encoded).is_none(),
            "wrong count must cause decode to fail (length mismatch)"
        );
    }

    // --- Processing tests ---

    #[tokio::test]
    async fn process_rejects_bad_signature() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let signer = test_signer(1);
        let mut commitment = make_commitment(&signer, 1);
        commitment.new_state_root = [0xff; 32]; // tamper → bad sig

        let peer = test_signer(2).id().0;
        let accepted =
            process_commitment_gossip(&lifecycle, &mempool, &peer, commitment).await;
        assert!(!accepted, "bad signature should be rejected");
    }

    #[tokio::test]
    async fn process_rejects_non_validator() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let unknown = test_signer(99); // not in validator set
        let commitment = make_commitment(&unknown, 1);

        let peer = test_signer(2).id().0;
        let accepted =
            process_commitment_gossip(&lifecycle, &mempool, &peer, commitment).await;
        assert!(!accepted, "non-validator should be rejected");
    }

    #[tokio::test]
    async fn process_accepts_valid_commitment() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let signer = test_signer(1);
        let commitment = make_commitment(&signer, 1);

        let peer = test_signer(2).id().0;
        let accepted =
            process_commitment_gossip(&lifecycle, &mempool, &peer, commitment).await;
        assert!(accepted, "valid commitment should be accepted");
        assert_eq!(mempool.len().await, 1);
    }

    #[tokio::test]
    async fn process_rejects_wrong_digest() {
        let content: ContentStore =
            Arc::new(RwLock::new(BoundedContentStore::new(64)));

        let signer = test_signer(1);
        let commitments = vec![make_commitment(&signer, 1)];

        // Use a fake digest that doesn't match the commitments
        use commonware_cryptography::{Hasher, Sha256};
        let fake_digest = Sha256::hash(b"wrong-digest");

        let accepted =
            process_block_content(&content, fake_digest, commitments).await;
        assert!(!accepted, "wrong digest should be rejected");
    }

    #[tokio::test]
    async fn process_accepts_valid_content() {
        let content: ContentStore =
            Arc::new(RwLock::new(BoundedContentStore::new(64)));

        let signer_a = test_signer(1);
        let signer_b = test_signer(2);
        let commitments = vec![
            make_commitment(&signer_a, 1),
            make_commitment(&signer_b, 1),
        ];
        let digest = MetaLedgerActor::hash_commitments(&commitments);

        let accepted =
            process_block_content(&content, digest, commitments).await;
        assert!(accepted, "valid content should be accepted");
        assert!(content.read().await.get(&digest).is_some());
    }
}
