//! Shared document loading utilities for RLM and other services

use crate::storage::ErgorsStorage;
use anyhow::Result;
use ho_std::types::ergors::orch::v1::Document;
use std::sync::Arc;
use tracing::{debug, error};

/// Load documents from storage by source URI prefix
///
/// Optionally accepts a reqwest::Client for connection pooling.
/// If None, creates a new client (less efficient for repeated calls).
pub async fn load_documents_by_prefix(
    storage: &Arc<ErgorsStorage>,
    prefix: &str,
    limit: usize,
    client: Option<reqwest::Client>,
) -> Result<Vec<Document>> {
    // Query sources from storage
    let sources = storage
        .list_rag_sources_by_prefix(prefix, limit)
        .await?;

    // Get RAG config to create RAG instance
    let rag_config = storage
        .get_rag_config()
        .await?
        .ok_or_else(|| anyhow::anyhow!("RAG not configured"))?;

    // Create RAG instance (with client reuse if provided)
    let rag = if let Some(client) = client {
        crate::rag::new_remote_with_client(
            storage,
            client,
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        )?
    } else {
        crate::rag::new_remote(
            storage,
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        )?
    };

    let mut documents = Vec::new();

    for source in sources {
        let chunks = match rag.get_chunks_by_source(&source.uri).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get RAG chunks for {}: {}", source.uri, e);
                continue;
            }
        };

        if chunks.is_empty() {
            continue;
        }

        // Combine chunks into full document
        let content = chunks
            .iter()
            .map(|c| c.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        // Convert Timestamp to i64
        let ingested_at = chunks[0].ingested_at.seconds;

        documents.push(Document {
            source_uri: source.uri,
            content,
            doc_type: "text/plain".to_string(),
            tags: vec![],
            ingested_at,
        });
    }

    debug!("Loaded {} documents with prefix: {}", documents.len(), prefix);

    Ok(documents)
}
