//! Session identification and management for proxy requests.

use axum::http::HeaderMap;
use ho_std::types::ergors::proxy::v1::ClientType;
use uuid::Uuid;

/// Extract or generate a session ID from request headers.
pub fn extract_session_id(headers: &HeaderMap) -> String {
    // Try x-request-id first (common in many CLI tools)
    if let Some(request_id) = headers.get("x-request-id") {
        if let Ok(id) = request_id.to_str() {
            return id.to_string();
        }
    }

    // Try x-session-id (custom header for explicit session tracking)
    if let Some(session_id) = headers.get("x-session-id") {
        if let Ok(id) = session_id.to_str() {
            return id.to_string();
        }
    }

    // Try anthropic-beta header (used by Claude Code)
    if let Some(beta) = headers.get("anthropic-beta") {
        if let Ok(beta_str) = beta.to_str() {
            // Extract any session-like identifier from beta features
            if beta_str.contains("computer-use") || beta_str.contains("tools") {
                // Generate a deterministic ID based on timestamp + random
                return format!("claude-{}", Uuid::new_v4());
            }
        }
    }

    // Generate a new UUID as fallback
    Uuid::new_v4().to_string()
}

/// Detect the client type from request headers and body hints.
pub fn detect_client_type(headers: &HeaderMap, model: Option<&str>) -> ClientType {
    // Check User-Agent for known CLI tools
    if let Some(user_agent) = headers.get("user-agent") {
        if let Ok(ua) = user_agent.to_str() {
            let ua_lower = ua.to_lowercase();
            if ua_lower.contains("claude") || ua_lower.contains("anthropic") {
                return ClientType::ClaudeCode;
            }
            if ua_lower.contains("opencode") {
                return ClientType::Opencode;
            }
            if ua_lower.contains("cursor") {
                return ClientType::Cursor;
            }
        }
    }

    // Check for anthropic-specific headers (indicates Claude Code or similar)
    if headers.contains_key("anthropic-version") || headers.contains_key("x-api-key") {
        // Could be Claude Code or another Anthropic client
        if headers.contains_key("anthropic-beta") {
            return ClientType::ClaudeCode;
        }
    }

    // Check model name for hints
    if let Some(model_name) = model {
        let model_lower = model_name.to_lowercase();
        if model_lower.contains("claude") {
            return ClientType::ClaudeCode;
        }
        if model_lower.contains("gpt") || model_lower.contains("o1") {
            return ClientType::Opencode;
        }
    }

    // Check for OpenAI-style authorization (indicates opencode or similar)
    if let Some(auth) = headers.get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Bearer ") {
                return ClientType::Opencode;
            }
        }
    }

    ClientType::Custom
}

/// Extract API key from request headers based on the API format.
pub fn extract_api_key(headers: &HeaderMap, is_anthropic: bool) -> Option<String> {
    if is_anthropic {
        // Anthropic uses x-api-key header
        headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    } else {
        // OpenAI uses Authorization: Bearer <key>
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_id_from_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "test-session-123".parse().unwrap());
        assert_eq!(extract_session_id(&headers), "test-session-123");
    }

    #[test]
    fn test_extract_session_id_generates_uuid() {
        let headers = HeaderMap::new();
        let session_id = extract_session_id(&headers);
        // Should be a valid UUID format
        assert!(Uuid::parse_str(&session_id).is_ok());
    }

    #[test]
    fn test_detect_client_type_claude() {
        let mut headers = HeaderMap::new();
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        headers.insert("anthropic-beta", "computer-use-2024-10-22".parse().unwrap());
        assert_eq!(
            detect_client_type(&headers, Some("claude-3-opus")),
            ClientType::ClaudeCode
        );
    }

    #[test]
    fn test_detect_client_type_openai() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-xxx".parse().unwrap());
        assert_eq!(
            detect_client_type(&headers, Some("gpt-4")),
            ClientType::Opencode
        );
    }
}
