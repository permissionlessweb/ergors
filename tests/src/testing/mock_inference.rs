//! Mock Inference Provider
//!
//! Simulates Ollama, vLLM, and TGI inference API responses for testing
//! without requiring GPU resources. Supports realistic latency simulation
//! and configurable error scenarios.

use anyhow::{anyhow, Result};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Mock inference provider configuration
#[derive(Debug, Clone)]
pub struct MockInferenceConfig {
    /// Port to listen on (0 for auto-assign)
    pub port: u16,
    /// Simulated response latency range (min, max) in milliseconds
    pub latency_range_ms: (u64, u64),
    /// Available models
    pub models: Vec<MockModel>,
    /// Error rate (0.0 - 1.0)
    pub error_rate: f32,
    /// Enable streaming responses
    pub enable_streaming: bool,
    /// Max tokens per response
    pub max_tokens: usize,
}

impl Default for MockInferenceConfig {
    fn default() -> Self {
        Self {
            port: 0, // Auto-assign
            latency_range_ms: (50, 200),
            models: vec![
                MockModel::new("llama2", "llama2:latest"),
                MockModel::new("llama2:7b", "llama2:7b-chat"),
                MockModel::new("mistral", "mistral:latest"),
                MockModel::new("codellama", "codellama:latest"),
            ],
            error_rate: 0.0,
            enable_streaming: true,
            max_tokens: 2048,
        }
    }
}

/// Mock model definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockModel {
    pub name: String,
    pub model_id: String,
    pub size_bytes: u64,
    pub parameter_count: String,
    pub quantization: String,
}

impl MockModel {
    pub fn new(name: &str, model_id: &str) -> Self {
        Self {
            name: name.to_string(),
            model_id: model_id.to_string(),
            size_bytes: 4_000_000_000, // 4GB default
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        }
    }
}

/// Mock inference provider state
#[derive(Clone)]
struct MockState {
    config: MockInferenceConfig,
    request_count: Arc<RwLock<u64>>,
    tool_calls: Arc<RwLock<Vec<ToolCallRecord>>>,
}

/// Record of tool calls for testing agentic behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub timestamp: u64,
    pub model: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// Mock inference provider
///
/// Provides simulated inference endpoints compatible with:
/// - Ollama API (`/api/generate`, `/api/chat`, `/api/tags`)
/// - OpenAI API (`/v1/completions`, `/v1/chat/completions`)
/// - TGI API (`/generate`, `/generate_stream`)
pub struct MockInferenceProvider {
    config: MockInferenceConfig,
    addr: Option<SocketAddr>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    request_count: Arc<RwLock<u64>>,
    tool_calls: Arc<RwLock<Vec<ToolCallRecord>>>,
}

