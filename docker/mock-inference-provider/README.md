# Mock Inference Provider

A standalone service that simulates inference provider APIs (Ollama, OpenAI, TGI) for testing Akash deployments without requiring GPU resources.

## Features

- **Ollama API**: `/api/generate`, `/api/chat`, `/api/tags`, `/api/pull`, `/api/show`, `/api/embeddings`
- **OpenAI API**: `/v1/completions`, `/v1/chat/completions`, `/v1/models`, `/v1/embeddings`
- **TGI API**: `/generate`, `/generate_stream`, `/info`
- **Agentic Endpoints**: `/api/agentic/execute`, `/api/agentic/tool-calls`
- Configurable latency simulation
- Configurable error rates
- Streaming response support
- Tool call simulation for agentic testing

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
