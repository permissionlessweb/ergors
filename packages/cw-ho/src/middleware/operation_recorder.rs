use crate::ErgorsAppState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use ho_std::llm::HoError;
use http_body_util::BodyExt;

use tracing::{error, info, warn};
use uuid::Uuid;

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
    let operation_type = classify_operation(&endpoint);

    // Set span fields for tracing context
    tracing::Span::current().record("operation_id", &operation_id.as_str());
    tracing::Span::current().record("operation_type", &operation_type.as_str());
    tracing::Span::current().record("endpoint", &endpoint.as_str());

    // Extract session_id if present (could come from headers or body)
    let session_id = extract_session_id(&req);

    info!(
        operation_id = %operation_id,
        method = %method,
        endpoint = %endpoint,
        operation_type = %operation_type,
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

    // Store the request
    if let Err(e) = state
        .s
        .op_req(
            &operation_id,
            &operation_type,
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

    // Process the request
    let response = next.run(req).await;

    // Capture response
    let (parts, body) = response.into_parts();
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

/// Classify operation type based on endpoint path
fn classify_operation(endpoint: &str) -> String {
    match endpoint {
        p if p.contains("/api/prompt") => "prompt".to_string(),
        p if p.contains("/orchestrate/bootstrap") => "bootstrap".to_string(),
        p if p.contains("/orchestrate/fractal") => "fractal".to_string(),
        p if p.contains("/orchestrate/prune") => "prune".to_string(),
        p if p.contains("/network/topology") => "topology".to_string(),
        p if p.contains("/health") => "health".to_string(),
        p if p.contains("/api/prompts") => "query".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Extract session ID from request (from headers or body)
fn extract_session_id(_req: &Request) -> Option<String> {
    // TODO: Implement extraction from headers or parsed body
    // For now, return None
    None
}