impl MockInferenceProvider {
    /// Create a new mock provider with default configuration
    pub fn new() -> Self {
        Self::with_config(MockInferenceConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: MockInferenceConfig) -> Self {
        Self {
            config,
            addr: None,
            shutdown_tx: None,
            request_count: Arc::new(RwLock::new(0)),
            tool_calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the mock provider server
    pub async fn start() -> Result<Self> {
        let mut provider = Self::new();
        provider.startup().await?;
        Ok(provider)
    }

    /// Start with custom configuration
    pub async fn start_with_config(config: MockInferenceConfig) -> Result<Self> {
        let mut provider = Self::with_config(config);
        provider.startup().await?;
        Ok(provider)
    }

    /// Internal startup
    async fn startup(&mut self) -> Result<()> {
        let state = MockState {
            config: self.config.clone(),
            request_count: self.request_count.clone(),
            tool_calls: self.tool_calls.clone(),
        };

        let app = Router::new()
            // Health check
            .route("/health", get(health_handler))
            // Ollama API
            .route("/api/generate", post(ollama_generate_handler))
            .route("/api/chat", post(ollama_chat_handler))
            .route("/api/tags", get(ollama_tags_handler))
            .route("/api/pull", post(ollama_pull_handler))
            .route("/api/show", post(ollama_show_handler))
            // OpenAI API
            .route("/v1/completions", post(openai_completions_handler))
            .route("/v1/chat/completions", post(openai_chat_handler))
            .route("/v1/models", get(openai_models_handler))
            // TGI API
            .route("/generate", post(tgi_generate_handler))
            .route("/generate_stream", post(tgi_stream_handler))
            .route("/info", get(tgi_info_handler))
            // Agentic endpoints
            .route("/api/agentic/execute", post(agentic_execute_handler))
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], self.config.port));
        let listener = TcpListener::bind(addr).await?;
        let actual_addr = listener.local_addr()?;
        self.addr = Some(actual_addr);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        info!("Mock inference provider listening on {}", actual_addr);

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .ok();
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    /// Get the server address
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    /// Get the base URL
    pub fn base_url(&self) -> Option<String> {
        self.addr.map(|a| format!("http://{}", a))
    }

    /// Check if the provider is responsive
    pub async fn is_responsive(&self) -> bool {
        if let Some(url) = self.base_url() {
            let client = reqwest::Client::new();
            client
                .get(format!("{}/health", url))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Get total request count
    pub async fn request_count(&self) -> u64 {
        *self.request_count.read().await
    }

    /// Get recorded tool calls
    pub async fn tool_calls(&self) -> Vec<ToolCallRecord> {
        self.tool_calls.read().await.clone()
    }

    /// Clear recorded tool calls
    pub async fn clear_tool_calls(&self) {
        self.tool_calls.write().await.clear();
    }

    /// Stop the provider
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.addr = None;
        info!("Mock inference provider stopped");
    }
}

    /// Generate a ProxyRouterConfig that routes all models to this mock server.
    /// Useful for e2e tests that need proxy routing to hit a local mock.
    pub fn to_proxy_config(&self) -> ho_std::types::ergors::orch::v1::ProxyRouterConfig {
        use ho_std::types::ergors::orch::v1::{
            InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig,
        };
        use std::collections::HashMap;

        let base_url = self.base_url().expect("MockInferenceProvider must be started before calling to_proxy_config");

        let mut providers = HashMap::new();
        providers.insert("openai".to_string(), InferenceProviderConfig {
            provider_id: "openai".to_string(),
            base_url: base_url.clone(),
            enabled: true,
            provider_type: InferenceProviderType::Openai as i32,
            ..Default::default()
        });
        providers.insert("anthropic".to_string(), InferenceProviderConfig {
            provider_id: "anthropic".to_string(),
            base_url: base_url.clone(),
            enabled: true,
            provider_type: InferenceProviderType::Anthropic as i32,
            ..Default::default()
        });
        providers.insert("ollama".to_string(), InferenceProviderConfig {
            provider_id: "ollama".to_string(),
            base_url: base_url.clone(),
            enabled: true,
            provider_type: InferenceProviderType::Ollama as i32,
            ..Default::default()
        });

        let mut model_routes = HashMap::new();
        model_routes.insert("gpt-*".to_string(), "openai".to_string());
        model_routes.insert("claude-*".to_string(), "anthropic".to_string());
        model_routes.insert("mistral*".to_string(), "ollama".to_string());

        ProxyRouterConfig {
            providers,
            model_routes,
            ..Default::default()
        }
    }
}

impl Default for MockInferenceProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Handler Types ====================

#[derive(Debug, Deserialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    options: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OllamaGenerateResponse {
    model: String,
    created_at: String,
    response: String,
    done: bool,
    context: Vec<i64>,
    total_duration: u64,
    load_duration: u64,
    prompt_eval_count: u32,
    prompt_eval_duration: u64,
    eval_count: u32,
    eval_duration: u64,
}

#[derive(Debug, Deserialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OllamaChatResponse {
    model: String,
    created_at: String,
    message: ChatMessage,
    done: bool,
    total_duration: u64,
    load_duration: u64,
    prompt_eval_count: u32,
    eval_count: u32,
}

#[derive(Debug, Serialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Serialize)]
struct OllamaModelInfo {
    name: String,
    model: String,
    modified_at: String,
    size: u64,
    digest: String,
    details: OllamaModelDetails,
}

