use axum::{extract::State, response::IntoResponse, Json};

use crate::state::AppState;

/// Returns the headers captured from the last OpenAI /v1/chat/completions request.
/// Used by E2E tests to verify whether Authorization headers are forwarded.
pub async fn last_headers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let headers = state.last_request_headers.read().await;
    Json(serde_json::json!({
        "headers": *headers
    }))
}
