# Mock Inference Provider — Canonical Contracts

Every endpoint returns a **single deterministic response** for any valid request.
Token counts are consistent: **10 input, 25 output** across all providers.

---

## Anthropic: POST /v1/messages

### Headers
```
x-api-key: <any non-empty string>
content-type: application/json
```

### Request Body
```json
{
  "model": "claude-3-haiku-20240307",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 1024
}
```

### Response (200)
```json
{
  "id": "msg-mock-001",
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "This is a mock response from the Anthropic Messages API."
    }
  ],
  "model": "claude-3-haiku-20240307",
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 10,
    "output_tokens": 25
  }
}
```

### Error (401 — missing x-api-key)
```json
{
  "type": "error",
  "error": {
    "type": "authentication_error",
    "message": "Missing or empty x-api-key header"
  }
}
```

---

## OpenAI: POST /v1/chat/completions

### Request Body
```json
{
  "model": "gpt-3.5-turbo",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

### Response (200)
```json
{
  "id": "chatcmpl-mock-001",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "gpt-3.5-turbo",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "This is a mock response from the OpenAI Chat API."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 25,
    "total_tokens": 35
  }
}
```

---

## OpenAI: POST /v1/completions

### Request Body
```json
{
  "model": "text-davinci-003",
  "prompt": "Hello"
}
```

### Response (200)
```json
{
  "id": "cmpl-mock-001",
  "object": "text_completion",
  "created": 1700000000,
  "model": "text-davinci-003",
  "choices": [
    {
      "text": "This is a mock response from the OpenAI Completions API.",
      "index": 0,
      "logprobs": null,
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 25,
    "total_tokens": 35
  }
}
```

---

## OpenAI: GET /v1/models

### Response (200)
```json
{
  "object": "list",
  "data": [
    {"id": "llama2:latest", "object": "model", "created": 1700000000, "owned_by": "organization"},
    {"id": "llama2:7b-chat", "object": "model", "created": 1700000000, "owned_by": "organization"},
    {"id": "mistral:latest", "object": "model", "created": 1700000000, "owned_by": "organization"},
    {"id": "codellama:latest", "object": "model", "created": 1700000000, "owned_by": "organization"}
  ]
}
```

---

## OpenAI: POST /v1/embeddings

### Request Body
```json
{
  "model": "text-embedding-ada-002",
  "input": "Hello"
}
```

### Response (200)
```json
{
  "object": "list",
  "data": [
    {"object": "embedding", "embedding": [0.0, ...], "index": 0}
  ],
  "model": "text-embedding-ada-002",
  "usage": {
    "prompt_tokens": 10,
    "total_tokens": 10
  }
}
```
Embedding dimension: 384 (all zeros).

---

## Ollama: POST /api/generate

### Request Body
```json
{
  "model": "llama2",
  "prompt": "Hello",
  "stream": false
}
```

### Response (200)
```json
{
  "model": "llama2",
  "created_at": "2024-01-01T00:00:00Z",
  "response": "This is a mock response from the Ollama Generate API.",
  "done": true,
  "context": [1, 2, 3],
  "total_duration": 150000000,
  "load_duration": 10000000,
  "prompt_eval_count": 10,
  "prompt_eval_duration": 50000000,
  "eval_count": 25,
  "eval_duration": 90000000
}
```

---

## Ollama: POST /api/chat

### Request Body
```json
{
  "model": "llama2",
  "messages": [{"role": "user", "content": "Hello"}],
  "stream": false
}
```

### Response (200)
```json
{
  "model": "llama2",
  "created_at": "2024-01-01T00:00:00Z",
  "message": {
    "role": "assistant",
    "content": "This is a mock response from the Ollama Chat API."
  },
  "done": true,
  "total_duration": 150000000,
  "load_duration": 10000000,
  "prompt_eval_count": 10,
  "eval_count": 25
}
```

---

## Ollama: GET /api/tags

### Response (200)
Returns model list with deterministic metadata. See source for full shape.

---

## Ollama: POST /api/pull

### Request Body
```json
{"name": "llama2"}
```

### Response (200)
```json
{
  "status": "success",
  "digest": "sha256:<hex-encoded-model-name>"
}
```

---

## Ollama: POST /api/show

### Request Body
```json
{"name": "llama2"}
```

### Response (200)
Returns model metadata (modelfile, parameters, details, model_info).

---

## Ollama: POST /api/embeddings

### Request Body
```json
{
  "model": "llama2",
  "prompt": "Hello"
}
```

### Response (200)
```json
{
  "model": "llama2",
  "embeddings": [[0.0, ...]]
}
```
Embedding dimension: 384 (all zeros).

---

## TGI: POST /generate

### Request Body
```json
{
  "inputs": "Hello",
  "parameters": {"max_new_tokens": 100}
}
```

### Response (200)
```json
{
  "generated_text": "This is a mock response from the TGI API.",
  "details": {
    "finish_reason": "eos_token",
    "generated_tokens": 25,
    "seed": 42
  }
}
```

---

## TGI: POST /generate_stream

Same request as `/generate`. Returns SSE stream of token events.

---

## TGI: GET /info

### Response (200)
```json
{
  "model_id": "mock-model",
  "model_sha": "abc123def456",
  "model_dtype": "float16",
  "model_device_type": "cuda",
  "model_pipeline_tag": "text-generation",
  "max_input_length": 4096,
  "max_total_tokens": 8192,
  "version": "1.4.0"
}
```

---

## API Keys: POST /api/keys/generate

Generate a mock API key for a specific provider. Keys are stored in-memory for the lifetime of the server.

### Request Body
```json
{
  "provider": "openai",
  "valid": true,
  "expiry_seconds": 3600
}
```

`valid` defaults to `true`. `expiry_seconds` is optional (omit for no expiry).

### Response (200)
```json
{
  "api_key": "sk-mock-openai-000000000000000000000000",
  "provider": "openai",
  "expires_at": 1700003600,
  "valid": true
}
```

Key format: `sk-mock-{provider}-{counter:024}`. Deterministic — first key for provider X is always `sk-mock-X-000000000000000000000000`.

---

## API Keys: POST /api/keys/validate

Check whether a previously generated key is valid, expired, or revoked.

### Request Body
```json
{
  "api_key": "sk-mock-openai-000000000000000000000000"
}
```

### Response (200) — valid key
```json
{
  "valid": true,
  "provider": "openai",
  "expired": false,
  "message": "Key is valid"
}
```

### Response (200) — unknown key
```json
{
  "valid": false,
  "provider": null,
  "expired": false,
  "message": "Key not found"
}
```

---

## API Keys: GET /api/keys/list

List all generated keys with their current status.

### Response (200)
```json
{
  "keys": [
    {
      "key": "sk-mock-openai-000000000000000000000000",
      "provider": "openai",
      "created_at": 1700000000,
      "expires_at": 1700003600,
      "valid": true,
      "expired": false,
      "usage_count": 0
    }
  ],
  "total": 1
}
```

---

## API Keys: POST /api/keys/revoke

Invalidate a key. Subsequent `/validate` calls will return `valid: false`.

### Request Body
```json
{
  "api_key": "sk-mock-openai-000000000000000000000000"
}
```

### Response (200)
```json
{
  "success": true,
  "message": "Key revoked successfully"
}
```

### Error (404 — key not found)
```json
{
  "success": false,
  "message": "Key not found"
}
```

---

## System: GET /health

### Response (200)
```json
{"status": "ok"}
```

---

## System: GET /

### Response (200)
Lists all available endpoints and provider version.

---

## System: GET /metrics

### Response (200)
```json
{
  "total_requests": 0,
  "models_loaded": 4
}
```
