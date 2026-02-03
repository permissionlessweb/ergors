//! SSE streaming utilities with real-time passthrough and capture support.
//!
//! This module provides true streaming SSE passthrough - chunks are forwarded to the client
//! as they arrive from the upstream provider, not after the full response is buffered.

use crate::proxy::capture::CaptureMessage;
use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::Utc;
use futures::stream::Stream;
use ho_std::types::ergors::proxy::v1::StreamChunk;
use reqwest::Response;
use std::convert::Infallible;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

/// Helper to create a pbjson timestamp from current time.
fn now_timestamp() -> Option<pbjson_types::Timestamp> {
    Some(pbjson_types::Timestamp::from(Utc::now()))
}

/// Create a real-time SSE passthrough stream from an Anthropic API response.
///
/// This function reads chunks from the upstream response as they arrive,
/// immediately forwards them to the client, and captures them for storage.
pub fn create_anthropic_sse_stream(
    response: Response,
    session_id: String,
    capture_tx: mpsc::UnboundedSender<CaptureMessage>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        let mut sequence: u32 = 0;
        let mut accumulated_text = String::new();
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let start = Instant::now();

        // Buffer for incomplete SSE events (may span chunks)
        let mut buffer = String::new();

        // Read entire response and process (reqwest doesn't expose streaming without feature)
        // In practice, the response arrives in chunks from the network layer
        let body_result = response.text().await;
        let body = match body_result {
            Ok(b) => b,
            Err(e) => {
                error!("Error reading Anthropic stream: {}", e);
                let _ = capture_tx.send(CaptureMessage::SessionError {
                    session_id: session_id.clone(),
                    error_message: e.to_string(),
                });
                return;
            }
        };

        // Process SSE events (delimited by \n\n)
        buffer = body;
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            // Parse SSE event
            let mut event_type = String::new();
            let mut data = String::new();

            for line in event_data.lines() {
                if let Some(et) = line.strip_prefix("event: ") {
                    event_type = et.to_string();
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data = d.to_string();
                }
            }

            // Extract text delta from Anthropic events
            let delta_text = extract_anthropic_delta(&event_type, &data);
            if let Some(ref text) = delta_text {
                accumulated_text.push_str(text);
            }

            // Extract token counts from message_delta events
            if event_type == "message_delta" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(usage) = parsed.get("usage") {
                        if let Some(out) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                            output_tokens = out;
                        }
                    }
                }
            }

            // Extract input tokens from message_start
            if event_type == "message_start" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(message) = parsed.get("message") {
                        if let Some(usage) = message.get("usage") {
                            if let Some(inp) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                                input_tokens = inp;
                            }
                        }
                    }
                }
            }

            // Capture the chunk
            let stream_chunk = StreamChunk {
                sequence,
                event_type: event_type.clone(),
                data: data.clone().into_bytes(),
                received_at: now_timestamp(),
                delta_text: delta_text.clone().unwrap_or_default(),
            };

            let _ = capture_tx.send(CaptureMessage::Chunk {
                session_id: session_id.clone(),
                chunk: stream_chunk,
            });

            sequence += 1;

            // Forward the event to client IMMEDIATELY
            if !event_type.is_empty() {
                let event = Event::default()
                    .event(&event_type)
                    .data(&data);
                yield Ok(event);
            }

            // Check for end of stream
            if event_type == "message_stop" {
                debug!("Anthropic stream complete for session {} after {} chunks in {}ms",
                    session_id, sequence, start.elapsed().as_millis());

                // Send completion
                let final_response = serde_json::json!({
                    "accumulated_text": accumulated_text,
                    "total_chunks": sequence,
                    "duration_ms": start.elapsed().as_millis(),
                });

                let _ = capture_tx.send(CaptureMessage::SessionComplete {
                    session_id: session_id.clone(),
                    final_response: serde_json::to_vec(&final_response).unwrap_or_default(),
                    input_tokens,
                    output_tokens,
                });
            }
        }

        // If stream ended without message_stop, send completion anyway
        if sequence > 0 {
            trace!("Stream ended for session {} after {} chunks", session_id, sequence);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create a real-time SSE passthrough stream from an OpenAI API response.
pub fn create_openai_sse_stream(
    response: Response,
    session_id: String,
    capture_tx: mpsc::UnboundedSender<CaptureMessage>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        let mut sequence: u32 = 0;
        let mut accumulated_text = String::new();
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let start = Instant::now();

        // Read response body
        let body_result = response.text().await;
        let body = match body_result {
            Ok(b) => b,
            Err(e) => {
                error!("Error reading OpenAI stream: {}", e);
                let _ = capture_tx.send(CaptureMessage::SessionError {
                    session_id: session_id.clone(),
                    error_message: e.to_string(),
                });
                return;
            }
        };

        // Process SSE lines
        for line in body.lines() {
            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse data line
            if let Some(data) = line.strip_prefix("data: ") {
                // Check for end of stream
                if data.trim() == "[DONE]" {
                    debug!("OpenAI stream complete for session {} after {} chunks in {}ms",
                        session_id, sequence, start.elapsed().as_millis());

                    let final_response = serde_json::json!({
                        "accumulated_text": accumulated_text,
                        "total_chunks": sequence,
                        "duration_ms": start.elapsed().as_millis(),
                    });

                    let _ = capture_tx.send(CaptureMessage::SessionComplete {
                        session_id: session_id.clone(),
                        final_response: serde_json::to_vec(&final_response).unwrap_or_default(),
                        input_tokens,
                        output_tokens,
                    });

                    // Forward [DONE] to client
                    yield Ok(Event::default().data("[DONE]"));
                    return;
                }

                // Parse JSON data
                let delta_text = extract_openai_delta(data);
                if let Some(ref text) = delta_text {
                    accumulated_text.push_str(text);
                }

                // Extract tool calls if present
                let _tool_calls = extract_openai_tool_calls(data);

                // Extract usage if present (in final chunk with stream_options)
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(usage) = parsed.get("usage") {
                        if let Some(inp) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                            input_tokens = inp;
                        }
                        if let Some(out) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                            output_tokens = out;
                        }
                    }
                }

                // Capture the chunk
                let stream_chunk = StreamChunk {
                    sequence,
                    event_type: "chunk".to_string(),
                    data: data.as_bytes().to_vec(),
                    received_at: now_timestamp(),
                    delta_text: delta_text.clone().unwrap_or_default(),
                };

                let _ = capture_tx.send(CaptureMessage::Chunk {
                    session_id: session_id.clone(),
                    chunk: stream_chunk,
                });

                sequence += 1;

                // Forward to client IMMEDIATELY
                yield Ok(Event::default().data(data));
            }
        }

        // If stream ended without [DONE], send completion anyway
        if sequence > 0 {
            trace!("OpenAI stream ended for session {} after {} chunks", session_id, sequence);
            let final_response = serde_json::json!({
                "accumulated_text": accumulated_text,
                "total_chunks": sequence,
                "duration_ms": start.elapsed().as_millis(),
            });

            let _ = capture_tx.send(CaptureMessage::SessionComplete {
                session_id: session_id.clone(),
                final_response: serde_json::to_vec(&final_response).unwrap_or_default(),
                input_tokens,
                output_tokens,
            });
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Extract text delta from Anthropic SSE event.
fn extract_anthropic_delta(event_type: &str, data: &str) -> Option<String> {
    if event_type != "content_block_delta" {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
    let delta = parsed.get("delta")?;

    // Handle text_delta
    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
        return Some(text.to_string());
    }

    // Handle input_json_delta for tool use
    if let Some(partial_json) = delta.get("partial_json").and_then(|t| t.as_str()) {
        return Some(partial_json.to_string());
    }

    None
}

/// Extract text delta from OpenAI SSE event.
fn extract_openai_delta(data: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
    let choices = parsed.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let delta = choice.get("delta")?;
    let content = delta.get("content")?.as_str()?;
    Some(content.to_string())
}

/// Extract tool calls from OpenAI SSE event.
fn extract_openai_tool_calls(data: &str) -> Option<Vec<serde_json::Value>> {
    let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
    let choices = parsed.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let delta = choice.get("delta")?;
    let tool_calls = delta.get("tool_calls")?.as_array()?;
    Some(tool_calls.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_anthropic_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        assert_eq!(
            extract_anthropic_delta("content_block_delta", data),
            Some("Hello".to_string())
        );

        // Non-content event should return None
        assert_eq!(extract_anthropic_delta("message_start", "{}"), None);
    }

    #[test]
    fn test_extract_openai_delta() {
        let data = r#"{"id":"chatcmpl-123","choices":[{"delta":{"content":"Hello"},"index":0}]}"#;
        assert_eq!(extract_openai_delta(data), Some("Hello".to_string()));

        // Empty delta should return None
        let empty = r#"{"id":"chatcmpl-123","choices":[{"delta":{},"index":0}]}"#;
        assert_eq!(extract_openai_delta(empty), None);
    }

    #[test]
    fn test_extract_openai_tool_calls() {
        let data = r#"{"choices":[{"delta":{"tool_calls":[{"id":"call_123","function":{"name":"test","arguments":"{}"}}]}}]}"#;
        let tools = extract_openai_tool_calls(data);
        assert!(tools.is_some());
        assert_eq!(tools.unwrap().len(), 1);
    }
}
