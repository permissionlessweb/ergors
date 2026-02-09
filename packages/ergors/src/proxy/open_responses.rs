use ho_std::types::ergors::orch::v1::*;
use serde_json::json;
use uuid::Uuid;

/// Convert a ContentBlock to a ResponseOutputItem
pub fn content_block_to_output_item(block: &ContentBlock, _index: u32) -> ResponseOutputItem {
    let item_id = format!("item_{}", Uuid::new_v4().simple());

    match &block.block {
        Some(content_block::Block::Text(_text)) => ResponseOutputItem {
            id: item_id,
            r#type: "message".to_string(),
            status: "completed".to_string(),
            content: Some(response_output_item::Content::Message(
                MessageItemContent {
                    role: "assistant".to_string(),
                    content: vec![block.clone()],
                },
            )),
        },
        Some(content_block::Block::ToolUse(tool)) => {
            let func = tool.function.as_ref();
            ResponseOutputItem {
                id: item_id,
                r#type: "function_call".to_string(),
                status: "completed".to_string(),
                content: Some(response_output_item::Content::FunctionCall(
                    FunctionCallItemContent {
                        name: func.map(|f| f.name.clone()).unwrap_or_default(),
                        arguments: func.map(|f| f.arguments.clone()).unwrap_or_default(),
                        call_id: tool.id.clone(),
                    },
                )),
            }
        }
        Some(content_block::Block::ToolResult(result)) => ResponseOutputItem {
            id: item_id,
            r#type: "function_call_output".to_string(),
            status: "completed".to_string(),
            content: Some(response_output_item::Content::FunctionCallOutput(
                FunctionCallOutputItemContent {
                    call_id: result.tool_call_id.clone(),
                    output: result.content.clone(),
                },
            )),
        },
        _ => ResponseOutputItem {
            id: item_id,
            r#type: "message".to_string(),
            status: "completed".to_string(),
            content: Some(response_output_item::Content::Message(
                MessageItemContent {
                    role: "assistant".to_string(),
                    content: vec![block.clone()],
                },
            )),
        },
    }
}

/// Convert a PromptResponse into Open Responses JSON format
pub fn prompt_response_to_open_responses(
    response: &PromptResponse,
    response_id: &str,
) -> serde_json::Value {
    // If the response has output items already populated, use them
    let output_items: Vec<serde_json::Value> = if !response.output.is_empty() {
        response
            .output
            .iter()
            .map(output_item_to_json)
            .collect()
    } else {
        // Otherwise, convert the flat response strings to message items
        response
            .response
            .iter()
            
            .map(|text| {
                json!({
                    "id": format!("msg_{}", Uuid::new_v4().simple()),
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": text,
                    }]
                })
            })
            .collect()
    };

    let usage = response.tokens_used.as_ref().map(|t| {
        json!({
            "input_tokens": t.prompt,
            "output_tokens": t.completion,
            "total_tokens": t.total,
        })
    });

    json!({
        "id": response_id,
        "object": "response",
        "status": response.status.as_deref().unwrap_or("completed"),
        "model": response.model,
        "output": output_items,
        "usage": usage,
    })
}

/// Convert a ResponseOutputItem to JSON
fn output_item_to_json(item: &ResponseOutputItem) -> serde_json::Value {
    match &item.content {
        Some(response_output_item::Content::Message(msg)) => {
            let content_parts: Vec<serde_json::Value> = msg
                .content
                .iter()
                .map(|block| match &block.block {
                    Some(content_block::Block::Text(text)) => json!({
                        "type": "output_text",
                        "text": text,
                    }),
                    _ => json!({
                        "type": "output_text",
                        "text": "",
                    }),
                })
                .collect();

            json!({
                "id": item.id,
                "type": "message",
                "status": item.status,
                "role": msg.role,
                "content": content_parts,
            })
        }
        Some(response_output_item::Content::FunctionCall(fc)) => {
            json!({
                "id": item.id,
                "type": "function_call",
                "status": item.status,
                "name": fc.name,
                "call_id": fc.call_id,
                "arguments": fc.arguments,
            })
        }
        Some(response_output_item::Content::FunctionCallOutput(fco)) => {
            json!({
                "id": item.id,
                "type": "function_call_output",
                "status": item.status,
                "call_id": fco.call_id,
                "output": fco.output,
            })
        }
        None => {
            json!({
                "id": item.id,
                "type": item.r#type,
                "status": item.status,
            })
        }
    }
}

