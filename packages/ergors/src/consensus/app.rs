//! Concrete implementation of the ConsensusLifecycle trait.
//!
//! Backs the ABCI lifecycle with Cnidarium storage, validator set management,
//! and optional CosmWasm contract hooks.
//!
//! Mirrors the Penumbra Consensus struct pattern:
//! - Uses cnidarium::Storage for persistent state
//! - Processes proposals against isolated snapshots
//! - Commits via StateDelta for finalization

use super::{
    lifecycle::ConsensusLifecycle,
    mempool::Mempool,
    types::{EndBlockResponse, Event, NodeCommitment},
};
use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite, Storage};
use commonware_cryptography::ed25519;
use std::{collections::BTreeMap, sync::Arc};
use tracing::{debug, info, warn};

// --- Storage key prefixes for meta-ledger data ---

const META_COMMITMENTS_PREFIX: &str = "meta_ledger/commitments";
const META_SEQUENCES_PREFIX: &str = "meta_ledger/sequences";
const META_HEIGHT_KEY: &str = "meta_ledger/latest_height";
const CONSENSUS_HOOKS_PREFIX: &str = "consensus_hooks";

/// Concrete ABCI lifecycle implementation for Ergors.
///
/// Manages the meta-ledger: validates commitments, tracks per-node sequences,
/// and commits finalized blocks to Cnidarium storage.
///
/// # Commit failure
///
/// If `commit()` fails, the app is **poisoned** and refuses further operations.
/// Cnidarium's `Storage::commit()` consumes the `StateDelta` — on failure,
/// the delta is gone and the app is in an inconsistent state. The only safe
/// recovery is restart with `load_sequences()`.
pub struct ErgorsConsensusApp {
    /// Cnidarium storage backend
    storage: Storage,
    /// Shared mempool for draining proposals
    mempool: Arc<Mempool>,
    /// Per-node sequence tracking (node_pubkey_bytes → latest committed sequence).
    /// Only updated on successful commit — never mid-pipeline.
    sequences: BTreeMap<[u8; 32], u64>,
    /// Sequence updates pending commit. Accumulated during deliver_tx,
    /// merged into `sequences` on commit success, discarded on begin_block
    /// (start of a new pipeline discards any leftovers from a failed one).
    pending_sequences: BTreeMap<[u8; 32], u64>,
    /// Active validator set (pubkey_bytes → voting power)
    validators: BTreeMap<[u8; 32], u64>,
    /// Current block's state delta (created in begin_block, consumed in commit)
    current_delta: Option<cnidarium::StateDelta<cnidarium::Snapshot>>,
    /// Set to true if commit() fails. All subsequent operations will error.
    poisoned: bool,
}

impl ErgorsConsensusApp {
    /// Create a new consensus app.
    ///
    /// `validators` is the initial validator set: Vec of (pubkey, power).
    pub fn new(
        storage: Storage,
        validators: Vec<(ed25519::PublicKey, u64)>,
        mempool: Arc<Mempool>,
    ) -> Self {
        let validators = validators
            .into_iter()
            .map(|(pk, power)| (NodeCommitment::pubkey_bytes(&pk), power))
            .collect();

        Self {
            storage,
            mempool,
            sequences: BTreeMap::new(),
            pending_sequences: BTreeMap::new(),
            validators,
            current_delta: None,
            poisoned: false,
        }
    }

    /// Bail if the app is poisoned from a failed commit.
    fn check_poisoned(&self) -> Result<()> {
        if self.poisoned {
            anyhow::bail!("app is poisoned from a failed commit — restart required");
        }
        Ok(())
    }

    /// Check if a node is in the validator set.
    fn is_validator(&self, node_id_bytes: &[u8; 32]) -> bool {
        self.validators.contains_key(node_id_bytes)
    }

    /// Get the latest committed sequence for a node.
    pub fn latest_sequence(&self, node_id_bytes: &[u8; 32]) -> Option<u64> {
        self.sequences.get(node_id_bytes).copied()
    }

