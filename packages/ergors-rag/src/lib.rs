//! # ERGORS RAG: Hybrid Vector Database with Verifiable Provenance
//!
//! A practical implementation of the hybrid RAG architecture described in
//! RAG_VECTOR_DB_SPEC.md. Separates concerns between:
//!
//! 1. **Fast retrieval**: HNSW vector index for sub-100ms ANN search
//! 2. **Verifiable provenance**: Cnidarium/JMT for cryptographic guarantees
//!
//! ## Architecture
//!
//! ```text
//! Query Vector
//!     ↓
//! [1] HNSW Index → top-k chunk_ids + similarity scores (fast)
//!     ↓
//! [2] Cnidarium Verification → verify hashes, get provenance (optional)
//!     ↓
//! [3] Verified Context → pass to LLM
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ergors_rag::{HybridRAG, Document, QueryOptions};
//! use cnidarium::Storage;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Initialize storage (must include "rag_chunks" and "rag_source_index" prefixes)
//! let prefixes = vec!["rag_chunks".to_string(), "rag_source_index".to_string()];
//! std::fs::create_dir_all("./storage").ok();
//! let storage = Storage::load("./storage".into(), prefixes).await?;
//!
//! // Create RAG system with dummy embedder (for testing)
//! let rag = HybridRAG::with_dummy(Arc::new(storage), 128)?;
//!
//! // For production, use Candle (local inference), Remote (Akash), or OpenAI:
//! // let rag = HybridRAG::with_candle(Arc::new(storage)).await?;  // requires "candle" feature
//! // let rag = HybridRAG::with_remote(Arc::new(storage), "http://...", "model", 384)?;  // requires "openai" feature
//! // let rag = HybridRAG::with_openai(Arc::new(storage))?;         // requires "openai" feature
//!
//! // Ingest a document
//! let doc = Document {
//!     content: "Machine learning is a subset of AI...".to_string(),
//!     uri: "docs/ml-intro.md".to_string(),
//!     doc_type: "markdown".to_string(),
//!     tags: vec!["ml".to_string(), "ai".to_string()],
//! };
//! let chunk_ids = rag.ingest(doc, None).await?;
//! println!("Ingested {} chunks", chunk_ids.len());
//!
//! // Query (fast, no verification)
//! let results = rag.query("What is machine learning?", 5, QueryOptions::default()).await?;
//! match results {
//!     ergors_rag::QueryResult::Standard(results) => {
//!         for result in results {
//!             println!("Similarity: {:.3}, Preview: {}", result.similarity, result.metadata.preview);
//!         }
//!     }
//!     _ => {}
//! }
//!
//! // Query with verification (slower, adds provenance + hash checks)
//! let options = QueryOptions { verify: true, ..Default::default() };
//! let verified = rag.query("What is machine learning?", 5, options).await?;
//! match verified {
//!     ergors_rag::QueryResult::Verified(chunks) => {
//!         for chunk in chunks {
//!             println!("Source: {}, Ingested: {:?}", chunk.provenance.source_uri, chunk.provenance.ingested_at);
//!         }
//!     }
//!     _ => {}
//! }
//! # Ok(())
//! # }
//! ```

pub mod embedder;
pub mod ingest;
pub mod query;
pub mod storage;
pub mod types;
pub mod vector_index;

use anyhow::{Context, Result};
use embedder::DummyEmbedder;
use cnidarium::Storage;
use ingest::ingest_document;
use query::{query_standard, query_verified};
use storage::RagStorage;
use std::sync::Arc;
use uuid::Uuid;
use vector_index::HnswVectorIndex;

// Re-export public types
pub use embedder::Embedder;
pub use types::{
    ChunkMetadata, ChunkProvenance, ChunkVector, Document, MetadataFilters, QueryOptions,
    SearchResult, VerifiableChunk, VerifiedChunk,
};

/// The main hybrid RAG system.
///
/// Generic over the embedder type. Use:
/// - `HybridRAG::with_dummy()` for testing
/// - `HybridRAG::with_candle()` for local inference (requires "candle" feature)
/// - `HybridRAG::with_openai()` for OpenAI API (requires "openai" feature)
/// - `HybridRAG::with_embedder()` for custom embedder
///
/// All operations are async and thread-safe.
pub struct HybridRAG<E: Embedder> {
    /// Vector index for fast ANN search
    vector_index: HnswVectorIndex,
    /// Cnidarium storage for verifiable chunks
    storage: RagStorage,
    /// Embedder (generic, can be Candle, OpenAI, or dummy)
    embedder: E,
}

impl<E: Embedder> HybridRAG<E> {
    /// Create HybridRAG with a custom embedder.
    ///
    /// **Requirements:**
    /// - `storage` must be initialized with "rag_chunks" and "rag_source_index" prefixes
    /// - `embedder.dimension()` determines vector index dimension
    ///
    /// **HNSW Parameters (sensible defaults):**
    /// - max_elements: 100_000 (will grow if needed)
    /// - ef_construction: 200 (build quality)
    /// - max_nb_connection: 24 (graph connectivity)
    pub fn with_embedder(storage: Arc<Storage>, embedder: E) -> Result<Self> {
        let dimension = embedder.dimension();
        let vector_index = HnswVectorIndex::new(
            dimension,
            100_000, // max elements
            200,     // ef_construction
            24,      // max_nb_connection
        )?;

        let rag_storage = RagStorage::new(storage);

        Ok(Self {
            vector_index,
            storage: rag_storage,
            embedder,
        })
    }
}

