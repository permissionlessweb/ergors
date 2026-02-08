//! CLI command for making inference calls through the node's HTTP proxy.
//!
//! Detects API format (Anthropic vs OpenAI) from the model name and
//! POSTs to the node's own proxy endpoint. Streaming is on by default.

use std::io::{self, IsTerminal as _, Read, Write};

use anyhow::{Context, Result};
use camino::Utf8Path;

const DEFAULT_API_PORT: u16 = 8080;

// =============================================================================
// CLI types
// =============================================================================

#[derive(Debug, clap::Parser)]
pub struct CallCmd {
    /// The prompt text (reads from stdin if omitted)
    pub prompt: Option<String>,

    /// Model name (used for format detection and routing)
    #[arg(short, long, default_value = "claude-sonnet-4-5-20250929")]
    pub model: String,

    /// System prompt
    #[arg(short, long)]
    pub system: Option<String>,

    /// Maximum tokens to generate
    #[arg(long, default_value = "4096")]
    pub max_tokens: u32,

    /// Disable streaming (wait for full response)
    #[arg(long)]
    pub no_stream: bool,

    /// Sampling temperature
    #[arg(long)]
    pub temperature: Option<f64>,

    /// HTTP API address override (e.g. http://host:8080)
    #[arg(long, env = "ERGORS_API_ADDR")]
    pub api_addr: Option<String>,
}

// =============================================================================
// Format detection
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiFormat {
    Anthropic,
    OpenAi,
}

fn detect_format(model: &str) -> ApiFormat {
    let m = model.to_lowercase();
    if m.contains("claude")
        || m.contains("haiku")
        || m.contains("sonnet")
        || m.contains("opus")
        || m.contains("anthropic")
    {
        ApiFormat::Anthropic
    } else {
        ApiFormat::OpenAi
    }
}

// =============================================================================
// Implementation
// =============================================================================

impl CallCmd {
    pub fn exec(&self, _home_dir: &Utf8Path, grpc_addr: &str, json_output: bool) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.run(grpc_addr, json_output))
    }

    async fn run(&self, grpc_addr: &str, json_output: bool) -> Result<()> {
        // Resolve prompt: positional arg or stdin
        let prompt = match &self.prompt {
            Some(p) => p.clone(),
            None => {
                if io::stdin().is_terminal() {
                    anyhow::bail!("No prompt provided. Pass as argument or pipe via stdin.");
                }
                let mut buf = String::new();
                io::stdin()
                    .read_to_string(&mut buf)
                    .context("failed to read stdin")?;
                let trimmed = buf.trim().to_string();
                if trimmed.is_empty() {
                    anyhow::bail!("Empty stdin — provide a prompt.");
                }
                trimmed
            }
        };

        // Resolve API base URL
        let base_url = self.resolve_api_addr(grpc_addr);
        let format = detect_format(&self.model);
        let stream = !self.no_stream;

        // Build request
        let (url, body) = self.build_request(&base_url, &prompt, format, stream);

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .context("failed to connect to node API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        if stream {
            self.handle_stream(resp, format).await
        } else if json_output {
            let text = resp.text().await?;
            // Pretty-print the JSON
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{}", text);
            }
            Ok(())
        } else {
            self.handle_non_stream(resp, format).await
        }
    }

    fn resolve_api_addr(&self, grpc_addr: &str) -> String {
        if let Some(addr) = &self.api_addr {
            return addr.clone();
        }
        // Extract host from gRPC address, use port 8080
        let host = grpc_addr
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(':')
            .next()
            .unwrap_or("localhost");
        format!("http://{}:{}", host, DEFAULT_API_PORT)
    }

    fn build_request(
        &self,
        base_url: &str,
        prompt: &str,
        format: ApiFormat,
        stream: bool,
    ) -> (String, String) {
        match format {
            ApiFormat::Anthropic => {
                let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
                let mut body = serde_json::json!({
                    "model": self.model,
                    "messages": [{"role": "user", "content": prompt}],
                    "max_tokens": self.max_tokens,
                    "stream": stream,
                });
                if let Some(sys) = &self.system {
                    body["system"] = serde_json::json!(sys);
                }
                if let Some(temp) = self.temperature {
                    body["temperature"] = serde_json::json!(temp);
                }
                (url, body.to_string())
            }
            ApiFormat::OpenAi => {
                let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
                let mut messages = Vec::new();
                if let Some(sys) = &self.system {
                    messages.push(serde_json::json!({"role": "system", "content": sys}));
                }
                messages.push(serde_json::json!({"role": "user", "content": prompt}));
                let mut body = serde_json::json!({
                    "model": self.model,
                    "messages": messages,
                    "max_tokens": self.max_tokens,
                    "stream": stream,
                });
                if let Some(temp) = self.temperature {
                    body["temperature"] = serde_json::json!(temp);
                }
                (url, body.to_string())
            }
        }
    }

    async fn handle_stream(
        &self,
        resp: reqwest::Response,
        format: ApiFormat,
    ) -> Result<()> {
        let mut stdout = io::stdout().lock();
        let mut buf = String::new();

        // Read chunks and process SSE events
        let mut resp = resp;
        while let Some(chunk) = resp.chunk().await? {
            let text = String::from_utf8_lossy(&chunk);
            buf.push_str(&text);

            // Process complete SSE events (separated by double newline)
            while let Some(pos) = buf.find("\n\n") {
                let event = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();

                for line in event.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
                            continue;
                        }
                        if let Some(text) = extract_stream_text(data, format) {
                            write!(stdout, "{}", text)?;
                            stdout.flush()?;
                        }
                    }
                }
            }
        }

        // Process any remaining buffer
        for line in buf.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() != "[DONE]" {
                    if let Some(text) = extract_stream_text(data, format) {
                        write!(stdout, "{}", text)?;
                    }
                }
            }
        }

        writeln!(stdout)?;
        Ok(())
    }

    async fn handle_non_stream(
        &self,
        resp: reqwest::Response,
        format: ApiFormat,
    ) -> Result<()> {
        let body: serde_json::Value = resp.json().await.context("invalid JSON response")?;

        let text = match format {
            ApiFormat::Anthropic => body
                .pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            ApiFormat::OpenAi => body
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        };

        println!("{}", text);
        Ok(())
    }
}

