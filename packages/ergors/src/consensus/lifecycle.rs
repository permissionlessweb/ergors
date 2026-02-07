//! ABCI-like consensus lifecycle trait.
//!
//! This provides programmable hooks between consensus states, inspired by
//! CometBFT's ABCI but driven by Commonware Simplex consensus instead.
//!
//! The lifecycle is:
//!   CheckTx → PrepareProposal → ProcessProposal
//!   → BeginBlock → DeliverTx (×N) → EndBlock → Commit
//!
//! Simplex calls Application::propose() → we run PrepareProposal.
//! Simplex calls Application::verify() → we run ProcessProposal.
//! Simplex finalizes (via Reporter) → we run BeginBlock → DeliverTx → EndBlock → Commit.
//!
//! CosmWasm contracts can hook into each step for custom validation,
//! auth, or execution logic.

use super::types::{EndBlockResponse, Event, NodeCommitment};
use anyhow::Result;
use async_trait::async_trait;
use commonware_cryptography::ed25519;

/// Programmable lifecycle for meta-ledger block processing.
///
/// Implement this trait to define how your node validates, proposes,
/// and executes blocks of state transition commitments.
#[async_trait]
pub trait ConsensusLifecycle: Send + Sync {
    /// Validate an incoming commitment before adding it to the mempool.
    ///
    /// Called on every node for every received commitment (local or gossipped).
    /// Must verify:
    /// - Signature validity
    /// - Sequence monotonicity (> last committed sequence for this node)
    /// - Node is in the validator set (or allowed set)
    /// - Any CosmWasm contract validation hooks
    ///
    /// This is a read-only check — no state mutations.
    async fn check_tx(&self, commitment: &NodeCommitment) -> Result<()>;

    /// Validate and order candidate commitments for a block proposal.
    ///
    /// The bridge drains the mempool and passes candidates here.
    /// This method filters (re-checks via check_tx) and returns the valid subset
    /// in canonical order. The bridge requeues candidates if this fails.
    ///
    /// Returns the ordered commitments to include in the block.
    async fn prepare_proposal(
        &self,
        height: u64,
        max_bytes: usize,
        candidates: &[NodeCommitment],
    ) -> Result<Vec<NodeCommitment>>;

    /// Validators verify a proposed block of commitments.
    ///
    /// Also runs against an isolated state fork. Returns true if the block is valid.
    /// Must verify:
    /// - All commitments pass check_tx
    /// - Commitments are in canonical order (sorted by mempool_key)
    /// - No duplicate sequences per node
    async fn process_proposal(
        &self,
        height: u64,
        commitments: &[NodeCommitment],
    ) -> Result<bool>;

    /// Called when a finalized block begins processing.
    ///
    /// State mutations start here. Good place for:
    /// - Updating validator set metadata
    /// - Emitting begin-block events
    /// - Triggering CosmWasm begin_block hooks
    async fn begin_block(&mut self, height: u64, timestamp: u64) -> Result<Vec<Event>>;

    /// Process a single commitment within a finalized block.
    ///
    /// Records the commitment in the meta-ledger, updates sequence tracking,
    /// and invokes any CosmWasm deliver_tx hooks.
    async fn deliver_tx(&mut self, commitment: &NodeCommitment) -> Result<Vec<Event>>;

    /// Called after all commitments in a block are delivered.
    ///
    /// Good place for:
    /// - Validator power updates (returned in EndBlockResponse)
    /// - Epoch transition logic
    /// - Emitting end-block events
    /// - CosmWasm end_block hooks
    async fn end_block(&mut self, height: u64) -> Result<EndBlockResponse>;

    /// Finalize the block and produce the new app state hash.
    ///
    /// Commits the accumulated StateDelta to Cnidarium storage.
    /// Returns the new Merkle root.
    async fn commit(&mut self) -> Result<[u8; 32]>;

    // --- Optional vote extensions ---

    /// Extend your vote with additional data (e.g., LLM inference proofs).
    ///
    /// Called after ProcessProposal succeeds. The extension is included
    /// in the node's notarize vote. Other nodes verify it via
    /// verify_vote_extension.
    ///
    /// Default: no extension.
    async fn extend_vote(
        &self,
        _height: u64,
        _commitments: &[NodeCommitment],
    ) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    /// Verify another node's vote extension.
    ///
    /// Default: accept empty extensions, reject non-empty.
    async fn verify_vote_extension(
        &self,
        _voter: &ed25519::PublicKey,
        extension: &[u8],
    ) -> Result<bool> {
        Ok(extension.is_empty())
    }
}
