//! Embedder trait and implementations.
//!
//! Production embedders:
//! - CandleEmbedder: Generic local model via Candle (BGE, Qwen, E5, etc.) — enable "candle" feature
//! - RemoteEmbedder: Remote OpenAI-compatible endpoint (e.g., Akash deployments) — enable "openai" feature
//! - OpenAIEmbedder: OpenAI API — enable "openai" feature
//! - DummyEmbedder: Deterministic testing only
//!
//! ## Feature flags
//! ```toml
//! ergors-rag = { version = "0.1", features = ["candle"] }  # Local inference
//! ergors-rag = { version = "0.1", features = ["openai"] }  # Remote/OpenAI API
//! ```
//!
//! ## Supported models (candle feature)
//! - BAAI/bge-small-en-v1.5 (384 dims, English, ~134MB)
//! - BAAI/bge-base-en-v1.5 (768 dims, English, ~438MB)
//! - BAAI/bge-large-en-v1.5 (1024 dims, English, ~1.3GB)
//! - Qwen/Qwen2.5-Math-RM-72B (check dimension, large)
//! - intfloat/multilingual-e5-small (384 dims, 100+ languages)
//! - Any BERT-compatible model on HuggingFace

use anyhow::Result;
use async_trait::async_trait;

/// Trait for generating text embeddings.
///
/// Implementations should be Send + Sync for use in async contexts.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Generate an embedding for a single text string.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for a batch of texts (more efficient).
    ///
    /// Default implementation just calls embed() in a loop, but real
    /// implementations should batch for efficiency.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Return the dimensionality of embeddings produced by this model.
    fn dimension(&self) -> usize;
}

/// Dummy embedder for testing.
///
/// Returns random vectors of fixed dimension. DO NOT USE IN PRODUCTION.
pub struct DummyEmbedder {
    dimension: usize,
}

impl DummyEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl Embedder for DummyEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Hash the text to get a deterministic "embedding"
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        // Generate pseudo-random vector seeded by text hash
        let mut vec = Vec::with_capacity(self.dimension);
        let mut rng_state = seed;
        for _ in 0..self.dimension {
            // Simple LCG for deterministic pseudo-random numbers
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let val = ((rng_state >> 16) as f32) / 32768.0 - 1.0; // map to [-1, 1]
            vec.push(val);
        }

        Ok(vec)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

// ============================================================================
// Candle Embedder (Generic BERT-like models)
// ============================================================================

#[cfg(feature = "candle")]
pub mod candle {
    use super::*;
    use candle_core::{DType, Device, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config, DTYPE};
    use std::sync::Arc;
    use tokenizers::Tokenizer;

    /// Generic embedder for any BERT-compatible model via Candle.
    ///
    /// Supports BGE, Qwen, E5, and other BERT-like models from HuggingFace.
    /// Downloads model on first use (~100MB-1GB depending on model).
    ///
    /// ## Example
    /// ```rust
    /// use ergors_rag::embedder::candle::CandleEmbedder;
    ///
    /// // BGE small (fast, good for most use cases)
    /// let embedder = CandleEmbedder::new("BAAI/bge-small-en-v1.5").await?;
    ///
    /// // Multilingual E5
    /// let embedder = CandleEmbedder::new("intfloat/multilingual-e5-small").await?;
    ///
    /// let vec = embedder.embed("hello world").await?;
    /// ```
    pub struct CandleEmbedder {
        model: Arc<BertModel>,
        tokenizer: Arc<Tokenizer>,
        device: Device,
        dimension: usize,
    }

    impl CandleEmbedder {
        /// Create with default model (bge-small-en-v1.5, fast and good).
        pub async fn new_default() -> Result<Self> {
            Self::new("BAAI/bge-small-en-v1.5").await
        }

        /// Create with any BERT-compatible HuggingFace model.
        ///
        /// Popular models:
        /// - BAAI/bge-small-en-v1.5 (384 dims, fast)
        /// - BAAI/bge-base-en-v1.5 (768 dims, better quality)
        /// - intfloat/multilingual-e5-small (384 dims, 100+ langs)
        pub async fn new(model_id: &str) -> Result<Self> {
            Self::with_device(model_id, Device::Cpu).await
        }

