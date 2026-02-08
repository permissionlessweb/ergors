use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::state::{bump, simulate_latency, should_error, AppState};
use crate::types::{
    AnthropicContentBlock, AnthropicMessagesRequest, AnthropicMessagesResponse, AnthropicUsage,
};

pub async fn messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AnthropicMessagesRequest>,
) -> Response {
    // Validate x-api-key header
    let has_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    if !has_key {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "type": "error",
                "error": {
                    "type": "authentication_error",
                    "message": "Missing or empty x-api-key header"
                }
            })),
        )
            .into_response();
    }

    bump(&state);
    simulate_latency(&state.config).await;

    if should_error(&state.config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "type": "error",
                "error": {
                    "type": "api_error",
                    "message": "Simulated inference error"
                }
            })),
        )
            .into_response();
    }

    let resp = AnthropicMessagesResponse {
        id: "msg-mock-001".to_string(),
        r#type: "message".to_string(),
        role: "assistant".to_string(),
        content: vec![AnthropicContentBlock {
            r#type: "text".to_string(),
            text: "This is a mock response from the Anthropic Messages API.".to_string(),
        }],
        model: req.model,
        stop_reason: "end_turn".to_string(),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens: 10,
            output_tokens: 25,
        },
    };

    Json(resp).into_response()
}
