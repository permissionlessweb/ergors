pub mod claim;
pub mod indexer;
pub mod ipfs;
pub mod vote_ext;
// ipfs:
// - dedicated server api: internal deploy/teardown/migration client for ipfs nodes (via akash & local mirror)
// - filter keys for use of ipfs node + websocket events
// -
// vote-extension:
// - wire in core functionality/communication with validator
// - dedicate api to verifying proofs, including nullifier into vote-extension
//
//
// cosmos-indexer:
// - implement web-socket with rpc for smart-contract events
// - mirror/reference penumbras indexer implementation
// - ensure dedicated storage layer for classification of storing indexed events into db

// proof-verification:
// - import/export all keys for headstash instances
// - dedicated modular function for proof verification (designed to be reusable and not dependent on its members type)
// - handle errors/logs/tracing gracefully

// wavs-service:
// - wrap api in wavs runtime,
// - write deployment & verification scripts
