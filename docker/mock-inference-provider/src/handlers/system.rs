use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
};
use std::sync::atomic::Ordering;

use crate::state::AppState;

pub async fn root_handler() -> Response {
    Json(serde_json::json!({
        "name": "Mock Inference Provider",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Simulates Ollama, OpenAI, Anthropic, and TGI APIs for testing",
        "endpoints": {
            "ollama": ["/api/generate", "/api/chat", "/api/tags", "/api/pull", "/api/show", "/api/embeddings"],
            "openai": ["/v1/completions", "/v1/chat/completions", "/v1/models", "/v1/embeddings"],
            "anthropic": ["/v1/messages"],
            "tgi": ["/generate", "/generate_stream", "/info"],
            "api_keys": ["/api/keys/generate", "/api/keys/validate", "/api/keys/list", "/api/keys/revoke"],
            "system": ["/health", "/metrics"]
        }
    }))
    .into_response()
}

pub async fn health_handler() -> Response {
    Json(serde_json::json!({"status": "ok"})).into_response()
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.request_count.load(Ordering::Relaxed);

    Json(serde_json::json!({
        "total_requests": count,
        "models_loaded": state.config.models.len()
    }))
}
