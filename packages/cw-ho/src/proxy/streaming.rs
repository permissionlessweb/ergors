//! SSE streaming utilities with capture support.

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
use tracing::{debug, error, trace, warn};

/// Helper to create a pbjson timestamp from current time.
fn now_timestamp() -> Option<pbjson_types::Timestamp> {
    Some(pbjson_types::Timestamp::from(Utc::now()))
}

/// Create an SSE stream from an Anthropic API response.
///
/// This function reads chunks from the upstream response, captures them,
/// and forwards them to the client in real-time.
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

        // Read the full response text and process as SSE
        let body_result = response.text().await;
        let body = match body_result {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to read Anthropic response body: {}", e);
                let _ = capture_tx.send(CaptureMessage::SessionError {
                    session_id: session_id.clone(),
                    error_message: e.to_string(),
                });
                return;
            }
        };

        // Process SSE events from the body
        let mut buffer = body.as_str();

        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = &buffer[..event_end];
            buffer = &buffer[event_end + 2..];

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
            let chunk = StreamChunk {
                sequence,
                event_type: event_type.clone(),
                data: data.clone().into_bytes(),
                received_at: now_timestamp(),
                delta_text: delta_text.unwrap_or_default(),
            };

            let _ = capture_tx.send(CaptureMessage::Chunk {
                session_id: session_id.clone(),
                chunk,
            });

            sequence += 1;

            // Forward the event to client
            if !event_type.is_empty() {
                let event = Event::default()
                    .event(&event_type)
                    .data(&data);
                yield Ok(event);
            }

            // Check for end of stream
            if event_type == "message_stop" {
                debug!("Anthropic stream complete for session {}", session_id);

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

        // If we exit without message_stop, send completion anyway
        if sequence > 0 {
            trace!("Stream ended for session {} after {} chunks", session_id, sequence);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create an SSE stream from an OpenAI API response.
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

        // Read the full response text
        let body_result = response.text().await;
        let body = match body_result {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to read OpenAI response body: {}", e);
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
                    debug!("OpenAI stream complete for session {}", session_id);

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
                    break;
                }

                // Parse JSON data
                let delta_text = extract_openai_delta(data);
                if let Some(ref text) = delta_text {
                    accumulated_text.push_str(text);
                }

                // Extract usage if present (in final chunk)
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
                let chunk = StreamChunk {
                    sequence,
                    event_type: "chunk".to_string(),
                    data: data.as_bytes().to_vec(),
                    received_at: now_timestamp(),
                    delta_text: delta_text.unwrap_or_default(),
                };

                let _ = capture_tx.send(CaptureMessage::Chunk {
                    session_id: session_id.clone(),
                    chunk,
                });

                sequence += 1;

                // Forward to client
                yield Ok(Event::default().data(data));
            }
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
    let text = delta.get("text")?.as_str()?;
    Some(text.to_string())
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
