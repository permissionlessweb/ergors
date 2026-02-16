//! Write coalescer and async commitment pipeline.
//!
//! Replaces the `write_lock` mutex in ErgorsStorage with a background actor
//! that batches pending writes into single cnidarium commits. After each batch
//! commits locally, an async `CommitmentGenerator` creates a signed
//! `NodeCommitment` and submits it to the consensus mempool — without blocking
//! the original caller.
//!
//! # Flow
//!
//! ```text
//! HTTP request → inference → WriteAction → coalescer batches → commit (~50ms)
//!   → caller unblocked
//!   → async: NodeCommitment → mempool → Simplex → gossip
//! ```

use super::{
    mempool::Mempool,
    types::{CommitmentKind, NodeCommitment},
};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::llm::{HoError, HoResult};
use sha2::{Digest as _, Sha256};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// WriteAction
// ---------------------------------------------------------------------------

/// A storage write action. Pre-computed puts/deletes from ErgorsStorage methods.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WriteAction {
    pub kind: CommitmentKind,
    pub puts: Vec<(String, Vec<u8>)>,
    pub deletes: Vec<String>,
}

impl WriteAction {
    /// Apply this action's puts and deletes to a StateDelta.
    pub fn apply(&self, delta: &mut cnidarium::StateDelta<cnidarium::Snapshot>) {
        use cnidarium::StateWrite;
        for (k, v) in &self.puts {
            delta.put_raw(k.clone(), v.clone());
        }
        for k in &self.deletes {
            delta.delete(k.clone());
        }
    }

    /// SHA-256 digest of the serialized action (for commitment chain).
    pub fn digest(&self) -> [u8; 32] {
        let bytes = serde_json::to_vec(self).expect("WriteAction serializable");
        Sha256::digest(&bytes).into()
    }
}

// Serde support for CommitmentKind (used inside WriteAction)
impl serde::Serialize for CommitmentKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> serde::Deserialize<'de> for CommitmentKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        CommitmentKind::from_u8(v).ok_or_else(|| serde::de::Error::custom("invalid CommitmentKind"))
    }
}

// ---------------------------------------------------------------------------
// CommittedBatch
// ---------------------------------------------------------------------------

/// A batch of actions that was successfully committed locally.
/// Sent to the `CommitmentGenerator` for async consensus submission.
pub struct CommittedBatch {
    pub prev_root: [u8; 32],
    pub new_root: [u8; 32],
    pub actions: Vec<WriteAction>,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// WriteHandle (thin cloneable sender)
// ---------------------------------------------------------------------------

/// Thin cloneable handle for submitting writes to the [`WriteCoalescer`].
#[derive(Clone)]
pub struct WriteHandle {
    tx: mpsc::UnboundedSender<(WriteAction, oneshot::Sender<HoResult<()>>)>,
}

impl WriteHandle {
    /// Submit a write action and wait for the batch commit to complete.
    pub async fn submit(&self, action: WriteAction) -> HoResult<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send((action, tx))
            .map_err(|_| HoError::Storage("write coalescer shut down".into()))?;
        rx.await
            .map_err(|_| HoError::Storage("write coalescer dropped response".into()))?
    }
}

// ---------------------------------------------------------------------------
// WriteCoalescer (background actor)
// ---------------------------------------------------------------------------

/// Background actor that batches [`WriteAction`]s into single cnidarium commits.
///
/// Eliminates version conflicts by ensuring one `StateDelta` per commit batch.
/// After each successful batch, optionally fires a [`CommittedBatch`] for async
/// consensus commitment generation.
pub struct WriteCoalescer {
    storage: cnidarium::Storage,
    rx: mpsc::UnboundedReceiver<(WriteAction, oneshot::Sender<HoResult<()>>)>,
    commitment_tx: Option<mpsc::UnboundedSender<CommittedBatch>>,
}

