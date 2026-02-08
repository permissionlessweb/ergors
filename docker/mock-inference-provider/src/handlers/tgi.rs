use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
};
use futures::stream::{self, Stream};
use std::convert::Infallible;

use crate::state::{bump, simulate_latency, should_error, AppState};
use crate::types::{TGIDetails, TGIGenerateRequest, TGIGenerateResponse};

pub async fn generate_handler(
    State(state): State<AppState>,
    Json(_req): Json<TGIGenerateRequest>,
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

    let resp = TGIGenerateResponse {
        generated_text: "This is a mock response from the TGI API.".to_string(),
        details: Some(TGIDetails {
            finish_reason: "eos_token".to_string(),
            generated_tokens: 25,
            seed: Some(42),
        }),
    };

    Json(resp).into_response()
}

pub async fn stream_handler(
    State(state): State<AppState>,
    Json(_req): Json<TGIGenerateRequest>,
) -> impl IntoResponse {
    bump(&state);

    let text = "This is a mock response from the TGI API.";
    let chunks = create_tgi_stream_chunks(text);

    Sse::new(chunks)
}

pub async fn info_handler() -> impl IntoResponse {
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
