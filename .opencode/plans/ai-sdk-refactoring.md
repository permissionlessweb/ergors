# AI SDK Proxy Refactoring Plan

## Goal

Transform the proxy layer into a transparent interceptor for CLI tools (Claude Code, OpenCode, Cursor) with:
- **Zero configuration** - tools just change base URL
- **SSE streaming passthrough** - real-time token streaming
- **Provider routing** - route to any configured provider
- **Full capture** - store all requests/responses for observability

## Architecture

```
┌──────────────────┐     ┌─────────────────────────────────────────────────┐
│  Claude Code     │     │                  ERGORS Proxy                    │
│  OpenCode        │────►│  /v1/messages         → Anthropic-compatible    │
│  Cursor          │     │  /v1/chat/completions → OpenAI-compatible       │
└──────────────────┘     │                                                  │
                         │  ┌─────────────┐    ┌─────────────────────────┐ │
                         │  │   Capture   │───►│  Provider Router        │ │
                         │  │   Service   │    │  (joints/anthro.rs,     │ │
                         │  └─────────────┘    │   joints/openai.rs)     │ │
                         │         │           └───────────┬─────────────┘ │
                         │         ▼                       ▼               │
                         │  ┌─────────────┐    ┌─────────────────────────┐ │
                         │  │   Storage   │    │  Upstream Provider      │ │
                         │  │  (capture)  │    │  api.anthropic.com      │ │
                         │  └─────────────┘    │  api.openai.com         │ │
                         └─────────────────────└─────────────────────────┘ │
```

## Existing Types to Reuse

**DO NOT create new types for these - they already exist:**

| Type | Location | Purpose |
|------|----------|---------|
| `PromptMessage` | `proto/ergors/orch/v1/orch.proto:106` | role + content |
| `PromptRequest` | `proto/ergors/orch/v1/orch.proto:87` | messages array + model + config |
| `PromptResponse` | `proto/ergors/orch/v1/orch.proto:94` | response + tokens + cost |
| `StreamChunk` | `proto/ergors/proxy/v1/proxy.proto:242` | SSE chunk capture |
| `ProxySession` | `proto/ergors/proxy/v1/proxy.proto:8` | full session capture |
| `OpenAiRequest` | `proto/ergors/orch/v1/orch.proto:154` | OpenAI format |
| `OpenAiMessage` | `proto/ergors/orch/v1/orch.proto:160` | OpenAI message |
| `TokenUsage` | `proto/ergors/orch/v1/orch.proto:117` | token tracking |

---

## Phase 1: Add Tool Call Types to Proto

**File:** `proto/ergors/orch/v1/orch.proto`

Add tool call support (missing from current types):

```protobuf
// Tool definition for function calling
message ToolDefinition {
  string name = 1;
  string description = 2;
  google.protobuf.Struct input_schema = 3;  // JSON Schema
}

// Tool use in assistant message
message ToolUse {
  string id = 1;
  string name = 2;
  google.protobuf.Struct input = 3;
}

// Tool result from user
message ToolResult {
  string tool_use_id = 1;
  oneof content {
    string text = 2;
    bytes binary = 3;
    bool is_error = 4;
  }
}

// Extend PromptMessage to support tool content
message PromptMessage {
  string role = 1;                    // user, assistant, system, tool
  oneof content {
    string text = 2;                  // Simple text content
    repeated ContentBlock blocks = 3; // Multi-part content
  }
  repeated ToolUse tool_use = 4;      // For assistant messages with tool calls
  ToolResult tool_result = 5;         // For tool role messages
}

message ContentBlock {
  oneof block {
    string text = 1;
    ToolUse tool_use = 2;
    ToolResult tool_result = 3;
  }
}

// Extend PromptRequest for tools
message PromptRequest {
  repeated PromptMessage messages = 1;
  string model = 2;
  PromptContext context = 3;
  LlmPromptConfig llm_config = 4;
  repeated ToolDefinition tools = 5;  // NEW: Available tools
  string tool_choice = 6;             // NEW: auto, none, required, or tool name
}
```

---

## Phase 2: SSE Streaming Infrastructure

**File:** `packages/cw-ho/src/proxy/streaming.rs`

Current state: Parses SSE but doesn't properly stream back to client.

### 2.1 Create StreamingResponse wrapper

