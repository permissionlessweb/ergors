# Ergors Consensus & Meta-Ledger Refactor

## Opening

This is the real design. No hand-waving, no enterprise astronaut bullshit, no "we'll figure it out later" cop-outs.

The core insight: Ergors nodes are **sovereign**. They don't share a global state like a normal blockchain. Each node has its own LLM configs, API keys, orchestration queues, mnemonic custody. What they *do* share is a **meta-ledger** — a consensus on the validity and ordering of each other's *state transition commitments*. Think of it as: "I don't need to know your state. I just need to know you committed to a state transition, and that commitment is valid."

This gives us verifiable LLM inferences, federated attestation, and permissionless node participation — without sacrificing privacy or sovereignty.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        ERGORS NODE                                  │
│                                                                     │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────────────────────┐   │
│  │ Mempool  │  │  Consensus   │  │     ABCI Lifecycle          │   │
│  │          │──│  (Simplex)   │──│                             │   │
│  │ Pending  │  │              │  │  PrepareProposal            │   │
│  │ Txs      │  │  propose()   │  │  ProcessProposal            │   │
│  │          │  │  verify()    │  │  BeginBlock                  │   │
│  │          │  │  finalize()  │  │  DeliverTx (per commitment) │   │
│  └──────────┘  └──────────────┘  │  EndBlock                    │   │
│       ▲                          │  Commit                      │   │
│       │                          └─────────────┬───────────────┘   │
│       │                                        │                    │
│  ┌────┴─────┐                          ┌───────┴──────────┐        │
│  │ P2P Net  │                          │   App Engine     │        │
│  │ (cw-p2p) │                          │                  │        │
│  │          │                          │  CosmWasm hooks  │        │
│  │ ch0: disc│                          │  State commits   │        │
│  │ ch1: task│                          │  Cnidarium store │        │
│  │ ch2: sync│                          └──────────────────┘        │
│  │ ch3: hlth│                                                      │
│  │ ch4: keys│                                                      │
│  │ ch5: vote│  ← NEW: consensus votes                              │
│  │ ch6: brod│  ← NEW: block broadcast                              │
│  │ ch7: rslv│  ← NEW: certificate resolution                       │
│  └──────────┘                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Nodes are validators in a BFT network.** The meta-ledger doesn't store LLM outputs or API keys — it stores *commitments* (hashes) to per-node state transitions. Consensus determines the canonical ordering and validity of these commitments.

### What goes into a block

```
Block {
    context: Round + Leader + Parent,
    height: u64,
    timestamp: u64,
    // THE MEAT:
    commitments: Vec<NodeCommitment>,
    // Each NodeCommitment is:
    //   node_id: Ed25519 pubkey
    //   prev_state_root: SHA-256
    //   new_state_root: SHA-256
    //   transition_hash: SHA-256(transition_data)
    //   signature: Ed25519 over (prev || new || transition_hash)
    //   proof: Option<Vec<u8>>  // ZK proof or Merkle proof, on demand
}
```

This is the critical difference from Alto (which has empty blocks — just ordering). Our blocks carry *semantic content*: the per-node commitment vector.

---

## 2. The Bridge: Simplex ↔ ABCI Lifecycle

Here's where it gets interesting. The user's Penumbra-style ABCI example uses CometBFT as the consensus engine, receiving `ConsensusRequest` messages via a tower actor queue. We're using Commonware Simplex instead. The bridge is the `Application` trait implementation.

```
                    COMMONWARE SIMPLEX
                    ==================

 Application::propose()  ──→  ABCI PrepareProposal
                               (collect from mempool, order, build block)

 Application::verify()   ──→  ABCI ProcessProposal
                               (validate proposed block commitments)

 Reporter::report()      ──→  Block Finalization Pipeline:
   (on finalization)           BeginBlock
                               → DeliverTx (per commitment)
                               → EndBlock
                               → Commit
```

**This does NOT conflict with Commonware.** Simplex calls `propose()` and `verify()` — those are our hooks. The ABCI lifecycle methods are *internal* to our Application implementation. Simplex doesn't know or care about them. It just sees "here's a digest" and "is this digest valid?".

The Penumbra pattern of using `tower_actor::Actor` for a message queue still works — but instead of CometBFT driving the queue, our `Reporter` (which Simplex calls on finalization) drives it.

---

## 3. Core Types

### 3.1 State Commitment (the transaction type)

```rust
use commonware_cryptography::{Digestible, Committable, Sha256};
use commonware_codec::{Codec, Read, Write, EncodeSize};

/// A single node's commitment to a state transition.
/// This is the "transaction" in our meta-ledger.
#[derive(Clone, Debug)]
pub struct NodeCommitment {
    /// Who made this commitment
    pub node_id: ed25519::PublicKey,
    /// Cnidarium state root BEFORE the transition
    pub prev_state_root: [u8; 32],
    /// Cnidarium state root AFTER the transition
    pub new_state_root: [u8; 32],
    /// SHA-256 hash of the transition data (opaque — privacy preserved)
    pub transition_digest: [u8; 32],
    /// What kind of transition this is
    pub kind: CommitmentKind,
    /// Monotonic sequence number per node (prevents replay)
    pub sequence: u64,
    /// Ed25519 signature over (prev_state_root || new_state_root || transition_digest || sequence)
    pub signature: ed25519::Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommitmentKind {
    /// LLM inference result commitment
    Inference,
    /// Orchestration queue state change
    Orchestration,
    /// API key rotation
    KeyRotation,
    /// Config update
    ConfigUpdate,
    /// CosmWasm contract execution
    ContractExec,
    /// Generic state transition
    Generic,
}

impl NodeCommitment {
    /// Create and sign a new commitment.
    /// The transition_data is hashed — never leaves the node unless requested.
    pub fn new(
        signer: &ed25519::PrivateKey,
        prev_state_root: [u8; 32],
        new_state_root: [u8; 32],
        transition_data: &[u8],
        kind: CommitmentKind,
        sequence: u64,
    ) -> Self {
        let transition_digest = Sha256::hash(transition_data);
        let sign_payload = Self::sign_payload(
            &prev_state_root,
            &new_state_root,
            &transition_digest,
            sequence,
        );
        let signature = signer.sign(b"ergors-commitment", &sign_payload);
        Self {
            node_id: signer.public_key(),
            prev_state_root,
            new_state_root,
            transition_digest,
            kind,
            sequence,
            signature,
        }
    }

    /// Verify the commitment signature
    pub fn verify(&self) -> bool {
        let payload = Self::sign_payload(
            &self.prev_state_root,
            &self.new_state_root,
            &self.transition_digest,
            self.sequence,
        );
        self.node_id.verify(b"ergors-commitment", &payload, &self.signature)
    }

    fn sign_payload(
        prev: &[u8; 32],
        new: &[u8; 32],
        transition: &[u8; 32],
        seq: u64,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 32 + 8);
        buf.extend_from_slice(prev);
        buf.extend_from_slice(new);
        buf.extend_from_slice(transition);
        buf.extend_from_slice(&seq.to_le_bytes());
        buf
    }
}
```

### 3.2 The Meta-Ledger Block

