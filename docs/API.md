# API Authentication Guide

This guide explains how to make authenticated calls to ERGORS permissioned endpoints using Ed25519 cryptographic signatures. ERGORS provides a minimal HTTP API for capturing and retrieving LLM prompt/response interactions. There are two types of API's served by nodes, public & protected.

## Overview

ERGORS uses Ed25519 signature-based authentication to protect sensitive endpoints. Public endpoints (like `/health`) are accessible without authentication, while protected endpoints require valid cryptographic signatures in request headers.

```sh
curl -X POST http://localhost:8080/api/prompt \
     -H "Content-Type: application/json" \
     -d '{"messages":[{"role": "user", "content": "how many barbers does jroc have?"}],"model":"akash","context":null,"llm_config":null}'
```

## Authentication Requirements: [Authentication Middlware](./API-AUTHENTICATION.md)

Authenticaton is required for granular control in how api are accessed. Each node has its own identity key pair, and is used to register to the network other keys that are able to access the protected apis.

### Keys

### 1. Node Keys

### 2. Operator Keys

## Using The Engine

- blake3 hash of entire prompt
- signature of hash
- encode message in header metadata

Protected endpoints require three headers:

- `x-signature`: Ed25519 signature (hex-encoded)
- `x-timestamp`: Unix timestamp in seconds
- `x-public-key`: Ed25519 public key (hex-encoded)

### Signature Generation

The signature is computed over a Blake3 hash of the request body concatenated with the timestamp:

```math
signature = sign(Blake3(body || timestamp))
```

Where:

- `body` is the raw request body bytes (empty for GET requests)
- `timestamp` is the Unix timestamp as a string
- `||` represents concatenation

### Public Endpoints (No Authentication)

- `GET /health` - Server health status

### Protected Endpoints (Authentication Required)

- `GET /api/prompts` - Query stored prompts
- `POST /api/prompt` - Submit a prompt for processing
- `POST /orchestrate/bootstrap` - Bootstrap a new node
- `POST /orchestrate/fractal` - Create fractal HOE
- `POST /orchestrate/prune` - Prune node state
- `GET /network/topology` - Get network topology

## Proto-based Type/Value Pattern

ERGORS follows a proto-based type/value tuple pattern for all request/response messages. Each endpoint expects:

- **Request**: Proto-generated message type (e.g., `BootstrapNodeRequest`)
- **Response**: Proto-generated message type (e.g., `BootstrapNodeResponse`)

This ensures type safety and enables versioning control across the API surface.

---

For more information on the ERGORS architecture and API design, see the [main README](../README.md).

---

## Endpoints

### 1. Submit Prompt - `POST /api/prompt`

Process a prompt through configured LLM providers and store the interaction.

**Proto Type**: `ergors.orch.v1.PromptRequest`
**Type URL**: `/ergors.orch.v1.PromptRequest`

#### Request Headers

```
Content-Type: application/json
```

#### Request Body Formats

The API accepts **three flexible formats** that are normalized to the canonical `PromptRequest` type:

**Format 1: Simple Prompt (Recommended for quick testing)**

```json
{
  "prompt": "Did jroc ever have a barber???",
  "model": "gpt-4"
}
```

**Format 2: Direct Content**

```json
{
  "content": "Did jroc ever have a barber???",
  "model": "gpt-4"
}
```

**Format 3: Canonical Messages Format (Full Control)**

```json
{
  "messages": [
    {
      "role": "user",
      "content": "Did jroc ever have a barber???"
    }
  ],
  "model": "gpt-4",
  "context": {
    "session_id": "user-session-123",
    "user_id": "john_doe",
    "thread_id": "thread-456"
  },
  "llm_config": {
    "temperature": 0.7,
    "max_tokens": 1000
  }
}
```

#### Canonical Request Fields (Proto Definition)

```protobuf
message PromptRequest {}
message PromptMessage {}
message PromptContext {}
```

#### Field Defaults

- **model**: Defaults to `"gpt-4"` if not specified
- **messages**: Simplified formats (`prompt`, `content`) are converted to `[{"role": "user", "content": "..."}]`
- **context**: Optional, omitted if not provided
- **llm_config**: Optional, omitted if not provided

#### cURL Examples

Simple prompt (minimal):

```bash
curl -X POST http://localhost:8080/api/prompt \
  -H "Content-Type: application/json" \
  -d '{"messages": [{"role": "user","content": "Did jroc have a barber in sunnyvale?", "model": "claude"}]}'
```

Prompt with context (no metadata):

```bash
curl -X POST http://localhost:8080/api/prompt \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain quantum computing in simple terms",
    "context": {
      "session_id": "learning-session-1",
      "user_id": "student123"
    },
    "model": "gpt-4",
    "temperature": 0.5,
    "max_tokens": 500
  }'
```