impl WriteCoalescer {
    /// Create a new coalescer and its associated [`WriteHandle`].
    ///
    /// If `commitment_tx` is provided, committed batches are forwarded for
    /// async `NodeCommitment` generation.
    pub fn new(
        storage: cnidarium::Storage,
        commitment_tx: Option<mpsc::UnboundedSender<CommittedBatch>>,
    ) -> (Self, WriteHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = WriteHandle { tx };
        let coalescer = Self {
            storage,
            rx,
            commitment_tx,
        };
        (coalescer, handle)
    }

    /// Run the coalescer loop. Exits when all [`WriteHandle`]s are dropped.
    pub async fn run(mut self) {
        info!("write coalescer started");

        loop {
            // 1. Wait for the first action (blocking recv)
            let first = match self.rx.recv().await {
                Some(item) => item,
                None => {
                    info!("write coalescer shutting down (all handles dropped)");
                    return;
                }
            };

            // 2. Drain any additional pending actions (non-blocking)
            let mut batch = vec![first];
            while let Ok(item) = self.rx.try_recv() {
                batch.push(item);
            }

            let batch_size = batch.len();
            debug!(batch_size, "coalescer processing batch");

            // 3. Get prev_root from latest snapshot
            let prev_snapshot = self.storage.latest_snapshot();
            let prev_root = prev_snapshot.root_hash().await.expect("root_hash").0;

            // 4. Create single StateDelta from latest snapshot
            let mut delta = cnidarium::StateDelta::new(prev_snapshot);

            // 5. Apply all actions in FIFO order (deterministic)
            let mut actions = Vec::with_capacity(batch_size);
            let mut senders = Vec::with_capacity(batch_size);
            for (action, sender) in batch {
                action.apply(&mut delta);
                actions.push(action);
                senders.push(sender);
            }

            // 6. Commit
            match self.storage.commit(delta).await {
                Ok(_) => {
                    // 7. Get new_root
                    let new_root = self.storage.latest_snapshot().root_hash().await.expect("root_hash").0;

                    // 8. Signal all callers
                    for sender in senders {
                        let _ = sender.send(Ok(()));
                    }

                    debug!(batch_size, "coalescer batch committed");

                    // 9. Forward to commitment generator if wired
                    if let Some(ref commitment_tx) = self.commitment_tx {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        let committed = CommittedBatch {
                            prev_root,
                            new_root,
                            actions,
                            timestamp,
                        };

                        if commitment_tx.send(committed).is_err() {
                            warn!("commitment generator channel closed");
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!("coalescer commit failed: {}", e);
                    error!("{}", err_msg);
                    for sender in senders {
                        let _ = sender.send(Err(HoError::Storage(err_msg.clone())));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CommitmentGenerator (async background task)
// ---------------------------------------------------------------------------

/// Background task that creates [`NodeCommitment`]s from committed batches
/// and submits them to the consensus mempool.
pub struct CommitmentGenerator {
    rx: mpsc::UnboundedReceiver<CommittedBatch>,
    mempool: Arc<Mempool>,
    signer: NodePrivKey,
    sequence: AtomicU64,
}

impl CommitmentGenerator {
    pub fn new(
        rx: mpsc::UnboundedReceiver<CommittedBatch>,
        mempool: Arc<Mempool>,
        signer: NodePrivKey,
        initial_sequence: u64,
    ) -> Self {
        Self {
            rx,
            mempool,
            signer,
            sequence: AtomicU64::new(initial_sequence),
        }
    }

    /// Run the generator loop. Exits when the coalescer drops its sender.
    pub async fn run(mut self) {
        info!("commitment generator started");

        while let Some(batch) = self.rx.recv().await {
            // Compute batch digest: SHA-256(concat of action digests)
            let mut hasher = Sha256::new();
            for action in &batch.actions {
                hasher.update(action.digest());
            }
            let batch_digest: [u8; 32] = hasher.finalize().into();

            // Determine kind from batch
            let kind = Self::derive_kind(&batch.actions);
            let seq = self.sequence.fetch_add(1, Ordering::Relaxed);

            let commitment = NodeCommitment::new(
                &self.signer,
                batch.prev_root,
                batch.new_root,
                &batch_digest,
                kind,
                seq,
            );

            if self.mempool.add(commitment).await {
                debug!(seq, ?kind, "commitment submitted to mempool");
            } else {
                warn!(seq, "mempool rejected commitment (full or duplicate)");
            }
        }

        info!("commitment generator shutting down");
    }

    /// Derive the commitment kind from a batch of actions.
    /// If all actions share the same kind, use that; otherwise use Generic.
    fn derive_kind(actions: &[WriteAction]) -> CommitmentKind {
        if actions.is_empty() {
            return CommitmentKind::Generic;
        }
        let first = actions[0].kind;
        if actions.iter().all(|a| a.kind == first) {
            first
        } else {
            CommitmentKind::Generic
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::StateRead;

    /// Helper: create a temporary cnidarium Storage for testing.
    async fn test_storage() -> cnidarium::Storage {
        let dir = tempfile::TempDir::new().unwrap();
        cnidarium::Storage::load(dir.path().to_path_buf(), vec![])
            .await
            .unwrap()
    }

    #[test]
    fn write_action_apply_puts_and_deletes() {
        // We can't easily test StateDelta without async, so test serde roundtrip here
        let action = WriteAction {
            kind: CommitmentKind::Inference,
            puts: vec![
                ("key1".into(), b"value1".to_vec()),
                ("key2".into(), b"value2".to_vec()),
            ],
            deletes: vec!["key3".into()],
        };

        let json = serde_json::to_string(&action).unwrap();
        let decoded: WriteAction = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.puts.len(), 2);
        assert_eq!(decoded.deletes.len(), 1);
        assert_eq!(decoded.kind, CommitmentKind::Inference);
    }

    #[test]
    fn write_action_digest_deterministic() {
        let action = WriteAction {
            kind: CommitmentKind::Inference,
            puts: vec![("k".into(), b"v".to_vec())],
            deletes: vec![],
        };
        let d1 = action.digest();
        let d2 = action.digest();
        assert_eq!(d1, d2);
    }

    #[test]
    fn write_action_digest_changes_with_content() {
        let a1 = WriteAction {
            kind: CommitmentKind::Inference,
            puts: vec![("k".into(), b"v1".to_vec())],
            deletes: vec![],
        };
        let a2 = WriteAction {
            kind: CommitmentKind::Inference,
            puts: vec![("k".into(), b"v2".to_vec())],
            deletes: vec![],
        };
        assert_ne!(a1.digest(), a2.digest());
    }

    #[tokio::test]
    async fn coalescer_batches_and_commits() {
        let storage = test_storage().await;
        let (coalescer, handle) = WriteCoalescer::new(storage.clone(), None);

        tokio::spawn(coalescer.run());

        // Submit multiple writes concurrently
        let h1 = {
            let wh = handle.clone();
            tokio::spawn(async move {
                wh.submit(WriteAction {
                    kind: CommitmentKind::Inference,
                    puts: vec![("test/key1".into(), b"value1".to_vec())],
                    deletes: vec![],
                })
                .await
            })
        };
        let h2 = {
            let wh = handle.clone();
            tokio::spawn(async move {
                wh.submit(WriteAction {
                    kind: CommitmentKind::Inference,
                    puts: vec![("test/key2".into(), b"value2".to_vec())],
                    deletes: vec![],
                })
                .await
            })
        };

        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();

        // Verify both keys are present in storage
        let snapshot = storage.latest_snapshot();
        let v1 = snapshot.get_raw("test/key1").await.unwrap();
        let v2 = snapshot.get_raw("test/key2").await.unwrap();
        assert_eq!(v1.unwrap(), b"value1");
        assert_eq!(v2.unwrap(), b"value2");
    }

    #[tokio::test]
    async fn coalescer_signals_all_callers() {
        let storage = test_storage().await;
        let (coalescer, handle) = WriteCoalescer::new(storage, None);

        tokio::spawn(coalescer.run());

        // Fire 10 concurrent writes and collect results
        let mut handles = Vec::new();
        for i in 0..10u32 {
            let wh = handle.clone();
            handles.push(tokio::spawn(async move {
                wh.submit(WriteAction {
                    kind: CommitmentKind::Generic,
                    puts: vec![(format!("k/{}", i), vec![i as u8])],
                    deletes: vec![],
                })
                .await
            }));
        }

        for h in handles {
            h.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn coalescer_shuts_down_when_handles_dropped() {
        let storage = test_storage().await;
        let (coalescer, handle) = WriteCoalescer::new(storage, None);

        let join = tokio::spawn(coalescer.run());

        // Drop the handle
        drop(handle);

        // Coalescer should exit
        tokio::time::timeout(std::time::Duration::from_secs(2), join)
            .await
            .expect("coalescer should shut down")
            .unwrap();
    }

    #[tokio::test]
    async fn commitment_generator_creates_commitments() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mempool = Arc::new(Mempool::new(100, 1000));
        let signer = NodePrivKey::from_seed(42);

        let generator = CommitmentGenerator::new(rx, mempool.clone(), signer, 1);
        tokio::spawn(generator.run());

        // Send a committed batch
        tx.send(CommittedBatch {
            prev_root: [0u8; 32],
            new_root: [1u8; 32],
            actions: vec![WriteAction {
                kind: CommitmentKind::Inference,
                puts: vec![("k".into(), b"v".to_vec())],
                deletes: vec![],
            }],
            timestamp: 12345,
        })
        .unwrap();

        // Give generator time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(mempool.len().await, 1);
        let batch = mempool.drain_for_proposal().await;
        assert_eq!(batch[0].kind, CommitmentKind::Inference);
        assert_eq!(batch[0].sequence, 1);
    }

    #[test]
    fn derive_kind_homogeneous() {
        let actions = vec![
            WriteAction { kind: CommitmentKind::Inference, puts: vec![], deletes: vec![] },
            WriteAction { kind: CommitmentKind::Inference, puts: vec![], deletes: vec![] },
        ];
        assert_eq!(CommitmentGenerator::derive_kind(&actions), CommitmentKind::Inference);
    }

    #[test]
    fn derive_kind_mixed() {
        let actions = vec![
            WriteAction { kind: CommitmentKind::Inference, puts: vec![], deletes: vec![] },
            WriteAction { kind: CommitmentKind::Orchestration, puts: vec![], deletes: vec![] },
        ];
        assert_eq!(CommitmentGenerator::derive_kind(&actions), CommitmentKind::Generic);
    }

    #[tokio::test]
    async fn full_pipeline_concurrent_writes() {
        let storage = test_storage().await;
        let (batch_tx, batch_rx) = mpsc::unbounded_channel();
        let mempool = Arc::new(Mempool::new(100, 1000));
        let signer = NodePrivKey::from_seed(99);

        let (coalescer, handle) = WriteCoalescer::new(storage.clone(), Some(batch_tx));
        let generator = CommitmentGenerator::new(batch_rx, mempool.clone(), signer, 0);

        tokio::spawn(coalescer.run());
        tokio::spawn(generator.run());

        // 10 concurrent writes
        let mut joins = Vec::new();
        for i in 0..10u32 {
            let wh = handle.clone();
            joins.push(tokio::spawn(async move {
                wh.submit(WriteAction {
                    kind: CommitmentKind::Inference,
                    puts: vec![(format!("pipeline/{}", i), format!("data-{}", i).into_bytes())],
                    deletes: vec![],
                })
                .await
            }));
        }

        for j in joins {
            j.await.unwrap().unwrap();
        }

        // Verify all keys present
        let snapshot = storage.latest_snapshot();
        for i in 0..10u32 {
            let val = snapshot
                .get_raw(&format!("pipeline/{}", i))
                .await
                .unwrap()
                .expect("key should exist");
            assert_eq!(val, format!("data-{}", i).into_bytes());
        }

        // Give commitment generator time to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // At least one commitment should have been created
        assert!(mempool.len().await > 0);
    }
}