// Specialized constructors for different embedder types
impl HybridRAG<DummyEmbedder> {
    /// Create HybridRAG with DummyEmbedder (for testing only).
    ///
    /// **DO NOT USE IN PRODUCTION.** This embedder generates deterministic
    /// pseudo-random vectors based on text hashes, not useful for real retrieval.
    pub fn with_dummy(storage: Arc<Storage>, dimension: usize) -> Result<Self> {
        let embedder = DummyEmbedder::new(dimension);
        Self::with_embedder(storage, embedder)
    }
}

#[cfg(feature = "candle")]
impl HybridRAG<embedder::candle::CandleEmbedder> {
    /// Create HybridRAG with Candle embedder (local inference).
    ///
    /// Uses BGE-small-en-v1.5 by default (~134MB model download on first run).
    /// CPU-only by default. For GPU, use `with_candle_gpu`.
    ///
    /// ## Example
    /// ```rust,no_run
    /// use ergors_rag::HybridRAG;
    /// use cnidarium::Storage;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let storage = Storage::load("./storage".into(), vec!["rag_chunks".into(), "rag_source_index".into()]).await?;
    /// let rag = HybridRAG::with_candle(Arc::new(storage)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_candle(storage: Arc<Storage>) -> Result<Self> {
        let embedder = embedder::candle::CandleEmbedder::new_default()
            .await
            .context("Failed to initialize Candle embedder")?;
        Self::with_embedder(storage, embedder)
    }

    /// Create with custom Candle model.
    ///
    /// Supported models:
    /// - BAAI/bge-small-en-v1.5 (384 dims, fast)
    /// - BAAI/bge-base-en-v1.5 (768 dims, better quality)
    /// - intfloat/multilingual-e5-small (384 dims, 100+ languages)
    pub async fn with_candle_model(storage: Arc<Storage>, model_id: &str) -> Result<Self> {
        let embedder = embedder::candle::CandleEmbedder::new(model_id)
            .await
            .context("Failed to initialize Candle embedder")?;
        Self::with_embedder(storage, embedder)
    }
}

#[cfg(feature = "openai")]
impl HybridRAG<embedder::remote::RemoteEmbedder> {
    /// Create HybridRAG with remote OpenAI-compatible embedder.
    ///
    /// Use this to connect to embedding services deployed on Akash or elsewhere.
    ///
    /// ## Example
    /// ```rust,no_run
    /// use ergors_rag::HybridRAG;
    /// use cnidarium::Storage;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let storage = Storage::load("./storage".into(), vec!["rag_chunks".into(), "rag_source_index".into()]).await?;
    /// let rag = HybridRAG::with_remote(
    ///     Arc::new(storage),
    ///     "http://provider.akash.network:8080",
    ///     "all-MiniLM-L6-v2",
    ///     384
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_remote(
        storage: Arc<Storage>,
        base_url: &str,
        model: &str,
        dimension: usize,
    ) -> Result<Self> {
        let embedder = embedder::remote::RemoteEmbedder::new(base_url, model, dimension)
            .context("Failed to initialize remote embedder")?;
        Self::with_embedder(storage, embedder)
    }
}

#[cfg(feature = "openai")]
impl HybridRAG<embedder::openai::OpenAIEmbedder> {
    /// Create HybridRAG with OpenAI embedder.
    ///
    /// Requires OPENAI_API_KEY environment variable.
    /// Uses text-embedding-3-small by default (1536 dims, $0.02/1M tokens).
    ///
    /// ## Example
    /// ```rust,no_run
    /// use ergors_rag::HybridRAG;
    /// use cnidarium::Storage;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// std::env::set_var("OPENAI_API_KEY", "sk-...");
    /// let storage = Storage::load("./storage".into(), vec!["rag_chunks".into(), "rag_source_index".into()]).await?;
    /// let rag = HybridRAG::with_openai(Arc::new(storage))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_openai(storage: Arc<Storage>) -> Result<Self> {
        let embedder = embedder::openai::OpenAIEmbedder::new()
            .context("Failed to initialize OpenAI embedder (check OPENAI_API_KEY)")?;
        Self::with_embedder(storage, embedder)
    }

    /// Create with custom OpenAI model.
    pub fn with_openai_model(
        storage: Arc<Storage>,
        model: &str,
        dimension: usize,
    ) -> Result<Self> {
        let embedder = embedder::openai::OpenAIEmbedder::with_model(model, dimension)?;
        Self::with_embedder(storage, embedder)
    }
}