        /// Create with custom device (use `Device::new_cuda(0)?` for GPU).
        pub async fn with_device(model_id: &str, device: Device) -> Result<Self> {
            tracing::info!("Loading embedding model: {} (device: {:?})", model_id, device);

            // Download from HuggingFace Hub
            let api = hf_hub::api::tokio::Api::new()
                .context("Failed to create HuggingFace API client")?;
            let repo = api.model(model_id.to_string());

            // Tokenizer
            let tokenizer_path = repo
                .get("tokenizer.json")
                .await
                .context("Failed to download tokenizer")?;
            let tokenizer = Tokenizer::from_file(tokenizer_path)
                .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

            // Config
            let config_path = repo
                .get("config.json")
                .await
                .context("Failed to download config")?;
            let config: Config = serde_json::from_reader(std::fs::File::open(config_path)?)
                .context("Failed to parse config")?;
            let dimension = config.hidden_size;

            // Weights (try safetensors first, fallback to pytorch_model.bin)
            let weights_path = repo
                .get("model.safetensors")
                .await
                .or_else(|_| async { repo.get("pytorch_model.bin").await })
                .await
                .context("Failed to download model weights (tried .safetensors and .bin)")?;

            let vb = if weights_path.extension().and_then(|s| s.to_str()) == Some("safetensors") {
                unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)? }
            } else {
                VarBuilder::from_pth(&weights_path, DTYPE, &device)?
            };

            let model = BertModel::load(vb, &config)?;

            tracing::info!(
                "Embedding model loaded: {} (dimension: {}, device: {:?})",
                model_id,
                dimension,
                device
            );

            Ok(Self {
                model: Arc::new(model),
                tokenizer: Arc::new(tokenizer),
                device,
                dimension,
            })
        }

        /// Tokenize and encode text to tensor.
        fn tokenize(&self, texts: &[&str]) -> Result<Tensor> {
            let tokens = self
                .tokenizer
                .encode_batch(texts.to_vec(), true)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

            let token_ids: Vec<Vec<u32>> = tokens
                .iter()
                .map(|t| t.get_ids().iter().map(|&id| id as u32).collect())
                .collect();

            // Pad to max length in batch
            let max_len = token_ids.iter().map(|t| t.len()).max().unwrap_or(0);
            let mut padded = Vec::new();
            for ids in token_ids {
                let mut row = ids.clone();
                row.resize(max_len, 0); // PAD token is usually 0
                padded.extend(row);
            }

            let shape = (texts.len(), max_len);
            Tensor::from_vec(padded, shape, &self.device).context("Failed to create input tensor")
        }

        /// Mean pooling over token embeddings (standard for sentence embeddings).
        fn mean_pool(&self, hidden_states: &Tensor) -> Result<Tensor> {
            // hidden_states shape: (batch, seq_len, hidden_dim)
            let sum = hidden_states.sum(1)?; // (batch, hidden_dim)
            let seq_len = hidden_states.dims()[1] as f64;
            let mean = (sum / seq_len)?;
            Ok(mean)
        }

        /// Normalize embeddings to unit length (for cosine similarity).
        fn normalize(&self, embeddings: &Tensor) -> Result<Tensor> {
            let norm = embeddings.sqr()?.sum_keepdim(1)?.sqrt()?;
            embeddings.broadcast_div(&norm)
        }

        /// Internal batch embedding (shared by embed and embed_batch).
        async fn embed_batch_internal(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }

            // Tokenize
            let input_ids = self.tokenize(texts)?;

            // Forward pass
            let hidden_states = self.model.forward(&input_ids)?;

            // Mean pooling + normalize
            let pooled = self.mean_pool(&hidden_states)?;
            let normalized = self.normalize(&pooled)?;

            // Extract to Vec<Vec<f32>>
            let embeddings_flat = normalized
                .to_vec2::<f32>()
                .context("Failed to convert embeddings to Vec")?;
            Ok(embeddings_flat)
        }
    }

    #[async_trait]
    impl Embedder for CandleEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let batch = self.embed_batch_internal(&[text]).await?;
            Ok(batch.into_iter().next().unwrap())
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            self.embed_batch_internal(texts).await
        }

        fn dimension(&self) -> usize {
            self.dimension
        }
    }
}

