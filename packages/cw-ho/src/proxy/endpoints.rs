//! HTTP endpoint handlers for the LLM proxy.

use crate::proxy::capture::{create_capture_service, CaptureMessage};
use crate::proxy::session::{detect_client_type, extract_api_key, extract_session_id};
use crate::proxy::streaming::{create_anthropic_sse_stream, create_openai_sse_stream};
use crate::proxy::upstream::{create_upstream_client, forward_to_anthropic, forward_to_openai};
use crate::ErgorsAppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use ho_std::traits::ApiKeyProvider;
use ho_std::types::ergors::proxy::v1::{ProxyApiFormat, QueryProxySessionsRequest};
use tokio::sync::{mpsc, OnceCell};
use tracing::{debug, error, info, warn};

/// Global capture service sender (initialized on first use)
static CAPTURE_TX: OnceCell<mpsc::UnboundedSender<CaptureMessage>> = OnceCell::const_new();

/// Get or initialize the capture service.
async fn get_capture_tx(state: &ErgorsAppState) -> &mpsc::UnboundedSender<CaptureMessage> {
    CAPTURE_TX
        .get_or_init(|| async {
            info!("Initializing proxy capture service");
            create_capture_service(state.s.clone())
        })
        .await
}

/// Convert reqwest status code to axum status code.
fn convert_status(status: reqwest::StatusCode) -> StatusCode {
    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

/// Handle Anthropic API proxy requests (/v1/messages).
pub async fn handle_anthropic_proxy(
    State(state): State<ErgorsAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session_id = extract_session_id(&headers);
    debug!("Anthropic proxy request, session: {}", session_id);

    // Parse request to extract model and check if streaming
    let (model, is_streaming) = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(json) => {
            let model = json
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let streaming = json
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            (model, streaming)
        }
        Err(e) => {
            error!("Failed to parse Anthropic request: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "message": format!("Invalid request body: {}", e),
                        "type": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // Detect client type
    let client_type = detect_client_type(&headers, Some(&model));

    // Extract API key (passthrough from client or use configured)
    let api_key = match extract_api_key(&headers, true) {
        Some(key) => key,
        None => {
            // Try to get from configured API keys via environment
            match std::env::var("ANTHROPIC_API_KEY") {
                Ok(key) if !key.is_empty() => key,
                _ => {
                    error!("No API key available for Anthropic");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": {
                                "message": "No API key provided. Set x-api-key header or ANTHROPIC_API_KEY env var",
                                "type": "authentication_error"
                            }
                        })),
                    )
                        .into_response();
                }
            }
        }
    };

    // Get capture service
    let capture_tx = get_capture_tx(&state).await;

    // Start capture session
    let _ = capture_tx.send(CaptureMessage::SessionStart {
        session_id: session_id.clone(),
        raw_request: body.to_vec(),
        api_format: ProxyApiFormat::Anthropic,
        client_type,
        model: model.clone(),
    });

    // Extract additional headers
    let anthropic_version = headers
        .get("anthropic-version")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let anthropic_beta = headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Create upstream client and forward request
    let client = create_upstream_client();
    let response = match forward_to_anthropic(
        &client,
        body,
        &api_key,
        anthropic_version.as_deref(),
        anthropic_beta.as_deref(),
    )
    .await
    {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to forward to Anthropic: {}", e);
            let _ = capture_tx.send(CaptureMessage::SessionError {
                session_id,
                error_message: e.to_string(),
            });
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "message": format!("Upstream error: {}", e),
                        "type": "api_error"
                    }
                })),
            )
                .into_response();
        }
    };

    let status = response.status();

    // Check for error response
    if !status.is_success() {
        let error_body = response.bytes().await.unwrap_or_default();
        let _ = capture_tx.send(CaptureMessage::SessionError {
            session_id,
            error_message: format!("HTTP {}: {}", status, String::from_utf8_lossy(&error_body)),
        });
        return Response::builder()
            .status(convert_status(status))
            .header("content-type", "application/json")
            .body(Body::from(error_body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // Handle streaming vs non-streaming response
    if is_streaming {
        info!(
            "Starting Anthropic streaming response for session {}",
            session_id
        );
        create_anthropic_sse_stream(response, session_id, capture_tx.clone()).into_response()
    } else {
        // Non-streaming: capture and forward response
        let response_body = response.bytes().await.unwrap_or_default();

        // Extract token counts from response
        let (input_tokens, output_tokens) =
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response_body) {
                let usage = json.get("usage");
                let input = usage
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                (input, output)
            } else {
                (0, 0)
            };

        let _ = capture_tx.send(CaptureMessage::SessionComplete {
            session_id,
            final_response: response_body.to_vec(),
            input_tokens,
            output_tokens,
        });

        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(response_body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// Handle OpenAI API proxy requests (/v1/chat/completions).
pub async fn handle_openai_proxy(
    State(state): State<ErgorsAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session_id = extract_session_id(&headers);
    debug!("OpenAI proxy request, session: {}", session_id);

    // Parse request to extract model and check if streaming
    let (model, is_streaming) = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(json) => {
            let model = json
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let streaming = json
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            (model, streaming)
        }
        Err(e) => {
            error!("Failed to parse OpenAI request: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "message": format!("Invalid request body: {}", e),
                        "type": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // Detect client type
    let client_type = detect_client_type(&headers, Some(&model));

    // Extract API key
    let api_key = match extract_api_key(&headers, false) {
        Some(key) => key,
        None => {
            // Try to get from configured API keys via environment
            match std::env::var("OPENAI_API_KEY") {
                Ok(key) if !key.is_empty() => key,
                _ => {
                    error!("No API key available for OpenAI");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": {
                                "message": "No API key provided. Set Authorization header or OPENAI_API_KEY env var",
                                "type": "authentication_error"
                            }
                        })),
                    )
                        .into_response();
                }
            }
        }
    };

    // Get capture service
    let capture_tx = get_capture_tx(&state).await;

    // Start capture session
    let _ = capture_tx.send(CaptureMessage::SessionStart {
        session_id: session_id.clone(),
        raw_request: body.to_vec(),
        api_format: ProxyApiFormat::Openai,
        client_type,
        model: model.clone(),
    });

    // Extract organization header if present
    let organization = headers
        .get("openai-organization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Create upstream client and forward request
    let client = create_upstream_client();
    let response = match forward_to_openai(&client, body, &api_key, organization.as_deref()).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to forward to OpenAI: {}", e);
            let _ = capture_tx.send(CaptureMessage::SessionError {
                session_id,
                error_message: e.to_string(),
            });
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "message": format!("Upstream error: {}", e),
                        "type": "api_error"
                    }
                })),
            )
                .into_response();
        }
    };

    let status = response.status();

    // Check for error response
    if !status.is_success() {
        let error_body = response.bytes().await.unwrap_or_default();
        let _ = capture_tx.send(CaptureMessage::SessionError {
            session_id,
            error_message: format!("HTTP {}: {}", status, String::from_utf8_lossy(&error_body)),
        });
        return Response::builder()
            .status(convert_status(status))
            .header("content-type", "application/json")
            .body(Body::from(error_body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // Handle streaming vs non-streaming response
    if is_streaming {
        info!(
            "Starting OpenAI streaming response for session {}",
            session_id
        );
        create_openai_sse_stream(response, session_id, capture_tx.clone()).into_response()
    } else {
        // Non-streaming: capture and forward response
        let response_body = response.bytes().await.unwrap_or_default();

        // Extract token counts from response
        let (input_tokens, output_tokens) =
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response_body) {
                let usage = json.get("usage");
                let input = usage
                    .and_then(|u| u.get("prompt_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage
                    .and_then(|u| u.get("completion_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                (input, output)
            } else {
                (0, 0)
            };

        let _ = capture_tx.send(CaptureMessage::SessionComplete {
            session_id,
            final_response: response_body.to_vec(),
            input_tokens,
            output_tokens,
        });

        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(response_body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// Query captured proxy sessions.
pub async fn handle_query_sessions(
    State(state): State<ErgorsAppState>,
    Query(params): Query<serde_json::Value>,
) -> Json<serde_json::Value> {
    let query = QueryProxySessionsRequest {
        client_type: params
            .get("client_type")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0),
        api_format: params
            .get("api_format")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0),
        model: params
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        after: None,
        before: None,
        limit: params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(100),
        offset: params
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0),
        include_chunks: params
            .get("include_chunks")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };

    match state.s.query_proxy_sessions(&query).await {
        Ok(sessions) => Json(serde_json::json!({
            "sessions": sessions,
            "total_count": sessions.len(),
            "has_more": false
        })),
        Err(e) => {
            error!("Failed to query proxy sessions: {}", e);
            Json(serde_json::json!({
                "error": format!("Failed to query sessions: {}", e),
                "sessions": [],
                "total_count": 0
            }))
        }
    }
}

/// Get a specific proxy session by ID.
pub async fn handle_get_session(
    State(state): State<ErgorsAppState>,
    Path(session_id): Path<String>,
    Query(params): Query<serde_json::Value>,
) -> Json<serde_json::Value> {
    let include_chunks = params
        .get("include_chunks")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    match state.s.get_proxy_session(&session_id).await {
        Ok(Some(mut session)) => {
            if !include_chunks {
                session.chunks.clear();
            }
            Json(serde_json::json!({
                "session": session
            }))
        }
        Ok(None) => Json(serde_json::json!({
            "error": "Session not found",
            "session": null
        })),
        Err(e) => {
            error!("Failed to get proxy session {}: {}", session_id, e);
            Json(serde_json::json!({
                "error": format!("Failed to get session: {}", e),
                "session": null
            }))
        }
    }
}