#[derive(Debug, Serialize)]
struct OllamaModelDetails {
    parent_model: String,
    format: String,
    family: String,
    families: Vec<String>,
    parameter_size: String,
    quantization_level: String,
}

#[derive(Debug, Deserialize)]
struct OpenAICompletionsRequest {
    model: String,
    prompt: String,
    #[serde(default = "default_max_tokens")]
    max_tokens: usize,
    #[serde(default)]
    temperature: f32,
    #[serde(default)]
    stream: bool,
}

fn default_max_tokens() -> usize {
    256
}

#[derive(Debug, Serialize)]
struct OpenAICompletionsResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Debug, Serialize)]
struct OpenAIChoice {
    text: String,
    index: usize,
    logprobs: Option<serde_json::Value>,
    finish_reason: String,
}

#[derive(Debug, Serialize)]
struct OpenAIChatChoice {
    index: usize,
    message: ChatMessage,
    finish_reason: String,
}

#[derive(Debug, Serialize)]
struct OpenAIUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct TGIGenerateRequest {
    inputs: String,
    #[serde(default)]
    parameters: Option<TGIParameters>,
}

#[derive(Debug, Deserialize)]
struct TGIParameters {
    #[serde(default = "default_max_new_tokens")]
    max_new_tokens: usize,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
}

fn default_max_new_tokens() -> usize {
    256
}

#[derive(Debug, Serialize)]
struct TGIGenerateResponse {
    generated_text: String,
    details: Option<TGIDetails>,
}

#[derive(Debug, Serialize)]
struct TGIDetails {
    finish_reason: String,
    generated_tokens: usize,
    seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AgenticRequest {
    model: String,
    prompt: String,
    tools: Vec<AgenticTool>,
}

#[derive(Debug, Deserialize)]
struct AgenticTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct AgenticResponse {
    response: String,
    tool_calls: Vec<ToolCall>,
}

// ==================== Handlers ====================

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn ollama_generate_handler(
    State(state): State<MockState>,
    Json(req): Json<OllamaGenerateRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;

    // Simulate latency
    simulate_latency(&state.config).await;

    // Check for simulated errors
    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        )
            .into_response();
    }

    let response = generate_mock_response(&req.prompt, &req.model);

    let resp = OllamaGenerateResponse {
        model: req.model,
        created_at: chrono::Utc::now().to_rfc3339(),
        response,
        done: true,
        context: vec![1, 2, 3],
        total_duration: 150_000_000,
        load_duration: 10_000_000,
        prompt_eval_count: 10,
        prompt_eval_duration: 50_000_000,
        eval_count: 50,
        eval_duration: 90_000_000,
    };

    Json(resp).into_response()
}

async fn ollama_chat_handler(
    State(state): State<MockState>,
    Json(req): Json<OllamaChatRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        )
            .into_response();
    }

    // Get last user message
    let prompt = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let response_text = generate_mock_response(prompt, &req.model);

    // Check if tools are provided and simulate tool calls
    let tool_calls = if req.tools.is_some() && prompt.contains("tool") {
        Some(vec![ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4()),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "mock_tool".to_string(),
                arguments: r#"{"arg": "value"}"#.to_string(),
            },
        }])
    } else {
        None
    };

    let resp = OllamaChatResponse {
        model: req.model,
        created_at: chrono::Utc::now().to_rfc3339(),
        message: ChatMessage {
            role: "assistant".to_string(),
            content: response_text,
            tool_calls,
        },
        done: true,
        total_duration: 150_000_000,
        load_duration: 10_000_000,
        prompt_eval_count: 10,
        eval_count: 50,
    };

    Json(resp).into_response()
}

async fn ollama_tags_handler(State(state): State<MockState>) -> impl IntoResponse {
    let models: Vec<OllamaModelInfo> = state
        .config
        .models
        .iter()
        .map(|m| OllamaModelInfo {
            name: m.name.clone(),
            model: m.model_id.clone(),
            modified_at: chrono::Utc::now().to_rfc3339(),
            size: m.size_bytes,
            digest: format!("sha256:{}", hex::encode(&m.name.as_bytes()[..8.min(m.name.len())])),
            details: OllamaModelDetails {
                parent_model: String::new(),
                format: "gguf".to_string(),
                family: "llama".to_string(),
                families: vec!["llama".to_string()],
                parameter_size: m.parameter_count.clone(),
                quantization_level: m.quantization.clone(),
            },
        })
        .collect();

    Json(OllamaTagsResponse { models })
}