impl<E: Embedder> HybridRAG<E> {
    /// Ingest a document into the RAG system.
    ///
    /// Steps:
    /// 1. Chunk the document text (~1000 char chunks)
    /// 2. Generate embeddings (batched)
    /// 3. Insert into vector index
    /// 4. Store verifiable records in Cnidarium
    ///
    /// Returns the chunk IDs that were created.
    pub async fn ingest(&self, doc: Document, uploader_id: Option<String>) -> Result<Vec<Uuid>> {
        ingest_document(doc, &self.embedder, &self.vector_index, &self.storage, uploader_id).await
    }

    /// Delete a chunk by ID.
    ///
    /// Removes from both vector index and Cnidarium.
    pub async fn delete(&self, chunk_id: Uuid) -> Result<()> {
        self.vector_index.delete(chunk_id)?;
        self.storage.delete_chunk(chunk_id).await?;
        Ok(())
    }

    /// Query the RAG system.
    ///
    /// Behavior depends on `options.verify`:
    /// - `false` (default): Fast vector search only (~20-100ms)
    /// - `true`: Vector search + Cnidarium verification (~50-200ms)
    ///
    /// Returns either SearchResult (fast) or VerifiedChunk (verified) depending on options.
    pub async fn query(
        &self,
        query_text: &str,
        k: usize,
        options: QueryOptions,
    ) -> Result<QueryResult> {
        if options.verify {
            let verified = query_verified(
                query_text,
                k,
                &self.embedder,
                &self.vector_index,
                &self.storage,
                &options,
            )
            .await?;
            Ok(QueryResult::Verified(verified))
        } else {
            let standard = query_standard(query_text, k, &self.embedder, &self.vector_index).await?;
            Ok(QueryResult::Standard(standard))
        }
    }

    /// Get current index size (number of chunks).
    pub fn size(&self) -> usize {
        self.vector_index.size()
    }

    /// Get all chunks from a specific source URI.
    pub async fn get_chunks_by_source(&self, source_uri: &str) -> Result<Vec<types::VerifiableChunk>> {
        self.storage.get_chunks_by_source(source_uri).await
    }
}

/// Query result (either standard or verified).
#[derive(Debug)]
pub enum QueryResult {
    /// Fast search results (no verification)
    Standard(Vec<SearchResult>),
    /// Verified search results (with provenance)
    Verified(Vec<VerifiedChunk>),
}

impl QueryResult {
    /// Get the number of results.
    pub fn len(&self) -> usize {
        match self {
            QueryResult::Standard(v) => v.len(),
            QueryResult::Verified(v) => v.len(),
        }
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_rag() -> (HybridRAG<DummyEmbedder>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_path).unwrap();

        let prefixes = vec!["rag_chunks".to_string(), "rag_source_index".to_string()];
        let storage = Storage::load(storage_path, prefixes)
            .await
            .unwrap();

        let rag = HybridRAG::with_dummy(Arc::new(storage), 128).unwrap();
        (rag, temp_dir)
    }

    #[tokio::test]
    async fn test_ingest_and_query() {
        let (rag, _temp) = setup_test_rag().await;

        // Ingest a document
        let doc = Document {
            content: "Rust is a systems programming language focused on safety and performance. \
                     It achieves memory safety without garbage collection.".to_string(),
            uri: "docs/rust.md".to_string(),
            doc_type: "markdown".to_string(),
            tags: vec!["rust".to_string(), "programming".to_string()],
        };
        let chunk_ids = rag.ingest(doc, None).await.unwrap();
        assert!(!chunk_ids.is_empty());

        // Query (standard)
        let results = rag.query("What is Rust?", 3, QueryOptions::default()).await.unwrap();
        assert!(!results.is_empty());

        // Query (verified)
        let options = QueryOptions {
            verify: true,
            include_proof: false,
            filters: Default::default(),
        };
        let verified = rag.query("memory safety", 3, options).await.unwrap();
        assert!(!verified.is_empty());
    }

    #[tokio::test]
    async fn test_delete_chunk() {
        let (rag, _temp) = setup_test_rag().await;

        // Ingest
        let doc = Document {
            content: "Test content".to_string(),
            uri: "test.txt".to_string(),
            doc_type: "text".to_string(),
            tags: vec![],
        };
        let chunk_ids = rag.ingest(doc, None).await.unwrap();
        let chunk_id = chunk_ids[0];

        // Verify it exists
        let results = rag.query("Test", 1, QueryOptions::default()).await.unwrap();
        assert!(!results.is_empty());

        // Delete
        rag.delete(chunk_id).await.unwrap();

        // Verify size decreased
        assert!(rag.size() < chunk_ids.len());
    }

    #[tokio::test]
    async fn test_get_chunks_by_source() {
        let (rag, _temp) = setup_test_rag().await;

        let source_uri = "docs/multi-chunk.md";
        let doc = Document {
            content: "First chunk.\n\nSecond chunk.\n\nThird chunk.".to_string(),
            uri: source_uri.to_string(),
            doc_type: "markdown".to_string(),
            tags: vec![],
        };
        let chunk_ids = rag.ingest(doc, None).await.unwrap();
        assert_eq!(chunk_ids.len(), 3);

        // Query by source
        let chunks = rag.get_chunks_by_source(source_uri).await.unwrap();
        assert_eq!(chunks.len(), 3);
    }
}
