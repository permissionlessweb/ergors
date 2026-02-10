//! RLM query types for Rust-Python communication

use serde::{Deserialize, Serialize};

/// RLM query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmQuery {
    pub query: String,
    pub guild_id: String,
    pub max_iterations: u32,
    pub max_sub_calls: u32,
}

/// Document for RLM context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub source_uri: String,
    pub content: String,
    pub doc_type: String,
    pub tags: Vec<String>,
    pub ingested_at: i64,
}

/// RLM query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmResponse {
    pub answer: String,
    pub source_uris: Vec<String>,
    pub iterations: u32,
    pub sub_llm_calls: u32,
    pub cost_usd: f64,
    pub latency_ms: u64,
}

/// Document metadata returned by list_documents (no content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub doc_id: String,
    pub name: String,
    pub source: String,
    pub size: usize,
}

/// Search excerpt from a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentExcerpt {
    pub doc_id: String,
    pub offset: usize,
    pub content: String,
    pub match_count: usize,
}

/// JSON-RPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

/// JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}
