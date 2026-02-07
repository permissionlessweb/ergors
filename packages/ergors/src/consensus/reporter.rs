//! Finalization reporter that drives the ABCI execution pipeline.
//!
//! When Simplex finalizes a block (via the [`Reporter`] trait), this drives:
//!   BeginBlock → DeliverTx (×N) → EndBlock → Commit
//!
//! Other consensus activities (notarizations, nullifications, fault evidence)
//! are logged but do not trigger state changes.
//!
//! # Atomicity
//!
//! Height and content eviction are ONLY updated after a successful commit.
//! If any step of the pipeline fails (begin_block, deliver_tx, end_block, commit),
//! the state remains at the previous height and content is NOT evicted.
//! The app's `begin_block` will discard any pending state from the failed pipeline.

use super::{
    bridge::{BlockDigest, ContentStore, SharedHeight},
    lifecycle::ConsensusLifecycle,
    types::trace_events,
};
use commonware_consensus::simplex::{
    signing_scheme::ed25519::Scheme as Ed25519Scheme,
    types::Activity,
};
use commonware_consensus::Reporter;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Ed25519 signing scheme type alias for Simplex.
pub type SimplexScheme = Ed25519Scheme;

/// The Activity type produced by a Simplex engine using Ed25519 + SHA-256.
pub type SimplexActivity = Activity<SimplexScheme, BlockDigest>;

/// Reporter that executes the ABCI pipeline on block finalization.
///
/// Simplex calls [`report()`](Reporter::report) for every consensus activity.
/// We only act on [`Activity::Finalization`] — that's when a block is
/// irreversibly committed by 2/3+ of validators.
///
/// # Lifecycle on Finalization
///
/// 1. Look up block content by digest from the shared [`ContentStore`]
/// 2. `begin_block(height, timestamp)`
/// 3. `deliver_tx(commitment)` for each commitment — **fail-fast on error**
/// 4. `end_block(height)`
/// 5. `commit()` → produces new Merkle root
/// 6. ONLY on success: update height counter, evict content
#[derive(Clone)]
pub struct FinalizationReporter {
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    content: ContentStore,
    height: SharedHeight,
}

impl FinalizationReporter {
    /// Create a new finalization reporter.
    ///
    /// The `content` and `height` must be the same instances shared with the
    /// [`MetaLedgerActor`](super::bridge::MetaLedgerActor).
    pub fn new(
        lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
        content: ContentStore,
        height: SharedHeight,
    ) -> Self {
        Self {
            lifecycle,
            content,
            height,
        }
    }

    /// Execute the ABCI pipeline atomically.
    ///
    /// Returns the app hash on success. On any failure, returns Err
    /// and the caller must NOT update height or evict content.
    async fn execute_pipeline(
        lifecycle: &mut (dyn ConsensusLifecycle + '_),
        height: u64,
        timestamp: u64,
        commitments: &[super::types::NodeCommitment],
    ) -> anyhow::Result<[u8; 32]> {
        let begin_events = lifecycle.begin_block(height, timestamp).await?;
        trace_events(&begin_events);

        for commitment in commitments {
            let tx_events = lifecycle.deliver_tx(commitment).await?;
            trace_events(&tx_events);
        }

        let end_response = lifecycle.end_block(height).await?;
        trace_events(&end_response.events);

        lifecycle.commit().await
    }
}

impl Reporter for FinalizationReporter {
    type Activity = SimplexActivity;

    async fn report(&mut self, activity: Self::Activity) {
        match activity {
            Activity::Finalization(finalization) => {
                let digest = finalization.proposal.payload;

                // Compute height speculatively — only stored on success
                let new_height = self
                    .height
                    .load(std::sync::atomic::Ordering::SeqCst)
                    + 1;

                // Look up block content
                let commitments = {
                    let store = self.content.read().await;
                    match store.get(&digest) {
                        Some(c) => c.clone(),
                        None => {
                            error!(
                                ?digest,
                                height = new_height,
                                "CRITICAL: finalized block content not found — \
                                 content may have been evicted before finalization"
                            );
                            return;
                        }
                    }
                };

                // Acquire exclusive lifecycle access for the entire pipeline
                let mut lifecycle = self.lifecycle.write().await;

                // TODO: Block timestamps should come from Simplex consensus context
                // (deterministic across nodes), not from each node's local clock.
                // Using SystemTime::now() means different nodes may commit blocks
                // with slightly different timestamps. Fix in Phase 4 (engine wiring)
                // when we have access to the Simplex round/epoch timing.
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                match Self::execute_pipeline(
                    &mut *lifecycle,
                    new_height,
                    timestamp,
                    &commitments,
                )
                .await
                {
                    Ok(app_hash) => {
                        // Pipeline succeeded — now atomically update shared state
                        self.height
                            .store(new_height, std::sync::atomic::Ordering::SeqCst);
                        self.content.write().await.remove(&digest);

                        info!(
                            height = new_height,
                            app_hash = %hex::encode(app_hash),
                            commitments = commitments.len(),
                            "block finalized and committed"
                        );
                    }
                    Err(e) => {
                        // Pipeline failed — state unchanged.
                        // Height NOT incremented, content NOT evicted.
                        // The app may be poisoned if commit() failed.
                        // Next begin_block will discard any pending state from this
                        // failed pipeline (pending_sequences, current_delta).
                        error!(
                            ?e,
                            height = new_height,
                            ?digest,
                            "finalization pipeline failed — state unchanged"
                        );
                    }
                }
            }
            Activity::Notarization(_) => {
                debug!("block notarized (pending finalization)");
            }
            Activity::ConflictingNotarize(_) => {
                error!("FAULT: conflicting notarize votes detected");
            }
            Activity::ConflictingFinalize(_) => {
                error!("FAULT: conflicting finalize votes detected");
            }
            Activity::NullifyFinalize(_) => {
                error!("FAULT: nullify-finalize conflict detected");
            }
            _ => {
                // Individual votes (Notarize, Nullify, Nullification, Finalize)
                // are routine consensus activity — no action needed.
            }
        }
    }
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
    use commonware_consensus::simplex::{
        signing_scheme::{ed25519::Certificate, utils::Signers},
        types::{Finalization, Proposal},
    };
    use commonware_consensus::types::Round;
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

