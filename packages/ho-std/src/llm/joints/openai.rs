use crate::llm::{CostCalculator, HoError, HoResult};
use crate::traits::{ApiJoint, ApiKeyProvider, MessageExt};
use crate::types::ergors::orch::v1::*;
use async_trait::async_trait;
use chrono::DateTime;
use commonware_cryptography::{blake3, Hasher};
use pbjson_types::Timestamp;
use reqwest::Client;
use tracing::error;

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
            .as_ref().map(|c| c.max_tokens)
            .unwrap_or_default();

        // Build request body, omitting empty/default fields that strict servers reject
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": false,
        });

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

        if response.status().is_success() {
            let timestamp: Timestamp = response
                .headers()
                .get("date")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| DateTime::parse_from_rfc2822(s).ok())
                .map(|dt| dt.to_utc().into())
                .unwrap_or_else(|| chrono::Utc::now().into());

            let api_response: OpenAiResponse = response.json().await?;

            let content: Vec<String> = api_response
                .choices
                .iter()
                .filter_map(|c| c.message.as_ref().map(|m| m.content.clone()))
                .collect();

            let usage = api_response.usage.unwrap_or(OpenAiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });

            Ok(PromptResponse {
                id: blake3::Blake3::hash(&req.to_bytes().unwrap()).to_vec(),
                provider: provider_name.to_string(),
                model: req.model.to_string(),
                prompt: blake3::Blake3::hash(&req.to_bytes().unwrap()).to_string(),
                response: content,
                timestamp: Some(timestamp),
                tokens_used: Some(TokenUsage {
                    prompt: usage.prompt_tokens,
                    completion: usage.completion_tokens,
                    total: usage.total_tokens,
                }),
                cost: CostCalculator::calculate_cost(
                    provider_name,
                    &req.model,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                ),
                latency_ms,
                status: None,
                output: vec![],
                response_metadata: None,
            })
        } else {
            let error_text = response.text().await?;
            error!("{} API error: {}", provider_name, error_text);
            Err(HoError::Llm(format!(
                "{} error: {}",
                provider_name, error_text
            )))
        }
    }
}