```rust
/// A block in the meta-ledger.
/// Contains ordered commitments from multiple nodes.
pub struct MetaBlock {
    /// Simplex consensus context (round, leader, parent)
    pub context: consensus::Context<Sha256Digest, ed25519::PublicKey>,
    /// Parent block digest
    pub parent: Sha256Digest,
    /// Block height (sequential)
    pub height: Height,
    /// Timestamp (ms since epoch)
    pub timestamp: u64,
    /// Ordered vector of node state commitments
    pub commitments: Vec<NodeCommitment>,
    /// Pre-computed digest
    digest: Sha256Digest,
}

impl commonware_consensus::Block for MetaBlock {
    fn parent(&self) -> Self::Commitment { self.parent }
}

impl Heightable for MetaBlock {
    fn height(&self) -> Height { self.height }
}

// Digestible: SHA-256(parent || height || timestamp || commitment_digests...)
// Committable: same as digest (content-addressed)
```

### 3.3 The Mempool

```rust
use std::collections::BTreeMap;
use tokio::sync::RwLock;

/// Mempool holds pending commitments from local and remote nodes.
/// BTreeMap keyed by (node_id, sequence) for dedup and ordering.
pub struct Mempool {
    /// Pending commitments, keyed by (node_pubkey_bytes, sequence)
    pending: RwLock<BTreeMap<([u8; 32], u64), NodeCommitment>>,
    /// Maximum commitments per block
    max_per_block: usize,
    /// Maximum total pending
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

    /// Add a commitment after CheckTx validation.
    /// Returns false if mempool is full or commitment is duplicate.
    pub async fn add(&self, commitment: NodeCommitment) -> bool {
        let mut pending = self.pending.write().await;
        if pending.len() >= self.max_pending {
            return false;
        }
        let key = (commitment.node_id.as_bytes(), commitment.sequence);
        pending.insert(key, commitment).is_none()
    }

    /// Drain up to max_per_block commitments for proposal.
    /// Returns them in deterministic order (by node_id, then sequence).
    pub async fn drain_for_proposal(&self) -> Vec<NodeCommitment> {
        let mut pending = self.pending.write().await;
        let mut batch = Vec::with_capacity(self.max_per_block);
        let keys: Vec<_> = pending.keys().take(self.max_per_block).cloned().collect();
        for key in keys {
            if let Some(c) = pending.remove(&key) {
                batch.push(c);
            }
        }
        batch
    }

    /// Re-add commitments that failed to finalize (block rejected).
    pub async fn requeue(&self, commitments: Vec<NodeCommitment>) {
        let mut pending = self.pending.write().await;
        for c in commitments {
            let key = (c.node_id.as_bytes(), c.sequence);
            pending.insert(key, c);
        }
    }
}
```

---

## 4. The ABCI Lifecycle Trait

This is the Penumbra-inspired piece. But instead of receiving CometBFT requests via tower, our lifecycle is driven by the Commonware Simplex Application trait. The trait itself is engine-agnostic — CosmWasm contracts hook into it.

```rust
use anyhow::Result;

/// ABCI-like lifecycle for meta-ledger block processing.
///
/// This trait provides the programmable hooks between consensus states.
/// CosmWasm contracts can be registered as handlers for each step.
///
/// The lifecycle is:
///   CheckTx → PrepareProposal → ProcessProposal
///   → BeginBlock → DeliverTx (×N) → EndBlock → Commit
#[async_trait]
pub trait ConsensusLifecycle: Send + Sync {
    /// Validate an incoming commitment before adding to mempool.
    /// This is called on every node for every received commitment.
    ///
    /// Checks: signature validity, sequence monotonicity, node in validator set,
    /// and any CosmWasm contract validation hooks.
    async fn check_tx(&self, commitment: &NodeCommitment) -> Result<()>;

    /// Leader proposes a block by selecting and ordering commitments from the mempool.
    ///
    /// This runs against an ISOLATED state fork (like Penumbra's prepare_proposal).
    /// If this proposal doesn't finalize, the fork is discarded.
    async fn prepare_proposal(
        &mut self,
        height: Height,
        max_bytes: usize,
    ) -> Result<Vec<NodeCommitment>>;

    /// Validators verify a proposed block of commitments.
    ///
    /// Also runs against an isolated state fork.
    /// Returns true if the block is valid.
    async fn process_proposal(
        &mut self,
        height: Height,
        commitments: &[NodeCommitment],
    ) -> Result<bool>;

    /// Called when a block is finalized — begin processing.
    ///
    /// Good place for: updating validator set, emitting begin-block events,
    /// triggering CosmWasm contract hooks.
    async fn begin_block(&mut self, height: Height, timestamp: u64) -> Result<Vec<Event>>;

    /// Process a single commitment within a finalized block.
    ///
    /// This is where each node's state transition commitment is recorded
    /// in the meta-ledger. CosmWasm contracts can validate or transform.
    async fn deliver_tx(
        &mut self,
        commitment: &NodeCommitment,
    ) -> Result<Vec<Event>>;

    /// Called after all commitments in a block are delivered.
    ///
    /// Good place for: validator power updates, epoch transitions,
    /// emitting end-block events.
    async fn end_block(&mut self, height: Height) -> Result<EndBlockResponse>;

    /// Finalize the block and produce the new app state hash.
    ///
    /// Commits the StateDelta to Cnidarium storage.
    /// Returns the new Merkle root (app_hash).
    async fn commit(&mut self) -> Result<[u8; 32]>;

    // --- Vote Extensions (optional, for custom consensus data) ---

    /// Extend your vote with additional data (e.g., LLM inference proofs).
    ///
    /// Called after ProcessProposal succeeds. The extension is included
    /// in the node's notarize vote.
    async fn extend_vote(
        &self,
        height: Height,
        commitments: &[NodeCommitment],
    ) -> Result<Vec<u8>> {
        // Default: no extension
        Ok(Vec::new())
    }

    /// Verify another node's vote extension.
    async fn verify_vote_extension(
        &self,
        voter: &ed25519::PublicKey,
        extension: &[u8],
    ) -> Result<bool> {
        // Default: accept empty extensions, reject non-empty
        Ok(extension.is_empty())
    }
}
```

### 4.1 Events (for tracing and CosmWasm hooks)

```rust
/// Consensus lifecycle event — same shape as Tendermint events,
/// but we own the type so we're not dragging in the tendermint crate.
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: String,
    pub attributes: Vec<EventAttribute>,
}

#[derive(Clone, Debug)]
pub struct EventAttribute {
    pub key: String,
    pub value: String,
    /// If true, this attribute is not included in the event index
    pub index: bool,
}

pub struct EndBlockResponse {
    pub events: Vec<Event>,
    /// Updated validator set (if changed this block)
    pub validator_updates: Option<Vec<ValidatorUpdate>>,
}

pub struct ValidatorUpdate {
    pub pubkey: ed25519::PublicKey,
    /// 0 = remove from validator set
    pub power: u64,
}
```

---

## 5. The Application Bridge (Simplex ↔ ABCI)

This is the actual `commonware_consensus::Application` implementation that bridges Simplex consensus to our ABCI lifecycle.

