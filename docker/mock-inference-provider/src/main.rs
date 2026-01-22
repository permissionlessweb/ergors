//! Mock Inference Provider
//!
//! A standalone service that simulates inference provider APIs for testing
//! Akash deployments without requiring GPU resources.
//!
//! Supported APIs:
//! - Ollama (`/api/generate`, `/api/chat`, `/api/tags`, `/api/pull`, `/api/show`)
//! - OpenAI (`/v1/completions`, `/v1/chat/completions`, `/v1/models`)
//! - TGI (`/generate`, `/generate_stream`, `/info`)
//! - Custom agentic endpoints (`/api/agentic/execute`)

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
    Router,
};
use clap::Parser;
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{info, Level};

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "mock-inference-provider")]
#[command(about = "Mock inference provider for Akash deployment testing")]
struct Args {
    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value = "11434")]
    port: u16,

    /// Host to bind to
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,

    /// Minimum simulated latency in milliseconds
    #[arg(long, env = "MIN_LATENCY_MS", default_value = "50")]
    min_latency_ms: u64,

    /// Maximum simulated latency in milliseconds
    #[arg(long, env = "MAX_LATENCY_MS", default_value = "200")]
    max_latency_ms: u64,

    /// Error rate (0.0 - 1.0)
    #[arg(long, env = "ERROR_RATE", default_value = "0.0")]
    error_rate: f32,

    /// Model name to report
    #[arg(long, env = "MODEL_NAME", default_value = "llama2")]
    model_name: String,

    /// Enable verbose logging
    #[arg(short, long, env = "VERBOSE")]
    verbose: bool,
}

/// Application state
#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    request_count: Arc<AtomicU64>,
    tool_calls: Arc<RwLock<Vec<ToolCallRecord>>>,
}

/// Application configuration
struct AppConfig {
    min_latency_ms: u64,
    max_latency_ms: u64,
    error_rate: f32,
    models: Vec<ModelInfo>,
}

/// Model information
#[derive(Clone, Serialize)]
struct ModelInfo {
    name: String,
    model_id: String,
    size_bytes: u64,
    parameter_count: String,
    quantization: String,
}

