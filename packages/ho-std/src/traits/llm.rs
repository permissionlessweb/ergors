//! LLM-related traits for CW-HO system

use crate::constants::*;
use crate::error::HoResult;
use crate::prelude::{LlmEntity, LlmModel};
use crate::traits::LLMRouterConfigTrait;
use async_trait::async_trait;
use std::collections::HashMap;

/// Core trait for LLM routing and processing
#[async_trait]
pub trait OpenAITrait {
    type OpenAIRequest: PromptRequestTrait;
    type OpenAiResponse: PromptResponseTrait;
    type OpenAiMessage: PromptRequestTrait;
    fn process_request(&self) -> HoResult<Self::OpenAiResponse>;
    fn get_available_models(&self) -> Vec<String>;
}

/// Core trait for LLM prompt requests
pub trait PromptRequestTrait {
    type Message;
    type Context;
    type Config;

    /// Get messages
    fn messages(&self) -> &[Self::Message];

    /// Get model name
    fn model(&self) -> &str;

    /// Get context
    fn context(&self) -> Option<&Self::Context>;

    /// Get LLM configuration
    fn llm_config(&self) -> Option<&Self::Config>;

    /// Add message to request
    fn add_message(&mut self, message: Self::Message);

    /// Set model
    fn set_model(&mut self, model: String);

    /// Set context
    fn set_context(&mut self, context: Self::Context);
}

/// Core trait for LLM responses
pub trait PromptResponseTrait {
    type TokenUsage;
    type Context;
    type Timestamp;

    /// Get response ID
    fn id(&self) -> &Vec<u8>;

    /// Get provider name
    fn provider(&self) -> &str;

    /// Get model used
    fn model(&self) -> &str;

    /// Get original prompt
    fn prompt(&self) -> &str;

    /// Get response content
    fn response(&self) -> &str;

    /// Get timestamp
    fn timestamp(&self) -> &Self::Timestamp;

    /// Get token usage
    fn tokens_used(&self) -> &Self::TokenUsage;

    /// Get cost
    fn cost(&self) -> Option<f64>;

    /// Get latency
    fn latency_ms(&self) -> Option<u64>;

    // /// Get context
    // fn context(&self) -> Option<&Self::Context>;

    /// Set response content
    fn set_response(&mut self, response: String);

    /// Set cost
    fn set_cost(&mut self, cost: f64);

    /// Set latency
    fn set_latency(&mut self, latency_ms: u64);
}

/// Core trait for LLM messages
pub trait LlmMessageTrait {
    /// Get message role
    fn role(&self) -> &str;

    /// Get message content  
    fn content(&self) -> &str;

    /// Set role
    fn set_role(&mut self, role: String);

    /// Set content
    fn set_content(&mut self, content: String);

    /// Create user message
    fn user_message(content: String) -> Self;

    /// Create assistant message
    fn assistant_message(content: String) -> Self;

    /// Create system message
    fn system_message(content: String) -> Self;
}

/// Core trait for prompt context
pub trait PromptContextTrait {
    /// Get session ID
    fn session_id(&self) -> Option<&str>;

    /// Get user ID
    fn user_id(&self) -> Option<&str>;

    /// Get thread ID
    fn thread_id(&self) -> Option<&str>;

    /// Get metadata
    fn metadata(&self) -> &HashMap<String, String>;

    /// Set session ID
    fn set_session_id(&mut self, session_id: String);

    /// Set user ID
    fn set_user_id(&mut self, user_id: String);

    /// Set thread ID
    fn set_thread_id(&mut self, thread_id: String);

    /// Add metadata
    fn add_metadata(&mut self, key: String, value: String);
}

/// Core trait for token usage tracking
pub trait TokenUsageTrait {
    /// Get prompt tokens
    fn prompt_tokens(&self) -> u32;

    /// Get completion tokens
    fn completion_tokens(&self) -> u32;

    /// Get total tokens
    fn total_tokens(&self) -> u32;

    /// Set prompt tokens
    fn set_prompt_tokens(&mut self, tokens: u32);

    /// Set completion tokens
    fn set_completion_tokens(&mut self, tokens: u32);

    /// Update total tokens
    fn update_total(&mut self);
}

/// Core trait for LLM providers.
/// LLMProviderTrait is implemented where we define the struct maintaining all llm instances available for a single hoe-instance. This means that different ho-providers may have different
pub trait LLMProviderTrait {
    type ProviderType;

    /// Get provider name
    fn name(&self) -> &str;

    /// Get base URL
    fn base_url(&self) -> &str;

    /// Get API key
    fn api_key(&self) -> &str;

    /// Get supported models
    fn supported_models(&self) -> &[String];

    /// Get provider type
    fn provider_type(&self) -> &Self::ProviderType;

    /// Check if model is supported
    fn supports_model(&self, model: &str) -> bool {
        self.supported_models().contains(&model.to_string())
    }

    /// Set API key
    fn set_api_key(&mut self, api_key: String);

    /// Add supported model
    fn add_supported_model(&mut self, model: String);
}

#[async_trait]
pub trait LlmModelTrait {
    fn models(&self) -> (String, Vec<String>);
    fn default_base_url(&self) -> String;
    fn default_entity(&self) -> LlmEntity;
}

#[async_trait]
pub trait LLMRouterTrait {
    type Request: PromptRequestTrait;
    type Response: PromptResponseTrait;
    type Config: LLMRouterConfigTrait;
    type Provider: LLMProviderTrait;

    /// Create new LLM router
    async fn new(config: &Self::Config) -> HoResult<Self>
    where
        Self: Sized;

    /// Process a prompt request
    async fn process_request(
        &self,
        request: &Self::Request,
        model: &str,
    ) -> HoResult<Self::Response>;

    /// Route request to appropriate provider
    async fn route_to_provider(
        &self,
        provider_name: &str,
        request: &Self::Request,
    ) -> HoResult<Self::Response>;

    /// Get available providers
    fn providers(&self) -> Vec<&Self::Provider>;

    /// Get provider by name
    fn get_provider(&self, name: &str) -> Option<&Self::Provider>;

    /// Add provider
    fn add_provider(&mut self, provider: Self::Provider);

    /// Remove provider
    fn remove_provider(&mut self, name: &str);

    /// Check provider health
    async fn check_provider_health(&self, provider_name: &str) -> HoResult<bool>;
}
