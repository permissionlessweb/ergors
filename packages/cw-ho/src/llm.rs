use crate::error::{CwHoError, Result};
use crate::LlmRouter;
use ho_std::constants::*;
use ho_std::orchestrate::*;
use reqwest::Client;
use std::{collections::HashMap, time::Duration};
use tracing::{error, warn};

#[derive(Debug, Clone)]
pub struct ApiKeys {
    pub openai: Option<String>,
    pub anthropic: Option<String>,
    pub grok: Option<String>,
    pub akash: Option<String>,
    pub kimi: Option<String>,
    pub qwen: Option<String>,
    pub venice: Option<String>,
}

impl LlmRouter {
    pub async fn new(config: &LlmRouterConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| CwHoError::Config(format!("Failed to create HTTP client: {}", e)))?;

        let api_keys = Self::load_api_keys(&config.api_keys_file).await?;

        Ok(Self {
            client,
            api_keys,
            config: config.clone(),
        })
    }

    async fn load_api_keys(path: &str) -> Result<ApiKeys> {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            let keys: HashMap<String, String> = serde_json::from_str(&content)
                .map_err(|e| CwHoError::Config(format!("Failed to parse API keys: {}", e)))?;

            Ok(ApiKeys {
                openai: keys.get(OPEN_AI).cloned(),
                anthropic: keys.get(ANTHROPIC).cloned(),
                grok: keys.get(GROK).cloned(),
                akash: keys.get(AKASH_CHAT).cloned(),
                kimi: keys.get(AKASH_CHAT).cloned(),
                qwen: keys.get(QUEN).cloned(),
                venice: keys.get(VENICE).cloned(),
            })
        } else {
            warn!("API keys file not found: {}", path);
            // Updated code block
            Ok(ApiKeys {
                openai: std::env::var(OPENAI_API_KEY).ok(),
                anthropic: std::env::var(ANTHROPIC_API_KEY).ok(),
                grok: std::env::var(GROK_API_KEY).ok(),
                akash: std::env::var(AKASH_API_KEY).ok(),
                kimi: std::env::var(KIMI_API_KEY).ok(),
                qwen: std::env::var(QWEN_API_KEY).ok(),
                venice: std::env::var(VENICE_API_KEY).ok(),
            })
        }
    }

    pub async fn process_request(
        &self,
        request: &PromptRequest,
        model: &str,
    ) -> Result<PromptResponse> {
        // Determine provider based on model name for now
        // TODO: Add provider field to request or use model-based routing
        if model.contains("gpt") || model.contains("openai") {
            self.call_openai(&request).await
        } else if model.contains("claude") || model.contains("anthropic") {
            self.call_anthropic(&request).await
        } else if model.contains("grok") {
            self.call_grok(&request).await
        } else if model.contains("akash") {
            self.call_akash(&request).await
        } else {
            // Default to OpenAI for unknown models
            self.call_openai(&request).await
        }
    }

    async fn call_akash(&self, req: &PromptRequest) -> Result<PromptResponse> {
        let api_key = self
            .api_keys
            .openai
            .as_ref()
            .ok_or_else(|| CwHoError::LlmEntity("OpenAI API key not configured".to_string()))?;

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
            temperature: Some(69),    // Default temperature
            max_tokens: Some(10_000), // Default max tokens
        };

        let response = self
            .client
            .post(AKASH_CHAT_BASE_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let openai_response: OpenAiResponse = response.json().await?;

            let content = openai_response
                .choices
                .first()
                .map(|c| c.message.clone().expect("should have msgs").content.clone())
                .unwrap_or_else(|| "No response".to_string());

            let usage = openai_response.usage.unwrap_or(OpenAiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });

            Ok(PromptResponse {
                tokens_used: Some(TokenUsage {
                    prompt: usage.prompt_tokens,
                    completion: usage.completion_tokens,
                    total: usage.total_tokens,
                }),
                model: req.model.to_string(),
                prompt: "".into(), //TODO: create idenifiable prompt hash
                response: content.clone(),
                timestamp: todo!(),
                cost: todo!(),
                latency_ms: todo!(),
                id: todo!(),
                provider: todo!(),
            })
        } else {
            let error_text = response.text().await?;
            error!("OpenAI API error: {}", error_text);
            Err(CwHoError::LlmEntity(format!(
                "OpenAI error: {}",
                error_text
            )))
        }
    }
    async fn call_openai(&self, req: &PromptRequest) -> Result<PromptResponse> {
        let api_key = self
            .api_keys
            .openai
            .as_ref()
            .ok_or_else(|| CwHoError::LlmEntity("OpenAI API key not configured".to_string()))?;

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
            temperature: Some(69),    // Default temperature
            max_tokens: Some(10_000), // Default max tokens
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let openai_response: OpenAiResponse = response.json().await?;

            let content = openai_response
                .choices
                .first()
                .map(|c| c.message.clone().expect("should have msgs").content.clone())
                .unwrap_or_else(|| "No response".to_string());

            let usage = openai_response.usage.unwrap_or(OpenAiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });

            Ok(PromptResponse {
                tokens_used: Some(TokenUsage {
                    prompt: usage.prompt_tokens,
                    completion: usage.completion_tokens,
                    total: usage.total_tokens,
                }),
                model: req.model.to_string(),
                provider: "openai".to_string(),
                prompt: todo!(),
                response: todo!(),
                timestamp: todo!(),
                cost: todo!(),
                latency_ms: todo!(),
                id: todo!(),
            })
        } else {
            let error_text = response.text().await?;
            error!("OpenAI API error: {}", error_text);
            Err(CwHoError::LlmEntity(format!(
                "OpenAI error: {}",
                error_text
            )))
        }
    }

    async fn call_anthropic(&self, req: &PromptRequest) -> Result<PromptResponse> {
        let api_key =
            self.api_keys.anthropic.as_ref().ok_or_else(|| {
                CwHoError::LlmEntity("Anthropic API key not configured".to_string())
            })?;

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

        let mut request = serde_json::json!({
            "model": req.model,
            "max_tokens": req.llm_config.clone().expect("hah").max_tokens,
            "messages": messages,
            "temperature":  req.llm_config.clone().expect("hah").temperature,
        });

        if let Some(system) = system_opt {
            request["system"] = serde_json::json!(system);
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let anthropic_response: serde_json::Value = response.json().await?;

            let content = anthropic_response
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|text| text.as_str())
                .unwrap_or("No response")
                .to_string();

            let usage = anthropic_response
                .get("usage")
                .map(|u| TokenUsage {
                    prompt: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    completion: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    total: 0, // Will be calculated below
                })
                .unwrap_or(TokenUsage {
                    prompt: 0,
                    completion: 0,
                    total: 0,
                });

            let mut final_usage = usage;
            final_usage.total = final_usage.prompt + final_usage.completion;

            Ok(PromptResponse {
                tokens_used: Some(final_usage),
                model: req.model.to_string(),
                provider: "anthropic".to_string(),
                prompt: todo!(),
                response: todo!(),
                timestamp: todo!(),
                cost: todo!(),
                latency_ms: todo!(),
                id: todo!(),
            })
        } else {
            let error_text = response.text().await?;
            error!("Anthropic API error: {}", error_text);
            Err(CwHoError::LlmEntity(format!(
                "Anthropic error: {}",
                error_text
            )))
        }
    }

    async fn call_grok(&self, req: &PromptRequest) -> Result<PromptResponse> {
        let api_key = self
            .api_keys
            .grok
            .as_ref()
            .ok_or_else(|| CwHoError::LlmEntity("Grok API key not configured".to_string()))?;

        // Grok uses OpenAI-compatible API
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
            temperature: Some(69),    // Default temperature
            max_tokens: Some(10_000), // Default max tokens
        };

        let response = self
            .client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            let grok_response: OpenAiResponse = response.json().await?;

            let content = grok_response
                .choices
                .first()
                .map(|c| c.message.clone().expect("should have msgs").content.clone())
                .unwrap_or_else(|| "No response".to_string());

            let usage = grok_response.usage.unwrap_or(OpenAiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });

            Ok(PromptResponse {
                tokens_used: Some(TokenUsage {
                    prompt: usage.prompt_tokens,
                    completion: usage.completion_tokens,
                    total: usage.total_tokens,
                }),
                model: req.model.to_string(),
                provider: "grok".to_string(),
                prompt: todo!(),
                response: todo!(),
                timestamp: todo!(),
                cost: todo!(),
                latency_ms: todo!(),
                id: todo!(),
            })
        } else {
            let error_text = response.text().await?;
            error!("Grok API error: {}", error_text);
            Err(CwHoError::LlmEntity(format!("Grok error: {}", error_text)))
        }
    }

    pub fn get_available_models(&self) -> Vec<String> {
        let mut models = Vec::new();

        if self.api_keys.openai.is_some() {
            models.extend_from_slice(OPENAI_MODELS);
        }

        if self.api_keys.anthropic.is_some() {
            models.extend_from_slice(ANTHROPIC_MODELS);
        }

        if self.api_keys.grok.is_some() {
            models.extend_from_slice(GROK_MODELS);
        }
        if self.api_keys.kimi.is_some() {
            models.extend_from_slice(KIMI_RESEARCH_MODELS);
        }
        if self.api_keys.akash.is_some() {
            models.extend_from_slice(AKASH_CHAT_MODELS);
        }
        if self.api_keys.qwen.is_some() {
            models.extend_from_slice(QWEN_MODELS);
        }
        if self.api_keys.venice.is_some() {
            models.extend_from_slice(QWEN_MODELS);
        }

        models.iter().map(|m| m.to_string()).collect()
    }
}
