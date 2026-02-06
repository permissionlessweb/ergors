//! Cosmic Orchestrator Module - Python-to-Rust Migration
//!
//! This module implements the complete AgentOrchestrator functionality from orchestrator.py,
//! incorporating cosmic/geometric principles and fractal recursion for AI agent orchestration.

use crate::open_responses::{
    converters::{filter_tools, parse_open_responses_request, prompt_response_to_open_responses},
    error::OpenResponsesError,
    streaming::OpenResponsesStreamTransformer,
};
use crate::proxy::upstream::{forward_to_anthropic, forward_to_openai};
use crate::ErgorsAppState;
use async_stream::stream;
use axum::{
    extract::State,
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use bytes::Bytes;
use commonware_cryptography::{blake3, Hasher};

use ho_std::error::error_json;
use ho_std::{error::error_json_detailed, types::ergors::orch::v1::*};
use std::convert::Infallible;
use tracing::{debug, error, info};
use uuid::Uuid;

pub async fn handle_prompt(
    State(state): State<ErgorsAppState>,
    Json(r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let pr = match serde_json::from_value::<PromptRequest>(r.clone()) {
        Ok(r) => r,
        Err(e) => {
            return Json(error_json(
                &format!(
                    "Invalid request format: {}. Expected format: {:#?}",
                    e,
                    PromptRequest::default()
                ),
                "INVALID_REQUEST",
            ));
        }
    };

    if pr.messages.is_empty() {
        return Json(error_json(
            "Prompt messages cannot be empty",
            "INVALID_PROMPT",
        ));
    }

    // Check if Open Responses format is requested
    let use_open_responses = pr
        .response_format
        .as_deref()
        .map(|f| f == "open_responses")
        .unwrap_or(false);

    match state.r.handle_request(&pr, &pr.model).await {
        Ok(llm_response) => {
            if use_open_responses {
                // Return in Open Responses format
                let response_id = format!("resp_{}", Uuid::new_v4().simple());
                let json = prompt_response_to_open_responses(&llm_response, &response_id);
                Json(json)
            } else {
                // Return in original ERGORS format
                let response = PromptResponse {
                    id: Uuid::new_v4().into(),
                    prompt: blake3::Blake3::hash(serde_json::to_string(&pr).unwrap().as_bytes())
                        .to_string(),
                    response: llm_response.response,
                    model: pr.model.to_string(),
                    timestamp: Some(pbjson_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    tokens_used: llm_response.tokens_used,
                    provider: "default".to_string(),
                    cost: 0.0,
                    latency_ms: 0,
                    status: None,
                    output: vec![],
                    response_metadata: None,
                };

                // Store to Cnidarium with original request context
                if let Err(e) = state.s.put_prompt_w_ctx(&response, Some(&pr)).await {
                    error!("Failed to store prompt to storage: {}", e);
                }

                Json(serde_json::to_value(response).unwrap())
            }
        }
        Err(e) => {
            if use_open_responses {
                // Return Open Responses error format
                Json(
                    OpenResponsesError::ModelError {
                        message: format!("LLM processing failed: {}", e),
                    }
                    .to_json(),
                )
            } else {
                // Log error with full chain if detail enabled
                let error_chain = e.error_chain();
                error!(
                    error_type = e.to_string(),
                    error = %e,
                    error_chain = ?error_chain,
                    root_cause = ?error_chain.last(),
                    "LLM processing failed"
                );
                // Use detailed error response which respects RUST_LOG_DETAIL env
                Json(error_json_detailed(&e))
            }
        }
    }
}

/// Handle Open Responses API requests (/v1/responses).
/// Supports both streaming and non-streaming modes with standardized events.
pub async fn handle_open_responses(
    State(state): State<ErgorsAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Parse the request body
    let req_json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return OpenResponsesError::InvalidRequest {
                param: "body".to_string(),
                message: format!("Invalid JSON: {}", e),
            }
            .into_response();
        }
    };

    // Convert to PromptRequest
    let mut prompt_req = match parse_open_responses_request(&req_json) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    // Handle previous_response_id - load conversation context
    if let Some(ref prev_id) = prompt_req.previous_response_id {
        let prev_messages = state
            .s
            .get_open_response_context(prev_id)
            .await
            .unwrap_or_default();
        if !prev_messages.is_empty() {
            debug!(
                "Loaded {} previous messages from response {}",
                prev_messages.len(),
                prev_id
            );
            let mut combined = prev_messages;
            combined.append(&mut prompt_req.messages);
            prompt_req.messages = combined;
        }
    }

    // Apply allowed_tools filtering
    if !prompt_req.allowed_tools.is_empty() {
        prompt_req.tools = filter_tools(&prompt_req.tools, &prompt_req.allowed_tools);
    }

    // Generate response ID
    let response_id = format!("resp_{}", Uuid::new_v4().simple());
    let model = prompt_req.model.clone();

    if prompt_req.stream {
        // Streaming path: forward to upstream provider and transform events
        handle_open_responses_streaming(state, headers, prompt_req, response_id, model).await
    } else {
        // Non-streaming path: use LlmRouter
        handle_open_responses_non_streaming(state, prompt_req, response_id).await
    }
}

/// Non-streaming Open Responses handler
async fn handle_open_responses_non_streaming(
    state: ErgorsAppState,
    prompt_req: PromptRequest,
    response_id: String,
) -> Response {
    match state.r.handle_request(&prompt_req, &prompt_req.model).await {
        Ok(llm_response) => {
            // Store response for future previous_response_id lookups
            if let Err(e) = state
                .s
                .put_open_response(&response_id, &prompt_req, &llm_response.response)
                .await
            {
                error!("Failed to store open response session: {}", e);
            }

            let json = prompt_response_to_open_responses(&llm_response, &response_id);
            Json(json).into_response()
        }
        Err(e) => OpenResponsesError::ModelError {
            message: format!("LLM processing failed: {}", e),
        }
        .into_response(),
    }
}

/// Streaming Open Responses handler - forwards to upstream and transforms events
async fn handle_open_responses_streaming(
    _state: ErgorsAppState,
    headers: HeaderMap,
    prompt_req: PromptRequest,
    response_id: String,
    model: String,
) -> Response {
    // Determine provider type from model name
    let is_anthropic = is_anthropic_model(&model);

    // Extract API key from headers
    let api_key = if is_anthropic {
        headers
            .get("x-api-key")
            .or_else(|| headers.get("authorization"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.strip_prefix("Bearer ").unwrap_or(s).to_string())
    } else {
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    };

    let api_key = match api_key {
        Some(k) => k,
        None => {
            return OpenResponsesError::InvalidRequest {
                param: "authorization".to_string(),
                message: "API key required for streaming. Provide via Authorization header."
                    .to_string(),
            }
            .into_response();
        }
    };

    // Build upstream request body in native provider format
    let upstream_body = if is_anthropic {
        build_anthropic_request(&prompt_req)
    } else {
        build_openai_request(&prompt_req)
    };

    let upstream_bytes = Bytes::from(serde_json::to_vec(&upstream_body).unwrap_or_default());

    // Create HTTP client
    let client = reqwest::Client::new();

    // Forward to upstream
    let upstream_response = if is_anthropic {
        forward_to_anthropic(&client, upstream_bytes, &api_key, None, None).await
    } else {
        forward_to_openai(&client, upstream_bytes, &api_key, None).await
    };

    let response = match upstream_response {
        Ok(r) => r,
        Err(e) => {
            return OpenResponsesError::ServerError {
                message: format!("Upstream request failed: {}", e),
            }
            .into_response();
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return OpenResponsesError::ModelError {
            message: format!("Upstream error ({}): {}", status, body_text),
        }
        .into_response();
    }

    // Create streaming transformer and emit Open Responses events
    let mut transformer = OpenResponsesStreamTransformer::new(response_id, model.clone());

    let sse_stream = stream! {
        let body_text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading upstream stream: {}", e);
                yield Ok::<Event, Infallible>(Event::default()
                    .event("response.failed")
                    .data(serde_json::to_string(&OpenResponsesError::ServerError {
                        message: format!("Stream read error: {}", e),
                    }.to_json()).unwrap_or_default()));
                return;
            }
        };

        // Parse SSE events
        let mut buffer = body_text;
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            let mut event_type = String::new();
            let mut data = String::new();

            for line in event_data.lines() {
                if let Some(et) = line.strip_prefix("event: ") {
                    event_type = et.to_string();
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data = d.to_string();
                }
            }

            if data.is_empty() {
                continue;
            }

            // Transform based on provider type
            let events = if is_anthropic {
                transformer.transform_anthropic_event(&event_type, &data)
            } else {
                transformer.transform_openai_chunk(&data)
            };

            for evt in events {
                yield Ok::<Event, Infallible>(evt);
            }
        }

        // Emit terminal [DONE] event
        yield Ok::<Event, Infallible>(OpenResponsesStreamTransformer::done_event());
    };

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Determine if a model is an Anthropic model based on name
fn is_anthropic_model(model: &str) -> bool {
    model.starts_with("claude")
        || model.contains("anthropic")
        || model.contains("haiku")
        || model.contains("sonnet")
        || model.contains("opus")
}

/// Build an Anthropic Messages API request from a PromptRequest
fn build_anthropic_request(req: &PromptRequest) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "max_tokens": req.llm_config.as_ref().map(|c| c.max_tokens).unwrap_or(4096),
        "stream": true,
    });

    if !req.system.is_empty() {
        body["system"] = serde_json::json!(req.system);
    }

    if !req.tools.is_empty() {
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .filter_map(|t| {
                t.function.as_ref().map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "description": f.description,
                        "input_schema": {"type": "object"},
                    })
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    body
}

