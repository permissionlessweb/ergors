use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::state::{bump, simulate_latency, should_error, AppState};
use crate::types::{
    EmbeddingsRequest, OllamaChatRequest, OpenAIChoice, OpenAICompletionsRequest,
    OpenAICompletionsResponse, OpenAIUsage, StringOrVec,
};

pub async fn completions_handler(
    State(state): State<AppState>,
    Json(req): Json<OpenAICompletionsRequest>,
) -> Response {
    bump(&state);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        )
            .into_response();
    }

    let resp = OpenAICompletionsResponse {
        id: "cmpl-mock-001".to_string(),
        object: "text_completion".to_string(),
        created: 1700000000,
        model: req.model,
        choices: vec![OpenAIChoice {
            text: "This is a mock response from the OpenAI Completions API.".to_string(),
            index: 0,
            logprobs: None,
            finish_reason: "stop".to_string(),
        }],
        usage: OpenAIUsage {
            prompt_tokens: 10,
            completion_tokens: 25,
            total_tokens: 35,
        },
    };

    Json(resp).into_response()
}

pub async fn chat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OllamaChatRequest>,
) -> Response {
    bump(&state);
    simulate_latency(&state.config).await;

    // Capture headers for debug inspection
    {
        let mut last = state.last_request_headers.write().await;
        last.clear();
        for (name, value) in headers.iter() {
            if let Ok(v) = value.to_str() {
                last.insert(name.as_str().to_string(), v.to_string());
            }
        }
    }

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": "Simulated error", "type": "server_error"}})),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "id": "chatcmpl-mock-001",
        "object": "chat.completion",
        "created": 1700000000,
        "model": req.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "This is a mock response from the OpenAI Chat API."
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 25,
            "total_tokens": 35
        }
    }))
    .into_response()
}

pub async fn models_handler(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<serde_json::Value> = state
        .config
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.model_id,
                "object": "model",
                "created": 1700000000,
                "owned_by": "organization"
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": models
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

    let data: Vec<serde_json::Value> = prompts
        .iter()
        .enumerate()
        .map(|(i, _)| {
            serde_json::json!({
                "object": "embedding",
                "embedding": vec![0.0f32; 384],
                "index": i
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": data,
        "model": req.model,
        "usage": {
            "prompt_tokens": 10,
            "total_tokens": 10
        }
    }))
}
