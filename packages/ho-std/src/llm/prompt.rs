// Re-export extension traits that were previously defined
pub use crate::error::{HoError, HoResult};

// Re-export error types
// Re-export shared implementations
// pub use crate::shared_impl::*;
// Extension trait implementations for proto types
use crate::traits::llm::*;
use crate::types::cw_ho::orchestration::v1::*;

impl PromptRequestTrait for PromptRequest {
    type Message = PromptMessage;
    type Context = PromptContext;
    type Config = LlmPromptConfig;

    fn messages(&self) -> &[PromptMessage] {
        &self.messages
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn context(&self) -> Option<&PromptContext> {
        self.context.as_ref()
    }

    fn llm_config(&self) -> Option<&Self::Config> {
        self.llm_config.as_ref()
    }

    fn add_message(&mut self, message: Self::Message) {
        self.messages.push(message);
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn set_context(&mut self, context: Self::Context) {
        self.context = Some(context);
    }
}

impl PromptResponseTrait for PromptResponse {
    type TokenUsage = TokenUsage;
    type Context = PromptContext;
    type Timestamp = crate::shim::Timestamp;

    fn id(&self) -> &Vec<u8> {
        &self.id
    }

    fn provider(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn prompt(&self) -> &str {
        &self.prompt
    }

    fn response(&self) -> &str {
        &self.response
    }

    fn tokens_used(&self) -> &TokenUsage {
        self.tokens_used.as_ref().unwrap_or(&TokenUsage {
            prompt: 0,
            completion: 0,
            total: 0,
        })
    }

    fn cost(&self) -> Option<f64> {
        self.cost
    }

    fn latency_ms(&self) -> Option<u64> {
        self.latency_ms
    }

    fn timestamp(&self) -> &Self::Timestamp {
        static DEFAULT_TIMESTAMP: std::sync::LazyLock<crate::shim::Timestamp> =
            std::sync::LazyLock::new(|| crate::shim::Timestamp::default());
        self.timestamp.as_ref().unwrap_or(&DEFAULT_TIMESTAMP)
    }

    // fn context(&self) -> Option<&Self::Context> {
    //     self.cas_ref()
    // }

    fn set_response(&mut self, response: String) {
        self.response = response;
    }

    fn set_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    fn set_latency(&mut self, latency_ms: u64) {
        self.latency_ms = Some(latency_ms);
    }
}