    /// Load committed sequences from storage on startup.
    ///
    /// Rebuilds the in-memory sequence map from persisted state.
    /// Logs warnings for any malformed storage entries.
    pub async fn load_sequences(&mut self) -> Result<()> {
        let snapshot = self.storage.latest_snapshot();
        let prefix = format!("{}/", META_SEQUENCES_PREFIX);

        use futures::StreamExt;
        let mut stream = snapshot.prefix_raw(&prefix);
        let mut malformed = 0usize;
        while let Some(Ok((key, value))) = stream.next().await {
            // Key format: "meta_ledger/sequences/{hex_pubkey}"
            let Some(hex_pk) = key.strip_prefix(&prefix) else {
                warn!(key, "malformed sequence key: missing prefix");
                malformed += 1;
                continue;
            };
            let Ok(pk_bytes) = hex::decode(hex_pk) else {
                warn!(hex_pk, "malformed sequence key: invalid hex");
                malformed += 1;
                continue;
            };
            if pk_bytes.len() != 32 {
                warn!(hex_pk, len = pk_bytes.len(), "malformed sequence key: wrong pubkey length");
                malformed += 1;
                continue;
            }
            if value.len() != 8 {
                warn!(hex_pk, len = value.len(), "malformed sequence value: expected 8 bytes");
                malformed += 1;
                continue;
            }

            let mut arr = [0u8; 32];
            arr.copy_from_slice(&pk_bytes);
            let seq = u64::from_le_bytes(value[..8].try_into().unwrap());
            self.sequences.insert(arr, seq);
        }

        if malformed > 0 {
            warn!(malformed, "skipped malformed sequence entries during load");
        }
        info!(
            sequences_loaded = self.sequences.len(),
            "loaded committed sequences from storage"
        );
        Ok(())
    }
}

#[async_trait]
impl ConsensusLifecycle for ErgorsConsensusApp {
    async fn check_tx(&self, commitment: &NodeCommitment) -> Result<()> {
        self.check_poisoned()?;

        // 1. Signature verification
        if !commitment.verify() {
            anyhow::bail!("invalid commitment signature");
        }

        // 2. Node must be in validator set
        let pk_bytes = commitment.mempool_key().0;
        if !self.is_validator(&pk_bytes) {
            anyhow::bail!("node not in validator set");
        }

        // 3. Strict sequence: must be exactly last_known + 1 (or 1 if first).
        // Check pending_sequences first (for in-flight blocks), then committed sequences.
        // No gaps allowed — every state transition must be committed to the meta-ledger.
        let last_known = self
            .pending_sequences
            .get(&pk_bytes)
            .or_else(|| self.sequences.get(&pk_bytes))
            .copied();
        let expected = last_known.map_or(1, |s| s + 1);
        if commitment.sequence != expected {
            anyhow::bail!(
                "sequence mismatch: got {}, expected {}",
                commitment.sequence,
                expected
            );
        }

        Ok(())
    }

    async fn prepare_proposal(
        &self,
        height: u64,
        _max_bytes: usize,
        candidates: &[NodeCommitment],
    ) -> Result<Vec<NodeCommitment>> {
        // Filter: only include commitments that still pass check_tx
        // (state may have changed since they entered the mempool)
        let mut valid = Vec::with_capacity(candidates.len());
        let mut rejected = 0usize;

        for c in candidates {
            if self.check_tx(c).await.is_ok() {
                valid.push(c.clone());
            } else {
                rejected += 1;
            }
        }

        if rejected > 0 {
            debug!(height, rejected, "filtered stale commitments from proposal");
        }

        info!(height, commitments = valid.len(), "prepared proposal");
        Ok(valid)
    }

    async fn process_proposal(
        &self,
        height: u64,
        commitments: &[NodeCommitment],
    ) -> Result<bool> {
        // Verify every commitment in the proposal
        for (i, c) in commitments.iter().enumerate() {
            if let Err(e) = self.check_tx(c).await {
                warn!(
                    height,
                    index = i,
                    node = %hex::encode(c.mempool_key().0),
                    seq = c.sequence,
                    ?e,
                    "invalid commitment in proposal"
                );
                return Ok(false);
            }
        }

        // Verify canonical ordering: sorted by (node_id_bytes, sequence)
        for window in commitments.windows(2) {
            if window[0].mempool_key() >= window[1].mempool_key() {
                warn!(height, "proposal commitments not in canonical order");
                return Ok(false);
            }
        }

        // Verify no duplicate sequences per node within the proposal
        let mut seen: BTreeMap<[u8; 32], u64> = BTreeMap::new();
        for c in commitments {
            let pk = c.mempool_key().0;
            if let Some(&prev_seq) = seen.get(&pk) {
                if c.sequence <= prev_seq {
                    warn!(
                        height,
                        node = %hex::encode(pk),
                        "duplicate or non-monotonic sequence in proposal"
                    );
                    return Ok(false);
                }
            }
            seen.insert(pk, c.sequence);
        }

        Ok(true)
    }

