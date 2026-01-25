//! Core types for the hybrid RAG system.
//!
//! No bullshit, no over-abstraction. Just the data structures we actually need.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A chunk's vector embedding + fast-access metadata for the vector index.
///
/// This lives in the HNSW index for sub-100ms retrieval. Keep it lean —
/// don't stuff the entire document in here, that's what Cnidarium is for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkVector {
    pub chunk_id: Uuid,
    pub embedding: Vec<f32>, // e.g., 768 dims from BGE/BERT
    pub metadata: ChunkMetadata,
}

/// Metadata stored alongside the vector for filtering and display.
///
/// This is *not* the full provenance record — just enough to show results
/// and do basic filtering without hitting Cnidarium.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// First ~200 chars for preview in search results
    pub preview: String,
    /// Source type: "pdf", "code", "web", etc.
    pub source_type: String,
    /// Unix timestamp when ingested
    pub ingested_at: i64,
    /// User/system tags for filtering
    pub tags: Vec<String>,
}

/// The full, verifiable chunk record stored in Cnidarium.
///
/// This is the source of truth. If the vector index burns down, we can
/// rebuild from these records. Includes cryptographic hashes and full provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableChunk {
    pub chunk_id: Uuid,

    // Content (stored here, not in a separate blob store for simplicity)
    /// The actual text of the chunk
    pub content: String,

    // Integrity hashes
    /// BLAKE3(content.as_bytes())
    pub content_hash: [u8; 32],
    /// BLAKE3(bytemuck::cast_slice(&embedding))
    pub embedding_hash: [u8; 32],

    // Provenance
    /// Cnidarium snapshot version when committed
    pub version: u64,
    /// When this chunk was ingested
    pub ingested_at: pbjson_types::Timestamp,
    /// Original source URI (file path, URL, etc.)
    pub source_uri: String,
    /// Multi-tenant attribution (who uploaded this)
    pub uploader_id: Option<String>,

    // Optional fields for advanced use cases
    /// Serialized access control rules (not implemented in MVP)
    pub access_policy: Option<Vec<u8>>,
    /// Git commit hash if from a repo
    pub commit_ref: Option<String>,
    /// Previous version for chunk lineage tracking
    pub previous_version: Option<Uuid>,
}

/// Search result from the vector index (before verification).
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk_id: Uuid,
    pub similarity: f32, // cosine similarity or whatever distance metric
    pub metadata: ChunkMetadata,
}

/// A chunk with verified provenance (after Cnidarium lookup).
#[derive(Debug, Clone)]
pub struct VerifiedChunk {
    pub chunk_id: Uuid,
    pub content: String,
    pub similarity: f32,
    pub provenance: ChunkProvenance,
    /// JMT inclusion proof (optional, expensive to generate)
    pub proof: Option<Vec<u8>>,
}

/// Provenance information extracted from VerifiableChunk.
#[derive(Debug, Clone)]
pub struct ChunkProvenance {
    pub version: u64,
    pub ingested_at: pbjson_types::Timestamp,
    pub source_uri: String,
    pub uploader_id: Option<String>,
    pub commit_ref: Option<String>,
}

impl From<&VerifiableChunk> for ChunkProvenance {
    fn from(chunk: &VerifiableChunk) -> Self {
        Self {
            version: chunk.version,
            ingested_at: chunk.ingested_at,
            source_uri: chunk.source_uri.clone(),
            uploader_id: chunk.uploader_id.clone(),
            commit_ref: chunk.commit_ref.clone(),
        }
    }
}

/// Options for query behavior.
#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    /// Enable hash verification (adds ~10-50ms per chunk)
    pub verify: bool,
    /// Generate JMT inclusion proofs (expensive, only if needed)
    pub include_proof: bool,
    /// Metadata filters (applied after vector search, before verification)
    pub filters: MetadataFilters,
}

/// Metadata filters for search results.
///
/// Filters are applied AFTER vector search (to get top-k candidates)
/// but BEFORE verification (to avoid wasting time on chunks that don't match).
#[derive(Debug, Clone, Default)]
pub struct MetadataFilters {
    /// Filter by source type (e.g., "rust", "markdown", "pdf")
    pub source_type: Option<String>,
    /// Filter by tags (result must have ALL specified tags)
    pub tags: Vec<String>,
    /// Filter by minimum ingestion timestamp
    pub min_ingested_at: Option<i64>,
    /// Filter by maximum ingestion timestamp
    pub max_ingested_at: Option<i64>,
    /// Filter by source URI prefix (e.g., "github.com/foo/bar")
    pub source_uri_prefix: Option<String>,
}

impl MetadataFilters {
    /// Check if any filters are set.
    pub fn is_empty(&self) -> bool {
        self.source_type.is_none()
            && self.tags.is_empty()
            && self.min_ingested_at.is_none()
            && self.max_ingested_at.is_none()
            && self.source_uri_prefix.is_none()
    }

    /// Check if a chunk matches all filters.
    pub fn matches(&self, metadata: &ChunkMetadata, source_uri: Option<&str>) -> bool {
        // Source type filter
        if let Some(ref filter_type) = self.source_type {
            if &metadata.source_type != filter_type {
                return false;
            }
        }

        // Tags filter (must have ALL specified tags)
        if !self.tags.is_empty() {
            for tag in &self.tags {
                if !metadata.tags.contains(tag) {
                    return false;
                }
            }
        }

        // Timestamp filters
        if let Some(min_ts) = self.min_ingested_at {
            if metadata.ingested_at < min_ts {
                return false;
            }
        }

        if let Some(max_ts) = self.max_ingested_at {
            if metadata.ingested_at > max_ts {
                return false;
            }
        }

        // Source URI prefix filter
        if let Some(ref prefix) = self.source_uri_prefix {
            if let Some(uri) = source_uri {
                if !uri.starts_with(prefix) {
                    return false;
                }
            } else {
                return false; // No source URI, can't match prefix
            }
        }

        true
    }
}

/// Document to be ingested.
#[derive(Debug, Clone)]
pub struct Document {
    /// Document content (will be chunked)
    pub content: String,
    /// Document URI (file path, URL, etc.)
    pub uri: String,
    /// Document type ("pdf", "code", "web", etc.)
    pub doc_type: String,
    /// Tags to apply to all chunks
    pub tags: Vec<String>,
}