// ============================================================================
// Remote Embedder (OpenAI-compatible endpoints)
// ============================================================================

#[cfg(feature = "openai")]
pub mod remote {
    use super::*;
    use anyhow::Context;
    use reqwest::Client;
    use serde::{Deserialize, Serialize};

    /// Remote embedder for OpenAI-compatible endpoints (e.g., Akash deployments).
    ///
    /// Use this to call embedding services that expose `/v1/embeddings` endpoint.
    ///
    /// ## Example
    /// ```rust,ignore
    /// use ergors_rag::embedder::{Embedder, remote::RemoteEmbedder};
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     // Point to Akash deployment
    ///     let embedder = RemoteEmbedder::new(
    ///         "http://provider.akash.network:8080",
    ///         "all-MiniLM-L6-v2",
    ///         384
    ///     )?;
    ///     let vec = embedder.embed("hello world").await?;
    ///     Ok(())
    /// }
    /// ```
    pub struct RemoteEmbedder {
        client: Client,
        base_url: String,
        model: String,
        dimension: usize,
    }

    #[derive(Serialize)]
    struct EmbedRequest {
        input: Vec<String>,
        model: String,
    }

    #[derive(Deserialize)]
    struct EmbedResponse {
        data: Vec<EmbedData>,
    }

    #[derive(Deserialize)]
    struct EmbedData {
        embedding: Vec<f32>,
    }

    impl RemoteEmbedder {
        /// Create a remote embedder pointing to an OpenAI-compatible endpoint.
        ///
        /// - `base_url`: The base URL (e.g., "http://provider.akash.network:8080")
        /// - `model`: Model name (must match what the server expects)
        /// - `dimension`: Expected embedding dimension
        pub fn new(base_url: &str, model: &str, dimension: usize) -> Result<Self> {
            Self::with_client(Client::new(), base_url, model, dimension)
        }

        /// Create a remote embedder with a shared HTTP client.
        ///
        /// Use this to reuse an existing `reqwest::Client` for connection pooling
        /// when making many embedding requests.
        ///
        /// - `client`: Shared HTTP client (for connection pooling)
        /// - `base_url`: The base URL (e.g., "http://provider.akash.network:8080")
        /// - `model`: Model name (must match what the server expects)
        /// - `dimension`: Expected embedding dimension
        pub fn with_client(client: Client, base_url: &str, model: &str, dimension: usize) -> Result<Self> {
            let base_url = base_url.trim_end_matches('/').to_string();

            tracing::info!(
                "Remote embedder initialized (base_url: {}, model: {}, dim: {})",
                base_url,
                model,
                dimension
            );

            Ok(Self {
                client,
                base_url,
                model: model.to_string(),
                dimension,
            })
        }

        async fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            let url = format!("{}/v1/embeddings", self.base_url);
            let request = EmbedRequest {
                input: texts.iter().map(|&s| s.to_string()).collect(),
                model: self.model.clone(),
            };

            let response = self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Remote embedding API request failed")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                anyhow::bail!("Remote API error ({}): {}", status, error_text);
            }

            let embed_response: EmbedResponse = response
                .json()
                .await
                .context("Failed to parse remote API response")?;

            let embeddings = embed_response
                .data
                .into_iter()
                .map(|d| d.embedding)
                .collect();

            Ok(embeddings)
        }
    }

    #[async_trait]
    impl Embedder for RemoteEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let batch = self.call_api(&[text]).await?;
            Ok(batch.into_iter().next().unwrap())
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            self.call_api(texts).await
        }

        fn dimension(&self) -> usize {
            self.dimension
        }
    }
}