/// Tool call record for agentic testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCallRecord {
    timestamp: u64,
    model: String,
    tool_name: String,
    arguments: serde_json::Value,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Setup logging
    let level = if args.verbose { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    // Create models list
    let models = vec![
        ModelInfo {
            name: args.model_name.clone(),
            model_id: format!("{}:latest", args.model_name),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "llama2:7b".to_string(),
            model_id: "llama2:7b-chat".to_string(),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "mistral".to_string(),
            model_id: "mistral:latest".to_string(),
            size_bytes: 4_500_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "codellama".to_string(),
            model_id: "codellama:latest".to_string(),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
    ];

    let config = AppConfig {
        min_latency_ms: args.min_latency_ms,
        max_latency_ms: args.max_latency_ms,
        error_rate: args.error_rate,
        models,
    };

    let state = AppState {
        config: Arc::new(config),
        request_count: Arc::new(AtomicU64::new(0)),
        tool_calls: Arc::new(RwLock::new(Vec::new())),
    };

    let app = Router::new()
        // Health check
        .route("/health", get(health_handler))
        .route("/", get(root_handler))
        // Ollama API
        .route("/api/generate", post(ollama_generate_handler))
        .route("/api/chat", post(ollama_chat_handler))
        .route("/api/tags", get(ollama_tags_handler))
        .route("/api/pull", post(ollama_pull_handler))
        .route("/api/show", post(ollama_show_handler))
        .route("/api/embeddings", post(ollama_embeddings_handler))
        // OpenAI API
        .route("/v1/completions", post(openai_completions_handler))
        .route("/v1/chat/completions", post(openai_chat_handler))
        .route("/v1/models", get(openai_models_handler))
        .route("/v1/embeddings", post(openai_embeddings_handler))
        // TGI API
        .route("/generate", post(tgi_generate_handler))
        .route("/generate_stream", post(tgi_stream_handler))
        .route("/info", get(tgi_info_handler))
        // Agentic endpoints
        .route("/api/agentic/execute", post(agentic_execute_handler))
        .route("/api/agentic/tool-calls", get(agentic_tool_calls_handler))
        // Metrics
        .route("/metrics", get(metrics_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse().unwrap();
    info!("Starting mock inference provider on {}", addr);
    info!("Latency range: {}ms - {}ms", args.min_latency_ms, args.max_latency_ms);
    info!("Error rate: {:.1}%", args.error_rate * 100.0);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ==================== Request/Response Types ====================

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

fn default_max_tokens() -> usize { 256 }

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
    #[serde(default)]
    do_sample: Option<bool>,
}

fn default_max_new_tokens() -> usize { 256 }

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
    #[serde(default)]
    max_iterations: Option<usize>,
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
    iterations: usize,
    completed: bool,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsRequest {
    model: String,
    #[serde(alias = "input")]
    prompt: StringOrVec,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Serialize)]
struct EmbeddingsResponse {
    model: String,
    embeddings: Vec<Vec<f32>>,
}

// ==================== Handlers ====================

async fn root_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "Mock Inference Provider",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Simulates Ollama, OpenAI, and TGI APIs for testing",
        "endpoints": {
            "ollama": ["/api/generate", "/api/chat", "/api/tags", "/api/pull", "/api/show", "/api/embeddings"],
            "openai": ["/v1/completions", "/v1/chat/completions", "/v1/models", "/v1/embeddings"],
            "tgi": ["/generate", "/generate_stream", "/info"],
            "agentic": ["/api/agentic/execute", "/api/agentic/tool-calls"],
            "system": ["/health", "/metrics"]
        }
    }))
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "timestamp": chrono::Utc::now().to_rfc3339()}))
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.request_count.load(Ordering::Relaxed);
    let tool_calls = state.tool_calls.read().await.len();

    Json(serde_json::json!({
        "total_requests": count,
        "total_tool_calls": tool_calls,
        "uptime_seconds": 0, // Would track actual uptime
        "models_loaded": state.config.models.len()
    }))
}

