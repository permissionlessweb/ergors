//! Embedding API client.
//!
//! Simple, direct embedding generation via OpenAI-compatible endpoints.
//! No traits, no abstractions - just a function that does the job.

use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct EmbedError(pub String);

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for EmbedError {}

impl From<reqwest::Error> for EmbedError {
    fn from(e: reqwest::Error) -> Self {
        EmbedError(e.to_string())
    }
}

#[derive(Serialize)]
struct Request {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Embedding>,
}

#[derive(Deserialize)]
struct Embedding {
    embedding: Vec<f32>,
}

/// Generate embeddings from an OpenAI-compatible endpoint.
///
/// # Arguments
/// * `endpoint` - Base URL (e.g., "http://provider.akash.network:8080")
/// * `texts` - Texts to embed
/// * `model` - Model name (e.g., "all-MiniLM-L6-v2")
/// * `api_key` - Optional API key for authenticated endpoints
///
/// # Example
/// ```rust,no_run
/// use ho_std::llm::embed;
///
/// let vecs = embed::generate(
///     "http://localhost:8080",
///     &["hello world", "foo bar"],
///     "all-MiniLM-L6-v2",
///     None,
/// ).await?;
/// ```
pub async fn generate(
    endpoint: &str,
    texts: &[&str],
    model: &str,
    api_key: Option<&str>,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{}/v1/embeddings", endpoint.trim_end_matches('/'));
    let req = Request {
        input: texts.iter().map(|s| s.to_string()).collect(),
        model: model.to_string(),
    };

    let client = Client::new();
    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&req);

    if let Some(key) = api_key {
        builder = builder.header("Authorization", format!("Bearer {}", key));
    }

    let resp = builder.send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(EmbedError(format!("{}: {}", status, body)));
    }

    let data: Response = resp.json().await.map_err(|e| {
        EmbedError(format!("failed to parse response: {}", e))
    })?;

    Ok(data.data.into_iter().map(|e| e.embedding).collect())
}

/// Generate a single embedding.
pub async fn generate_one(
    endpoint: &str,
    text: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<Vec<f32>, EmbedError> {
    let mut result = generate(endpoint, &[text], model, api_key).await?;
    result.pop().ok_or_else(|| EmbedError("empty response".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running embedding server
    async fn test_generate() {
        let vecs = generate(
            "http://localhost:8080",
            &["hello", "world"],
            "all-MiniLM-L6-v2",
            None,
        )
        .await
        .unwrap();

        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].len(), 384); // MiniLM dimension
    }
}
