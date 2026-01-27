// Re-export extension traits that were previously defined
pub use crate::error::{HoError, HoResult};

// Re-export error types
// Re-export shared implementations
// pub use crate::shared_impl::*;
// Extension trait implementations for proto types
use crate::traits::*;
use crate::types::ergors::orch::v1::*;

impl PromptRequestTrait for PromptRequest {
    type Message = PromptMessage;
    type Context = PromptContext;

    fn messages(&self) -> &[PromptMessage] {
        &self.messages
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn context(&self) -> Option<&PromptContext> {
        self.context.as_ref()
    }

    fn add_message(&mut self, message: Self::Message) {
        self.messages.push(message);
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }
}

impl LlmMessageTrait for PromptMessage {
    type Message = PromptMessage;
    type Context = PromptContext;
    type Config = LlmPromptConfig;

    fn role(&self) -> &str {
        &self.role
    }

    fn content(&self) -> &str {
        &self.content
    }

    fn set_role(&mut self, role: String) {
        self.role = role;
    }

    fn set_content(&mut self, content: String) {
        self.content = content;
    }

    fn user_message(content: String) -> Self {
        PromptMessage {
            role: "user".to_string(),
            content,
            tool_calls: vec![],
            tool_result: None,
            content_blocks: vec![],
        }
    }

    fn assistant_message(content: String) -> Self {
        PromptMessage {
            role: "assistant".to_string(),
            content,
            tool_calls: vec![],
            tool_result: None,
            content_blocks: vec![],
        }
    }

    fn system_message(content: String) -> Self {
        PromptMessage {
            role: "system".to_string(),
            content,
            tool_calls: vec![],
            tool_result: None,
            content_blocks: vec![],
        }
    }

    // Request-level methods that don't apply to single messages
    fn messages(&self) -> &[Self::Message] {
        std::slice::from_ref(self)
    }

    fn model(&self) -> &str {
        ""
    }

    fn context(&self) -> Option<&Self::Context> {
        None
    }

    fn llm_config(&self) -> Option<&Self::Config> {
        None
    }

    fn add_message(&mut self, _message: Self::Message) {
        // Single message can't add messages
    }

    fn set_model(&mut self, _model: String) {
        // Single message doesn't have model
    }

    fn set_context(&mut self, _context: Self::Context) {
        // Single message doesn't have context
    }
}

impl PromptResponseTrait for PromptResponse {
    type TokenUsage = TokenUsage;
    type Context = PromptContext;
    type Timestamp = pbjson_types::Timestamp;

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

    fn tokens_used(&self) -> &TokenUsage {
        self.tokens_used.as_ref().unwrap_or(&TokenUsage {
            prompt: 0,
            completion: 0,
            total: 0,
        })
    }

    fn cost(&self) -> f64 {
        self.cost
    }

    fn latency_ms(&self) -> u64 {
        self.latency_ms
    }

    fn timestamp(&self) -> &Self::Timestamp {
        static DEFAULT_TIMESTAMP: std::sync::LazyLock<pbjson_types::Timestamp> =
            std::sync::LazyLock::new(pbjson_types::Timestamp::default);
        self.timestamp.as_ref().unwrap_or(&DEFAULT_TIMESTAMP)
    }

    // fn context(&self) -> Option<&Self::Context> {
    //     self.cas_ref()
    // }

    fn set_response(&mut self, response: Vec<String>) {
        self.response = response;
    }

    fn set_cost(&mut self, cost: f64) {
        self.cost = cost;
    }

    fn set_latency(&mut self, latency_ms: u64) {
        self.latency_ms = latency_ms;
    }
}