/// Parse an Open Responses request into a PromptRequest
pub fn parse_open_responses_request(
    req: &serde_json::Value,
) -> Result<PromptRequest, super::error::OpenResponsesError> {
    let model = req
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    if model.is_empty() {
        return Err(super::error::OpenResponsesError::InvalidRequest {
            param: "model".to_string(),
            message: "The 'model' field is required.".to_string(),
        });
    }

    // Parse input items (Open Responses uses "input" instead of "messages")
    let input = req.get("input").or_else(|| req.get("messages"));
    let messages = match input {
        Some(items) => parse_input_items(items)?,
        None => {
            return Err(super::error::OpenResponsesError::InvalidRequest {
                param: "input".to_string(),
                message: "The 'input' field is required.".to_string(),
            });
        }
    };

    // Parse tools
    let tools = req
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(parse_tool_definition)
                .collect()
        })
        .unwrap_or_default();

    let tool_choice = req
        .get("tool_choice")
        .map(|v| {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else if let Some(obj) = v.as_object() {
                // Structured tool_choice: {"type": "function", "name": "fn_name"}
                obj.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("auto")
                    .to_string()
            } else {
                "auto".to_string()
            }
        })
        .unwrap_or_default();

    let stream = req
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let system = req
        .get("instructions")
        .or_else(|| req.get("system"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let previous_response_id = req
        .get("previous_response_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let allowed_tools = req
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let truncation = req
        .get("truncation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let service_tier = req
        .get("service_tier")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(PromptRequest {
        messages,
        model,
        context: None,
        llm_config: None,
        tools,
        tool_choice,
        stream,
        system,
        previous_response_id,
        allowed_tools,
        truncation,
        service_tier,
        response_format: Some("open_responses".to_string()),
    })
}

/// Parse Open Responses input items into PromptMessages
fn parse_input_items(
    items: &serde_json::Value,
) -> Result<Vec<PromptMessage>, super::error::OpenResponsesError> {
    let arr = items.as_array().ok_or_else(|| {
        super::error::OpenResponsesError::InvalidRequest {
            param: "input".to_string(),
            message: "The 'input' field must be an array.".to_string(),
        }
    })?;

    let mut messages = Vec::new();

    for item in arr {
        let role = item
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user")
            .to_string();

        // Handle content as string or array of content parts
        let content = if let Some(content_str) = item.get("content").and_then(|v| v.as_str()) {
            content_str.to_string()
        } else if let Some(content_arr) = item.get("content").and_then(|v| v.as_array()) {
            // Extract text from content parts
            content_arr
                .iter()
                .filter_map(|part| {
                    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match part_type {
                        "input_text" | "text" => {
                            part.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        } else {
            String::new()
        };

        messages.push(PromptMessage {
            role,
            content,
            tool_calls: vec![],
            tool_result: None,
            content_blocks: vec![],
        });
    }

    Ok(messages)
}

/// Parse a tool definition from JSON
fn parse_tool_definition(tool: &serde_json::Value) -> Option<ToolDefinition> {
    let tool_type = tool.get("type").and_then(|v| v.as_str()).unwrap_or("function");

    if tool_type == "function" {
        let name = tool
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let description = tool
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let strict = tool
            .get("strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Some(ToolDefinition {
            r#type: "function".to_string(),
            function: Some(FunctionDefinition {
                name,
                description,
                parameters: None, // TODO: convert JSON Schema to proto Struct
                strict,
            }),
        })
    } else {
        None
    }
}

/// Filter tools by allowed_tools list
pub fn filter_tools(tools: &[ToolDefinition], allowed: &[String]) -> Vec<ToolDefinition> {
    if allowed.is_empty() {
        return tools.to_vec();
    }
    tools
        .iter()
        .filter(|t| {
            t.function
                .as_ref()
                .map(|f| allowed.contains(&f.name))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}
