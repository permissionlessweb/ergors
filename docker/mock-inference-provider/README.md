# Mock Inference Provider

A standalone service that simulates inference provider APIs (Ollama, OpenAI, TGI) for testing Akash deployments without requiring GPU resources.

## Features

- **Ollama API**: `/api/generate`, `/api/chat`, `/api/tags`, `/api/pull`, `/api/show`, `/api/embeddings`
- **OpenAI API**: `/v1/completions`, `/v1/chat/completions`, `/v1/models`, `/v1/embeddings`
- **TGI API**: `/generate`, `/generate_stream`, `/info`
- **Agentic Endpoints**: `/api/agentic/execute`, `/api/agentic/tool-calls`
- **API Key Management**: `/api/keys/generate`, `/api/keys/validate`, `/api/keys/list`, `/api/keys/revoke`
- Configurable latency simulation
- Configurable error rates
- Streaming response support
- Tool call simulation for agentic testing
- Testdata mode for deterministic responses

## Testdata Mode

When enabled with `--testdata-mode` or `TESTDATA_MODE=true`, the mock provider loads predefined request-response pairs from `testdata.json`. This provides:

- **Deterministic responses** for reliable testing
- **Network connectivity testing** without response variability
- **Error scenario testing** with predefined failure cases
- **Performance testing** with known response sizes
- **Integration testing** with consistent data

### Using Testdata Mode

```bash
# Enable testdata mode
cargo run -- --testdata-mode

# Or with Docker
docker run -v $(pwd)/testdata.json:/app/testdata.json -e TESTDATA_MODE=true -p 11434:11434 mock-inference-provider

# Test with predefined responses
curl http://localhost:11434/api/generate -d '{"model":"llama2","prompt":"Hello world","stream":false}'
# Returns the exact response defined in testdata.json
```

### Testdata Structure

The `testdata.json` file contains predefined test cases organized by API:

```json
{
  "ollama": {
    "generate": [
      {
        "request": {
          "method": "POST",
          "path": "/api/generate",
          "body": {"model": "llama2", "prompt": "Hello world"}
        },
        "response": {
          "status": 200,
          "body": {"model": "llama2", "response": "Test response..."}
        },
        "description": "Basic connectivity test"
      }
    ]
  }
}
```

## API Reference

| API | Endpoint | Method | Request Format | Response Format | Description |
|-----|----------|--------|----------------|-----------------|-------------|
| **System** | `/` | GET | - | JSON | API information and available endpoints |
| **System** | `/health` | GET | - | JSON | Health check status |
| **System** | `/metrics` | GET | - | JSON | Request metrics and statistics |
| **Ollama** | `/api/generate` | POST | `{"model": "string", "prompt": "string", "stream": boolean, "options": object}` | JSON/SSE | Text generation with optional streaming |
| **Ollama** | `/api/chat` | POST | `{"model": "string", "messages": [{"role": "string", "content": "string"}], "stream": boolean, "tools": [object]}` | JSON | Chat completion with tool support |
| **Ollama** | `/api/tags` | GET | - | JSON | List available models |
| **Ollama** | `/api/pull` | POST | `{"name": "string"}` | JSON | Pull/download a model |
| **Ollama** | `/api/show` | POST | `{"name": "string"}` | JSON | Show model information and configuration |
| **Ollama** | `/api/embeddings` | POST | `{"model": "string", "prompt": "string"}` or `{"model": "string", "prompt": ["string"]}` | JSON | Generate embeddings for text |
| **OpenAI** | `/v1/completions` | POST | `{"model": "string", "prompt": "string", "max_tokens": number, "temperature": number, "stream": boolean}` | JSON | Text completion |
| **OpenAI** | `/v1/chat/completions` | POST | `{"model": "string", "messages": [{"role": "string", "content": "string"}], "max_tokens": number, "temperature": number}` | JSON | Chat completion |
| **OpenAI** | `/v1/models` | GET | - | JSON | List available models |
| **OpenAI** | `/v1/embeddings` | POST | `{"model": "string", "input": "string"}` or `{"model": "string", "input": ["string"]}` | JSON | Generate embeddings |
| **TGI** | `/generate` | POST | `{"inputs": "string", "parameters": {"max_new_tokens": number, "temperature": number}}` | JSON | Text generation |
| **TGI** | `/generate_stream` | POST | `{"inputs": "string", "parameters": {"max_new_tokens": number}}` | SSE | Streaming text generation |
| **TGI** | `/info` | GET | - | JSON | Model and server information |
| **Agentic** | `/api/agentic/execute` | POST | `{"model": "string", "prompt": "string", "tools": [{"name": "string", "description": "string", "parameters": object}], "max_iterations": number}` | JSON | Execute agentic workflow with tools |
| **Agentic** | `/api/agentic/tool-calls` | GET | - | JSON | Get recorded tool call history |
| **API Keys** | `/api/keys/generate` | POST | `{"provider": "string", "valid": boolean, "expiry_seconds": number}` | JSON | Generate mock API key |
| **API Keys** | `/api/keys/validate` | POST | `{"api_key": "string"}` | JSON | Validate API key |
| **API Keys** | `/api/keys/list` | GET | - | JSON | List generated API keys |
| **API Keys** | `/api/keys/revoke` | POST | `{"api_key": "string"}` | JSON | Revoke/invalidate API key |