async fn ollama_generate_handler(
    State(state): State<AppState>,
    Json(req): Json<OllamaGenerateRequest>,
) -> Response {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        ).into_response();
    }

    let response_text = generate_response(&req.prompt, &req.model);

    if req.stream {
        // Return streaming response
        let chunks = create_stream_chunks(&response_text, &req.model);
        return Sse::new(chunks).into_response();
    }

    let resp = OllamaGenerateResponse {
        model: req.model,
        created_at: chrono::Utc::now().to_rfc3339(),
        response: response_text,
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
    State(state): State<AppState>,
    Json(req): Json<OllamaChatRequest>,
) -> Response {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        ).into_response();
    }

    let prompt = req.messages.iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let response_text = generate_response(prompt, &req.model);

    // Check if tools are provided and simulate tool calls
    let tool_calls = if req.tools.is_some() && should_use_tools(prompt) {
        Some(vec![ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4()),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: r#"{"query": "relevant information"}"#.to_string(),
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

async fn ollama_tags_handler(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<serde_json::Value> = state.config.models.iter().map(|m| {
        serde_json::json!({
            "name": m.name,
            "model": m.model_id,
            "modified_at": chrono::Utc::now().to_rfc3339(),
            "size": m.size_bytes,
            "digest": format!("sha256:{}", hex::encode(&m.name.as_bytes()[..8.min(m.name.len())])),
            "details": {
                "parent_model": "",
                "format": "gguf",
                "family": "llama",
                "families": ["llama"],
                "parameter_size": m.parameter_count,
                "quantization_level": m.quantization
            }
        })
    }).collect();

    Json(serde_json::json!({"models": models}))
}

async fn ollama_pull_handler(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let model = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    info!("Mock pull request for model: {}", model);

    // Simulate pull progress
    tokio::time::sleep(Duration::from_millis(100)).await;

    Json(serde_json::json!({
        "status": "success",
        "digest": format!("sha256:{}", hex::encode(model.as_bytes()))
    }))
}

async fn ollama_show_handler(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let model_name = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

    let model = state.config.models.iter()
        .find(|m| m.name == model_name || m.model_id == model_name)
        .cloned()
        .unwrap_or_else(|| ModelInfo {
            name: model_name.to_string(),
            model_id: model_name.to_string(),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        });

    Json(serde_json::json!({
        "modelfile": format!("FROM {}", model.name),
        "parameters": "temperature 0.7\ntop_p 0.9",
        "template": "{{ .System }}\n\n{{ .Prompt }}",
        "details": {
            "parent_model": "",
            "format": "gguf",
            "family": "llama",
            "parameter_size": model.parameter_count,
            "quantization_level": model.quantization
        },
        "model_info": {
            "general.architecture": "llama",
            "general.file_type": 2,
            "general.parameter_count": 7_000_000_000u64,
            "general.quantization_version": 2
        }
    }))
}

async fn ollama_embeddings_handler(
    State(state): State<AppState>,
    Json(req): Json<EmbeddingsRequest>,
) -> impl IntoResponse {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    let prompts = match req.prompt {
        StringOrVec::Single(s) => vec![s],
        StringOrVec::Multiple(v) => v,
    };

    // Generate mock embeddings (384 dimensions)
    let embeddings: Vec<Vec<f32>> = prompts.iter().map(|p| {
        generate_mock_embedding(p)
    }).collect();

    Json(EmbeddingsResponse {
        model: req.model,
        embeddings,
    })
}

async fn openai_completions_handler(
    State(state): State<AppState>,
    Json(req): Json<OpenAICompletionsRequest>,
) -> Response {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        ).into_response();
    }

    let response_text = generate_response(&req.prompt, &req.model);
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
    State(state): State<AppState>,
    Json(req): Json<OllamaChatRequest>,
) -> Response {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        ).into_response();
    }

    let prompt = req.messages.iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let response_text = generate_response(prompt, &req.model);
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
    })).into_response()
}

async fn openai_models_handler(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<serde_json::Value> = state.config.models.iter().map(|m| {
        serde_json::json!({
            "id": m.model_id,
            "object": "model",
            "created": chrono::Utc::now().timestamp(),
            "owned_by": "organization"
        })
    }).collect();

    Json(serde_json::json!({
        "object": "list",
        "data": models
    }))
}

async fn openai_embeddings_handler(
    State(state): State<AppState>,
    Json(req): Json<EmbeddingsRequest>,
) -> impl IntoResponse {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    let prompts = match req.prompt {
        StringOrVec::Single(s) => vec![s],
        StringOrVec::Multiple(v) => v,
    };

    let data: Vec<serde_json::Value> = prompts.iter().enumerate().map(|(i, p)| {
        serde_json::json!({
            "object": "embedding",
            "embedding": generate_mock_embedding(p),
            "index": i
        })
    }).collect();

    Json(serde_json::json!({
        "object": "list",
        "data": data,
        "model": req.model,
        "usage": {
            "prompt_tokens": prompts.iter().map(|p| p.split_whitespace().count()).sum::<usize>(),
            "total_tokens": prompts.iter().map(|p| p.split_whitespace().count()).sum::<usize>()
        }
    }))
}

async fn tgi_generate_handler(
    State(state): State<AppState>,
    Json(req): Json<TGIGenerateRequest>,
) -> Response {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        ).into_response();
    }

    let response_text = generate_response(&req.inputs, "tgi-model");
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
    State(state): State<AppState>,
    Json(req): Json<TGIGenerateRequest>,
) -> impl IntoResponse {
    state.request_count.fetch_add(1, Ordering::Relaxed);

    let response_text = generate_response(&req.inputs, "tgi-model");
    let chunks = create_tgi_stream_chunks(&response_text);

    Sse::new(chunks)
}

