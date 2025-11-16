use crate::llm::{CostCalculator, HoError, HoResult};
use crate::orchestrate::PromptRequest;
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

        let request = OpenAiRequest {
            model: req.model.to_string(),
            messages: req
                .messages
                .iter()
                .map(|p| OpenAiMessage {
                    role: p.role.to_string(),
                    content: p.content.to_string(),
                })
                .collect(),
            temperature: req
                .llm_config
                .as_ref()
                .and_then(|c| Some(c.temperature))
                .or(Some(1)),
            max_tokens: req.llm_config.as_ref().and_then(|c| Some(c.max_tokens)),
        };

        let start = std::time::Instant::now();
        let endpoint = if base_url.ends_with("/chat/completions") || base_url.ends_with("/v1") {
            if base_url.ends_with("/v1") {
                format!("{}/chat/completions", base_url)
            } else {
                base_url.to_string()
            }
        } else {
            format!("{}/chat/completions", base_url)
        };

        let response = client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
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
                id: (&blake3::Blake3::hash(&req.to_bytes().unwrap())).to_vec(),
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
                cost: Some(CostCalculator::calculate_cost(
                    provider_name,
                    &req.model,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                )),
                latency_ms: Some(latency_ms),
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