```rust
use commonware_consensus::{Application, VerifyingApplication};
use commonware_runtime::tokio::Context as RuntimeContext;

/// Bridges Commonware Simplex to our ABCI lifecycle.
///
/// When Simplex calls propose(), we run PrepareProposal.
/// When Simplex calls verify(), we run ProcessProposal.
/// When blocks finalize (via Reporter), we run the full
/// BeginBlock → DeliverTx → EndBlock → Commit pipeline.
pub struct MetaLedgerApp {
    mempool: Arc<Mempool>,
    storage: cnidarium::Storage,
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    /// Tracks latest finalized height for sequence validation
    latest_height: Arc<AtomicU64>,
}

impl<E> Application<E> for MetaLedgerApp
where
    E: commonware_runtime::Clock + commonware_runtime::Spawner + Send + Sync + 'static,
{
    type SigningScheme = consensus_types::Scheme;
    type Context = consensus_types::Context;
    type Block = MetaBlock;

    async fn genesis(&mut self) -> MetaBlock {
        // Genesis block: empty commitments, deterministic seed
        let digest = Sha256::hash(b"ergors-genesis");
        MetaBlock {
            context: Default::default(),
            parent: Sha256Digest::EMPTY,
            height: Height::zero(),
            timestamp: 0,
            commitments: vec![],
            digest,
        }
    }

    async fn propose(
        &mut self,
        (runtime, context): (E, Self::Context),
        mut ancestry: AncestorStream<'_, Self::Block>,
    ) -> Option<MetaBlock> {
        let parent = ancestry.next().await?;

        // --- THIS IS WHERE ABCI MEETS SIMPLEX ---
        // Run PrepareProposal: drain mempool, order commitments
        let mut lifecycle = self.lifecycle.write().await;
        let commitments = lifecycle
            .prepare_proposal(parent.height.next(), 1_048_576) // 1MB max
            .await
            .ok()?;

        let timestamp = std::cmp::max(
            runtime.current().elapsed().as_millis() as u64,
            parent.timestamp + 1,
        );

        Some(MetaBlock::new(
            context,
            parent.digest(),
            parent.height.next(),
            timestamp,
            commitments,
        ))
    }
}

impl<E> VerifyingApplication<E> for MetaLedgerApp
where
    E: commonware_runtime::Clock + commonware_runtime::Spawner + Send + Sync + 'static,
{
    async fn verify(
        &mut self,
        (runtime, _context): (E, Self::Context),
        mut ancestry: AncestorStream<'_, Self::Block>,
    ) -> bool {
        let parent = match ancestry.next().await {
            Some(p) => p,
            None => return false,
        };

        let block = match ancestry.next().await {
            Some(b) => b,
            None => return false,
        };

        // Timestamp sanity (Alto uses 500ms synchrony bound, we use 2s for geo-distributed nodes)
        if block.timestamp <= parent.timestamp {
            return false;
        }
        let now_ms = runtime.current().elapsed().as_millis() as u64;
        if block.timestamp > now_ms + 2000 {
            return false;
        }

        // --- ABCI ProcessProposal ---
        let mut lifecycle = self.lifecycle.write().await;
        match lifecycle.process_proposal(block.height, &block.commitments).await {
            Ok(valid) => valid,
            Err(e) => {
                tracing::warn!(?e, "process_proposal failed");
                false
            }
        }
    }
}
```

### 5.1 The Finalization Reporter (drives BeginBlock → Commit)

```rust
/// When Simplex finalizes a block, this reporter drives the ABCI
/// execution pipeline: BeginBlock → DeliverTx → EndBlock → Commit.
///
/// This is the equivalent of what CometBFT does after consensus,
/// but we own the execution loop.
pub struct FinalizationReporter {
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    block_store: Arc<RwLock<BlockStore>>,
}

impl<E> commonware_consensus::Reporter for FinalizationReporter
where
    E: commonware_runtime::Spawner + Send + 'static,
{
    type Activity = consensus_types::Activity;

    async fn report(&mut self, activity: Self::Activity) {
        match activity {
            Activity::Finalized { height, block, .. } => {
                let mut lifecycle = self.lifecycle.write().await;

                // 1. BeginBlock
                let begin_events = lifecycle
                    .begin_block(height, block.timestamp)
                    .await
                    .unwrap_or_default();
                trace_events(&begin_events);

                // 2. DeliverTx for each commitment
                for commitment in &block.commitments {
                    match lifecycle.deliver_tx(commitment).await {
                        Ok(events) => trace_events(&events),
                        Err(e) => {
                            tracing::error!(
                                node = %hex::encode(commitment.node_id.as_bytes()),
                                seq = commitment.sequence,
                                ?e,
                                "deliver_tx failed"
                            );
                        }
                    }
                }

                // 3. EndBlock
                let end_response = lifecycle.end_block(height).await.unwrap_or_else(|e| {
                    tracing::error!(?e, "end_block failed");
                    EndBlockResponse { events: vec![], validator_updates: None }
                });
                trace_events(&end_response.events);

                // 4. Commit
                match lifecycle.commit().await {
                    Ok(app_hash) => {
                        tracing::info!(
                            ?height,
                            app_hash = %hex::encode(app_hash),
                            commitments = block.commitments.len(),
                            "block committed"
                        );
                        // Store finalized block
                        if let Ok(mut store) = self.block_store.write() {
                            store.insert(height, block, app_hash);
                        }
                    }
                    Err(e) => {
                        tracing::error!(?e, "CRITICAL: commit failed — state may be inconsistent");
                    }
                }

                // 5. Apply validator updates if any
                if let Some(updates) = end_response.validator_updates {
                    // Update the peer set for next epoch
                    // (This feeds back into commonware-p2p's Provider trait)
                    self.apply_validator_updates(updates).await;
                }
            }
            Activity::Notarized { .. } => {
                // Block notarized but not yet finalized — no state changes
            }
        }
    }
}
```

---

## 6. The Concrete Lifecycle Implementation

This is where Ergors' actual logic lives. The `ConsensusLifecycle` trait is implemented by `ErgorsConsensusApp`, which wraps Cnidarium storage and CosmWasm hooks.