async fn tgi_info_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "model_id": "mock-model",
        "model_sha": "abc123def456",
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
        "version": "1.4.0",
        "sha": "abc123",
        "docker_label": "ghcr.io/huggingface/text-generation-inference:1.4"
    }))
}

async fn agentic_execute_handler(
    State(state): State<AppState>,
    Json(req): Json<AgenticRequest>,
) -> impl IntoResponse {
    state.request_count.fetch_add(1, Ordering::Relaxed);
    simulate_latency(&state.config).await;

    let max_iterations = req.max_iterations.unwrap_or(3);
    let mut iterations = 0;
    let mut tool_calls_made = Vec::new();

    // Simulate tool usage based on prompt
    for tool in &req.tools {
        if iterations >= max_iterations {
            break;
        }

        if should_use_tool(&req.prompt, &tool.name) {
            let call = ToolCall {
                id: format!("call_{}", uuid::Uuid::new_v4()),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: tool.name.clone(),
                    arguments: generate_tool_arguments(&tool.name, &req.prompt),
                },
            };

            // Record the tool call
            let record = ToolCallRecord {
                timestamp: chrono::Utc::now().timestamp() as u64,
                model: req.model.clone(),
                tool_name: tool.name.clone(),
                arguments: serde_json::from_str(&call.function.arguments).unwrap_or_default(),
            };
            state.tool_calls.write().await.push(record);

            tool_calls_made.push(call);
            iterations += 1;
        }
    }

    let response = if tool_calls_made.is_empty() {
        generate_response(&req.prompt, &req.model)
    } else {
        format!(
            "I'll help you with that. I'm using {} tool(s) to gather the necessary information.",
            tool_calls_made.len()
        )
    };

    Json(AgenticResponse {
        response,
        tool_calls: tool_calls_made,
        iterations,
        completed: true,
    })
}

async fn agentic_tool_calls_handler(State(state): State<AppState>) -> impl IntoResponse {
    let calls = state.tool_calls.read().await.clone();
    Json(serde_json::json!({
        "tool_calls": calls,
        "total": calls.len()
    }))
}

// ==================== Helper Functions ====================

async fn simulate_latency(config: &AppConfig) {
    let (min, max) = (config.min_latency_ms, config.max_latency_ms);
    let latency = if min == max {
        min
    } else {
        min + (rand::random::<u64>() % (max - min))
    };
    tokio::time::sleep(Duration::from_millis(latency)).await;
}

fn should_error(config: &AppConfig) -> bool {
    config.error_rate > 0.0 && rand::random::<f32>() < config.error_rate
}

fn should_use_tools(prompt: &str) -> bool {
    let keywords = ["search", "find", "lookup", "get", "fetch", "call", "use tool", "execute"];
    let prompt_lower = prompt.to_lowercase();
    keywords.iter().any(|k| prompt_lower.contains(k))
}

fn should_use_tool(prompt: &str, tool_name: &str) -> bool {
    let prompt_lower = prompt.to_lowercase();
    let tool_lower = tool_name.to_lowercase();

    // Check if prompt mentions the tool or related concepts
    prompt_lower.contains(&tool_lower) ||
    (tool_lower.contains("search") && prompt_lower.contains("find")) ||
    (tool_lower.contains("code") && (prompt_lower.contains("write") || prompt_lower.contains("implement"))) ||
    (tool_lower.contains("file") && (prompt_lower.contains("read") || prompt_lower.contains("save")))
}

fn generate_tool_arguments(tool_name: &str, prompt: &str) -> String {
    match tool_name.to_lowercase().as_str() {
        "search" | "web_search" => {
            serde_json::json!({"query": prompt.chars().take(100).collect::<String>()}).to_string()
        }
        "code_interpreter" | "execute_code" => {
            serde_json::json!({"code": "print('Hello from mock!')", "language": "python"}).to_string()
        }
        "file_read" | "read_file" => {
            serde_json::json!({"path": "/tmp/example.txt"}).to_string()
        }
        "file_write" | "write_file" => {
            serde_json::json!({"path": "/tmp/output.txt", "content": "Mock output"}).to_string()
        }
        _ => {
            serde_json::json!({"input": prompt.chars().take(50).collect::<String>()}).to_string()
        }
    }
}