    fn make_commitment(signer: &NodePrivKey, seq: u64) -> super::super::types::NodeCommitment {
        super::super::types::NodeCommitment::new(
            signer,
            [0u8; 32],
            [(seq & 0xff) as u8; 32],
            format!("transition-{seq}").as_bytes(),
            CommitmentKind::Inference,
            seq,
        )
    }

    /// Construct a real Activity::Finalization with dummy certificate.
    /// The reporter only reads `finalization.proposal.payload` — it doesn't
    /// verify the certificate, so dummy signatures are fine for unit tests.
    fn make_finalization_activity(digest: BlockDigest) -> SimplexActivity {
        let proposal = Proposal::new(
            Round::new(0, 1),
            0, // parent view
            digest,
        );
        let certificate = Certificate {
            signers: Signers::from(2, vec![0, 1]),
            signatures: vec![], // dummy — reporter doesn't verify
        };
        Activity::Finalization(Finalization {
            proposal,
            certificate,
        })
    }

    #[tokio::test]
    async fn reporter_finalization_commits_block() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        // Build commitments and compute digest
        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);
        let mut commitments = vec![
            make_commitment(&signer_a, 1),
            make_commitment(&signer_b, 1),
        ];
        commitments.sort_by_key(|c| c.mempool_key());

        let digest = MetaLedgerActor::hash_commitments(&commitments);

        // Populate content store
        let content: ContentStore =
            Arc::new(RwLock::new(BoundedContentStore::new(64)));
        content.write().await.insert(digest, commitments);

        let height: SharedHeight = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut reporter = FinalizationReporter::new(
            lifecycle.clone(),
            content.clone(),
            height.clone(),
        );

        // Construct real Activity::Finalization and call report()
        let activity = make_finalization_activity(digest);
        reporter.report(activity).await;

        // Height should be incremented to 1
        assert_eq!(
            height.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "height should be 1 after finalization"
        );

        // Content should be evicted
        assert!(
            content.read().await.get(&digest).is_none(),
            "content should be evicted after successful finalization"
        );
    }

    #[tokio::test]
    async fn reporter_missing_content_does_not_update_height() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        // Empty content store — no content for the digest
        let content: ContentStore =
            Arc::new(RwLock::new(BoundedContentStore::new(64)));
        let height: SharedHeight = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let mut reporter = FinalizationReporter::new(
            lifecycle.clone(),
            content.clone(),
            height.clone(),
        );

        // Report finalization for a block whose content is missing
        use commonware_cryptography::Hasher;
        let fake_digest = commonware_cryptography::Sha256::hash(b"nonexistent");
        let activity = make_finalization_activity(fake_digest);
        reporter.report(activity).await;

        // Height should NOT have changed
        assert_eq!(
            height.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "height should remain 0 when content is missing"
        );
    }

    #[tokio::test]
    async fn reporter_sequential_finalizations() {
        let (storage, _dir) = test_storage().await;
        let mempool = Arc::new(Mempool::new(100, 1000));
        let app = ErgorsConsensusApp::new(storage, test_validators(), mempool.clone());
        let lifecycle: Arc<RwLock<dyn ConsensusLifecycle>> = Arc::new(RwLock::new(app));

        let content: ContentStore =
            Arc::new(RwLock::new(BoundedContentStore::new(64)));
        let height: SharedHeight = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let mut reporter = FinalizationReporter::new(
            lifecycle.clone(),
            content.clone(),
            height.clone(),
        );

        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);

        // Block 1: seq=1 for both signers
        let mut c1 = vec![
            make_commitment(&signer_a, 1),
            make_commitment(&signer_b, 1),
        ];
        c1.sort_by_key(|c| c.mempool_key());
        let d1 = MetaLedgerActor::hash_commitments(&c1);
        content.write().await.insert(d1, c1);
        reporter.report(make_finalization_activity(d1)).await;
        assert_eq!(height.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Block 2: seq=2 for both signers
        let mut c2 = vec![
            make_commitment(&signer_a, 2),
            make_commitment(&signer_b, 2),
        ];
        c2.sort_by_key(|c| c.mempool_key());
        let d2 = MetaLedgerActor::hash_commitments(&c2);
        content.write().await.insert(d2, c2);
        reporter.report(make_finalization_activity(d2)).await;
        assert_eq!(height.load(std::sync::atomic::Ordering::SeqCst), 2);

        // Both block contents should be evicted
        assert!(content.read().await.get(&d1).is_none());
        assert!(content.read().await.get(&d2).is_none());
    }
}
