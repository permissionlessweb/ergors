use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use tracing::info;

use crate::state::{bump, simulate_latency, should_error, AppState};
use crate::types::{
    ChatMessage, EmbeddingsRequest, EmbeddingsResponse, OllamaChatRequest, OllamaChatResponse,
    OllamaGenerateRequest, OllamaGenerateResponse, StringOrVec,
};

pub async fn generate_handler(
    State(state): State<AppState>,
    Json(req): Json<OllamaGenerateRequest>,
) -> Response {
    bump(&state);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        )
            .into_response();
    }

    let response_text = "This is a mock response from the Ollama Generate API.";

    if req.stream {
        let chunks = create_stream_chunks(response_text, &req.model);
        return Sse::new(chunks).into_response();
    }

    let resp = OllamaGenerateResponse {
        model: req.model,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        response: response_text.to_string(),
        done: true,
        context: vec![1, 2, 3],
        total_duration: 150_000_000,
        load_duration: 10_000_000,
        prompt_eval_count: 10,
        prompt_eval_duration: 50_000_000,
        eval_count: 25,
        eval_duration: 90_000_000,
    };

    Json(resp).into_response()
}

pub async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<OllamaChatRequest>,
) -> Response {
    bump(&state);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Simulated inference error"})),
        )
            .into_response();
    }

    let resp = OllamaChatResponse {
        model: req.model,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        message: ChatMessage {
            role: "assistant".to_string(),
            content: "This is a mock response from the Ollama Chat API.".to_string(),
            tool_calls: None,
        },
        done: true,
        total_duration: 150_000_000,
        load_duration: 10_000_000,
        prompt_eval_count: 10,
        eval_count: 25,
    };

    Json(resp).into_response()
}

pub async fn tags_handler(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<serde_json::Value> = state
        .config
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "model": m.model_id,
                "modified_at": "2024-01-01T00:00:00Z",
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
        })
        .collect();

    Json(serde_json::json!({"models": models}))
}

pub async fn pull_handler(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let model = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    info!("Mock pull request for model: {}", model);

    Json(serde_json::json!({
        "status": "success",
        "digest": format!("sha256:{}", hex::encode(model.as_bytes()))
    }))
}

pub async fn show_handler(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let model_name = req.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

    let model = state
        .config
        .models
        .iter()
        .find(|m| m.name == model_name || m.model_id == model_name)
        .cloned()
        .unwrap_or_else(|| crate::types::ModelInfo {
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

pub async fn embeddings_handler(
    State(state): State<AppState>,
    Json(req): Json<EmbeddingsRequest>,
) -> impl IntoResponse {
    bump(&state);
    simulate_latency(&state.config).await;

    let prompts = match req.prompt {
        StringOrVec::Single(s) => vec![s],
        StringOrVec::Multiple(v) => v,
    };

    let embeddings: Vec<Vec<f32>> = prompts.iter().map(|_| vec![0.0f32; 384]).collect();

    Json(EmbeddingsResponse {
        model: req.model,
        embeddings,
    })
}

fn create_stream_chunks(
    text: &str,
    model: &str,
) -> impl Stream<Item = Result<axum::response::sse::Event, Infallible>> {
    let words: Vec<String> = text.split_whitespace().map(String::from).collect();
    let model = model.to_string();
    let model_final = model.clone();

    stream::iter(
        words
            .into_iter()
            .map(move |word| {
                let chunk = serde_json::json!({
                    "model": model,
                    "created_at": "2024-01-01T00:00:00Z",
                    "response": format!("{} ", word),
                    "done": false
                });
                Ok(axum::response::sse::Event::default().data(chunk.to_string()))
            })
            .chain(std::iter::once({
                let final_chunk = serde_json::json!({
                    "model": model_final,
                    "created_at": "2024-01-01T00:00:00Z",
                    "response": "",
                    "done": true,
                    "total_duration": 150_000_000u64,
                    "eval_count": 25
                });
                Ok(axum::response::sse::Event::default().data(final_chunk.to_string()))
            })),
    )
}