async fn ollama_pull_handler(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let model = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    debug!("Mock pull request for model: {}", model);

    // Simulate pull completion
    Json(serde_json::json!({
        "status": "success",
        "digest": format!("sha256:{}", hex::encode(model.as_bytes()))
    }))
}

async fn ollama_show_handler(
    State(state): State<MockState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let model_name = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

    let model = state
        .config
        .models
        .iter()
        .find(|m| m.name == model_name || m.model_id == model_name)
        .cloned()
        .unwrap_or_else(|| MockModel::new(model_name, model_name));

    Json(serde_json::json!({
        "modelfile": "FROM llama2",
        "parameters": "temperature 0.7",
        "template": "{{ .Prompt }}",
        "details": {
            "parent_model": "",
            "format": "gguf",
            "family": "llama",
            "parameter_size": model.parameter_count,
            "quantization_level": model.quantization
        }
    }))
}

async fn openai_completions_handler(
    State(state): State<MockState>,
    Json(req): Json<OpenAICompletionsRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        )
            .into_response();
    }

    let response_text = generate_mock_response(&req.prompt, &req.model);
    let completion_tokens = response_text.split_whitespace().count();
    let prompt_tokens = req.prompt.split_whitespace().count();

    let resp = OpenAICompletionsResponse {
        id: format!("cmpl-{}", uuid::Uuid::new_v4()),
        object: "text_completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: req.model,
        choices: vec![OpenAIChoice {
            text: response_text,
            index: 0,
            logprobs: None,
            finish_reason: "stop".to_string(),
        }],
        usage: OpenAIUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    };

    Json(resp).into_response()
}

async fn openai_chat_handler(
    State(state): State<MockState>,
    Json(req): Json<OllamaChatRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        )
            .into_response();
    }

    let prompt = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let response_text = generate_mock_response(prompt, &req.model);
    let completion_tokens = response_text.split_whitespace().count();
    let prompt_tokens: usize = req.messages.iter().map(|m| m.content.split_whitespace().count()).sum();

    Json(serde_json::json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": req.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response_text
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
    }))
    .into_response()
}

async fn openai_models_handler(State(state): State<MockState>) -> impl IntoResponse {
    let models: Vec<serde_json::Value> = state
        .config
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.model_id,
                "object": "model",
                "created": chrono::Utc::now().timestamp(),
                "owned_by": "organization"
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": models
    }))
}

async fn tgi_generate_handler(
    State(state): State<MockState>,
    Json(req): Json<TGIGenerateRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        )
            .into_response();
    }

    let response_text = generate_mock_response(&req.inputs, "tgi-model");
    let tokens = response_text.split_whitespace().count();

    let resp = TGIGenerateResponse {
        generated_text: response_text,
        details: Some(TGIDetails {
            finish_reason: "eos_token".to_string(),
            generated_tokens: tokens,
            seed: Some(42),
        }),
    };

    Json(resp).into_response()
}

async fn tgi_stream_handler(
    State(state): State<MockState>,
    Json(req): Json<TGIGenerateRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;

    // For simplicity, return non-streaming response
    // Real implementation would use SSE
    let response_text = generate_mock_response(&req.inputs, "tgi-model");

    Json(serde_json::json!({
        "generated_text": response_text,
        "details": {
            "finish_reason": "eos_token",
            "generated_tokens": response_text.split_whitespace().count()
        }
    }))
}

