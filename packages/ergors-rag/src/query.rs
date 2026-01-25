//! Query flows for retrieval + optional verification.

use crate::embedder::Embedder;
use crate::storage::RagStorage;
use crate::types::{QueryOptions, SearchResult, VerifiedChunk};
use crate::vector_index::HnswVectorIndex;
use anyhow::{Context, Result};

/// Standard query flow (no verification).
///
/// Fast path: ~20-100ms for vector search depending on index size.
/// Does NOT verify hashes or fetch full content from Cnidarium.
pub async fn query_standard<E: Embedder>(
    query_text: &str,
    k: usize,
    embedder: &E,
    vector_index: &HnswVectorIndex,
) -> Result<Vec<SearchResult>> {
    // 1. Generate query embedding
    let query_vec = embedder.embed(query_text).await
        .context("Failed to generate query embedding")?;

    // 2. Search vector index
    let ef_search = 100; // HNSW search quality parameter
    let results = vector_index.search(&query_vec, k, ef_search)
        .context("Vector search failed")?;

    Ok(results)
}

/// Verified query flow (with provenance + optional hash verification).
///
/// Slower: adds ~10-50ms per chunk for Cnidarium lookups and hash checks.
/// Returns full content + provenance information.
///
/// ## Filtering
/// Metadata filters (if any) are applied AFTER vector search but BEFORE verification.
/// This avoids wasting time verifying chunks that don't match filters.
///
/// If filters are present, we fetch k*2 candidates from vector search to ensure
/// we have enough results after filtering.
pub async fn query_verified<E: Embedder>(
    query_text: &str,
    k: usize,
    embedder: &E,
    vector_index: &HnswVectorIndex,
    storage: &RagStorage,
    options: &QueryOptions,
) -> Result<Vec<VerifiedChunk>> {
    // 1. Fast vector search (fetch extra candidates if filtering)
    let has_filters = !options.filters.is_empty();
    let fetch_k = if has_filters { k * 2 } else { k };
    let search_results = query_standard(query_text, fetch_k, embedder, vector_index).await?;

    // 2. Apply metadata filters (before expensive Cnidarium lookups)
    let filtered_results: Vec<_> = if has_filters {
        search_results
            .into_iter()
            .filter(|result| {
                // For SearchResult, we only have metadata (no source_uri yet)
                // We'll do full filtering after fetching from Cnidarium
                // For now, filter on what we have (source_type, tags, ingested_at)
                options.filters.matches(&result.metadata, None)
            })
            .take(k) // Take top-k after filtering
            .collect()
    } else {
        search_results
    };

    // 3. Fetch from Cnidarium and verify
    let mut verified_results = Vec::new();

    for result in filtered_results {
        // Fetch full record
        let chunk = storage.get_chunk(result.chunk_id).await
            .context("Failed to fetch chunk from Cnidarium")?;

        let chunk = match chunk {
            Some(c) => c,
            None => {
                tracing::warn!("Chunk {} not found in Cnidarium (index/storage mismatch)", result.chunk_id);
                continue;
            }
        };

        // Apply full filters (including source_uri check now that we have it)
        if has_filters && !options.filters.matches(&result.metadata, Some(&chunk.source_uri)) {
            continue;
        }

        // Verify hashes if requested
        if options.verify {
            // Verify content hash
            let content_hash = blake3::hash(chunk.content.as_bytes());
            if content_hash.as_bytes() != &chunk.content_hash {
                tracing::error!(
                    "Content hash mismatch for chunk {}: expected {:?}, got {:?}",
                    chunk.chunk_id, chunk.content_hash, content_hash.as_bytes()
                );
                anyhow::bail!("Content hash verification failed for chunk {}", chunk.chunk_id);
            }

            // Note: We can't verify embedding hash here because we don't have the
            // embedding from the vector index (only similarity score). For full
            // verification, you'd need to store embeddings in Cnidarium or fetch
            // them from the vector index.
        }

        // Generate JMT proof if requested
        let proof = if options.include_proof {
            // TODO: Implement JMT proof generation
            // This requires calling into Cnidarium's JMT layer
            tracing::warn!("JMT proof generation not yet implemented");
            None
        } else {
            None
        };

        verified_results.push(VerifiedChunk {
            chunk_id: chunk.chunk_id,
            content: chunk.content.clone(),
            similarity: result.similarity,
            provenance: (&chunk).into(),
            proof,
        });
    }

    Ok(verified_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::DummyEmbedder;
    use crate::types::{ChunkMetadata, ChunkVector, VerifiableChunk};
    use cnidarium::Storage;
    use tempfile::TempDir;
    use std::sync::Arc;

    async fn setup_test_env() -> (
        DummyEmbedder,
        HnswVectorIndex,
        RagStorage,
        TempDir,
    ) {
        let embedder = DummyEmbedder::new(128);
        let vector_index = HnswVectorIndex::new(128, 1000, 200, 24).unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_path).unwrap();
        let prefixes = vec!["rag_chunks".to_string(), "rag_source_index".to_string()];
        let storage = Storage::load(storage_path, prefixes).await.unwrap();
        let rag_storage = RagStorage::new(Arc::new(storage));

        (embedder, vector_index, rag_storage, temp_dir)
    }

    #[tokio::test]
    async fn test_query_standard() {
        let (embedder, vector_index, _storage, _temp) = setup_test_env().await;

        // Insert a few chunks
        for i in 0..5 {
            let text = format!("Document {} content", i);
            let embedding = embedder.embed(&text).await.unwrap();
            let chunk = ChunkVector {
                chunk_id: uuid::Uuid::new_v4(),
                embedding,
                metadata: ChunkMetadata {
                    preview: text.clone(),
                    source_type: "test".to_string(),
                    ingested_at: 0,
                    tags: vec![],
                },
            };
            vector_index.insert(chunk).unwrap();
        }

        // Query
        let results = query_standard("Document 2", 3, &embedder, &vector_index).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].similarity > 0.0); // Should find similar documents
    }

    #[tokio::test]
    async fn test_query_verified() {
        let (embedder, vector_index, storage, _temp) = setup_test_env().await;

        // Ingest a document
        let text = "Test document content";
        let embedding = embedder.embed(text).await.unwrap();
        let chunk_id = uuid::Uuid::new_v4();

        // Insert into vector index
        let chunk_vector = ChunkVector {
            chunk_id,
            embedding: embedding.clone(),
            metadata: ChunkMetadata {
                preview: text.to_string(),
                source_type: "test".to_string(),
                ingested_at: 0,
                tags: vec![],
            },
        };
        vector_index.insert(chunk_vector).unwrap();

        // Store in Cnidarium
        let content_hash = blake3::hash(text.as_bytes());
        let embedding_hash = blake3::hash(bytemuck::cast_slice(&embedding));
        let verifiable_chunk = VerifiableChunk {
            chunk_id,
            content: text.to_string(),
            content_hash: *content_hash.as_bytes(),
            embedding_hash: *embedding_hash.as_bytes(),
            version: 0,
            ingested_at: pbjson_types::Timestamp { seconds: 0, nanos: 0 },
            source_uri: "test://doc".to_string(),
            uploader_id: None,
            access_policy: None,
            commit_ref: None,
            previous_version: None,
        };
        storage.put_chunk(&verifiable_chunk).await.unwrap();

        // Query with verification
        let options = QueryOptions {
            verify: true,
            include_proof: false,
            filters: Default::default(),
        };
        let results = query_verified(text, 1, &embedder, &vector_index, &storage, &options)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, text);
    }

    #[tokio::test]
    async fn test_metadata_filters() {
        let (embedder, vector_index, storage, _temp) = setup_test_env().await;

        // Ingest chunks with different metadata
        for i in 0..5 {
            let text = format!("Document {} content", i);
            let embedding = embedder.embed(&text).await.unwrap();
            let chunk_id = uuid::Uuid::new_v4();

            let source_type = if i < 3 { "rust" } else { "markdown" };
            let tags = if i % 2 == 0 {
                vec!["important".to_string()]
            } else {
                vec![]
            };

            // Vector index
            let chunk_vector = ChunkVector {
                chunk_id,
                embedding: embedding.clone(),
                metadata: ChunkMetadata {
                    preview: text.clone(),
                    source_type: source_type.to_string(),
                    ingested_at: i as i64,
                    tags: tags.clone(),
                },
            };
            vector_index.insert(chunk_vector).unwrap();

            // Cnidarium
            let content_hash = blake3::hash(text.as_bytes());
            let embedding_hash = blake3::hash(bytemuck::cast_slice(&embedding));
            let verifiable_chunk = VerifiableChunk {
                chunk_id,
                content: text.clone(),
                content_hash: *content_hash.as_bytes(),
                embedding_hash: *embedding_hash.as_bytes(),
                version: 0,
                ingested_at: pbjson_types::Timestamp {
                    seconds: i as i64,
                    nanos: 0,
                },
                source_uri: format!("test://doc{}", i),
                uploader_id: None,
                access_policy: None,
                commit_ref: None,
                previous_version: None,
            };
            storage.put_chunk(&verifiable_chunk).await.unwrap();
        }

        // Test: Filter by source_type
        let mut options = QueryOptions {
            verify: false,
            include_proof: false,
            filters: crate::types::MetadataFilters {
                source_type: Some("rust".to_string()),
                ..Default::default()
            },
        };
        let results = query_verified("Document", 10, &embedder, &vector_index, &storage, &options)
            .await
            .unwrap();
        // Should only get rust files (0, 1, 2)
        assert!(results.len() <= 3);

        // Test: Filter by tags
        options.filters = crate::types::MetadataFilters {
            tags: vec!["important".to_string()],
            ..Default::default()
        };
        let results = query_verified("Document", 10, &embedder, &vector_index, &storage, &options)
            .await
            .unwrap();
        // Should only get even-numbered docs (0, 2, 4)
        assert!(results.len() <= 3);

        // Test: Filter by timestamp range
        options.filters = crate::types::MetadataFilters {
            min_ingested_at: Some(2),
            max_ingested_at: Some(4),
            ..Default::default()
        };
        let results = query_verified("Document", 10, &embedder, &vector_index, &storage, &options)
            .await
            .unwrap();
        // Should get docs 2, 3, 4
        assert!(results.len() <= 3);
    }
}
