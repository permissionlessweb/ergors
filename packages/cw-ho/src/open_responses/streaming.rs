use axum::response::sse::Event;
use serde_json::json;
use uuid::Uuid;

/// Transforms provider-specific streaming events into Open Responses semantic events.
pub struct OpenResponsesStreamTransformer {
    pub response_id: String,
    pub sequence: u32,
    pub output_index: u32,
    pub content_index: u32,
    pub model: String,
    /// Accumulated text for content_part.done events
    accumulated_text: String,
}

impl OpenResponsesStreamTransformer {
    pub fn new(response_id: String, model: String) -> Self {
        Self {
            response_id,
            sequence: 0,
            output_index: 0,
            content_index: 0,
            model,
            accumulated_text: String::new(),
        }
    }

    fn next_seq(&mut self) -> u32 {
        self.sequence += 1;
        self.sequence
    }

    /// Transform an Anthropic SSE event into Open Responses events.
    /// Returns a list of SSE Events to emit.
    pub fn transform_anthropic_event(
        &mut self,
        event_type: &str,
        data: &str,
    ) -> Vec<Event> {
        match event_type {
            "message_start" => {
                self.accumulated_text.clear();
                let seq = self.next_seq();
                vec![self.make_event("response.in_progress", json!({
                    "type": "response.in_progress",
                    "sequence_number": seq,
                    "response": {
                        "id": self.response_id,
                        "object": "response",
                        "status": "in_progress",
                        "model": self.model,
                        "output": [],
                    }
                }))]
            }
            "content_block_start" => {
                self.accumulated_text.clear();
                let item_id = format!("msg_{}", Uuid::new_v4().simple());
                let seq = self.next_seq();
                let mut events = vec![self.make_event(
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "sequence_number": seq,
                        "output_index": self.output_index,
                        "item": {
                            "id": item_id,
                            "type": "message",
                            "status": "in_progress",
                            "role": "assistant",
                            "content": [],
                        }
                    }),
                )];

                let seq2 = self.next_seq();
                events.push(self.make_event(
                    "response.content_part.added",
                    json!({
                        "type": "response.content_part.added",
                        "sequence_number": seq2,
                        "item_id": item_id,
                        "output_index": self.output_index,
                        "content_index": self.content_index,
                        "part": {
                            "type": "output_text",
                            "text": "",
                        }
                    }),
                ));

                events
            }
            "content_block_delta" => {
                // Extract text delta from Anthropic format
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(delta) = parsed.get("delta") {
                        // Text delta
                        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                            self.accumulated_text.push_str(text);
                            let seq = self.next_seq();
                            return vec![self.make_event(
                                "response.output_text.delta",
                                json!({
                                    "type": "response.output_text.delta",
                                    "sequence_number": seq,
                                    "output_index": self.output_index,
                                    "content_index": self.content_index,
                                    "delta": text,
                                }),
                            )];
                        }
                        // Tool use input delta (input_json_delta)
                        if let Some(pjson) = delta.get("partial_json").and_then(|v| v.as_str()) {
                            self.accumulated_text.push_str(pjson);
                            let seq = self.next_seq();
                            return vec![self.make_event(
                                "response.function_call_arguments.delta",
                                json!({
                                    "type": "response.function_call_arguments.delta",
                                    "sequence_number": seq,
                                    "output_index": self.output_index,
                                    "delta": pjson,
                                }),
                            )];
                        }
                    }
                }
                vec![]
            }
            "content_block_stop" => {
                let seq = self.next_seq();
                let mut events = vec![self.make_event(
                    "response.output_text.done",
                    json!({
                        "type": "response.output_text.done",
                        "sequence_number": seq,
                        "output_index": self.output_index,
                        "content_index": self.content_index,
                        "text": self.accumulated_text,
                    }),
                )];

                let seq2 = self.next_seq();
                events.push(self.make_event(
                    "response.content_part.done",
                    json!({
                        "type": "response.content_part.done",
                        "sequence_number": seq2,
                        "output_index": self.output_index,
                        "content_index": self.content_index,
                        "part": {
                            "type": "output_text",
                            "text": self.accumulated_text,
                        }
                    }),
                ));

                let seq3 = self.next_seq();
                events.push(self.make_event(
                    "response.output_item.done",
                    json!({
                        "type": "response.output_item.done",
                        "sequence_number": seq3,
                        "output_index": self.output_index,
                        "item": {
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{
                                "type": "output_text",
                                "text": self.accumulated_text,
                            }]
                        }
                    }),
                ));

                self.output_index += 1;
                self.content_index = 0;
                self.accumulated_text.clear();
                events
            }
            "message_stop" | "message_delta" => {
                if event_type == "message_stop" {
                    let seq = self.next_seq();
                    vec![self.make_event(
                        "response.completed",
                        json!({
                            "type": "response.completed",
                            "sequence_number": seq,
                            "response": {
                                "id": self.response_id,
                                "object": "response",
                                "status": "completed",
                                "model": self.model,
                            }
                        }),
                    )]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// Transform an OpenAI streaming chunk into Open Responses events.
    pub fn transform_openai_chunk(
        &mut self,
        data: &str,
    ) -> Vec<Event> {
        if data.trim() == "[DONE]" {
            let seq = self.next_seq();
            return vec![self.make_event(
                "response.completed",
                json!({
                    "type": "response.completed",
                    "sequence_number": seq,
                    "response": {
                        "id": self.response_id,
                        "object": "response",
                        "status": "completed",
                        "model": self.model,
                    }
                }),
            )];
        }

        let parsed: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut events = Vec::new();

        if let Some(choices) = parsed.get("choices").and_then(|v| v.as_array()) {
            for choice in choices {
                if let Some(delta) = choice.get("delta") {
                    // Text content delta
                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            self.accumulated_text.push_str(content);
                            let seq = self.next_seq();
                            events.push(self.make_event(
                                "response.output_text.delta",
                                json!({
                                    "type": "response.output_text.delta",
                                    "sequence_number": seq,
                                    "output_index": self.output_index,
                                    "content_index": self.content_index,
                                    "delta": content,
                                }),
                            ));
                        }
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tool_calls {
                            if let Some(func) = tc.get("function") {
                                if let Some(args) = func.get("arguments").and_then(|v| v.as_str())
                                {
                                    let seq = self.next_seq();
                                    events.push(self.make_event(
                                        "response.function_call_arguments.delta",
                                        json!({
                                            "type": "response.function_call_arguments.delta",
                                            "sequence_number": seq,
                                            "output_index": self.output_index,
                                            "delta": args,
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                }

                // Check finish_reason
                if let Some(finish) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    match finish {
                        "stop" | "tool_calls" | "length" => {
                            let seq = self.next_seq();
                            events.push(self.make_event(
                                "response.output_item.done",
                                json!({
                                    "type": "response.output_item.done",
                                    "sequence_number": seq,
                                    "output_index": self.output_index,
                                    "item": {
                                        "type": "message",
                                        "status": if finish == "length" { "incomplete" } else { "completed" },
                                        "role": "assistant",
                                        "content": [{
                                            "type": "output_text",
                                            "text": self.accumulated_text,
                                        }]
                                    }
                                }),
                            ));
                            self.output_index += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        events
    }

    /// Create an SSE Event with the given event type and JSON payload.
    fn make_event(&self, event_type: &str, payload: serde_json::Value) -> Event {
        Event::default()
            .event(event_type)
            .data(serde_json::to_string(&payload).unwrap_or_default())
    }

    /// Create the terminal [DONE] event
    pub fn done_event() -> Event {
        Event::default().data("[DONE]")
    }
}