### Request/Response Details

#### Ollama Generate
```json
// Request
{
  "model": "llama2",
  "prompt": "Hello world",
  "stream": false,
  "options": {
    "temperature": 0.7,
    "num_predict": 100
  }
}

// Response
{
  "model": "llama2",
  "created_at": "2024-01-01T12:00:00Z",
  "response": "Generated text response...",
  "done": true,
  "context": [1, 2, 3],
  "total_duration": 150000000,
  "load_duration": 10000000,
  "prompt_eval_count": 2,
  "prompt_eval_duration": 50000000,
  "eval_count": 25,
  "eval_duration": 90000000
}
```

#### Ollama Chat
```json
// Request
{
  "model": "llama2",
  "messages": [
    {"role": "user", "content": "Hello!"}
  ],
  "stream": false,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "web_search",
        "description": "Search the web",
        "parameters": {
          "type": "object",
          "properties": {"query": {"type": "string"}},
          "required": ["query"]
        }
      }
    }
  ]
}

// Response
{
  "model": "llama2",
  "created_at": "2024-01-01T12:00:00Z",
  "message": {
    "role": "assistant",
    "content": "Response text...",
    "tool_calls": [
      {
        "id": "call_123",
        "type": "function",
        "function": {
          "name": "web_search",
          "arguments": "{\"query\": \"search term\"}"
        }
      }
    ]
  },
  "done": true,
  "total_duration": 180000000,
  "load_duration": 12000000,
  "prompt_eval_count": 6,
  "eval_count": 35
}
```

#### OpenAI Completions
```json
// Request
{
  "model": "text-davinci-003",
  "prompt": "What is 2+2?",
  "max_tokens": 50,
  "temperature": 0.7
}

// Response
{
  "id": "cmpl-test123",
  "object": "text_completion",
  "created": 1704110400,
  "model": "text-davinci-003",
  "choices": [
    {
      "text": "2 + 2 = 4",
      "index": 0,
      "logprobs": null,
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 4,
    "completion_tokens": 7,
    "total_tokens": 11
  }
}
```

#### OpenAI Chat Completions
```json
// Request
{
  "model": "gpt-3.5-turbo",
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hello!"}
  ],
  "max_tokens": 100,
  "temperature": 0.7
}

// Response
{
  "id": "chatcmpl-test123",
  "object": "chat.completion",
  "created": 1704110400,
  "model": "gpt-3.5-turbo",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 8,
    "total_tokens": 23
  }
}
```

#### TGI Generate
```json
// Request
{
  "inputs": "What is machine learning?",
  "parameters": {
    "max_new_tokens": 100,
    "temperature": 0.7,
    "do_sample": true
  }
}

// Response
{
  "generated_text": "Machine learning is a subset of artificial intelligence...",
  "details": {
    "finish_reason": "eos_token",
    "generated_tokens": 45,
    "seed": 42
  }
}
```

#### Agentic Execute
```json
// Request
{
  "model": "llama2",
  "prompt": "Search for Rust programming info",
  "tools": [
    {
      "name": "web_search",
      "description": "Search the web",
      "parameters": {
        "type": "object",
        "properties": {"query": {"type": "string"}},
        "required": ["query"]
      }
    }
  ],
  "max_iterations": 2
}

// Response
{
  "response": "I'll search for Rust programming information...",
  "tool_calls": [
    {
      "id": "call_123456789",
      "type": "function",
      "function": {
        "name": "web_search",
        "arguments": "{\"query\": \"Rust programming language\"}"
      }
    }
  ],
  "iterations": 1,
  "completed": true
}
```

#### API Key Management
```json
// Generate Request
{
  "provider": "openai",
  "valid": true,
  "expiry_seconds": 3600
}

// Generate Response
{
  "api_key": "sk-mock-openai-abcdef1234567890abcdef1234567890abcdef12",
  "provider": "openai",
  "expires_at": 1704114000,
  "valid": true
}

// Validate Request
{
  "api_key": "sk-mock-openai-abcdef1234567890abcdef1234567890abcdef12"
}

// Validate Response
{
  "valid": true,
  "provider": "openai",
  "expired": false,
  "message": "Key is valid"
}
```

## Quick Start

### Local Development

