//! Hierarchical response ID system for agentic task tracing.
//!
//! See `docs/specs/session-tracking.md` for full specification.
//!
//! Key types:
//! - [`ResponseId`] - Unique ID with parent linking and classification
//! - [`RequestContext`] - Bundles session, endpoint, and timing state
//! - [`RequestClassification`] - Categorizes request type for routing

use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Response ID with support for hierarchical/recursive tracking.
#[derive(Debug, Clone)]
pub struct ResponseId {
    /// Unique identifier (UUID v4)
    pub id: Uuid,
    /// Optional parent ID for conversation threading
    pub parent_id: Option<Uuid>,
    /// Classification tag (e.g., "chat", "embedding", "tool_call")
    pub classification: String,
    /// Timestamp of generation
    pub timestamp_ms: u64,
    /// Sequence number within a conversation/session
    pub sequence: u32,
    /// Provider-specific ID (e.g., OpenAI's response ID)
    pub provider_id: Option<String>,
}

impl ResponseId {
    /// Generate a new response ID with optional parent.
    pub fn new(classification: &str, parent_id: Option<Uuid>, sequence: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id,
            classification: classification.to_string(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            sequence,
            provider_id: None,
        }
    }

    /// Create from a previous response ID (for conversation chaining).
    pub fn from_parent(parent: &ResponseId, classification: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: Some(parent.id),
            classification: classification.to_string(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            sequence: parent.sequence + 1,
            provider_id: None,
        }
    }

    /// Parse from previous_response_id string (Open Responses format).
    pub fn parse_previous(previous_id: &str) -> Option<Uuid> {
        // Try parsing as UUID directly
        if let Ok(uuid) = Uuid::parse_str(previous_id) {
            return Some(uuid);
        }
        // Try extracting UUID from prefixed format (e.g., "resp_abc123...")
        if let Some(hex_part) = previous_id.strip_prefix("resp_") {
            // Try parsing the hex portion
            if let Ok(uuid) = Uuid::parse_str(hex_part) {
                return Some(uuid);
            }
        }
        None
    }

    /// With provider-specific ID (e.g., from upstream response).
    pub fn with_provider_id(mut self, provider_id: String) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    /// Convert to bytes for storage in PromptResponse.id field.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.id.as_bytes().to_vec()
    }

    /// Convert to Open Responses format string.
    pub fn to_open_responses_format(&self) -> String {
        format!("resp_{}", self.id.simple())
    }

    /// Convert to bytes with full metadata (for extended storage).
    pub fn to_extended_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        // ID (16 bytes)
        bytes.extend_from_slice(self.id.as_bytes());
        // Parent ID (16 bytes or zeros)
        if let Some(parent) = &self.parent_id {
            bytes.extend_from_slice(parent.as_bytes());
        } else {
            bytes.extend_from_slice(&[0u8; 16]);
        }
        // Timestamp (8 bytes)
        bytes.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        // Sequence (4 bytes)
        bytes.extend_from_slice(&self.sequence.to_le_bytes());
        // Classification length + bytes (variable)
        bytes.extend_from_slice(&(self.classification.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.classification.as_bytes());
        bytes
    }

    /// Parse from extended bytes.
    pub fn from_extended_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 44 {
            return None;
        }
        let id = Uuid::from_slice(&bytes[0..16]).ok()?;
        let parent_bytes = &bytes[16..32];
        let parent_id = if parent_bytes.iter().all(|&b| b == 0) {
            None
        } else {
            Some(Uuid::from_slice(parent_bytes).ok()?)
        };
        let timestamp_ms = u64::from_le_bytes(bytes[32..40].try_into().ok()?);
        let sequence = u32::from_le_bytes(bytes[40..44].try_into().ok()?);
        let class_len = u32::from_le_bytes(bytes[44..48].try_into().ok()?) as usize;
        if bytes.len() < 48 + class_len {
            return None;
        }
        let classification = String::from_utf8_lossy(&bytes[48..48 + class_len]).to_string();

        Some(Self {
            id,
            parent_id,
            classification,
            timestamp_ms,
            sequence,
            provider_id: None,
        })
    }
}

impl Default for ResponseId {
    fn default() -> Self {
        Self::new("unknown", None, 0)
    }
}

/// Request context for tracking through routing.
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    /// Session ID (from headers or generated)
    pub session_id: String,
    /// Previous response ID for conversation chaining
    pub previous_response_id: Option<Uuid>,
    /// Current sequence number in the conversation
    pub sequence: u32,
    /// Original endpoint path (e.g., "/v1/chat/completions")
    pub endpoint_path: String,
    /// Request classification
    pub classification: RequestClassification,
    /// Request start time for latency tracking
    pub start_time: Option<std::time::Instant>,
    /// Provider-specific request ID (if any)
    pub provider_request_id: Option<String>,
}

