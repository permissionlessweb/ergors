// RLM Document Service
//
// Provides document retrieval for RLM (Recursive Language Model) execution.
// Used by embedded Python REPL workers to load guild-specific documents.

use crate::storage::ErgorsStorage;
use anyhow::Result;
use ho_std::types::ergors::orch::v1::{
    rlm_document_service_server::{RlmDocumentService, RlmDocumentServiceServer}, GetDocumentsRequest, GetDocumentsResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error};
use ho_std::types::ergors::orch::v1::Document;

/// RLM Document Service implementation
pub struct RlmDocService {
    storage: Arc<ErgorsStorage>,
}

impl RlmDocService {
    /// Create new RLM document service
    pub fn new(storage: Arc<ErgorsStorage>) -> Self {
        Self { storage }
    }

    /// Create gRPC service server
    pub fn into_server(self) -> RlmDocumentServiceServer<Self> {
        RlmDocumentServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl RlmDocumentService for RlmDocService {
    async fn get_documents(
        &self,
        req: Request<GetDocumentsRequest>,
    ) -> Result<Response<GetDocumentsResponse>, Status> {
        let req = req.into_inner();

        debug!(
            "RLM document request: prefix={}, limit={}",
            req.source_uri_prefix, req.limit
        );

        // Load documents using shared utility (no client reuse in gRPC context)
        let documents = crate::client::load_documents_by_prefix(
            &self.storage,
            &req.source_uri_prefix,
            req.limit as usize,
            None,
        )
        .await
        .map_err(|e| {
            error!("Failed to load documents: {}", e);
            Status::internal(format!("Document loading error: {}", e))
        })?;

        Ok(Response::new(GetDocumentsResponse { documents }))
    }
}

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
        crate::proxy::rag::new_remote_with_client(
            storage,
            client,
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        )?
    } else {
        crate::proxy::rag::new_remote(
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