```rust
/// The actual ABCI lifecycle implementation for Ergors.
///
/// Mirrors the Penumbra Consensus struct pattern:
/// - Uses cnidarium::Storage for state
/// - Processes against isolated snapshots for proposal/verification
/// - Commits via StateDelta for finalization
pub struct ErgorsConsensusApp {
    storage: cnidarium::Storage,
    /// Committed node sequences: node_pubkey → latest sequence
    sequences: HashMap<[u8; 32], u64>,
    /// Current block's state delta (accumulated during DeliverTx)
    current_delta: Option<cnidarium::StateDelta>,
    /// Validator set (Ed25519 pubkeys → voting power)
    validators: BTreeMap<ed25519::PublicKey, u64>,
    /// CosmWasm runtime for contract hooks
    #[cfg(feature = "cw")]
    wasm: Arc<WasmRuntime>,
}

#[async_trait]
impl ConsensusLifecycle for ErgorsConsensusApp {
    async fn check_tx(&self, commitment: &NodeCommitment) -> Result<()> {
        // 1. Signature verification (non-negotiable)
        if !commitment.verify() {
            anyhow::bail!("invalid commitment signature");
        }

        // 2. Sequence monotonicity (prevent replay)
        let node_key = commitment.node_id.as_bytes();
        if let Some(&last_seq) = self.sequences.get(node_key) {
            if commitment.sequence <= last_seq {
                anyhow::bail!(
                    "stale sequence: got {}, expected > {}",
                    commitment.sequence, last_seq
                );
            }
        }

        // 3. Node must be in validator set (or in allowed set)
        if !self.validators.contains_key(&commitment.node_id) {
            anyhow::bail!("unknown node: not in validator set");
        }

        // 4. CosmWasm hook: custom validation
        #[cfg(feature = "cw")]
        {
            let snapshot = self.storage.latest_snapshot();
            self.wasm_check_tx(&snapshot, commitment).await?;
        }

        Ok(())
    }

    async fn prepare_proposal(
        &mut self,
        height: Height,
        _max_bytes: usize,
    ) -> Result<Vec<NodeCommitment>> {
        // Prepare against an ISOLATED snapshot (like Penumbra)
        // If this proposal doesn't finalize, nothing is corrupted
        let snapshot = self.storage.latest_snapshot();

        // Drain mempool — already ordered by (node_id, sequence)
        // The mempool.drain_for_proposal() gives us deterministic ordering
        let commitments = self.mempool.drain_for_proposal().await;

        // Filter: only include commitments that pass check_tx
        // against the current snapshot
        let mut valid = Vec::with_capacity(commitments.len());
        for c in commitments {
            if self.check_tx(&c).await.is_ok() {
                valid.push(c);
            }
        }

        tracing::info!(
            height = height.as_u64(),
            commitments = valid.len(),
            "prepared proposal"
        );

        Ok(valid)
    }

    async fn process_proposal(
        &mut self,
        height: Height,
        commitments: &[NodeCommitment],
    ) -> Result<bool> {
        // Verify against isolated snapshot
        let snapshot = self.storage.latest_snapshot();

        for c in commitments {
            // Every commitment in the proposal must pass check_tx
            if let Err(e) = self.check_tx(c).await {
                tracing::warn!(
                    node = %hex::encode(c.node_id.as_bytes()),
                    seq = c.sequence,
                    ?e,
                    "invalid commitment in proposal"
                );
                return Ok(false);
            }
        }

        // Check ordering: must be sorted by (node_id, sequence)
        for window in commitments.windows(2) {
            let key_a = (window[0].node_id.as_bytes(), window[0].sequence);
            let key_b = (window[1].node_id.as_bytes(), window[1].sequence);
            if key_a >= key_b {
                tracing::warn!("proposal commitments not in canonical order");
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn begin_block(&mut self, height: Height, timestamp: u64) -> Result<Vec<Event>> {
        // Create fresh StateDelta for this block
        let snapshot = self.storage.latest_snapshot();
        self.current_delta = Some(cnidarium::StateDelta::new(snapshot));

        let mut events = vec![Event {
            kind: "begin_block".into(),
            attributes: vec![
                EventAttribute { key: "height".into(), value: height.as_u64().to_string(), index: true },
                EventAttribute { key: "timestamp".into(), value: timestamp.to_string(), index: true },
            ],
        }];

        // CosmWasm begin_block hook
        #[cfg(feature = "cw")]
        {
            let hook_events = self.wasm_begin_block(height, timestamp).await?;
            events.extend(hook_events);
        }

        Ok(events)
    }

    async fn deliver_tx(&mut self, commitment: &NodeCommitment) -> Result<Vec<Event>> {
        let delta = self.current_delta.as_mut()
            .ok_or_else(|| anyhow::anyhow!("deliver_tx called before begin_block"))?;

        // Record the commitment in the meta-ledger
        let key = format!(
            "meta_ledger/commitments/{}/{}",
            hex::encode(commitment.node_id.as_bytes()),
            commitment.sequence,
        );
        delta.put_raw(key, commitment.encode_to_vec());

        // Update latest sequence for this node
        let seq_key = format!(
            "meta_ledger/sequences/{}",
            hex::encode(commitment.node_id.as_bytes()),
        );
        delta.put_raw(seq_key, commitment.sequence.to_le_bytes().to_vec());

        // Update in-memory sequence tracker
        self.sequences.insert(
            commitment.node_id.as_bytes(),
            commitment.sequence,
        );

        let mut events = vec![Event {
            kind: "deliver_commitment".into(),
            attributes: vec![
                EventAttribute {
                    key: "node".into(),
                    value: hex::encode(commitment.node_id.as_bytes()),
                    index: true,
                },
                EventAttribute {
                    key: "sequence".into(),
                    value: commitment.sequence.to_string(),
                    index: true,
                },
                EventAttribute {
                    key: "kind".into(),
                    value: format!("{:?}", commitment.kind),
                    index: true,
                },
                EventAttribute {
                    key: "transition_digest".into(),
                    value: hex::encode(commitment.transition_digest),
                    index: false,
                },
            ],
        }];

        // CosmWasm deliver_tx hook
        #[cfg(feature = "cw")]
        {
            let hook_events = self.wasm_deliver_tx(delta, commitment).await?;
            events.extend(hook_events);
        }

        Ok(events)
    }

    async fn end_block(&mut self, height: Height) -> Result<EndBlockResponse> {
        let delta = self.current_delta.as_mut()
            .ok_or_else(|| anyhow::anyhow!("end_block called before begin_block"))?;

        // Record block height
        delta.put_raw(
            "meta_ledger/latest_height".to_string(),
            height.as_u64().to_le_bytes().to_vec(),
        );

        let mut events = vec![];
        let mut validator_updates = None;

        // CosmWasm end_block hook — can return validator updates
        #[cfg(feature = "cw")]
        {
            let (hook_events, updates) = self.wasm_end_block(height).await?;
            events.extend(hook_events);
            validator_updates = updates;
        }

        Ok(EndBlockResponse { events, validator_updates })
    }

    async fn commit(&mut self) -> Result<[u8; 32]> {
        let delta = self.current_delta.take()
            .ok_or_else(|| anyhow::anyhow!("commit called before begin_block"))?;

        // Commit the StateDelta to Cnidarium — this produces the new Merkle root
        let app_hash = self.storage.commit(delta).await?;

        // Reset for next block
        // (App::new(self.storage.latest_snapshot()) equivalent from Penumbra)

        Ok(app_hash.0)
    }

    async fn extend_vote(
        &self,
        _height: Height,
        commitments: &[NodeCommitment],
    ) -> Result<Vec<u8>> {
        // If this node has its own pending commitment, include a proof
        // of its latest LLM inference as a vote extension
        //
        // This is where verifiable inference attestations go.
        // Other nodes verify these in verify_vote_extension().
        //
        // For now: empty. Enable when ZK proof infrastructure lands.
        Ok(Vec::new())
    }
}
```

---

## 7. Engine Wiring (Alto-style)

