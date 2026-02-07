//! Upstream provider forwarding for proxy requests.

use anyhow::Result;
use bytes::Bytes;
use reqwest::Client;
use tracing::{debug, error};
use ho_std::constants::{ANTHROPIC_BASE_URL, OPENAI_BASE_URL};

/// Re-export constants for backwards compatibility (deprecated - use ho_std::constants)
#[deprecated(note = "Use ho_std::constants::ANTHROPIC_BASE_URL instead")]
pub const ANTHROPIC_API_URL: &str = ANTHROPIC_BASE_URL;
/// Re-export constants for backwards compatibility (deprecated - use ho_std::constants)
#[deprecated(note = "Use ho_std::constants::OPENAI_BASE_URL instead")]
pub const OPENAI_API_URL: &str = OPENAI_BASE_URL;

/// Forward a request to the Anthropic API.
pub async fn forward_to_anthropic(
    client: &Client,
    body: Bytes,
    api_key: &str,
    anthropic_version: Option<&str>,
    anthropic_beta: Option<&str>,
) -> Result<reqwest::Response> {
    let url = format!("{}/v1/messages", ANTHROPIC_API_URL);
    debug!("Forwarding request to Anthropic: {}", url);

    let mut request = client
        .post(&url)
        .header("x-api-key", api_key)
        .header(
            "anthropic-version",
            anthropic_version.unwrap_or("2023-06-01"),
        )
        .header("content-type", "application/json")
        .body(body);

    // Add beta features header if present
    if let Some(beta) = anthropic_beta {
        request = request.header("anthropic-beta", beta);
    }

    let response = request.send().await.map_err(|e| {
        error!("Failed to forward request to Anthropic: {}", e);
        anyhow::anyhow!("Anthropic request failed: {}", e)
    })?;

    Ok(response)
}

/// Forward a request to the OpenAI API.
pub async fn forward_to_openai(
    client: &Client,
    body: Bytes,
    api_key: &str,
    organization: Option<&str>,
) -> Result<reqwest::Response> {
    let url = format!("{}/v1/chat/completions", OPENAI_API_URL);
    debug!("Forwarding request to OpenAI: {}", url);

    let mut request = client
        .post(&url)
        .header("authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .body(body);

    // Add organization header if present
    if let Some(org) = organization {
        request = request.header("openai-organization", org);
    }

    let response = request.send().await.map_err(|e| {
        error!("Failed to forward request to OpenAI: {}", e);
        anyhow::anyhow!("OpenAI request failed: {}", e)
    })?;

    Ok(response)
}

/// Create a configured HTTP client for upstream requests.
pub fn create_upstream_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for long responses
        .build()
        .expect("Failed to create HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_upstream_client() {
        let client = create_upstream_client();
        // Just verify it doesn't panic
        drop(client);
    }
}