```bash
# Build and run
cargo run -- --port 11434

# Or with Docker
docker build -t mock-inference-provider .
docker run -p 11434:11434 mock-inference-provider
```

### Docker Compose

```bash
# Start all test instances
docker-compose up -d

# Test endpoints
curl http://localhost:11434/api/tags
curl http://localhost:11434/api/generate -d '{"model":"llama2","prompt":"Hello"}'
```

### Deploy to Akash

```bash
# Deploy using SDL template
akash tx deployment create deploy.sdl.yaml --from <wallet> --chain-id akashnet-2

# With custom variables
akash tx deployment create deploy.sdl.yaml \
  --from <wallet> \
  --set MODEL_NAME=mistral \
  --set MIN_LATENCY_MS=100 \
  --set MAX_LATENCY_MS=500
```

## API Examples

### Ollama Generate

```bash
curl http://localhost:11434/api/generate \
  -d '{
    "model": "llama2",
    "prompt": "Why is the sky blue?",
    "stream": false
  }'
```

### Ollama Chat

```bash
curl http://localhost:11434/api/chat \
  -d '{
    "model": "llama2",
    "messages": [
      {"role": "user", "content": "Hello!"}
    ]
  }'
```

### OpenAI Completions

```bash
curl http://localhost:11434/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama2",
    "prompt": "What is 2+2?",
    "max_tokens": 100
  }'
```

### OpenAI Chat

```bash
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama2",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Hello!"}
    ]
  }'
```

### TGI Generate

```bash
curl http://localhost:11434/generate \
  -d '{
    "inputs": "What is machine learning?",
    "parameters": {"max_new_tokens": 100}
  }'
```

### Agentic Execute

```bash
curl http://localhost:11434/api/agentic/execute \
  -d '{
    "model": "llama2",
    "prompt": "Search for information about Akash Network",
    "tools": [
      {
        "name": "web_search",
        "description": "Search the web",
        "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
      }
    ]
  }'
```

### Get Tool Calls (for testing verification)

```bash
curl http://localhost:11434/api/agentic/tool-calls
```

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `PORT` | 11434 | Port to listen on |
| `HOST` | 0.0.0.0 | Host to bind to |
| `MIN_LATENCY_MS` | 50 | Minimum simulated latency |
| `MAX_LATENCY_MS` | 200 | Maximum simulated latency |
| `ERROR_RATE` | 0.0 | Error rate (0.0 - 1.0) |
| `MODEL_NAME` | llama2 | Default model name to report |
| `TESTDATA_MODE` | false | Enable testdata mode for deterministic responses |
| `RUST_LOG` | info | Log level |

## CLI Arguments

```
mock-inference-provider --help

Options:
  -p, --port <PORT>                    Port to listen on [env: PORT=] [default: 11434]
      --host <HOST>                    Host to bind to [env: HOST=] [default: 0.0.0.0]
      --min-latency-ms <MIN_LATENCY_MS> Minimum simulated latency [env: MIN_LATENCY_MS=] [default: 50]
      --max-latency-ms <MAX_LATENCY_MS> Maximum simulated latency [env: MAX_LATENCY_MS=] [default: 200]
      --error-rate <ERROR_RATE>        Error rate (0.0 - 1.0) [env: ERROR_RATE=] [default: 0.0]
      --model-name <MODEL_NAME>        Model name to report [env: MODEL_NAME=] [default: llama2]
      --testdata-mode                  Enable testdata mode for deterministic responses [env: TESTDATA_MODE=]
  -v, --verbose                        Enable verbose logging [env: VERBOSE=]
  -h, --help                           Print help
```

## Testing Scenarios

### Standard Testing
Default configuration with low latency and no errors:
```bash
docker run -p 11434:11434 mock-inference-provider
```

### Slow Network Testing
High latency to test timeout handling:
```bash
docker run -p 11434:11434 \
  -e MIN_LATENCY_MS=2000 \
  -e MAX_LATENCY_MS=5000 \
  mock-inference-provider
```

### Unreliable Service Testing
30% error rate to test error handling:
```bash
docker run -p 11434:11434 \
  -e ERROR_RATE=0.3 \
  mock-inference-provider
```

## Health Check

```bash
curl http://localhost:11434/health
# {"status":"ok","timestamp":"2024-01-01T00:00:00Z"}
```

## Metrics

```bash
curl http://localhost:11434/metrics
# {"total_requests":42,"total_tool_calls":5,"models_loaded":4}
```

## Integration with ERGORS

This mock provider is designed to work with the ERGORS Akash deployment workflow:

1. Deploy mock provider to Akash using SDL template
2. Configure ERGORS workflow to use the mock endpoint
3. Run integration tests validating the full 16-step deployment workflow
4. Verify agentic tool calls and responses

See `packages/cw-ho/tests/src/akash_integration.rs` for test examples.
