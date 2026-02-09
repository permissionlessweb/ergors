//! RAG integration for the ERGORS engine.
//!
//! Thin wrapper around ergors-rag that bridges engine storage.

use crate::storage::ErgorsStorage;
use anyhow::Result;
use std::sync::Arc;

// Re-export ergors-rag types for convenience
pub use ergors_rag::{
    Document, Embedder, HybridRAG, QueryOptions, QueryResult,
    SearchResult, VerifiedChunk,
};

/// Create a HybridRAG instance using the engine's storage.
///
/// Uses the remote embedder to call an external embedding service.
///
/// # Example
/// ```rust,no_run
/// use ergors::proxy::rag;
/// use ergors::storage::ErgorsStorage;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let storage = ErgorsStorage::new("./data", vec![
///     "rag_chunks".into(),
///     "rag_source_index".into(),
/// ]).await?;
///
/// let rag = rag::new_remote(
///     &storage,
///     "http://provider.akash.network:8080",
///     "all-MiniLM-L6-v2",
///     384,
/// )?;
///
/// // Ingest
/// let doc = rag::Document {
///     content: "Hello world".into(),
///     uri: "test.txt".into(),
///     doc_type: "text".into(),
///     tags: vec![],
/// };
/// rag.ingest(doc, None).await?;
///
/// // Query
/// let results = rag.query("hello", 5, rag::QueryOptions::default()).await?;
/// # Ok(())
/// # }
/// ```
pub fn new_remote(
    storage: &ErgorsStorage,
    endpoint: &str,
    model: &str,
    dimension: usize,
) -> Result<HybridRAG<ergors_rag::embedder::remote::RemoteEmbedder>> {
    let cnidarium_storage = Arc::new(storage.cs.clone());
    HybridRAG::with_remote(cnidarium_storage, endpoint, model, dimension)
}

/// Create a HybridRAG instance with a shared HTTP client.
///
/// Use this when making many requests to reuse HTTP connections.
/// More efficient than `new_remote` for high-volume usage.
pub fn new_remote_with_client(
    storage: &ErgorsStorage,
    client: reqwest::Client,
    endpoint: &str,
    model: &str,
    dimension: usize,
) -> Result<HybridRAG<ergors_rag::embedder::remote::RemoteEmbedder>> {
    let cnidarium_storage = Arc::new(storage.cs.clone());
    HybridRAG::with_remote_client(cnidarium_storage, client, endpoint, model, dimension)
}

/// Create a HybridRAG instance with a dummy embedder (testing only).
pub fn new_dummy(storage: &ErgorsStorage, dimension: usize) -> Result<HybridRAG<ergors_rag::embedder::DummyEmbedder>> {
    let cnidarium_storage = Arc::new(storage.cs.clone());
    HybridRAG::with_dummy(cnidarium_storage, dimension)
}
