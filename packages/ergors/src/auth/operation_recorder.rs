use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use bytes::Bytes;

use http_body_util::BodyExt;

use tracing::{error, info, warn};
use uuid::Uuid;

use crate::ErgorsAppState;
use ho_std::llm::HoError;

/// Middleware that automatically records all request/response pairs to storage
#[tracing::instrument(skip(state, req, next), fields(operation_id, operation_type, endpoint))]
pub async fn record_operation(
    State(state): State<ErgorsAppState>,
    req: Request,
    next: Next,
) -> Response {
    let operation_id = Uuid::new_v4().to_string();
    let endpoint = req.uri().path().to_string();
    let method = req.method().to_string();

    // Set span fields for tracing context
    tracing::Span::current().record("operation_id", operation_id.as_str());
    tracing::Span::current().record("endpoint", endpoint.as_str());

    // Try to extract session_id from headers first
    let header_session_id = extract_session_id(&req);

    info!(
        operation_id = %operation_id,
        method = %method,
        endpoint = %endpoint,
        "🚀 Request received"
    );

    // Capture request body
    let (parts, body) = req.into_parts();
    let request_bytes = match capture_body(body).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to capture request body: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Try to extract session_id from body if not found in headers
    let session_id = header_session_id.or_else(|| {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&request_bytes) {
            extract_session_id_from_body(&json)
        } else {
            None
        }
    });

    // Store the request
    if let Err(e) = state
        .s
        .op_req(
            &operation_id,
            &method,
            &endpoint,
            request_bytes.to_vec(),
            session_id,
        )
        .await
    {
        error!("Failed to store operation request: {}", e);
    }

    // Reconstruct request with captured body
    let req = Request::from_parts(parts, Body::from(request_bytes));
    let res = next.run(req).await;

    // Capture response
    let (parts, body) = res.into_parts();
    let response_bytes = match capture_body(body).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to capture response body: {}", e);
            // Record the error
            if let Err(storage_err) = state
                .s
                .op_err(
                    &operation_id,
                    &format!("Failed to capture response: {}", e),
                    "RESPONSE_CAPTURE_ERROR",
                    None,
                )
                .await
            {
                error!("Failed to store operation error: {}", storage_err);
            }
            return Response::from_parts(parts, Body::from(e.to_string()));
        }
    };

    // Store response or error based on status code
    if parts.status.is_success() {
        info!(
            operation_id = %operation_id,
            status = %parts.status,
            "✅ Request completed successfully"
        );

        if let Err(e) = state.s.op_res(&operation_id, response_bytes.to_vec()).await {
            error!(
                operation_id = %operation_id,
                error = %e,
               "Failed to store operation response"
            );
        }
    } else {
        // Extract error from response if possible
        let error_msg = String::from_utf8_lossy(&response_bytes).to_string();

        // Use format_error_trace if this is a HoError in the response
        let stack_trace =
            if let Ok(error_json) = serde_json::from_slice::<serde_json::Value>(&response_bytes) {
                error_json
                    .get("error_chain")
                    .and_then(|chain| serde_json::to_string_pretty(chain).ok())
            } else {
                None
            };

        warn!(
            operation_id = %operation_id,
            status = %parts.status,
            error = %error_msg,
            stack_trace = ?stack_trace,
            "⚠️  Request failed"
        );

        if let Err(e) = state
            .s
            .op_err(
                &operation_id,
                &error_msg,
                &parts.status.to_string(),
                stack_trace,
            )
            .await
        {
            error!(
                operation_id = %operation_id,
                error = %e,
                "Failed to store operation error"
            );
        }
    }

    Response::from_parts(parts, Body::from(response_bytes))
}

/// Capture body bytes from a request or response
async fn capture_body(body: Body) -> Result<Bytes, HoError> {
    body.collect()
        .await
        .map(|collected| collected.to_bytes())
        .map_err(|e| HoError::Cfg(format!("Failed to read body: {}", e)))
}

/// Extract a non-empty header value by key.
fn get_header(req: &Request, key: &str) -> Option<String> {
    req.headers()
        .get(key)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract session ID from request, checking x-session-id header.
fn extract_session_id(req: &Request) -> Option<String> {
    get_header(req, "x-session-id")
}

/// Extract session ID from parsed request body (for use when body is available).
///
/// This can be called after the body has been captured and parsed.
pub fn extract_session_id_from_body(body: &serde_json::Value) -> Option<String> {
    // Check for previous_response_id (Open Responses format)
    if let Some(prev_id) = body.get("previous_response_id").and_then(|v| v.as_str()) {
        if !prev_id.is_empty() {
            return Some(format!("resp_{}", &prev_id[..12.min(prev_id.len())]));
        }
    }

    // Check for metadata.session_id (custom format)
    if let Some(metadata) = body.get("metadata") {
        if let Some(session_id) = metadata.get("session_id").and_then(|v| v.as_str()) {
            if !session_id.is_empty() {
                return Some(session_id.to_string());
            }
        }
    }

    // Check for conversation_id (common in some APIs)
    if let Some(conv_id) = body.get("conversation_id").and_then(|v| v.as_str()) {
        if !conv_id.is_empty() {
            return Some(conv_id.to_string());
        }
    }

    None
}