```rust
// packages/cw-ho/src/proxy/stream.rs

use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::pin::Pin;

pub type SseStream = Pin<Box<dyn Stream<Item = Result<Event, std::io::Error>> + Send>>;

/// Wrap upstream SSE response for passthrough
pub fn passthrough_sse(
    upstream: reqwest::Response,
    capture_tx: mpsc::Sender<CaptureMessage>,
    session_id: String,
) -> Sse<SseStream> {
    let stream = async_stream::stream! {
        let mut sequence = 0u32;
        let mut reader = upstream.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = reader.next().await {
            let chunk = chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE events (split on \n\n)
            while let Some(pos) = buffer.find("\n\n") {
                let event_str = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                // Capture the chunk
                let _ = capture_tx.send(CaptureMessage::Chunk {
                    session_id: session_id.clone(),
                    chunk: StreamChunk {
                        sequence,
                        event_type: extract_event_type(&event_str),
                        data: event_str.as_bytes().to_vec(),
                        received_at: Some(now_timestamp()),
                        delta_text: extract_delta_text(&event_str),
                    },
                }).await;
                sequence += 1;

                // Yield to client
                yield Ok(Event::default().data(event_str));
            }
        }
    };

    Sse::new(Box::pin(stream))
}
```

### 2.2 Update proxy endpoints for streaming

**File:** `packages/cw-ho/src/proxy/endpoints.rs`

```rust
pub async fn handle_anthropic_proxy(
    State(state): State<ErgorsAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session_id = extract_session_id(&headers);
    let is_streaming = is_stream_request(&body);

    // Start capture
    let _ = capture_tx.send(CaptureMessage::SessionStart { ... }).await;

    // Forward to upstream
    let upstream_response = forward_to_anthropic(&state.http_client, &headers, &body).await?;

    if is_streaming {
        // Stream passthrough with capture
        passthrough_sse(upstream_response, capture_tx, session_id).into_response()
    } else {
        // Non-streaming: capture and return
        let response_body = upstream_response.bytes().await?;
        let _ = capture_tx.send(CaptureMessage::SessionComplete { ... }).await;
        Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(response_body))
            .unwrap()
    }
}
```

---

## Phase 3: Provider Routing

**File:** `packages/cw-ho/src/proxy/router.rs`

Allow routing requests to different providers based on configuration.

```rust
pub struct ProxyRouter {
    config: ProxyConfig,
    anthropic_client: reqwest::Client,
    openai_client: reqwest::Client,
}

pub struct ProxyConfig {
    /// Override upstream for Anthropic-format requests
    /// Default: https://api.anthropic.com
    pub anthropic_upstream: Option<String>,

    /// Override upstream for OpenAI-format requests
    /// Default: https://api.openai.com
    pub openai_upstream: Option<String>,

    /// Route specific models to different upstreams
    /// e.g., "gpt-4" -> "https://api.openai.com"
    /// e.g., "claude-*" -> "https://api.anthropic.com"
    pub model_routes: HashMap<String, String>,

    /// API key overrides per upstream
    pub api_keys: HashMap<String, String>,
}

impl ProxyRouter {
    pub fn route_anthropic(&self, model: &str) -> (&str, &str) {
        // Check model-specific route first
        if let Some(upstream) = self.config.model_routes.get(model) {
            let key = self.config.api_keys.get(upstream).map(|s| s.as_str()).unwrap_or("");
            return (upstream, key);
        }

        // Fall back to default anthropic upstream
        let upstream = self.config.anthropic_upstream
            .as_deref()
            .unwrap_or("https://api.anthropic.com");
        (upstream, "")
    }
}
```

---

## Phase 4: Update Joints for Bidirectional Conversion

**Files:**
- `packages/ho-std/src/llm/joints/anthro.rs`
- `packages/ho-std/src/llm/joints/openai.rs`

Add methods to convert between formats (for cross-provider routing):

```rust
// anthro.rs
impl AnthropicJoint {
    /// Convert internal PromptRequest to Anthropic API format
    pub fn to_anthropic_request(req: &PromptRequest) -> AnthropicApiRequest {
        // Extract system message
        let system = req.messages.iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.clone());

        // Convert messages (excluding system)
        let messages: Vec<_> = req.messages.iter()
            .filter(|m| m.role != "system")
            .map(|m| AnthropicMessage {
                role: m.role.clone(),
                content: convert_content(&m),
            })
            .collect();

        AnthropicApiRequest {
            model: req.model.clone(),
            max_tokens: req.llm_config.as_ref().map(|c| c.max_tokens).unwrap_or(4096),
            messages,
            system,
            tools: req.tools.iter().map(convert_tool).collect(),
            stream: true,  // Always stream for proxy
        }
    }

    /// Parse Anthropic API response to internal format
    pub fn from_anthropic_response(resp: &AnthropicApiResponse) -> PromptResponse { ... }
}
```

