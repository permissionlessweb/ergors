use serde::{Deserialize, Serialize};

// ==================== Ollama Types ====================

#[derive(Debug, Deserialize, Serialize)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct OllamaGenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    pub context: Vec<i64>,
    pub total_duration: u64,
    pub load_duration: u64,
    pub prompt_eval_count: u32,
    pub prompt_eval_duration: u64,
    pub eval_count: u32,
    pub eval_duration: u64,
}

#[derive(Debug, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: ChatMessage,
    pub done: bool,
    pub total_duration: u64,
    pub load_duration: u64,
    pub prompt_eval_count: u32,
    pub eval_count: u32,
}

// ==================== OpenAI Types ====================

#[derive(Debug, Deserialize)]
pub struct OpenAICompletionsRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub stream: bool,
}

fn default_max_tokens() -> usize {
    256
}

#[derive(Debug, Serialize)]
pub struct OpenAICompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: OpenAIUsage,
}

#[derive(Debug, Serialize)]
pub struct OpenAIChoice {
    pub text: String,
    pub index: usize,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

// ==================== Anthropic Types ====================

#[derive(Debug, Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    #[serde(default = "default_anthropic_max_tokens")]
    pub max_tokens: usize,
}

fn default_anthropic_max_tokens() -> usize {
    1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct AnthropicMessagesResponse {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    pub stop_reason: String,
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
pub struct AnthropicContentBlock {
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct AnthropicUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

// ==================== TGI Types ====================

#[derive(Debug, Deserialize)]
pub struct TGIGenerateRequest {
    pub inputs: String,
    #[serde(default)]
    pub parameters: Option<TGIParameters>,
}

#[derive(Debug, Deserialize)]
pub struct TGIParameters {
    #[serde(default = "default_max_new_tokens")]
    pub max_new_tokens: usize,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub do_sample: Option<bool>,
}

fn default_max_new_tokens() -> usize {
    256
}

#[derive(Debug, Serialize)]
pub struct TGIGenerateResponse {
    pub generated_text: String,
    pub details: Option<TGIDetails>,
}

#[derive(Debug, Serialize)]
pub struct TGIDetails {
    pub finish_reason: String,
    pub generated_tokens: usize,
    pub seed: Option<u64>,
}

// ==================== Embeddings Types ====================

#[derive(Debug, Deserialize)]
pub struct EmbeddingsRequest {
    pub model: String,
    #[serde(alias = "input")]
    pub prompt: StringOrVec,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Serialize)]
pub struct EmbeddingsResponse {
    pub model: String,
    pub embeddings: Vec<Vec<f32>>,
}

// ==================== Model Info ====================

#[derive(Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub model_id: String,
    pub size_bytes: u64,
    pub parameter_count: String,
    pub quantization: String,
}