```rust
/// Wire everything together, Alto-style.
///
/// Creates: p2p network → channels → mempool → consensus app →
///          marshal → simplex engine → start.
pub struct ConsensusEngine {
    // Commonware primitives (same as Alto)
    buffer: buffered::Engine<...>,
    marshal: marshal::Actor<...>,
    consensus: simplex::Engine<...>,
    // Ergors-specific
    mempool: Arc<Mempool>,
    lifecycle: Arc<RwLock<ErgorsConsensusApp>>,
}

impl ConsensusEngine {
    pub async fn new(
        context: &RuntimeContext,
        network: &mut authenticated::Network<ed25519::PublicKey>,
        signer: ed25519::PrivateKey,
        validators: Vec<(ed25519::PublicKey, u64)>,  // pubkey → voting power
        storage: cnidarium::Storage,
        mempool: Arc<Mempool>,
        #[cfg(feature = "cw")] wasm: Arc<WasmRuntime>,
    ) -> Self {
        // Register consensus channels (channels 5-7, extending existing 0-4)
        let (vote_sender, vote_receiver) = network.register(
            5, // consensus votes
            Quota::per_second(NonZeroU32::new(128).unwrap()),
            1024,  // backlog
        );
        let (recovered_sender, recovered_receiver) = network.register(
            6, // recovered votes
            Quota::per_second(NonZeroU32::new(128).unwrap()),
            1024,
        );
        let (resolver_sender, resolver_receiver) = network.register(
            7, // certificate resolution
            Quota::per_second(NonZeroU32::new(128).unwrap()),
            1024,
        );
        let (broadcast_sender, broadcast_receiver) = network.register(
            8, // block broadcast
            Quota::per_second(NonZeroU32::new(8).unwrap()),
            256,
        );
        let (marshal_sender, marshal_receiver) = network.register(
            9, // marshal backfill
            Quota::per_second(NonZeroU32::new(8).unwrap()),
            256,
        );

        // Build validator set
        let n = validators.len();
        let f = (n - 1) / 3;  // BFT threshold: tolerates f faults in 3f+1

        // Create consensus scheme (Ed25519 multisig for simplicity)
        // Alto uses BLS threshold — we can upgrade later when threshold custody lands
        let scheme = ed25519_multisig::Scheme::new(signer.clone());

        // Create application
        let lifecycle = Arc::new(RwLock::new(ErgorsConsensusApp::new(
            storage.clone(),
            validators.clone(),
            mempool.clone(),
            #[cfg(feature = "cw")] wasm,
        )));

        let app = MetaLedgerApp {
            mempool: mempool.clone(),
            storage: storage.clone(),
            lifecycle: lifecycle.clone(),
            latest_height: Arc::new(AtomicU64::new(0)),
        };

        // Create buffered broadcast engine
        let buffer = buffered::Engine::new(broadcast_sender, broadcast_receiver);

        // Create marshal (block lifecycle manager)
        let finalized_archive = immutable::Archive::new(/* storage config */);
        let block_archive = immutable::Archive::new(/* storage config */);
        let marshal = marshal::Actor::new(
            finalized_archive,
            block_archive,
            marshal_sender,
            marshal_receiver,
        );

        // Wrap application with marshal
        let marshaled = ConsensusMarshaled::new(app, marshal.mailbox());

        // Create reporter
        let reporter = FinalizationReporter {
            lifecycle: lifecycle.clone(),
            block_store: Arc::new(RwLock::new(BlockStore::new())),
        };

        // Create simplex consensus engine
        let elector = elector::RoundRobin::new(validators.iter().map(|(pk, _)| pk.clone()));
        let consensus = simplex::Engine::new(
            scheme,
            marshaled.clone(),  // automaton
            marshaled,          // relay
            reporter,
            elector,
            simplex::Config {
                leader_timeout: Duration::from_millis(1000),
                notarization_timeout: Duration::from_millis(2000),
                nullify_retry: Duration::from_millis(10000),
                ..Default::default()
            },
        );

        Self { buffer, marshal, consensus, mempool, lifecycle }
    }

    /// Start the consensus engine.
    pub async fn start(self, context: RuntimeContext) {
        let buffer_handle = context.spawn(self.buffer.run());
        let marshal_handle = context.spawn(self.marshal.run());
        let consensus_handle = context.spawn(self.consensus.run(
            vote_sender, vote_receiver,
            recovered_sender, recovered_receiver,
            resolver_sender, resolver_receiver,
        ));

        // Wait for any to complete (or crash)
        tokio::select! {
            _ = buffer_handle => tracing::error!("buffer engine exited"),
            _ = marshal_handle => tracing::error!("marshal exited"),
            _ = consensus_handle => tracing::error!("consensus exited"),
        }
    }
}
```

---

## 8. P2P Mempool Gossip

Commitments need to reach every validator's mempool. We add a dedicated gossip channel.

```rust
/// Gossips pending commitments to all validators.
/// Runs as a background task, receiving local commitments
/// and forwarding remote ones to the mempool.
pub struct MempoolGossip {
    mempool: Arc<Mempool>,
    lifecycle: Arc<RwLock<dyn ConsensusLifecycle>>,
    sender: authenticated::lookup::Sender<ed25519::PublicKey>,
    receiver: authenticated::lookup::Receiver<ed25519::PublicKey>,
}

impl MempoolGossip {
    /// Register a new channel for mempool gossip.
    /// Channel 10: mempool transactions, rate-limited to 256/sec.
    pub fn register(
        network: &mut authenticated::Network<ed25519::PublicKey>,
    ) -> (authenticated::lookup::Sender<ed25519::PublicKey>,
          authenticated::lookup::Receiver<ed25519::PublicKey>) {
        network.register(
            10,
            Quota::per_second(NonZeroU32::new(256).unwrap()),
            4096,
        )
    }

    /// Submit a local commitment: validate, add to mempool, gossip to peers.
    pub async fn submit_local(&self, commitment: NodeCommitment) -> Result<()> {
        // CheckTx before gossip — don't waste bandwidth
        let lifecycle = self.lifecycle.read().await;
        lifecycle.check_tx(&commitment).await?;
        drop(lifecycle);

        // Add to local mempool
        if !self.mempool.add(commitment.clone()).await {
            anyhow::bail!("mempool full");
        }

        // Gossip to all validators
        let encoded = commitment.encode_to_vec();
        self.sender.send(Recipients::All, encoded, false).await?;

        Ok(())
    }

    /// Background loop: receive gossipped commitments from peers.
    pub async fn run(self) {
        loop {
            let (sender_pk, msg) = match self.receiver.recv().await {
                Ok(m) => m,
                Err(_) => break,
            };

            let commitment = match NodeCommitment::decode(&msg) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(?e, "invalid commitment from peer");
                    continue;
                }
            };

            // CheckTx before accepting
            let lifecycle = self.lifecycle.read().await;
            if let Err(e) = lifecycle.check_tx(&commitment).await {
                tracing::debug!(
                    from = %hex::encode(sender_pk.as_bytes()),
                    ?e,
                    "rejected commitment from peer"
                );
                continue;
            }
            drop(lifecycle);

            self.mempool.add(commitment).await;
        }
    }
}
```

---