---

## Phase 5: Configuration

**File:** `packages/cw-ho/src/config.rs`

Add proxy configuration section:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    /// Enable proxy endpoints
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Bind address for proxy (default: 0.0.0.0:8080)
    #[serde(default = "default_proxy_addr")]
    pub bind_addr: String,

    /// Anthropic upstream URL override
    pub anthropic_upstream: Option<String>,

    /// OpenAI upstream URL override
    pub openai_upstream: Option<String>,

    /// Model-specific routing rules
    #[serde(default)]
    pub model_routes: HashMap<String, String>,

    /// Capture settings
    #[serde(default)]
    pub capture: CaptureConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CaptureConfig {
    /// Store captured sessions
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Include streaming chunks in capture
    #[serde(default = "default_true")]
    pub include_chunks: bool,

    /// Max sessions to retain (0 = unlimited)
    #[serde(default)]
    pub max_sessions: usize,
}
```

**Example config (ho.toml):**

```toml
[proxy]
enabled = true
bind_addr = "0.0.0.0:8080"

# Route to local Ollama for certain models
[proxy.model_routes]
"llama-*" = "http://localhost:11434"
"mistral-*" = "http://localhost:11434"

[proxy.capture]
enabled = true
include_chunks = true
max_sessions = 1000
```

---

## Implementation Order

1. **Add tool call types to proto** (~30 lines)
   - `proto/ergors/orch/v1/orch.proto`
   - Run `cargo run -p ho-proto` to regenerate

2. **Implement SSE passthrough**
   - Create `packages/cw-ho/src/proxy/stream.rs`
   - Update `packages/cw-ho/src/proxy/endpoints.rs`

3. **Add proxy router**
   - Create `packages/cw-ho/src/proxy/router.rs`
   - Update endpoint handlers to use router

4. **Configuration**
   - Add `ProxyConfig` to `packages/cw-ho/src/config.rs`
   - Wire into server startup

5. **Test with CLI tools**
   - Set `ANTHROPIC_API_BASE=http://localhost:8080` for Claude Code
   - Set `OPENAI_API_BASE=http://localhost:8080` for OpenCode

---

## Files to Modify

| File | Changes |
|------|---------|
| `proto/ergors/orch/v1/orch.proto` | Add ToolDefinition, ToolUse, ToolResult, extend PromptMessage |
| `packages/cw-ho/src/proxy/mod.rs` | Add stream, router modules |
| `packages/cw-ho/src/proxy/stream.rs` | NEW - SSE passthrough |
| `packages/cw-ho/src/proxy/router.rs` | NEW - Provider routing |
| `packages/cw-ho/src/proxy/endpoints.rs` | Use streaming passthrough |
| `packages/cw-ho/src/config.rs` | Add ProxyConfig |
| `packages/ho-std/src/llm/joints/anthro.rs` | Add bidirectional conversion |
| `packages/ho-std/src/llm/joints/openai.rs` | Add bidirectional conversion |

---

## Verification

```bash
# 1. Build
cargo chec

# 2. Start proxy
cargo run -p cw-ho -- --proxy-addr 0.0.0.0:8080

# 3. Test with curl (non-streaming)
curl -X POST http://localhost:8080/v1/messages \
  -H "x-api-key: $ANTHROPIC_API_KEY" \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{"model":"claude-sonnet-4-20250514","max_tokens":100,"messages":[{"role":"user","content":"Hi"}]}'

# 4. Test with curl (streaming)
curl -X POST http://localhost:8080/v1/messages \
  -H "x-api-key: $ANTHROPIC_API_KEY" \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{"model":"claude-sonnet-4-20250514","max_tokens":100,"stream":true,"messages":[{"role":"user","content":"Hi"}]}'

# 5. Test with Claude Code
ANTHROPIC_API_BASE=http://localhost:8080 claude

# 6. Query captured sessions
curl http://localhost:8080/api/proxy/sessions
```

---

**Status**: Ready for implementation
**Created**: 2026-01-20
**Updated**: 2026-01-20