fn generate_response(prompt: &str, model: &str) -> String {
    let prompt_lower = prompt.to_lowercase();

    if prompt_lower.contains("hello") || prompt_lower.contains("hi") {
        return format!(
            "Hello! I'm {} running on a mock inference provider. How can I help you today?",
            model
        );
    }

    if prompt_lower.contains("code") || prompt_lower.contains("function") || prompt_lower.contains("implement") {
        return r#"Here's a simple example:

```rust
fn hello() {
    println!("Hello, world!");
}

fn main() {
    hello();
}
```

This code defines a function that prints a greeting and calls it from main."#.to_string();
    }

    if prompt_lower.contains("explain") {
        return "Let me explain: This is a mock response from the inference provider. In a real deployment on Akash Network, this would be generated by an actual language model like Llama, Mistral, or CodeLlama processing your prompt with real inference capabilities.".to_string();
    }

    if prompt_lower.contains("test") || prompt_lower.contains("ping") {
        return "Test response received successfully! The mock inference provider is working correctly and ready to handle requests.".to_string();
    }

    if prompt_lower.contains("json") {
        return r#"Here's a JSON example:

```json
{
  "status": "success",
  "data": {
    "message": "Mock response",
    "timestamp": "2024-01-01T00:00:00Z"
  }
}
```"#.to_string();
    }

    // Default response
    format!(
        "This is a mock response from model '{}'. Your prompt was processed successfully. \
        In production on Akash Network, this would be replaced with actual model inference output. \
        The mock provider supports Ollama, OpenAI, and TGI APIs for comprehensive testing.",
        model
    )
}

fn generate_mock_embedding(text: &str) -> Vec<f32> {
    // Generate deterministic but text-dependent embedding
    let mut embedding = vec![0.0f32; 384];
    let hash = simple_hash(text);

    for (i, val) in embedding.iter_mut().enumerate() {
        let seed = hash.wrapping_add(i as u64);
        *val = ((seed % 1000) as f32 / 1000.0) - 0.5;
    }

    // Normalize
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut embedding {
            *val /= norm;
        }
    }

    embedding
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

fn create_stream_chunks(
    text: &str,
    model: &str,
) -> impl Stream<Item = Result<axum::response::sse::Event, Infallible>> {
    let words: Vec<String> = text.split_whitespace().map(String::from).collect();
    let model = model.to_string();
    let model_final = model.clone();

    stream::iter(words.into_iter().enumerate().map(move |(i, word)| {
        let chunk = serde_json::json!({
            "model": model,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "response": format!("{} ", word),
            "done": false
        });
        Ok(axum::response::sse::Event::default().data(chunk.to_string()))
    }).chain(std::iter::once({
        let final_chunk = serde_json::json!({
            "model": model_final,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "response": "",
            "done": true,
            "total_duration": 150_000_000u64,
            "eval_count": 50
        });
        Ok(axum::response::sse::Event::default().data(final_chunk.to_string()))
    })))
}

fn create_tgi_stream_chunks(
    text: &str,
) -> impl Stream<Item = Result<axum::response::sse::Event, Infallible>> {
    let words: Vec<String> = text.split_whitespace().map(String::from).collect();
    let total_tokens = words.len();

    stream::iter(words.into_iter().enumerate().map(move |(i, word)| {
        let is_last = i == total_tokens - 1;
        let chunk = serde_json::json!({
            "token": {
                "id": i,
                "text": format!("{} ", word),
                "logprob": -0.5,
                "special": false
            },
            "generated_text": if is_last { Some(word.clone()) } else { None },
            "details": if is_last {
                Some(serde_json::json!({
                    "finish_reason": "eos_token",
                    "generated_tokens": total_tokens,
                    "seed": 42
                }))
            } else {
                None
            }
        });
        Ok(axum::response::sse::Event::default().data(chunk.to_string()))
    }))
}