## 9. CosmWasm Contract Hooks

Each ABCI step can invoke a CosmWasm contract. Contracts are registered per-step, not per-endpoint.

```rust
/// CosmWasm contract hooks for consensus lifecycle steps.
///
/// Stored in Cnidarium under:
///   consensus_hooks/{step_name} → contract_address
///
/// Example:
///   consensus_hooks/check_tx → ergors1abc...
///   consensus_hooks/begin_block → ergors1def...
#[cfg(feature = "cw")]
impl ErgorsConsensusApp {
    /// Call the registered CosmWasm contract for check_tx validation.
    async fn wasm_check_tx(
        &self,
        snapshot: &cnidarium::Snapshot,
        commitment: &NodeCommitment,
    ) -> Result<()> {
        let contract = self.get_hook_contract(snapshot, "check_tx").await;
        if let Some(addr) = contract {
            let query_msg = serde_json::json!({
                "validate_commitment": {
                    "node_id": hex::encode(commitment.node_id.as_bytes()),
                    "kind": format!("{:?}", commitment.kind),
                    "sequence": commitment.sequence,
                }
            });
            let result = self.wasm.query(&addr, &query_msg).await?;
            let valid: bool = serde_json::from_slice(&result)?;
            if !valid {
                anyhow::bail!("CosmWasm check_tx hook rejected commitment");
            }
        }
        Ok(())
    }

    async fn wasm_begin_block(&self, height: Height, timestamp: u64) -> Result<Vec<Event>> {
        let contract = self.get_hook_contract(
            &self.storage.latest_snapshot(), "begin_block"
        ).await;
        if let Some(addr) = contract {
            let exec_msg = serde_json::json!({
                "begin_block": {
                    "height": height.as_u64(),
                    "timestamp": timestamp,
                }
            });
            let response = self.wasm.execute(&addr, &exec_msg).await?;
            return Ok(wasm_response_to_events(response));
        }
        Ok(vec![])
    }

    async fn wasm_deliver_tx(
        &self,
        delta: &mut cnidarium::StateDelta,
        commitment: &NodeCommitment,
    ) -> Result<Vec<Event>> {
        let contract = self.get_hook_contract(
            &self.storage.latest_snapshot(), "deliver_tx"
        ).await;
        if let Some(addr) = contract {
            let exec_msg = serde_json::json!({
                "deliver_commitment": {
                    "node_id": hex::encode(commitment.node_id.as_bytes()),
                    "prev_state_root": hex::encode(commitment.prev_state_root),
                    "new_state_root": hex::encode(commitment.new_state_root),
                    "transition_digest": hex::encode(commitment.transition_digest),
                    "kind": format!("{:?}", commitment.kind),
                    "sequence": commitment.sequence,
                }
            });
            let response = self.wasm.execute(&addr, &exec_msg).await?;
            return Ok(wasm_response_to_events(response));
        }
        Ok(vec![])
    }

    async fn wasm_end_block(
        &self,
        height: Height,
    ) -> Result<(Vec<Event>, Option<Vec<ValidatorUpdate>>)> {
        let contract = self.get_hook_contract(
            &self.storage.latest_snapshot(), "end_block"
        ).await;
        if let Some(addr) = contract {
            let exec_msg = serde_json::json!({
                "end_block": { "height": height.as_u64() }
            });
            let response = self.wasm.execute(&addr, &exec_msg).await?;
            // Parse validator updates from contract response attributes
            let updates = parse_validator_updates(&response);
            return Ok((wasm_response_to_events(response), updates));
        }
        Ok((vec![], None))
    }

    async fn get_hook_contract(
        &self,
        snapshot: &cnidarium::Snapshot,
        step: &str,
    ) -> Option<String> {
        let key = format!("consensus_hooks/{}", step);
        snapshot.get_raw(&key).await.ok().flatten()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }
}
```

---

## 10. Integration with Existing Ergors

### 10.1 Modified `ErgorsAppState`

```rust
/// Updated app state — adds consensus engine and mempool.
#[derive(Clone)]
pub struct ErgorsAppState {
    pub r: Arc<LlmRouter>,
    pub s: Arc<ErgorsStorage>,
    pub nm: Arc<tokio::sync::Mutex<ErgorsNetworkManifold>>,
    pub t: Instant,
    pub c: ErgorsConfig,
    pub pr: Arc<RwLock<ProxyRouter>>,
    pub akash: Option<AkashDeploymentContext>,
    pub gm: Option<Arc<gateway::GatewayManager>>,
    #[cfg(feature = "cw")]
    pub wasm: Arc<WasmRuntime>,
    // NEW: consensus
    pub consensus: Option<Arc<ConsensusEngine>>,
    pub mempool: Arc<Mempool>,
}
```

### 10.2 When a node does ANYTHING that changes state, it commits

```rust
/// Extension trait on ErgorsStorage for consensus-aware operations.
///
/// After any state mutation, the node creates a commitment
/// and submits it to the mempool for consensus.
#[async_trait]
pub trait ConsensusAwareStorage {
    /// Commit a state change and submit the commitment to consensus.
    async fn commit_with_consensus(
        &self,
        delta: cnidarium::StateDelta,
        kind: CommitmentKind,
        transition_data: &[u8],
        signer: &ed25519::PrivateKey,
        gossip: &MempoolGossip,
    ) -> Result<[u8; 32]>;
}

#[async_trait]
impl ConsensusAwareStorage for ErgorsStorage {
    async fn commit_with_consensus(
        &self,
        delta: cnidarium::StateDelta,
        kind: CommitmentKind,
        transition_data: &[u8],
        signer: &ed25519::PrivateKey,
        gossip: &MempoolGossip,
    ) -> Result<[u8; 32]> {
        let prev_root = self.cnidarium.latest_snapshot().root_hash().await?;

        // Commit locally (this is the node's sovereign state)
        let new_root = self.cnidarium.commit(delta).await?;

        // Get next sequence number
        let seq = self.next_sequence(signer.public_key()).await;

        // Create signed commitment
        let commitment = NodeCommitment::new(
            signer,
            prev_root.0,
            new_root.0,
            transition_data,
            kind,
            seq,
        );

        // Submit to mempool + gossip to peers
        gossip.submit_local(commitment).await?;

        Ok(new_root.0)
    }
}
```

### 10.3 LLM Inference → Commitment

```rust
// In the prompt handler (simplified):
async fn handle_prompt(state: &ErgorsAppState, req: PromptRequest) -> Result<PromptResponse> {
    // 1. Route to LLM provider
    let response = state.r.route(&req).await?;

    // 2. Store in local Cnidarium
    let snapshot = state.s.cnidarium.latest_snapshot();
    let mut delta = cnidarium::StateDelta::new(snapshot);
    state.s.put_prompt_w_ctx(&mut delta, &response, /* indexes */)?;

    // 3. Create transition data (hashed — not revealed)
    let transition_data = serde_json::to_vec(&serde_json::json!({
        "type": "inference",
        "model": response.model,
        "prompt_hash": sha256(req.prompt.as_bytes()),
        "response_hash": sha256(response.content.as_bytes()),
        "timestamp": response.timestamp,
    }))?;

    // 4. Commit with consensus attestation
    let signer = state.get_node_signer().await?;
    let gossip = state.mempool_gossip.as_ref().unwrap();
    state.s.commit_with_consensus(
        delta,
        CommitmentKind::Inference,
        &transition_data,
        &signer,
        gossip,
    ).await?;

    Ok(response)
}
```