/// Build an OpenAI Chat Completions request from a PromptRequest
fn build_openai_request(req: &PromptRequest) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    if !req.system.is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": req.system,
        }));
    }

    for m in &req.messages {
        messages.push(serde_json::json!({
            "role": m.role,
            "content": m.content,
        }));
    }

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
    });

    if let Some(cfg) = &req.llm_config {
        if cfg.max_tokens > 0 {
            body["max_tokens"] = serde_json::json!(cfg.max_tokens);
        }
    }

    if !req.tools.is_empty() {
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .filter_map(|t| {
                t.function.as_ref().map(|f| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": f.name,
                            "description": f.description,
                            "parameters": {"type": "object"},
                        }
                    })
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    body
}

pub async fn handle_fractal_hoe_creation(// State(_state): State<ErgorsAppState>,
    // Json(request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    info!("🌀 Creating fractal hoe");
    //TODO: boostrap new node via desired method
    // Create persistent SSH connection manager
    // let mut ssh_manager = SSHConnectionManager::new(target_node.to_string());
    info!("🔌 Step 1: Establishing persistent SSH connection");
    // match ssh_manager.connect().await {}
    info!("🛠️  Step 2: Installing development environment on target node");
    // ssh_manager.install_dev_environment_via_ssh(&mut ssh_manager).await
    info!("📊  Step 3: Closing SSH connection before returning");
    // Close SSH connection before returning
    // let _ = ssh_manager.close().await;
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}
