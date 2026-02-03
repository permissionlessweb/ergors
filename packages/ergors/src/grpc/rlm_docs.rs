//! RLM Document Service
//!
//! Provides document retrieval for RLM (Recursive Language Model) execution.
//! Used by embedded Python REPL workers to load guild-specific documents.

use crate::storage::ErgorsStorage;
use anyhow::Result;
use ho_std::types::ergors::orch::v1::{
    rlm_document_service_server::{RlmDocumentService, RlmDocumentServiceServer}, GetDocumentsRequest, GetDocumentsResponse,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error};

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
        let documents = crate::grpc::load_documents_by_prefix(
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