---

## 11. Initialization Flow

### 11.1 New CLI Command

```
ergors init consensus \
    --threshold 2 \
    --validators ed25519:abc123...,ed25519:def456...,ed25519:789abc... \
    --block-time 200 \
    --epoch-length 1000
```

### 11.2 Config Addition

```toml
# config.toml additions
[consensus]
enabled = true
# BFT threshold: tolerates f faults in 3f+1 nodes
# With 3 validators, f=0 (no fault tolerance) — need 4+ for f=1
threshold = 1
block_time_ms = 200
epoch_length = 1000
max_mempool_size = 10000
max_commitments_per_block = 500

# Initial validator set
[[consensus.validators]]
pubkey = "ed25519:abc123..."
power = 1

[[consensus.validators]]
pubkey = "ed25519:def456..."
power = 1

[[consensus.validators]]
pubkey = "ed25519:789abc..."
power = 1

# CosmWasm consensus hooks (optional)
[consensus.hooks]
check_tx = "ergors1_check_tx_contract"
begin_block = ""  # empty = no hook
deliver_tx = "ergors1_deliver_tx_contract"
end_block = "ergors1_end_block_contract"
```

### 11.3 SDL Update (Akash Deployment)

```yaml
services:
  ergors:
    image: ergors:latest
    env:
      - ERGORS_CONSENSUS_ENABLED=true
      - ERGORS_CONSENSUS_THRESHOLD=1
      - ERGORS_CONSENSUS_VALIDATORS=ed25519:abc123,ed25519:def456,ed25519:789abc
      - ERGORS_CONSENSUS_BLOCK_TIME_MS=200
      - ERGORS_CONSENSUS_EPOCH_LENGTH=1000
      - ERGORS_CUSTODY_PASSWORD=  # injected at deploy time
    expose:
      - port: 8080       # API
        as: 80
        to:
          - global: true
      - port: 26656      # P2P (consensus + gossip)
        as: 26656
        to:
          - global: true
      - port: 26657      # gRPC management
        as: 26657
        to:
          - global: true
```

### 11.4 Server Startup Addition

```rust
// In server.rs::new(), after network + storage initialization:

let consensus_engine = if config.consensus.enabled {
    let validators = config.consensus.validators
        .iter()
        .map(|v| (v.parse_pubkey(), v.power))
        .collect::<Vec<_>>();

    let mempool = Arc::new(Mempool::new(
        config.consensus.max_commitments_per_block,
        config.consensus.max_mempool_size,
    ));

    let engine = ConsensusEngine::new(
        &context,
        &mut network,
        node_signer,
        validators,
        storage.cnidarium.clone(),
        mempool.clone(),
        #[cfg(feature = "cw")] wasm.clone(),
    ).await;

    // Start mempool gossip
    let (gossip_tx, gossip_rx) = MempoolGossip::register(&mut network);
    let gossip = MempoolGossip {
        mempool: mempool.clone(),
        lifecycle: engine.lifecycle.clone(),
        sender: gossip_tx,
        receiver: gossip_rx,
    };
    context.spawn(gossip.run());

    // Start consensus engine in background
    let engine = Arc::new(engine);
    context.spawn(engine.clone().start(context.clone()));

    Some(engine)
} else {
    None
};
```

---

## 12. Module Structure

```
packages/ergors/src/
├── consensus/
│   ├── mod.rs              # Module exports
│   ├── types.rs            # NodeCommitment, MetaBlock, Event, etc.
│   ├── mempool.rs          # Mempool (BTreeMap-based, thread-safe)
│   ├── gossip.rs           # MempoolGossip (P2P commitment broadcast)
│   ├── lifecycle.rs        # ConsensusLifecycle trait (ABCI-like)
│   ├── app.rs              # ErgorsConsensusApp (trait implementation)
│   ├── bridge.rs           # MetaLedgerApp (Simplex ↔ ABCI bridge)
│   ├── reporter.rs         # FinalizationReporter (drives block execution)
│   ├── engine.rs           # ConsensusEngine (wiring everything together)
│   ├── storage_ext.rs      # ConsensusAwareStorage trait
│   └── wasm_hooks.rs       # CosmWasm contract hooks per ABCI step
```

---

## 13. Data Flow Diagram

```
                        NODE A                                    NODE B
                    ┌──────────┐                              ┌──────────┐
                    │ LLM Call │                              │ LLM Call │
                    └────┬─────┘                              └────┬─────┘
                         │                                         │
                    ┌────┴─────┐                              ┌────┴─────┐
                    │ Store in │                              │ Store in │
                    │Cnidarium │                              │Cnidarium │
                    └────┬─────┘                              └────┬─────┘
                         │                                         │
                    ┌────┴──────────┐                         ┌────┴──────────┐
                    │ Sign          │                         │ Sign          │
                    │ NodeCommitment│                         │ NodeCommitment│
                    └────┬──────────┘                         └────┬──────────┘
                         │                                         │
                    ┌────┴─────┐      gossip (ch10)          ┌────┴─────┐
                    │ Mempool  │◄────────────────────────────►│ Mempool  │
                    │ (local)  │                              │ (local)  │
                    └────┬─────┘                              └────┬─────┘
                         │                                         │
                         └──────────────┬──────────────────────────┘
                                        │
                              ┌─────────┴──────────┐
                              │  SIMPLEX CONSENSUS  │
                              │                     │
                              │  Leader proposes    │
                              │  block with         │
                              │  commitments from   │
                              │  mempool            │
                              │                     │
                              │  2f+1 notarize      │
                              │  2f+1 finalize      │
                              └─────────┬──────────┘
                                        │
                              ┌─────────┴──────────┐
                              │  FINALIZATION       │
                              │                     │
                              │  BeginBlock         │
                              │  DeliverTx (×N)     │
                              │    → CW hooks       │
                              │  EndBlock           │
                              │    → val updates    │
                              │  Commit             │
                              │    → new app_hash   │
                              └────────────────────┘
```

---

## 14. Privacy Resolution

### The commitment is zero-knowledge friendly by default.

`NodeCommitment` contains:
- `transition_digest`: SHA-256 hash of the actual transition data
- `prev_state_root` / `new_state_root`: Cnidarium Merkle roots

The actual state (LLM prompts, API keys, config) **never leaves the node** unless explicitly synced. The meta-ledger only records *that a transition happened*, not *what it was*.

### On-demand reveal

```rust
/// Request full state reveal for a specific commitment.
/// Only honored if the target node consents.
pub struct StateRevealRequest {
    /// Which commitment to reveal
    pub commitment_digest: [u8; 32],
    /// Requester's pubkey (for encrypted response)
    pub requester: ed25519::PublicKey,
    /// Signature proving requester owns the key
    pub signature: ed25519::Signature,
}

/// Encrypted response containing the full transition data.
pub struct StateRevealResponse {
    /// ChaCha20Poly1305-encrypted transition data
    /// Encrypted to the requester's X25519 key (derived from Ed25519)
    pub encrypted_data: Vec<u8>,
    /// Nonce for decryption
    pub nonce: [u8; 24],
    /// Merkle proof that this data matches the commitment
    pub merkle_proof: Vec<[u8; 32]>,
}
```

