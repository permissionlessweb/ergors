//! Bootstrap Module
//!
//! Provides utilities for bootstrapping new ergors nodes:
//! - Configuration generation (config_gen.rs)
//! - Secure file transport over commonware (transport.rs)
//! - File chunking and framing (framing.rs)

pub mod config_gen;
pub mod framing;
pub mod transport;

// Re-export main types
pub use config_gen::{
    BootstrapConfigGenerator, NodeBootstrapParams, NodeConfig,
};
pub use framing::{FileChunk, FileChunker, ChunkMetadata, MAX_CHUNK_SIZE};
pub use transport::{BootstrapTransport, FileType, BootstrapFileMessage};