// ============================================================================
// OpenAI Embedder (API-based)
// ============================================================================

#[cfg(feature = "openai")]
pub mod openai {
    use super::*;
    use anyhow::Context;
    use reqwest::Client;
    use serde::{Deserialize, Serialize};

    /// OpenAI embedder using text-embedding-3-small.
    ///
    /// Requires OPENAI_API_KEY environment variable.
    ///
    /// ## Example
    /// ```rust,ignore
    /// use ergors_rag::embedder::{Embedder, openai::OpenAIEmbedder};
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let embedder = OpenAIEmbedder::new()?;
    ///     let vec = embedder.embed("hello world").await?;
    ///     assert_eq!(vec.len(), 1536);  // text-embedding-3-small dimension
    ///     Ok(())
    /// }
    /// ```
    pub struct OpenAIEmbedder {
        client: Client,
        api_key: String,
        model: String,
        dimension: usize,
    }

    #[derive(Serialize)]
    struct EmbedRequest {
        input: Vec<String>,
        model: String,
    }

    #[derive(Deserialize)]
    struct EmbedResponse {
        data: Vec<EmbedData>,
    }

    #[derive(Deserialize)]
    struct EmbedData {
        embedding: Vec<f32>,
    }

    impl OpenAIEmbedder {
        /// Create with text-embedding-3-small (default, 1536 dims, $0.02/1M tokens).
        pub fn new() -> Result<Self> {
            Self::with_model("text-embedding-3-small", 1536)
        }

        /// Create with custom model.
        ///
        /// Supported models:
        /// - text-embedding-3-small (1536 dims, $0.02/1M tokens)
        /// - text-embedding-3-large (3072 dims, $0.13/1M tokens)
        /// - text-embedding-ada-002 (1536 dims, $0.10/1M tokens, older)
        pub fn with_model(model: &str, dimension: usize) -> Result<Self> {
            let api_key = std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY environment variable not set")?;

            if api_key.is_empty() {
                anyhow::bail!("OPENAI_API_KEY is empty");
            }

            let client = Client::new();

            tracing::info!("OpenAI embedder initialized (model: {})", model);

            Ok(Self {
                client,
                api_key,
                model: model.to_string(),
                dimension,
            })
        }

        async fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            let url = "https://api.openai.com/v1/embeddings";
            let request = EmbedRequest {
                input: texts.iter().map(|&s| s.to_string()).collect(),
                model: self.model.clone(),
            };

            let response = self
                .client
                .post(url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("OpenAI API request failed")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
            }

            let embed_response: EmbedResponse = response
                .json()
                .await
                .context("Failed to parse OpenAI API response")?;

            let embeddings = embed_response
                .data
                .into_iter()
                .map(|d| d.embedding)
                .collect();

            Ok(embeddings)
        }
    }

    #[async_trait]
    impl Embedder for OpenAIEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let batch = self.call_api(&[text]).await?;
            Ok(batch.into_iter().next().unwrap())
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            // OpenAI API supports up to 2048 inputs per request
            // For large batches, chunk them (simple version: just warn for now)
            if texts.len() > 2048 {
                tracing::warn!(
                    "OpenAI embed_batch called with {} texts (max 2048). Consider chunking.",
                    texts.len()
                );
            }
            self.call_api(texts).await
        }

        fn dimension(&self) -> usize {
            self.dimension
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dummy_embedder() {
        let embedder = DummyEmbedder::new(128);
        assert_eq!(embedder.dimension(), 128);

        let vec1 = embedder.embed("hello").await.unwrap();
        let vec2 = embedder.embed("hello").await.unwrap();
        let vec3 = embedder.embed("world").await.unwrap();

        assert_eq!(vec1.len(), 128);
        assert_eq!(vec1, vec2); // Same text -> same embedding (deterministic)
        assert_ne!(vec1, vec3); // Different text -> different embedding
    }
}
