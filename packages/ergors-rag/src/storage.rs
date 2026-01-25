//! Cnidarium storage integration for verifiable chunk records.
//!
//! Uses Cnidarium with prefix "rag_chunks/" for verifiable chunk storage.
//! Secondary index "rag_source_index/" maps source_uri -> Vec<chunk_id>.

use crate::types::VerifiableChunk;
use anyhow::{Context, Result};
use cnidarium::{StateDelta, StateRead, StateWrite, Storage};
use std::sync::Arc;
use uuid::Uuid;

/// Storage prefix for chunk records: "rag_chunks/{chunk_id}"
const CHUNK_PREFIX: &str = "rag_chunks";

/// Storage prefix for source index: "rag_source_index/{source_uri}"
const SOURCE_INDEX_PREFIX: &str = "rag_source_index";

/// Cnidarium storage adapter for RAG chunks.
pub struct RagStorage {
    storage: Arc<Storage>,
}

impl RagStorage {
    /// Create a new RagStorage wrapping a Cnidarium Storage instance.
    ///
    /// NOTE: The Storage must be initialized with "rag_chunks" and
    /// "rag_source_index" prefixes.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Store a single verifiable chunk (atomic commit).
    ///
    /// For ingesting multiple chunks, use `put_chunks_batch` instead (much faster).
    pub async fn put_chunk(&self, chunk: &VerifiableChunk) -> Result<()> {
        self.put_chunks_batch(&[chunk.clone()]).await
    }

