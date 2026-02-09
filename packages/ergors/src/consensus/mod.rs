//! Meta-ledger consensus for the Ergors network.
//!
//! Nodes are sovereign — they maintain their own state (LLM configs, API keys,
//! orchestration queues). The meta-ledger records signed *commitments* to state
//! transitions, achieving consensus on the ordering and validity of each node's
//! operations without sharing the actual state.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────┐     ┌───────────┐     ┌──────────────────┐
//! │ Mempool │────▶│  Simplex  │────▶│ ABCI Lifecycle   │
//! │         │     │ Consensus │     │                  │
//! │ pending │     │           │     │ PrepareProposal  │
//! │ commits │     │ propose() │     │ ProcessProposal  │
//! │         │     │ verify()  │     │ BeginBlock       │
//! │         │     │ finalize  │     │ DeliverTx (×N)   │
//! └─────────┘     └───────────┘     │ EndBlock         │
//!                                   │ Commit           │
//!                                   └──────────────────┘
//! ```
//!
//! # Modules
//!
//! - [`types`] — Core data types: NodeCommitment, CommitmentKind, Event
//! - [`mempool`] — Thread-safe transaction pool with deterministic ordering
//! - [`lifecycle`] — ABCI-like trait for programmable consensus hooks
//! - [`app`] — Concrete lifecycle implementation backed by Cnidarium
//! - [`bridge`] — Simplex ↔ ABCI bridge (Automaton + Relay implementations)
//! - [`reporter`] — Finalization reporter (BeginBlock → Commit pipeline)
//! - [`engine`] — Engine orchestrator (wires all components, starts Simplex)

pub mod app;
pub mod bridge;
pub mod engine;
pub mod gossip;
pub mod lifecycle;
pub mod mempool;
pub mod reporter;
pub mod types;

pub use bridge::{BlockDigest, BoundedContentStore, ContentStore, MetaLedgerMailbox, SharedHeight};
pub use engine::{ConsensusConfig, ConsensusSystem, LogOnlyBlocker, start_consensus};
pub use gossip::{GossipHandle, new_gossip};
pub use lifecycle::ConsensusLifecycle;
pub use mempool::Mempool;
pub use reporter::FinalizationReporter;
pub use types::{CommitmentKind, EndBlockResponse, Event, NodeCommitment, ValidatorUpdate};
