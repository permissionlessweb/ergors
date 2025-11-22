use crate::llm::{CostCalculator, HoError, HoResult};
use crate::orchestrate::PromptRequest;
use crate::traits::{ApiJoint, ApiKeyProvider, MessageExt};
use crate::types::ergors::orch::v1::*;
use async_trait::async_trait;
use commonware_cryptography::{blake3, Hasher};
use reqwest::Client;
use tracing::error;

// use chrono::DateTime;
// use pbjson_types::Timestamp;

/// Anthropic API handler
pub struct AnthropticJoint;

#[async_trait]
impl ApiJoint for AnthropticJoint {
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

        // Extract system prompt if present
        let mut system_opt: Option<String> = None;
        let mut messages: Vec<serde_json::Value> = Vec::new();
        for m in &req.messages {
            if m.role == "system" {
                if let Some(sys) = system_opt.as_mut() {
                    sys.push_str("\n\n");
                    sys.push_str(&m.content);
                } else {
                    system_opt = Some(m.content.clone());
                }
            } else {
                messages.push(serde_json::json!({
                    "role": m.role,
                    "content": m.content
                }));
            }
        }

        // let llm_config = req
        //     .llm_config
        //     .as_ref()
        //     .ok_or_else(|| HoError::Llm("LLM config required for Anthropic".to_string()))?;

        let mut request_body = serde_json::json!({
            "model": req.model,
            "max_tokens": 64000,
            "messages": messages,
            "temperature": 0.5,
        });
        tracing::debug!(?request_body);
        tracing::debug!(?api_key);

        if let Some(system) = system_opt {
            request_body["system"] = serde_json::json!(system);
        }

        let start = std::time::Instant::now();
        let endpoint = format!("{}/messages", base_url);

        let response = client
            .post(&endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if response.status().is_success() {
            let anthropic_response: serde_json::Value = response.json().await?;

            let content: Vec<String> = anthropic_response
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_else(|| vec!["No response".to_string()]);

            let usage = anthropic_response
                .get("usage")
                .map(|u| TokenUsage {
                    prompt: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    completion: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    total: 0,
                })
                .unwrap_or(TokenUsage {
                    prompt: 0,
                    completion: 0,
                    total: 0,
                });

            let mut final_usage = usage;
            final_usage.total = final_usage.prompt + final_usage.completion;

            Ok(PromptResponse {
                id: (&blake3::Blake3::hash(&req.to_bytes().unwrap())).to_vec(),
                provider: provider_name.to_string(),
                model: req.model.to_string(),
                prompt: blake3::Blake3::hash(&req.to_bytes().unwrap()).to_string(),
                response: content,
                timestamp: Some(chrono::Utc::now().into()),
                tokens_used: Some(final_usage),
                cost: Some(CostCalculator::calculate_cost(
                    provider_name,
                    &req.model,
                    final_usage.prompt,
                    final_usage.completion,
                )),
                latency_ms: Some(latency_ms),
            })
        } else {
            let error_text = response.text().await?;
            error!("{} API error: {}", provider_name, error_text);
            Err(HoError::Llm(format!("{}:{}", provider_name, error_text)))
        }
    }
}