    async fn begin_block(&mut self, height: u64, timestamp: u64) -> Result<Vec<Event>> {
        self.check_poisoned()?;

        // Discard any pending sequences from a previous failed pipeline.
        // A new begin_block means we're starting fresh.
        self.pending_sequences.clear();

        // Create fresh StateDelta for this block
        let snapshot = self.storage.latest_snapshot();
        self.current_delta = Some(cnidarium::StateDelta::new(snapshot));

        let events = vec![Event::new("begin_block")
            .attr("height", height.to_string())
            .attr("timestamp", timestamp.to_string())];

        debug!(height, timestamp, "begin_block");
        Ok(events)
    }

    async fn deliver_tx(&mut self, commitment: &NodeCommitment) -> Result<Vec<Event>> {
        self.check_poisoned()?;

        let delta = self
            .current_delta
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("deliver_tx called before begin_block"))?;

        let pk_hex = hex::encode(commitment.mempool_key().0);

        // 1. Store the commitment in the meta-ledger
        let commitment_key = format!(
            "{}/{}/{}",
            META_COMMITMENTS_PREFIX, pk_hex, commitment.sequence,
        );
        delta.put_raw(commitment_key, commitment.to_bytes());

        // 2. Update the sequence tracker
        let seq_key = format!("{}/{}", META_SEQUENCES_PREFIX, pk_hex);
        delta.put_raw(seq_key, commitment.sequence.to_le_bytes().to_vec());

        // 3. Buffer sequence update — NOT applied to self.sequences yet.
        // pending_sequences is merged into sequences on commit success,
        // discarded on the next begin_block (if this pipeline fails).
        // This prevents in-memory state divergence if finalization bails mid-pipeline.
        self.pending_sequences
            .insert(commitment.mempool_key().0, commitment.sequence);

        let events = vec![Event::new("deliver_commitment")
            .attr("node", &pk_hex)
            .attr("sequence", commitment.sequence.to_string())
            .attr("kind", commitment.kind.to_string())
            .attr_no_index("transition_digest", hex::encode(commitment.transition_digest))];

        debug!(
            node = %pk_hex,
            seq = commitment.sequence,
            kind = %commitment.kind,
            "deliver_tx"
        );

