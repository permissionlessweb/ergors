# CW-AGENT Minimal Prompt Capture Specification

## Overview

A minimized version of CW-AGENT that provides a simple HTTP API endpoint for routing LLM prompt requests while deterministically capturing all prompts, context, and responses to Cnidarium storage. This removes all sacred geometry complexity and focuses purely on reliable data capture.

## Core Requirements

### 1. Simple HTTP API Server
- Single endpoint: `POST /api/prompt`
- Accept prompt requests with context
- Route to configured LLM providers
- Return responses to clients
- Capture all data to Cnidarium storage

### 2. Deterministic Storage
- Use Cnidarium for append-only storage
- Store each request/response as a versioned entry
- Include timestamps and metadata
- Support querying historical data

### 3. Configuration
- Keep existing TOML config structure
- Support API keys configuration
- Remove all sacred geometry parameters
- Focus on essential settings only

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   CW-AGENT MINIMAL                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   HTTP Client                                               │
│       │                                                     │
│       ▼                                                     │
│   ┌─────────────────┐                                      │
│   │   HTTP API      │                                      │
│   │  POST /prompt   │                                      │
│   └────────┬────────┘                                      │
│            │                                                │
│            ▼                                                │
│   ┌─────────────────┐     ┌──────────────────┐           │
│   │  Request Router │────▶│  LLM Providers   │           │
│   │                 │     │  (OpenAI, etc)   │           │
│   └────────┬────────┘     └──────────────────┘           │
│            │                                                │
│            ▼                                                │
│   ┌─────────────────┐                                      │
│   │   Cnidarium     │                                      │
│   │   Storage       │                                      │
│   │  (Deterministic)│                                      │
│   └─────────────────┘                                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## API Specification

### POST /api/prompt

Request:
```json
{
  "prompt": "string",
  "context": {
    "session_id": "string (optional)",
    "user_id": "string (optional)",
    "metadata": {}
  },
  "model": "string (optional, default from config)",
  "temperature": 0.7,
  "max_tokens": 1000
}
```

Response:
```json
{
  "id": "uuid",
  "prompt": "string",
  "response": "string",
  "model": "string",
  "timestamp": "ISO 8601",
  "tokens_used": {
    "prompt": 100,
    "completion": 200,
    "total": 300
  }
}
```

### GET /api/prompts

Query historical prompts/responses:

```json
{
  "session_id": "string (optional)",
  "user_id": "string (optional)",
  "start_time": "ISO 8601 (optional)",
  "end_time": "ISO 8601 (optional)",
  "limit": 100
}
```

### GET /health

Basic health check endpoint.

## Storage Schema

### Cnidarium Key Structure

```rust
enum StorageKey {
    Prompt { id: Uuid },
    Session { session_id: String, prompt_id: Uuid },
    User { user_id: String, prompt_id: Uuid },
    Timestamp { timestamp: i64, prompt_id: Uuid },
}
```

### Stored Value Structure

```rust
struct PromptRecord {
    id: Uuid,
    prompt: String,
    context: HashMap<String, Value>,
    response: String,
    model: String,
    timestamp: DateTime<Utc>,
    tokens_used: TokenUsage,
    request_duration_ms: u64,
}
```

## Configuration Template

```toml
# Minimal CW-AGENT Configuration

[server]
host = "0.0.0.0"
port = 8080

[storage]
data_dir = "./data"
compression = true
snapshot_interval = 3600  # seconds

[llm]
api_keys_file = "api-keys.json"
default_model = "gpt-3.5-turbo"
timeout_seconds = 30
max_retries = 3

[logging]
level = "info"
file = "./logs/cw-agent.log"
```

## Implementation Plan

### Phase 1: Core Infrastructure
1. Strip down existing codebase to essentials
2. Remove all sacred geometry code
3. Simplify state management to just Cnidarium
4. Create minimal HTTP server with single endpoint

### Phase 2: Storage Implementation
1. Implement deterministic key generation
2. Create append-only storage pattern
3. Add basic indexing for queries
4. Implement snapshot functionality

### Phase 3: API Integration
1. Integrate existing LLM provider code
2. Add request routing logic
3. Implement response capture
4. Add error handling and retries

### Phase 4: Query Capabilities
1. Add historical query endpoint
2. Implement filtering by session/user
3. Add time-based queries
4. Export functionality (JSON/CSV)

## Benefits

1. **Simplicity**: Single-purpose service with minimal complexity
2. **Reliability**: Deterministic storage ensures no data loss
3. **Auditability**: Complete history of all prompts/responses
4. **Compatibility**: Uses existing config structure and LLM providers
5. **Performance**: Lightweight with minimal overhead

## Removed Features

- Sacred geometry calculations
- Tetrahedral topology
- Golden ratio resource allocation
- Möbius sandloops
- Fractal state management
- Complex orchestration
- Multi-node coordination
- Python integration
- Metrics collection (beyond basic prompt stats)

## Future Extensions

- Batch processing endpoint
- Streaming responses
- Custom model routing rules
- Response caching
- Rate limiting
- Authentication/authorization
- Prometheus metrics export

This minimal version focuses purely on the core value proposition: reliably capturing LLM interactions to persistent storage while maintaining the essential configuration and provider integration from the original system.
