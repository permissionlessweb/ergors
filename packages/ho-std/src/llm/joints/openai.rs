use crate::llm::{CostCalculator, HoError, HoResult};
use crate::traits::{ApiJoint, ApiKeyProvider, MessageExt};
use crate::types::ergors::orch::v1::*;
use async_trait::async_trait;
use chrono::DateTime;
use commonware_cryptography::{blake3, Hasher};
use pbjson_types::Timestamp;
use reqwest::Client;
use tracing::{error, warn};

/// OpenAiJoint to LLM-inference API
/// Used by OpenAI, Grok, Akash, and other OpenAI-compatible providers
pub struct OpenAiJoint;

#[async_trait]
impl ApiJoint for OpenAiJoint {
    async fn handle_request<T>(
        provider: &T,
        client: &Client,
        req: &PromptRequest,
        base_url: &str,
        provider_name: &str,
    ) -> HoResult<PromptResponse>
    where
        T: ApiKeyProvider + Send + Sync,
    {
        let api_key = provider.get_api_key().await?;

        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|p| serde_json::json!({
                "role": p.role,
                "content": p.content,
            }))
            .collect();

        let temperature = req
            .llm_config
            .as_ref().map(|c| c.temperature)
            .or(Some(1))
            .unwrap_or_default();
        let max_tokens = req
            .llm_config
            .as_ref()
            .map(|c| c.max_tokens)
            .unwrap_or(0);

        // Build request body, omitting empty/default fields that strict servers reject
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "temperature": temperature,
            "stream": false,
        });

        // Only include max_tokens when explicitly set (> 0).
        // Protobuf u32 default is 0, meaning "not set". Sending max_tokens: 0 to
        // sglang/vLLM causes content: null with finish_reason: "length".
        // Omitting the field lets the server use its own default.
        if max_tokens > 0 {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }

        // Only include tool_choice/tools when actually set — empty strings cause
        // validation errors on strict servers (sglang, vLLM)
        if !req.tool_choice.is_empty() {
            body["tool_choice"] = serde_json::Value::String(req.tool_choice.clone());
        }
        if !req.tools.is_empty() {
            body["tools"] = serde_json::to_value(&req.tools).unwrap_or_default();
        }

        let start = std::time::Instant::now();
        let base = base_url.trim_end_matches('/');
        let endpoint = if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        };

        let mut req_builder = client
            .post(&endpoint)
            .header("Content-Type", "application/json");

        // Only add Authorization header if we have a non-empty API key
        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .json(&body)
            .send()
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;
        let status = response.status();

        // Read body as text first — never fail on deserialization
        let timestamp: Timestamp = response
            .headers()
            .get("date")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| DateTime::parse_from_rfc2822(s).ok())
            .map(|dt| dt.to_utc().into())
            .unwrap_or_else(|| chrono::Utc::now().into());

        let body_text = response.text().await.map_err(|e| {
            HoError::Llm(format!("{} error reading response body: {}", provider_name, e))
        })?;

        if !status.is_success() {
            error!("{} API error (HTTP {}): {}", provider_name, status, body_text);
            return Err(HoError::Llm(format!(
                "{} error (HTTP {}): {}",
                provider_name, status, body_text
            )));
        }

        // Parse as generic JSON — handles nulls, missing fields, extra fields from any server
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            error!("{} response is not valid JSON: {} — body: {}", provider_name, e, &body_text[..body_text.len().min(500)]);
            HoError::Llm(format!("{} invalid JSON response: {}", provider_name, e))
        })?;

        // Extract content from choices[].message.content (handles null content gracefully)
        let content: Vec<String> = json["choices"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|choice| {
                choice["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
            })
            .collect();

        if content.is_empty() {
            warn!(
                "{} response had no extractable content. choices: {}",
                provider_name,
                json["choices"].to_string()
            );
        }

        // Extract usage (all fields optional, default to 0)
        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
        let total_tokens = json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(PromptResponse {
            id: blake3::Blake3::hash(&req.to_bytes().unwrap()).to_vec(),
            provider: provider_name.to_string(),
            model: req.model.to_string(),
            prompt: blake3::Blake3::hash(&req.to_bytes().unwrap()).to_string(),
            response: content,
            timestamp: Some(timestamp),
            tokens_used: Some(TokenUsage {
                prompt: prompt_tokens,
                completion: completion_tokens,
                total: total_tokens,
            }),
            cost: CostCalculator::calculate_cost(
                provider_name,
                &req.model,
                prompt_tokens,
                completion_tokens,
            ),
            latency_ms,
            status: None,
            output: vec![],
            response_metadata: None,
        })
    }
}