    /// Store multiple verifiable chunks in a single atomic commit.
    ///
    /// This is MUCH faster than calling `put_chunk` in a loop (10-100x speedup
    /// for large batches) because it does a single Cnidarium commit instead of N.
    ///
    /// Source index is updated correctly for all chunks.
    pub async fn put_chunks_batch(&self, chunks: &[VerifiableChunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let snapshot = self.storage.latest_snapshot();
        let mut delta = StateDelta::new(snapshot);

        // Group chunks by source_uri for efficient source index updates
        let mut chunks_by_source: std::collections::HashMap<&str, Vec<Uuid>> =
            std::collections::HashMap::new();

        for chunk in chunks {
            // Serialize and store chunk
            let chunk_bytes = bincode::serialize(chunk)
                .context("Failed to serialize VerifiableChunk")?;
            let key = format!("{}/{}", CHUNK_PREFIX, chunk.chunk_id);
            delta.put_raw(key, chunk_bytes);

            // Track for source index update
            chunks_by_source
                .entry(&chunk.source_uri)
                .or_default()
                .push(chunk.chunk_id);
        }

        // Update source indexes
        for (source_uri, chunk_ids) in chunks_by_source {
            let source_key = format!("{}/{}", SOURCE_INDEX_PREFIX, source_uri);
            let mut existing_ids: Vec<Uuid> = match delta.get_raw(&source_key).await? {
                Some(bytes) => bincode::deserialize(&bytes)
                    .context("Failed to deserialize source index")?,
                None => Vec::new(),
            };

            // Append new chunk IDs (avoid duplicates)
            for &chunk_id in &chunk_ids {
                if !existing_ids.contains(&chunk_id) {
                    existing_ids.push(chunk_id);
                }
            }

            let index_bytes = bincode::serialize(&existing_ids)
                .context("Failed to serialize source index")?;
            delta.put_raw(source_key, index_bytes);
        }

        // Single atomic commit for all chunks
        self.storage.commit(delta).await?;

        Ok(())
    }

    /// Retrieve a verifiable chunk by ID.
    pub async fn get_chunk(&self, chunk_id: Uuid) -> Result<Option<VerifiableChunk>> {
        let snapshot = self.storage.latest_snapshot();
        let key = format!("{}/{}", CHUNK_PREFIX, chunk_id);

        match snapshot.get_raw(&key).await? {
            Some(bytes) => {
                let chunk: VerifiableChunk = bincode::deserialize(&bytes)
                    .context("Failed to deserialize VerifiableChunk")?;
                Ok(Some(chunk))
            }
            None => Ok(None),
        }
    }

    /// Delete a chunk (atomic commit).
    ///
    /// Also removes from source index.
    pub async fn delete_chunk(&self, chunk_id: Uuid) -> Result<()> {
        let snapshot = self.storage.latest_snapshot();
        let mut delta = StateDelta::new(snapshot);

        // Get chunk to find its source_uri
        let key = format!("{}/{}", CHUNK_PREFIX, chunk_id);
        let chunk_bytes = match delta.get_raw(&key).await? {
            Some(bytes) => bytes,
            None => return Ok(()), // Already deleted
        };

        let chunk: VerifiableChunk = bincode::deserialize(&chunk_bytes)
            .context("Failed to deserialize chunk")?;

        // Remove from chunk store
        delta.delete(key);

        // Remove from source index
        let source_key = format!("{}/{}", SOURCE_INDEX_PREFIX, chunk.source_uri);
        if let Some(bytes) = delta.get_raw(&source_key).await? {
            let mut chunk_ids: Vec<Uuid> = bincode::deserialize(&bytes)
                .unwrap_or_default();
            chunk_ids.retain(|&id| id != chunk_id);

            if chunk_ids.is_empty() {
                delta.delete(source_key);
            } else {
                let index_bytes = bincode::serialize(&chunk_ids)?;
                delta.put_raw(source_key, index_bytes);
            }
        }

        // Commit
        self.storage.commit(delta).await?;

        Ok(())
    }

    /// Get all chunks from a given source URI.
    pub async fn get_chunks_by_source(&self, source_uri: &str) -> Result<Vec<VerifiableChunk>> {
        let snapshot = self.storage.latest_snapshot();
        let source_key = format!("{}/{}", SOURCE_INDEX_PREFIX, source_uri);

        let chunk_ids: Vec<Uuid> = match snapshot.get_raw(&source_key).await? {
            Some(bytes) => bincode::deserialize(&bytes)
                .context("Failed to deserialize source index")?,
            None => return Ok(Vec::new()),
        };

        let mut chunks = Vec::new();
        for chunk_id in chunk_ids {
            if let Some(chunk) = self.get_chunk(chunk_id).await? {
                chunks.push(chunk);
            }
        }

        Ok(chunks)
    }

    /// List all chunks (expensive, for admin/debugging only).
    pub async fn list_all_chunks(&self) -> Result<Vec<VerifiableChunk>> {
        let snapshot = self.storage.latest_snapshot();
        let prefix = format!("{}/", CHUNK_PREFIX);

        let mut chunks = Vec::new();
        let mut stream = snapshot.prefix_raw(&prefix);

        use futures::StreamExt; // for next() on stream
        while let Some(result) = stream.next().await {
            match result {
                Ok((_, value)) => {
                    if let Ok(chunk) = bincode::deserialize::<VerifiableChunk>(&value) {
                        chunks.push(chunk);
                    }
                }
                Err(e) => tracing::warn!("Error reading chunk from prefix scan: {}", e),
            }
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_storage() -> (Arc<Storage>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_path).unwrap();

        let prefixes = vec![
            CHUNK_PREFIX.to_string(),
            SOURCE_INDEX_PREFIX.to_string(),
        ];

        let storage = Storage::load(storage_path, prefixes)
            .await
            .expect("Failed to create Storage");

        (Arc::new(storage), temp_dir)
    }

    #[tokio::test]
    async fn test_chunk_crud() {
        let (storage, _temp) = setup_test_storage().await;
        let rag_storage = RagStorage::new(storage);

        let chunk = VerifiableChunk {
            chunk_id: Uuid::new_v4(),
            content: "Test content".to_string(),
            content_hash: [0u8; 32],
            embedding_hash: [1u8; 32],
            version: 0,
            ingested_at: pbjson_types::Timestamp { seconds: 0, nanos: 0 },
            source_uri: "test://doc".to_string(),
            uploader_id: None,
            access_policy: None,
            commit_ref: None,
            previous_version: None,
        };

        // Put
        rag_storage.put_chunk(&chunk).await.unwrap();

        // Get
        let retrieved = rag_storage.get_chunk(chunk.chunk_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "Test content");

        // Delete
        rag_storage.delete_chunk(chunk.chunk_id).await.unwrap();
        let after_delete = rag_storage.get_chunk(chunk.chunk_id).await.unwrap();
        assert!(after_delete.is_none());
    }

    #[tokio::test]
    async fn test_source_index() {
        let (storage, _temp) = setup_test_storage().await;
        let rag_storage = RagStorage::new(storage);

        let source_uri = "test://multi-chunk-doc";

        // Create multiple chunks from same source
        for i in 0..3 {
            let chunk = VerifiableChunk {
                chunk_id: Uuid::new_v4(),
                content: format!("Chunk {}", i),
                content_hash: [i as u8; 32],
                embedding_hash: [i as u8; 32],
                version: 0,
                ingested_at: pbjson_types::Timestamp { seconds: 0, nanos: 0 },
                source_uri: source_uri.to_string(),
                uploader_id: None,
                access_policy: None,
                commit_ref: None,
                previous_version: None,
            };
            rag_storage.put_chunk(&chunk).await.unwrap();
        }

        // Query by source
        let chunks = rag_storage.get_chunks_by_source(source_uri).await.unwrap();
        assert_eq!(chunks.len(), 3);
    }

    #[tokio::test]
    async fn test_batch_storage() {
        let (storage, _temp) = setup_test_storage().await;
        let rag_storage = RagStorage::new(storage);

        // Create multiple chunks
        let chunks: Vec<VerifiableChunk> = (0..10)
            .map(|i| VerifiableChunk {
                chunk_id: Uuid::new_v4(),
                content: format!("Chunk {}", i),
                content_hash: [i as u8; 32],
                embedding_hash: [i as u8; 32],
                version: 0,
                ingested_at: pbjson_types::Timestamp {
                    seconds: i as i64,
                    nanos: 0,
                },
                source_uri: "test://batch-doc".to_string(),
                uploader_id: None,
                access_policy: None,
                commit_ref: None,
                previous_version: None,
            })
            .collect();

        // Batch insert (single commit)
        rag_storage.put_chunks_batch(&chunks).await.unwrap();

        // Verify all chunks are stored
        for chunk in &chunks {
            let retrieved = rag_storage.get_chunk(chunk.chunk_id).await.unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().content, chunk.content);
        }

        // Verify source index is updated correctly
        let source_chunks = rag_storage
            .get_chunks_by_source("test://batch-doc")
            .await
            .unwrap();
        assert_eq!(source_chunks.len(), 10);
    }
}
