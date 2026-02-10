//! LLM router trait and document access trait for dependency injection

use anyhow::Result;
use async_trait::async_trait;
use ho_std::types::ergors::orch::v1::{PromptRequest, PromptResponse};

use crate::types::{DocumentExcerpt, DocumentMeta};

/// Minimal trait for LLM routing used by RLM service
#[async_trait]
pub trait LlmRouterTrait: Send + Sync {
    /// Handle a prompt request and return a response
    async fn handle_request(&self, req: &PromptRequest, model: &str) -> Result<PromptResponse>;
}

/// Blanket implementation for Arc<LlmRouter>
#[async_trait]
impl LlmRouterTrait for ho_std::llm::LlmRouter {
    async fn handle_request(&self, req: &PromptRequest, model: &str) -> Result<PromptResponse> {
        ho_std::llm::LlmRouter::handle_request(self, req, model)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}

/// Trait for accessing stored documents from the RLM REPL.
/// Documents are accessed by ID — full content stays in Rust.
#[async_trait]
pub trait DocumentAccessTrait: Send + Sync {
    /// List available documents (metadata only, no content).
    /// `limit`: max number of results (capped at 100). `offset`: skip first N results.
    async fn list_documents(&self, limit: usize, offset: usize) -> Result<Vec<DocumentMeta>>;

    /// Get a section of a document by char range.
    async fn get_document_section(&self, doc_id: &str, offset: usize, length: usize) -> Result<String>;

    /// Search within a document for keyword matches. Returns excerpts with surrounding context.
    async fn search_in_document(&self, doc_id: &str, query: &str, max_results: usize) -> Result<Vec<DocumentExcerpt>>;
}