impl RequestContext {
    /// Create context for a chat completion request.
    pub fn for_chat(session_id: &str, previous_response_id: Option<&str>) -> Self {
        Self {
            session_id: session_id.to_string(),
            previous_response_id: previous_response_id.and_then(ResponseId::parse_previous),
            sequence: 0,
            endpoint_path: "/v1/chat/completions".to_string(),
            classification: RequestClassification::Chat,
            start_time: Some(std::time::Instant::now()),
            provider_request_id: None,
        }
    }

    /// Create context for an embedding request.
    pub fn for_embedding(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            previous_response_id: None,
            sequence: 0,
            endpoint_path: "/v1/embeddings".to_string(),
            classification: RequestClassification::Embedding,
            start_time: Some(std::time::Instant::now()),
            provider_request_id: None,
        }
    }

    /// Create context for an Anthropic messages request.
    pub fn for_anthropic(session_id: &str, previous_response_id: Option<&str>) -> Self {
        Self {
            session_id: session_id.to_string(),
            previous_response_id: previous_response_id.and_then(ResponseId::parse_previous),
            sequence: 0,
            endpoint_path: "/v1/messages".to_string(),
            classification: RequestClassification::Chat,
            start_time: Some(std::time::Instant::now()),
            provider_request_id: None,
        }
    }

    /// With specific endpoint path override.
    pub fn with_endpoint_path(mut self, path: &str) -> Self {
        self.endpoint_path = path.to_string();
        self
    }

    /// With sequence number.
    pub fn with_sequence(mut self, seq: u32) -> Self {
        self.sequence = seq;
        self
    }

    /// Generate a response ID from this context.
    pub fn generate_response_id(&self) -> ResponseId {
        ResponseId::new(
            self.classification.as_str(),
            self.previous_response_id,
            self.sequence,
        )
    }

    /// Calculate latency if start_time was set.
    pub fn latency_ms(&self) -> u64 {
        self.start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Classification of request type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestClassification {
    #[default]
    Chat,
    Embedding,
    Completion,
    ToolCall,
    Image,
    Audio,
    Vision,
}

impl RequestClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Completion => "completion",
            Self::ToolCall => "tool_call",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Vision => "vision",
        }
    }

    /// Infer classification from endpoint path.
    pub fn from_endpoint(path: &str) -> Self {
        if path.contains("embedding") {
            Self::Embedding
        } else if path.contains("completion") {
            if path.contains("chat") {
                Self::Chat
            } else {
                Self::Completion
            }
        } else if path.contains("message") {
            Self::Chat
        } else if path.contains("image") {
            Self::Image
        } else if path.contains("audio") {
            Self::Audio
        } else {
            Self::Chat
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_id_generation() {
        let id = ResponseId::new("chat", None, 0);
        assert!(!id.id.is_nil());
        assert!(id.parent_id.is_none());
        assert_eq!(id.classification, "chat");
        assert_eq!(id.sequence, 0);
    }

    #[test]
    fn test_response_id_chaining() {
        let parent = ResponseId::new("chat", None, 0);
        let child = ResponseId::from_parent(&parent, "chat");

        assert_eq!(child.parent_id, Some(parent.id));
        assert_eq!(child.sequence, 1);
    }

    #[test]
    fn test_response_id_serialization() {
        let original = ResponseId::new("embedding", None, 5);
        let bytes = original.to_extended_bytes();
        let parsed = ResponseId::from_extended_bytes(&bytes).unwrap();

        assert_eq!(original.id, parsed.id);
        assert_eq!(original.classification, parsed.classification);
        assert_eq!(original.sequence, parsed.sequence);
    }

    #[test]
    fn test_parse_previous_id() {
        // UUID format
        let uuid = Uuid::new_v4();
        let parsed = ResponseId::parse_previous(&uuid.to_string());
        assert_eq!(parsed, Some(uuid));

        // Invalid format
        assert!(ResponseId::parse_previous("invalid").is_none());
    }

    #[test]
    fn test_request_classification_from_endpoint() {
        assert_eq!(
            RequestClassification::from_endpoint("/v1/chat/completions"),
            RequestClassification::Chat
        );
        assert_eq!(
            RequestClassification::from_endpoint("/v1/embeddings"),
            RequestClassification::Embedding
        );
        assert_eq!(
            RequestClassification::from_endpoint("/v1/completions"),
            RequestClassification::Completion
        );
    }

    #[test]
    fn test_open_responses_format() {
        let id = ResponseId::new("chat", None, 0);
        let formatted = id.to_open_responses_format();
        assert!(formatted.starts_with("resp_"));
    }
}
