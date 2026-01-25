//! Document ingestion pipeline.
//!
//! Takes documents, chunks them, generates embeddings, and stores in both
//! the vector index (fast retrieval) and Cnidarium (provenance).

use crate::embedder::Embedder;
use crate::storage::RagStorage;
use crate::types::{ChunkMetadata, ChunkVector, Document, VerifiableChunk};
use crate::vector_index::HnswVectorIndex;
use anyhow::{Context, Result};
use uuid::Uuid;

/// Simple text chunker that splits on paragraphs/sentences.
///
/// For production, use more sophisticated strategies:
/// - Sliding window with overlap
/// - Semantic chunking (split on topic boundaries)
/// - Language-specific tokenization
///
/// This MVP implementation just splits on double newlines and truncates.
pub fn chunk_text(text: &str, max_chunk_size: usize) -> Vec<String> {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks = Vec::new();

    for para in paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if para.len() <= max_chunk_size {
            chunks.push(para.to_string());
        } else {
            // Split large paragraphs by sentences (rough heuristic)
            let sentences: Vec<&str> = para.split(". ").collect();
            let mut current_chunk = String::new();

            for sentence in sentences {
                if current_chunk.len() + sentence.len() + 2 > max_chunk_size
                    && !current_chunk.is_empty() {
                        chunks.push(current_chunk.clone());
                        current_chunk.clear();
                    }
                if !current_chunk.is_empty() {
                    current_chunk.push_str(". ");
                }
                current_chunk.push_str(sentence);
            }

            if !current_chunk.is_empty() {
                chunks.push(current_chunk);
            }
        }
    }

    chunks
}

/// Ingest a document into the RAG system.
///
/// Steps:
/// 1. Chunk the document text
/// 2. Generate embeddings (batched for efficiency)
/// 3. Insert into vector index
/// 4. Store verifiable records in Cnidarium
///
/// Returns the chunk IDs that were created.
pub async fn ingest_document<E: Embedder>(
    doc: Document,
    embedder: &E,
    vector_index: &HnswVectorIndex,
    storage: &RagStorage,
    uploader_id: Option<String>,
) -> Result<Vec<Uuid>> {
    tracing::info!("Ingesting document: {} ({})", doc.uri, doc.doc_type);

    // 1. Chunk text
    let chunks = chunk_text(&doc.content, 1000); // 1000 char chunks
    if chunks.is_empty() {
        tracing::warn!("Document produced no chunks: {}", doc.uri);
        return Ok(Vec::new());
    }

    tracing::debug!("Split document into {} chunks", chunks.len());

    // 2. Generate embeddings (batch)
    let chunk_refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
    let embeddings = embedder.embed_batch(&chunk_refs).await
        .context("Failed to generate embeddings")?;

    if embeddings.len() != chunks.len() {
        anyhow::bail!(
            "Embedding count mismatch: {} embeddings for {} chunks",
            embeddings.len(),
            chunks.len()
        );
    }

    // 3. Prepare all chunks and insert into vector index (in-memory, fast)
    let now = chrono::Utc::now();
    let timestamp = pbjson_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    };

    let mut chunk_ids = Vec::new();
    let mut verifiable_chunks = Vec::new();

    for (chunk_text, embedding) in chunks.iter().zip(embeddings.iter()) {
        let chunk_id = Uuid::new_v4();

        // Compute hashes
        let content_hash = blake3::hash(chunk_text.as_bytes());
        let embedding_hash = blake3::hash(bytemuck::cast_slice(embedding));

        // Create preview (first 200 chars)
        let preview = if chunk_text.len() > 200 {
            format!("{}...", &chunk_text[..200])
        } else {
            chunk_text.clone()
        };

        // Insert into vector index (in-memory, no blocking I/O)
        let chunk_vector = ChunkVector {
            chunk_id,
            embedding: embedding.clone(),
            metadata: ChunkMetadata {
                preview: preview.clone(),
                source_type: doc.doc_type.clone(),
                ingested_at: now.timestamp(),
                tags: doc.tags.clone(),
            },
        };
        vector_index
            .insert(chunk_vector)
            .context("Failed to insert into vector index")?;

        // Prepare for batch Cnidarium insert
        verifiable_chunks.push(VerifiableChunk {
            chunk_id,
            content: chunk_text.clone(),
            content_hash: *content_hash.as_bytes(),
            embedding_hash: *embedding_hash.as_bytes(),
            version: 0,
            ingested_at: timestamp,
            source_uri: doc.uri.clone(),
            uploader_id: uploader_id.clone(),
            access_policy: None,
            commit_ref: None,
            previous_version: None,
        });

        chunk_ids.push(chunk_id);
    }

    // 4. Batch commit to Cnidarium (single atomic transaction)
    storage
        .put_chunks_batch(&verifiable_chunks)
        .await
        .context("Failed to store chunks in Cnidarium")?;

    tracing::info!(
        "Successfully ingested {} chunks from {} (batch mode)",
        chunk_ids.len(),
        doc.uri
    );
    Ok(chunk_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let chunks = chunk_text(text, 100);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "First paragraph.");
        assert_eq!(chunks[1], "Second paragraph.");
        assert_eq!(chunks[2], "Third paragraph.");
    }

    #[test]
    fn test_chunk_text_long_paragraph() {
        // Test with realistic text (sentences, not just "AAAA...")
        let sentences = (0..20)
            .map(|i| format!("Sentence {} with some content", i))
            .collect::<Vec<_>>()
            .join(". ");
        let chunks = chunk_text(&sentences, 100);

        // Should produce multiple chunks
        assert!(chunks.len() >= 2);

        // Each chunk should be under max size (with some tolerance for sentence boundaries)
        for chunk in &chunks {
            assert!(chunk.len() <= 150); // Allow some overage for sentence boundaries
        }
    }
}
