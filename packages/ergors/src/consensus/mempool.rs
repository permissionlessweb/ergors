//! Thread-safe mempool for pending state transition commitments.
//!
//! Commitments arrive from local state changes and remote gossip.
//! The mempool orders them deterministically by (node_pubkey, sequence)
//! for canonical block proposal ordering.

use super::types::NodeCommitment;
use std::collections::BTreeMap;
use tokio::sync::RwLock;

/// Thread-safe mempool for pending commitments.
///
/// Uses a BTreeMap keyed by (pubkey_bytes, sequence) for:
/// - Deterministic iteration order (required for canonical block proposals)
/// - O(log n) insertion and lookup
/// - Natural dedup (same key = same commitment)
pub struct Mempool {
    pending: RwLock<BTreeMap<([u8; 32], u64), NodeCommitment>>,
    max_per_block: usize,
    max_pending: usize,
}

impl Mempool {
    pub fn new(max_per_block: usize, max_pending: usize) -> Self {
        Self {
            pending: RwLock::new(BTreeMap::new()),
            max_per_block,
            max_pending,
        }
    }

    /// Add a commitment after validation (check_tx should be called before this).
    ///
    /// Returns false if the mempool is full or the commitment is a duplicate.
    pub async fn add(&self, commitment: NodeCommitment) -> bool {
        let mut pending = self.pending.write().await;
        if pending.len() >= self.max_pending {
            return false;
        }
        let key = commitment.mempool_key();
        // insert returns None if the key was new (success)
        pending.insert(key, commitment).is_none()
    }

    /// Drain up to max_per_block commitments for a block proposal.
    ///
    /// Returns commitments in deterministic order (by node_id bytes, then sequence).
    /// This is the canonical ordering that all validators must agree on.
    pub async fn drain_for_proposal(&self) -> Vec<NodeCommitment> {
        let mut pending = self.pending.write().await;
        let count = self.max_per_block.min(pending.len());
        let mut batch = Vec::with_capacity(count);

        // BTreeMap iteration is sorted by key — deterministic order guaranteed
        let keys: Vec<([u8; 32], u64)> = pending.keys().take(count).copied().collect();
        for key in keys {
            if let Some(commitment) = pending.remove(&key) {
                batch.push(commitment);
            }
        }

        batch
    }

    /// Re-add commitments that failed to finalize (e.g., block was rejected).
    ///
    /// Returns any commitments that were dropped because the mempool is at capacity.
    /// Callers must handle dropped commitments (log, re-gossip, etc.).
    pub async fn requeue(&self, commitments: Vec<NodeCommitment>) -> Vec<NodeCommitment> {
        let mut pending = self.pending.write().await;
        let mut dropped = Vec::new();
        for c in commitments {
            if pending.len() >= self.max_pending {
                dropped.push(c);
            } else {
                let key = c.mempool_key();
                pending.insert(key, c);
            }
        }
        if !dropped.is_empty() {
            tracing::warn!(
                dropped = dropped.len(),
                capacity = self.max_pending,
                "requeue dropped commitments — mempool at capacity"
            );
        }
        dropped
    }

    /// Remove commitments that are now stale (sequence <= last committed).
    ///
    /// Call this after a block is finalized to evict any commitments
    /// with sequences that have been superseded.
    pub async fn evict_stale(&self, node_id_bytes: &[u8; 32], up_to_sequence: u64) {
        let mut pending = self.pending.write().await;
        // Remove all entries for this node with sequence <= up_to_sequence
        pending.retain(|&(ref pk, seq), _| !(pk == node_id_bytes && seq <= up_to_sequence));
    }

    /// Number of pending commitments.
    pub async fn len(&self) -> usize {
        self.pending.read().await.len()
    }

    /// Whether the mempool is empty.
    pub async fn is_empty(&self) -> bool {
        self.pending.read().await.is_empty()
    }

