//! Embedded Python REPL service for RLM (Recursive Language Model) execution.
//!
//! Manages a pool of Python subprocess workers for isolated code execution.

use anyhow::Result;
use ho_std::types::ergors::orch::v1::Document as ProtoDocument;
use std::sync::Arc;
use tracing::{debug, info};

mod llm_trait;
pub mod pool;
pub mod process;
pub mod types;

pub use llm_trait::{DocumentAccessTrait, LlmRouterTrait};

pub use types::{Document, DocumentExcerpt, DocumentMeta, RlmQuery, RlmResponse};

use pool::ReplPool;

/// RLM service with subprocess pool for REPL execution
pub struct RlmService {
    pool: Arc<ReplPool>,
    router: Arc<dyn LlmRouterTrait>,
    docs: Option<Arc<dyn DocumentAccessTrait>>,
}

impl RlmService {
    /// Create new RLM service with subprocess pool
    pub async fn new(
        pool_size: usize,
        router: Arc<dyn LlmRouterTrait>,
        docs: Option<Arc<dyn DocumentAccessTrait>>,
    ) -> Result<Self> {
        info!("Initializing RLM service with pool size {}", pool_size);

        let pool = ReplPool::new(pool_size).await?;

        Ok(Self {
            pool: Arc::new(pool),
            router,
            docs,
        })
    }

    /// Execute RLM query with provided documents
    pub async fn query(&self, query: RlmQuery, documents: Vec<Document>) -> Result<RlmResponse> {
        debug!(
            "RLM query: {} ({} documents)",
            query.query,
            documents.len()
        );

        // Acquire worker from pool
        let worker = self.pool.acquire().await?;

        // Execute in subprocess
        let result = worker
            .execute(query, documents, Arc::clone(&self.router), self.docs.clone())
            .await;

        // Only return worker to pool if execution succeeded
        // On error, drop the worker (process may be dead)
        match result {
            Ok(response) => {
                self.pool.release(worker).await;
                Ok(response)
            }
            Err(e) => {
                debug!("Worker {} failed, dropping instead of recycling", worker.id());
                // Worker is dropped here, process is killed
                Err(e)
            }
        }
    }

    /// Get maximum pool size (configured capacity, not current available workers)
    pub async fn max_pool_size(&self) -> usize {
        self.pool.max_size().await
    }
}

/// Convert proto Document to RLM Document
impl From<ProtoDocument> for Document {
    fn from(proto: ProtoDocument) -> Self {
        Self {
            source_uri: proto.source_uri,
            content: proto.content,
            doc_type: proto.doc_type,
            tags: proto.tags,
            ingested_at: proto.ingested_at,
        }
    }
}

/// Convert RLM Document to proto Document
impl From<Document> for ProtoDocument {
    fn from(doc: Document) -> Self {
        Self {
            source_uri: doc.source_uri,
            content: doc.content,
            doc_type: doc.doc_type,
            tags: doc.tags,
            ingested_at: doc.ingested_at,
        }
    }
}

// Tests are in tests/test_integration.rs