/// Extract text delta from a streaming SSE data payload.
fn extract_stream_text(data: &str, format: ApiFormat) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    match format {
        ApiFormat::Anthropic => {
            // Anthropic: {"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}
            v.pointer("/delta/text")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        }
        ApiFormat::OpenAi => {
            // OpenAI: {"choices":[{"delta":{"content":"..."}}]}
            v.pointer("/choices/0/delta/content")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_anthropic() {
        assert_eq!(detect_format("claude-sonnet-4-5-20250929"), ApiFormat::Anthropic);
        assert_eq!(detect_format("claude-3-haiku-20240307"), ApiFormat::Anthropic);
        assert_eq!(detect_format("claude-3-opus-20240229"), ApiFormat::Anthropic);
        assert_eq!(detect_format("anthropic/custom-model"), ApiFormat::Anthropic);
    }

    #[test]
    fn test_detect_format_openai() {
        assert_eq!(detect_format("gpt-4o"), ApiFormat::OpenAi);
        assert_eq!(detect_format("o1-mini"), ApiFormat::OpenAi);
        assert_eq!(detect_format("o3"), ApiFormat::OpenAi);
        assert_eq!(detect_format("llama3"), ApiFormat::OpenAi);
        assert_eq!(detect_format("mistral-7b"), ApiFormat::OpenAi);
        assert_eq!(detect_format("qwen-inference"), ApiFormat::OpenAi);
    }

    #[test]
    fn test_extract_stream_text_anthropic() {
        let data = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#;
        assert_eq!(
            extract_stream_text(data, ApiFormat::Anthropic),
            Some("Hello".to_string())
        );
    }

    #[test]
    fn test_extract_stream_text_openai() {
        let data = r#"{"choices":[{"delta":{"content":"world"}}]}"#;
        assert_eq!(
            extract_stream_text(data, ApiFormat::OpenAi),
            Some("world".to_string())
        );
    }

    #[test]
    fn test_extract_stream_text_invalid() {
        assert_eq!(extract_stream_text("not json", ApiFormat::OpenAi), None);
        assert_eq!(extract_stream_text("{}", ApiFormat::Anthropic), None);
    }
}