### Future: ZK proofs

When the ZK infrastructure from `privacy.md` lands, commitments can include Groth16 proofs that the state transition is valid without revealing any data. The `extend_vote` / `verify_vote_extension` hooks are the natural place for this — validators attach ZK proofs as vote extensions.

---

## 15. Challenges and Solutions

| Challenge | Solution |
|-----------|----------|
| **Performance overhead of consensus** | Simplex achieves ~200ms block times (proven by Alto). Our blocks are small (just commitment hashes, ~100-200 bytes each). No heavy state machine execution in the critical path. |
| **Network latency for geo-distributed nodes** | Use 2s synchrony bound (vs Alto's 500ms). Block time can be 500ms-1s for geo-distributed deployments. Simplex is pipelining-friendly — finalization of block N overlaps proposal of block N+1. |
| **Mempool flooding** | Rate-limited gossip channel (256/sec). CheckTx validation before mempool admission. Max mempool size enforced. Per-node sequence numbers prevent replay. |
| **Validator set changes** | EndBlock returns `validator_updates`. CosmWasm contract manages the validator registry. Epoch transitions handled by `FixedEpocher` (same as Alto). |
| **Partial node failures** | Simplex tolerates f Byzantine faults in 3f+1 nodes. Nullification handles missing leaders. Marshal backfill catches up nodes that fall behind. |
| **Conflicting with existing P2P** | Consensus channels (5-9) are separate from existing channels (0-4). No shared state. Network manifold already supports multi-channel architecture. |
| **CosmWasm hook latency** | Hooks are optional. If no contract is registered for a step, it's a no-op. Hooks run async within the execution pipeline, not in the consensus hot path. |
| **Storage contention** | Meta-ledger writes to separate Cnidarium prefixes (`meta_ledger/*`). Node's own state uses existing prefixes. No cross-contamination. |

---

## 16. Testing Strategy

### 16.1 Deterministic Simulation (Commonware runtime::deterministic)

```rust
#[test]
fn test_consensus_three_validators() {
    // Use commonware deterministic runtime for reproducible tests
    let runner = deterministic::Runner::new(42); // fixed seed
    runner.start(|context| async move {
        // Create 3 validators with deterministic keys
        let validators = (0..3)
            .map(|i| ed25519::PrivateKey::from_seed(i))
            .collect::<Vec<_>>();

        // Create simulated network (commonware-p2p::simulated)
        let mut network = simulated::Network::new(context.clone());

        // Wire up 3 consensus engines
        // Each engine gets its own storage, mempool, lifecycle
        // ...

        // Submit commitments to node 0
        let commitment = NodeCommitment::new(
            &validators[0],
            [0u8; 32],  // genesis state root
            [1u8; 32],  // new state root
            b"test inference",
            CommitmentKind::Inference,
            1,
        );

        // Assert: commitment appears in finalized block
        // Assert: all 3 nodes have the same meta-ledger state
        // Assert: block height advances
        // Assert: commitment sequence tracked correctly
    });
}
```

### 16.2 Property-Based Tests

```rust
#[test]
fn test_mempool_ordering_is_deterministic() {
    // Insert commitments in random order
    // Assert: drain_for_proposal always returns same order
}

#[test]
fn test_commitment_replay_rejected() {
    // Submit commitment with sequence 5
    // Submit same commitment again
    // Assert: check_tx rejects the second one
}

#[test]
fn test_invalid_signature_rejected() {
    // Create commitment, corrupt signature
    // Assert: check_tx rejects it
}

#[test]
fn test_unknown_node_rejected() {
    // Create commitment from key not in validator set
    // Assert: check_tx rejects it
}
```

### 16.3 Integration Tests

```rust
#[tokio::test]
async fn test_full_block_lifecycle() {
    // 1. Create storage + app
    // 2. Submit 5 commitments via check_tx
    // 3. Run prepare_proposal → get block
    // 4. Run process_proposal → assert valid
    // 5. Run begin_block → deliver_tx (×5) → end_block → commit
    // 6. Assert: all commitments in meta-ledger storage
    // 7. Assert: sequences updated
    // 8. Assert: app_hash is new Merkle root
}
```

### 16.4 Conformance Tests

```rust
// Use commonware conformance patterns:
// - Liveness: blocks keep being produced even with f faulty nodes
// - Safety: no two honest nodes finalize conflicting blocks
// - Consistency: all honest nodes have the same meta-ledger state after finalization
```

---

## 17. Implementation Order

1. **`consensus/types.rs`** — Define `NodeCommitment`, `MetaBlock`, `Event`, `CommitmentKind`. Zero dependencies on existing code.
2. **`consensus/mempool.rs`** — The `Mempool` struct. Pure data structure, fully testable in isolation.
3. **`consensus/lifecycle.rs`** — The `ConsensusLifecycle` trait. Just a trait definition.
4. **`consensus/app.rs`** — `ErgorsConsensusApp` implementing the trait. Depends on Cnidarium.
5. **`consensus/bridge.rs`** — `MetaLedgerApp` implementing Commonware `Application`. The critical bridge.
6. **`consensus/reporter.rs`** — `FinalizationReporter`. Drives BeginBlock→Commit.
7. **`consensus/gossip.rs`** — `MempoolGossip`. Depends on commonware-p2p channels.
8. **`consensus/engine.rs`** — `ConsensusEngine` wiring. Depends on everything above.
9. **`consensus/storage_ext.rs`** — `ConsensusAwareStorage` trait. Integration with existing storage.
10. **`consensus/wasm_hooks.rs`** — CosmWasm integration. Feature-gated behind `cw`.
11. **Config + CLI** — Add consensus config section, `init consensus` command.
12. **Server integration** — Wire into `server.rs` startup sequence.

---

## Closing

This design does three things right:

1. **No unified global state.** Each node is sovereign. The meta-ledger records commitments, not state. Privacy is preserved by default.

2. **ABCI lifecycle coexists with Simplex, not conflicts.** Simplex calls `propose()` and `verify()` — our Application impl translates those into PrepareProposal and ProcessProposal. The BeginBlock→Commit pipeline runs on finalization, driven by the Reporter. CometBFT is not needed — we get the same programmability through our own trait.

3. **It's simple.** The entire consensus module is ~10 files. No framework, no magic, no 47 layers of abstraction. Just traits, structs, and channels. Like Alto, but with semantic blocks instead of empty ones.

The Penumbra pattern (tower-actor queue, Cnidarium state, isolated proposal forks) maps directly onto the Commonware composition model. The only thing we throw away is CometBFT itself — and good riddance. Simplex gives us 200ms blocks without the 10,000-line CometBFT dependency.

Build it bottom-up. Test each piece in isolation. Wire it together last. Don't add features until the basic pipeline works.