    /// Whether the mempool is at capacity.
    pub async fn is_full(&self) -> bool {
        self.pending.read().await.len() >= self.max_pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::keys::commonware::NodePrivKey;

    use crate::consensus::types::CommitmentKind;

    fn make_commitment(signer: &NodePrivKey, seq: u64) -> NodeCommitment {
        NodeCommitment::new(
            signer,
            [0u8; 32],
            [(seq & 0xff) as u8; 32],
            format!("tx-{seq}").as_bytes(),
            CommitmentKind::Generic,
            seq,
        )
    }

    #[tokio::test]
    async fn add_and_drain() {
        let pool = Mempool::new(10, 100);
        let signer = NodePrivKey::from_seed(1);

        assert!(pool.add(make_commitment(&signer, 1)).await);
        assert!(pool.add(make_commitment(&signer, 2)).await);
        assert!(pool.add(make_commitment(&signer, 3)).await);
        assert_eq!(pool.len().await, 3);

        let batch = pool.drain_for_proposal().await;
        assert_eq!(batch.len(), 3);
        assert!(pool.is_empty().await);

        // Verify ordering: sequence 1, 2, 3
        assert_eq!(batch[0].sequence, 1);
        assert_eq!(batch[1].sequence, 2);
        assert_eq!(batch[2].sequence, 3);
    }

    #[tokio::test]
    async fn rejects_duplicate() {
        let pool = Mempool::new(10, 100);
        let signer = NodePrivKey::from_seed(1);

        assert!(pool.add(make_commitment(&signer, 1)).await);
        assert!(!pool.add(make_commitment(&signer, 1)).await); // duplicate
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test]
    async fn respects_max_pending() {
        let pool = Mempool::new(10, 2); // max 2 pending
        let signer = NodePrivKey::from_seed(1);

        assert!(pool.add(make_commitment(&signer, 1)).await);
        assert!(pool.add(make_commitment(&signer, 2)).await);
        assert!(!pool.add(make_commitment(&signer, 3)).await); // full
        assert!(pool.is_full().await);
    }

    #[tokio::test]
    async fn respects_max_per_block() {
        let pool = Mempool::new(2, 100); // max 2 per block
        let signer = NodePrivKey::from_seed(1);

        pool.add(make_commitment(&signer, 1)).await;
        pool.add(make_commitment(&signer, 2)).await;
        pool.add(make_commitment(&signer, 3)).await;

        let batch = pool.drain_for_proposal().await;
        assert_eq!(batch.len(), 2); // only 2 drained
        assert_eq!(pool.len().await, 1); // 1 remaining
    }

    #[tokio::test]
    async fn deterministic_ordering_across_nodes() {
        let pool = Mempool::new(10, 100);
        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);

        // Add in non-sorted order
        pool.add(make_commitment(&signer_b, 2)).await;
        pool.add(make_commitment(&signer_a, 1)).await;
        pool.add(make_commitment(&signer_b, 1)).await;
        pool.add(make_commitment(&signer_a, 2)).await;

        let batch = pool.drain_for_proposal().await;
        assert_eq!(batch.len(), 4);

        // Must be sorted by (pubkey_bytes, sequence) regardless of insertion order
        for window in batch.windows(2) {
            assert!(window[0].mempool_key() < window[1].mempool_key());
        }
    }

    #[tokio::test]
    async fn requeue_after_failed_proposal() {
        let pool = Mempool::new(10, 100);
        let signer = NodePrivKey::from_seed(1);

        pool.add(make_commitment(&signer, 1)).await;
        pool.add(make_commitment(&signer, 2)).await;

        let batch = pool.drain_for_proposal().await;
        assert!(pool.is_empty().await);

        // Proposal failed — requeue
        let dropped = pool.requeue(batch).await;
        assert!(dropped.is_empty(), "nothing should be dropped with capacity");
        assert_eq!(pool.len().await, 2);
    }

    #[tokio::test]
    async fn requeue_returns_dropped_at_capacity() {
        let pool = Mempool::new(10, 2); // max 2 pending
        let signer = NodePrivKey::from_seed(1);

        pool.add(make_commitment(&signer, 1)).await;
        pool.add(make_commitment(&signer, 2)).await;
        assert!(pool.is_full().await);

        // Try to requeue 2 more — should all be dropped
        let extras = vec![
            make_commitment(&signer, 3),
            make_commitment(&signer, 4),
        ];
        let dropped = pool.requeue(extras).await;
        assert_eq!(dropped.len(), 2, "both should be dropped at capacity");
    }

    #[tokio::test]
    async fn concurrent_add_and_drain() {
        use std::sync::Arc;

        let pool = Arc::new(Mempool::new(100, 5000));

        // 10 writers, each adding 100 commitments from different signers
        let writers: Vec<_> = (1..=10u64)
            .map(|i| {
                let pool = pool.clone();
                let signer = NodePrivKey::from_seed(i);
                tokio::spawn(async move {
                    let mut added = 0u64;
                    for seq in 1..=100u64 {
                        if pool.add(make_commitment(&signer, seq)).await {
                            added += 1;
                        }
                    }
                    added
                })
            })
            .collect();

        // Concurrent reader draining proposals
        let reader = {
            let pool = pool.clone();
            tokio::spawn(async move {
                let mut total_drained = 0usize;
                for _ in 0..50 {
                    let batch = pool.drain_for_proposal().await;
                    total_drained += batch.len();
                    tokio::task::yield_now().await;
                }
                total_drained
            })
        };

        // Wait for all writers
        let mut total_added = 0u64;
        for w in writers {
            total_added += w.await.unwrap();
        }

        // Wait for reader
        let drained_during = reader.await.unwrap();

        // Drain remaining
        let remaining = pool.drain_for_proposal().await;

        // No commitments lost: everything either drained during or remaining after
        assert_eq!(
            (drained_during + remaining.len()) as u64,
            total_added,
            "no commitments should be lost under concurrent access"
        );
    }

    #[tokio::test]
    async fn evict_stale() {
        let pool = Mempool::new(10, 100);
        let signer = NodePrivKey::from_seed(1);

        pool.add(make_commitment(&signer, 1)).await;
        pool.add(make_commitment(&signer, 2)).await;
        pool.add(make_commitment(&signer, 3)).await;

        let key = make_commitment(&signer, 1).mempool_key();
        pool.evict_stale(&key.0, 2).await; // evict seq <= 2

        assert_eq!(pool.len().await, 1);
        let batch = pool.drain_for_proposal().await;
        assert_eq!(batch[0].sequence, 3);
    }
}
