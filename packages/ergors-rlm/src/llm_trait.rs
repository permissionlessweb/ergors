//! LLM router trait for dependency injection

use anyhow::Result;
use async_trait::async_trait;
use ho_std::types::ergors::orch::v1::{PromptRequest, PromptResponse};

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