        Ok(events)
    }

    async fn end_block(&mut self, height: u64) -> Result<EndBlockResponse> {
        self.check_poisoned()?;

        let delta = self
            .current_delta
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("end_block called before begin_block"))?;

        // Record the block height
        delta.put_raw(
            META_HEIGHT_KEY.to_string(),
            height.to_le_bytes().to_vec(),
        );

        let events = vec![Event::new("end_block").attr("height", height.to_string())];

        debug!(height, "end_block");

        Ok(EndBlockResponse {
            events,
            validator_updates: None, // No dynamic validator changes yet
        })
    }

    async fn commit(&mut self) -> Result<[u8; 32]> {
        self.check_poisoned()?;

        let delta = self
            .current_delta
            .take()
            .ok_or_else(|| anyhow::anyhow!("commit called before begin_block"))?;

        // Commit the StateDelta to Cnidarium — produces new Merkle root.
        // Storage::commit() takes ownership of the delta. If this fails,
        // the delta is gone and we're in an inconsistent state.
        match self.storage.commit(delta).await {
            Ok(root_hash) => {
                // Commit succeeded — now merge pending sequences into committed state.
                // This is the ONLY place sequences advance.
                for (pk, seq) in std::mem::take(&mut self.pending_sequences) {
                    self.sequences.insert(pk, seq);
                }
                info!(
                    app_hash = %hex::encode(root_hash.0),
                    "block committed"
                );
                Ok(root_hash.0)
            }
            Err(e) => {
                // FATAL: delta is consumed, pending state is inconsistent.
                // Poison the app — only safe recovery is restart with load_sequences().
                self.poisoned = true;
                self.pending_sequences.clear();
                tracing::error!(?e, "FATAL: commit failed, app is now poisoned");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::types::CommitmentKind;
    use ho_std::keys::commonware::NodePrivKey;
    use tempfile::TempDir;

    async fn test_storage() -> (Storage, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::load(dir.path().to_path_buf(), vec![])
            .await
            .unwrap();
        (storage, dir)
    }

    fn test_validators() -> Vec<(ed25519::PublicKey, u64)> {
        vec![
            (NodePrivKey::from_seed(1).id().0, 1),
            (NodePrivKey::from_seed(2).id().0, 1),
            (NodePrivKey::from_seed(3).id().0, 1),
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

    #[tokio::test]
    async fn check_tx_accepts_valid() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let signer = NodePrivKey::from_seed(1);
        let commitment = make_commitment(&signer, 1);

        assert!(app.check_tx(&commitment).await.is_ok());
    }

    #[tokio::test]
    async fn check_tx_rejects_bad_signature() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let signer = NodePrivKey::from_seed(1);
        let mut commitment = make_commitment(&signer, 1);
        commitment.new_state_root = [0xff; 32]; // tamper

        assert!(app.check_tx(&commitment).await.is_err());
    }

    #[tokio::test]
    async fn check_tx_rejects_unknown_node() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let unknown = NodePrivKey::from_seed(99); // not in validator set
        let commitment = make_commitment(&unknown, 1);

        let err = app.check_tx(&commitment).await.unwrap_err();
        assert!(err.to_string().contains("not in validator set"));
    }

    #[tokio::test]
    async fn check_tx_rejects_wrong_sequence() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let mut app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let signer = NodePrivKey::from_seed(1);
        let key = NodeCommitment::pubkey_bytes(&signer.id().0);
        app.sequences.insert(key, 5); // pretend we committed seq 5

        // Same sequence — reject
        let commitment = make_commitment(&signer, 5);
        let err = app.check_tx(&commitment).await.unwrap_err();
        assert!(err.to_string().contains("sequence mismatch"));

        // Older sequence — reject
        let commitment = make_commitment(&signer, 3);
        assert!(app.check_tx(&commitment).await.is_err());

        // Gap (skipped 6) — reject
        let commitment = make_commitment(&signer, 7);
        let err = app.check_tx(&commitment).await.unwrap_err();
        assert!(err.to_string().contains("sequence mismatch"));

        // Exactly next (6) — accept
        let commitment = make_commitment(&signer, 6);
        assert!(app.check_tx(&commitment).await.is_ok());
    }

    #[tokio::test]
    async fn full_block_lifecycle() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let mut app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);

        // Build a proposal with 2 commitments
        let commitments = vec![
            make_commitment(&signer_a, 1),
            make_commitment(&signer_b, 1),
        ];

        // Sort by mempool_key for canonical ordering
        let mut sorted = commitments.clone();
        sorted.sort_by_key(|c| c.mempool_key());

        // Process proposal — should accept
        assert!(app.process_proposal(1, &sorted).await.unwrap());

        // Run the full lifecycle
        let begin_events = app.begin_block(1, 1000).await.unwrap();
        assert!(!begin_events.is_empty());

        for c in &sorted {
            let events = app.deliver_tx(c).await.unwrap();
            assert!(!events.is_empty());
        }

        let end_response = app.end_block(1).await.unwrap();
        assert!(!end_response.events.is_empty());

        let app_hash = app.commit().await.unwrap();
        assert_ne!(app_hash, [0u8; 32], "app hash should be non-zero after commit");

        // Sequences should be updated
        assert_eq!(
            app.latest_sequence(&sorted[0].mempool_key().0),
            Some(1)
        );
        assert_eq!(
            app.latest_sequence(&sorted[1].mempool_key().0),
            Some(1)
        );
    }

    #[tokio::test]
    async fn process_proposal_rejects_unordered() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool);

        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);

        let c_a = make_commitment(&signer_a, 1);
        let c_b = make_commitment(&signer_b, 1);

        // Deliberately wrong order (if a > b in BTreeMap ordering)
        let mut commitments = vec![c_a, c_b];
        // Reverse the sorted order
        commitments.sort_by_key(|c| c.mempool_key());
        commitments.reverse();

        if commitments[0].mempool_key() >= commitments[1].mempool_key() {
            assert!(!app.process_proposal(1, &commitments).await.unwrap());
        }
    }
}