#### Python Example

```python
import requests
import json

url = "http://localhost:8080/api/prompt"
payload = {
    "prompt": "Write a haiku about programming",
    "context": {
        "session_id": "poetry-session",
        "user_id": "poet42"
    },
    "temperature": 0.8
}

response = requests.post(url, json=payload)
result = response.json()
print(f"Response: {result['response']}")
```

#### JavaScript Example

```javascript
const response = await fetch('http://localhost:8080/api/prompt', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({
    prompt: "Suggest a good book to read",
    context: {
      session_id: "book-recommendations",
      user_id: "reader789"
    },
    model: "claude-3-sonnet-20240229",
    max_tokens: 200
  })
});

const data = await response.json();
console.log('Book suggestion:', data.response);
```

---

### 2. Query Prompts - `GET /api/prompts`

Retrieve historical prompt/response interactions with filtering options.

#### Query Parameters

- **session_id** (optional): Filter by session ID
- **user_id** (optional): Filter by user ID  
- **start_time** (optional): ISO 8601 timestamp for start of time range
- **end_time** (optional): ISO 8601 timestamp for end of time range
- **limit** (optional): Maximum results to return (default: 100, max: 1000)

#### Response

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "prompt": "What is machine learning?",
    "response": "Machine learning is a subset of artificial intelligence...",
    "model": "gpt-3.5-turbo",
    "timestamp": "2024-01-01T12:00:00.000Z",
    "tokens_used": {
      "prompt": 15,
      "completion": 45,
      "total": 60
    },
    "request_duration_ms": 1800
  }
]
```

#### cURL Examples

Get all recent prompts:

```bash
curl "http://localhost:8080/api/prompts?limit=10"
```

Get prompts for a specific session:

```bash
curl "http://localhost:8080/api/prompts?session_id=user-session-123&limit=50"
```

Get prompts for a user within a time range:

```bash
curl "http://localhost:8080/api/prompts?user_id=john_doe&start_time=2024-01-01T00:00:00Z&end_time=2024-01-02T00:00:00Z"
```

#### Python Example

```python
import requests
from datetime import datetime, timedelta

# Get last hour of prompts for a user
end_time = datetime.utcnow()
start_time = end_time - timedelta(hours=1)

params = {
    "user_id": "student123",
    "start_time": start_time.isoformat() + "Z",
    "end_time": end_time.isoformat() + "Z",
    "limit": 25
}

response = requests.get("http://localhost:8080/api/prompts", params=params)
prompts = response.json()

for prompt in prompts:
    print(f"[{prompt['timestamp']}] {prompt['prompt'][:50]}...")
```

---

### 3. Health Check - `GET /health`

Check service health and status.

#### Response

```json
{
  "status": "ok",
  "version": "0.1.0", 
  "uptime_seconds": 3600,
  "storage_status": "healthy"
}
```

#### cURL Example

```bash
curl http://localhost:8080/health
```

#### Python Example

```python
import requests

response = requests.get("http://localhost:8080/health")
health = response.json()

if health["status"] == "ok":
    print(f"Service healthy, uptime: {health['uptime_seconds']}s")
else:
    print("Service unhealthy!")
```

---

## Error Responses

All endpoints return error responses in this format:

```json
{
  "error": "Error description",
  "code": "ERROR_CODE",
  "timestamp": "2024-01-01T12:00:00.000Z"
}
```

### Common HTTP Status Codes

- **200**: Success
- **400**: Bad Request (invalid parameters)
- **500**: Internal Server Error (LLM provider or storage issues)

### Example Error Response

```json
{
  "error": "Prompt cannot be empty",
  "code": "INVALID_PROMPT", 
  "timestamp": "2024-01-01T12:00:00.000Z"
}
```

## Rate Limits

Currently no rate limits enforced, but LLM providers may have their own limits.

---

## Data Storage

All prompt/response interactions are stored deterministically in Cnidarium with:

- **Indexing**: By session ID, user ID, and timestamp
- **Persistence**: Append-only storage for audit trails
- **Snapshots**: Periodic state snapshots for recovery

---

## Getting Started

1. **Start the service**:

   ```bash
   cd packages/ergors
   cargo run -- start
   ```

2. **Test with a simple prompt**:

   ```bash
   curl -X POST http://localhost:8080/api/prompt \
     -H "Content-Type: application/json" \
     -d '{"messages":[{"role": "user", "content": "hello are we connected"}],"model":"Qwen3-235B-A22B-Instruct-2507-FP8","context":null,"llm_config":null}'
   ```

3. **Check stored prompts**:

   ```bash
   curl http://localhost:8080/api/prompts?limit=5
   ```

For more detailed setup instructions, see [README.md](README.md).