async fn tgi_info_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "model_id": "mock-model",
        "model_sha": "abc123",
        "model_dtype": "float16",
        "model_device_type": "cuda",
        "model_pipeline_tag": "text-generation",
        "max_concurrent_requests": 128,
        "max_best_of": 2,
        "max_stop_sequences": 4,
        "max_input_length": 4096,
        "max_total_tokens": 8192,
        "waiting_served_ratio": 1.2,
        "max_batch_prefill_tokens": 4096,
        "max_batch_total_tokens": 32768,
        "validation_workers": 2,
        "version": "1.0.0"
    }))
}

async fn agentic_execute_handler(
    State(state): State<MockState>,
    Json(req): Json<AgenticRequest>,
) -> impl IntoResponse {
    *state.request_count.write().await += 1;
    simulate_latency(&state.config).await;

    // Simulate tool call based on prompt
    let tool_calls: Vec<ToolCall> = req
        .tools
        .iter()
        .take(1)
        .map(|tool| {
            let call = ToolCall {
                id: format!("call_{}", uuid::Uuid::new_v4()),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: tool.name.clone(),
                    arguments: r#"{"input": "test"}"#.to_string(),
                },
            };

            // Record the tool call
            let record = ToolCallRecord {
                timestamp: chrono::Utc::now().timestamp() as u64,
                model: req.model.clone(),
                tool_name: tool.name.clone(),
                arguments: serde_json::json!({"input": "test"}),
            };

            // Spawn task to record (avoid blocking)
            let tool_calls = state.tool_calls.clone();
            tokio::spawn(async move {
                tool_calls.write().await.push(record);
            });

            call
        })
        .collect();

    let resp = AgenticResponse {
        response: format!(
            "I will use the {} tool to help with your request.",
            tool_calls.first().map(|t| t.function.name.as_str()).unwrap_or("appropriate")
        ),
        tool_calls,
    };

    Json(resp)
}

// ==================== Helper Functions ====================

async fn simulate_latency(config: &MockInferenceConfig) {
    let (min, max) = config.latency_range_ms;
    let latency = if min == max {
        min
    } else {
        min + (rand::random::<u64>() % (max - min))
    };
    tokio::time::sleep(Duration::from_millis(latency)).await;
}

fn should_error(config: &MockInferenceConfig) -> bool {
    config.error_rate > 0.0 && rand::random::<f32>() < config.error_rate
}

fn generate_mock_response(prompt: &str, model: &str) -> String {
    // Generate contextual mock responses based on prompt keywords
    let prompt_lower = prompt.to_lowercase();

    if prompt_lower.contains("hello") || prompt_lower.contains("hi") {
        return format!(
            "Hello! I'm {} running on a mock inference provider. How can I help you today?",
            model
        );
    }

    if prompt_lower.contains("code") || prompt_lower.contains("function") {
        return "Here's a simple example:\n\n```rust\nfn hello() {\n    println!(\"Hello, world!\");\n}\n```\n\nThis function prints a greeting to the console.".to_string();
    }

    if prompt_lower.contains("explain") {
        return "Let me explain: This is a mock response from the inference provider. In a real deployment, this would be generated by an actual language model processing your prompt.".to_string();
    }

    if prompt_lower.contains("test") {
        return "Test response received successfully. The mock inference provider is working correctly.".to_string();
    }

    // Default response
    format!(
        "This is a mock response from model '{}'. Your prompt was: \"{}...\" \
        In production, this would be replaced with actual model output.",
        model,
        &prompt[..prompt.len().min(50)]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MockInferenceConfig::default();
        assert_eq!(config.port, 0);
        assert!(!config.models.is_empty());
    }

    #[test]
    fn test_mock_model_creation() {
        let model = MockModel::new("test", "test:latest");
        assert_eq!(model.name, "test");
        assert_eq!(model.model_id, "test:latest");
    }

    #[test]
    fn test_generate_mock_response() {
        let response = generate_mock_response("hello world", "test-model");
        assert!(response.contains("Hello"));

        let code_response = generate_mock_response("write some code", "test-model");
        assert!(code_response.contains("```"));
    }

    #[tokio::test]
    async fn test_mock_provider_lifecycle() {
        let mut provider = MockInferenceProvider::start().await.unwrap();
        assert!(provider.addr().is_some());
        assert!(provider.is_responsive().await);
        provider.stop().await;
    }
}
