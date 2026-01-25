//! Vector index abstraction and HNSW implementation.
//!
//! No Box<dyn Trait> bullshit in the hot path — we're using concrete types.
//! If you want to swap indexes later, refactor then. Premature abstraction is cancer.

use crate::types::{ChunkMetadata, ChunkVector, SearchResult};
use anyhow::Result;
use hnsw_rs::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// In-memory HNSW index for fast ANN search.
///
/// Uses hnsw-rs with cosine distance. Stores metadata alongside vectors
/// for fast filtering without hitting Cnidarium.
///
/// Thread-safe via RwLock. For production, consider sharding or external service.
pub struct HnswVectorIndex {
    /// The actual HNSW graph (cosine distance, f32 vectors)
    hnsw: Arc<RwLock<Hnsw<'static, f32, DistCosine>>>,
    /// Metadata lookup: chunk_id -> ChunkMetadata
    /// Stored separately because hnsw-rs doesn't have a good payload story
    metadata: Arc<RwLock<HashMap<Uuid, ChunkMetadata>>>,
    /// Bidirectional mapping between UUIDs and HNSW DataIds
    uuid_to_id: Arc<RwLock<HashMap<Uuid, usize>>>,
    id_to_uuid: Arc<RwLock<HashMap<usize, Uuid>>>,
    /// Counter for generating unique DataIds
    next_id: Arc<RwLock<usize>>,
    /// Dimensionality of vectors (must be consistent)
    dimension: usize,
}

impl HnswVectorIndex {
    /// Create a new HNSW index.
    ///
    /// - `dimension`: Vector dimensionality (e.g., 768 for BGE, 1536 for OpenAI ada-002)
    /// - `max_elements`: Estimated max chunks (used for initial allocation)
    /// - `ef_construction`: HNSW build quality (higher = better recall, slower insert)
    /// - `max_nb_connection`: HNSW graph connectivity (higher = better recall, more memory)
    ///
    /// Sane defaults: ef_construction=200, max_nb_connection=24
    pub fn new(
        dimension: usize,
        max_elements: usize,
        ef_construction: usize,
        max_nb_connection: usize,
    ) -> Result<Self> {
        let hnsw = Hnsw::<f32, DistCosine>::new(
            max_nb_connection,
            max_elements,
            16,  // ef_search_base (will be overridden at query time)
            ef_construction,
            DistCosine,
        );

        Ok(Self {
            hnsw: Arc::new(RwLock::new(hnsw)),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            uuid_to_id: Arc::new(RwLock::new(HashMap::new())),
            id_to_uuid: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(0)),
            dimension,
        })
    }

    /// Insert a chunk vector into the index.
    ///
    /// Panics if vector dimension doesn't match index dimension (that's a programming error, not recoverable).
    pub fn insert(&self, chunk: ChunkVector) -> Result<()> {
        if chunk.embedding.len() != self.dimension {
            anyhow::bail!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                chunk.embedding.len()
            );
        }

        // Allocate a new DataId
        let data_id = {
            let mut next_id = self.next_id.write().expect("next_id lock poisoned");
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Insert into HNSW
        self.hnsw
            .write()
            .expect("HNSW lock poisoned")
            .insert((&chunk.embedding, data_id));

        // Store mappings
        self.uuid_to_id
            .write()
            .expect("uuid_to_id lock poisoned")
            .insert(chunk.chunk_id, data_id);
        self.id_to_uuid
            .write()
            .expect("id_to_uuid lock poisoned")
            .insert(data_id, chunk.chunk_id);

        // Store metadata
        self.metadata
            .write()
            .expect("metadata lock poisoned")
            .insert(chunk.chunk_id, chunk.metadata);

        Ok(())
    }

    /// Search for k nearest neighbors.
    ///
    /// - `query_vec`: Query embedding (must match dimension)
    /// - `k`: Number of results
    /// - `ef_search`: HNSW search quality (higher = better recall, slower)
    ///
    /// Returns results sorted by similarity (highest first).
    pub fn search(&self, query_vec: &[f32], k: usize, ef_search: usize) -> Result<Vec<SearchResult>> {
        if query_vec.len() != self.dimension {
            anyhow::bail!(
                "Query vector dimension mismatch: expected {}, got {}",
                self.dimension,
                query_vec.len()
            );
        }

        // Set ef_search for this query
        self.hnsw
            .write()
            .expect("HNSW lock poisoned")
            .set_searching_mode(true);

        // Perform search
        let neighbors = self
            .hnsw
            .read()
            .expect("HNSW lock poisoned")
            .search(query_vec, k, ef_search);

        // Convert to SearchResults
        let id_to_uuid = self.id_to_uuid.read().expect("id_to_uuid lock poisoned");
        let metadata_lock = self.metadata.read().expect("metadata lock poisoned");
        let mut results = Vec::new();

        for neighbor in neighbors {
            if let Some(&chunk_id) = id_to_uuid.get(&neighbor.d_id) {
                if let Some(metadata) = metadata_lock.get(&chunk_id) {
                    results.push(SearchResult {
                        chunk_id,
                        similarity: neighbor.distance, // hnsw-rs returns cosine similarity
                        metadata: metadata.clone(),
                    });
                } else {
                    tracing::warn!("Metadata missing for chunk {}", chunk_id);
                }
            } else {
                tracing::warn!("DataId {} not found in id_to_uuid mapping", neighbor.d_id);
            }
        }

        Ok(results)
    }

    /// Delete a chunk from the index.
    ///
    /// Note: hnsw-rs doesn't support efficient deletion. For production,
    /// you'd need to rebuild periodically or use a different index.
    pub fn delete(&self, chunk_id: Uuid) -> Result<()> {
        // Remove metadata
        self.metadata
            .write()
            .expect("metadata lock poisoned")
            .remove(&chunk_id);

        // hnsw-rs doesn't have a delete operation — the DataId just becomes orphaned
        // in the graph. For a real implementation, you'd need to:
        // 1. Track deleted IDs and filter them in search results, OR
        // 2. Rebuild the index periodically, OR
        // 3. Use a different index that supports deletion (qdrant, LanceDB)

        tracing::warn!("HNSW delete is a no-op (ID orphaned in graph)");
        Ok(())
    }

    /// Get current index size (number of chunks).
    pub fn size(&self) -> usize {
        self.metadata.read().expect("metadata lock poisoned").len()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_insert_and_search() {
        let index = HnswVectorIndex::new(128, 1000, 200, 24).unwrap();

        // Insert a few chunks
        for i in 0..10 {
            let chunk = ChunkVector {
                chunk_id: Uuid::new_v4(),
                embedding: vec![i as f32; 128], // dummy embedding
                metadata: ChunkMetadata {
                    preview: format!("Chunk {}", i),
                    source_type: "test".to_string(),
                    ingested_at: 0,
                    tags: vec![],
                },
            };
            index.insert(chunk).unwrap();
        }

        assert_eq!(index.size(), 10);

        // Search
        let query = vec![5.0; 128];
        let results = index.search(&query, 3, 50).unwrap();
        assert_eq!(results.len(), 3);
    }
}
