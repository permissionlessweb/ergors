# Inference Provider Routing E2E Tests

This document describes the E2E tests for ERGORS inference provider routing functionality.

## Overview

These tests validate the complete workflow of deploying mock inference providers, generating API keys, configuring ERGORS proxy routing, and making inference requests through ERGORS to verify proper API key management and request routing.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      E2E Test Flow                           │
│                                                              │
│  1. Deploy Mock Provider                                     │
│     └─> ghcr.io/permissionlessweb/mock-inference-provider  │
│         (Ollama/OpenAI/TGI compatible APIs)                 │
│                                                              │
│  2. Generate API Keys                                        │
│     └─> POST /api/keys/generate                            │
│         {"provider": "openai", "valid": true}               │
│                                                              │
│  3. Configure ERGORS Proxy                                   │
│     └─> Store key in proxy router config                   │
│         model_routes: {"llama*": "http://localhost:11434"}  │
│         api_keys: {"http://localhost:11434": "sk-mock-..."} │
│                                                              │
│  4. Make Inference Requests                                  │
│     └─> Request → ERGORS Proxy → Mock Provider             │
│         Verify deterministic responses from testdata.json   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Mock Inference Provider

### Docker Image
`ghcr.io/permissionlessweb/mock-inference-provider:latest`

### Features
- **Ollama API**: `/api/generate`, `/api/chat`, `/api/tags`, `/api/embeddings`
- **OpenAI API**: `/v1/completions`, `/v1/chat/completions`, `/v1/models`
- **TGI API**: `/generate`, `/generate_stream`, `/info`
- **API Key Management**: Full CRUD operations for testing key workflows
- **Testdata Mode**: Deterministic responses for reliable testing

### Testdata Mode
When `TESTDATA_MODE=true`, the provider returns predefined responses from `testdata.json`:

| Prompt | Model | Expected Response |
|--------|-------|-------------------|
| `"Hello world"` | `llama2` | `"Hello! I'm a mock inference provider..."` |
| `"What is 2+2?"` | `mistral` | `"2 + 2 = 4. This is a simple arithmetic calculation."` |
| `"Hello, how are you?"` (chat) | `llama2` | `"Hello! I'm doing well..."` |

This ensures test determinism and eliminates variability from dynamic response generation.

## API Key Management Tests

### Generate Key
```bash
curl -s http://localhost:11434/api/keys/generate \
  -H "Content-Type: application/json" \
  -d '{"provider": "openai", "valid": true}'

# Response:
{
  "api_key": "sk-mock-openai-abcdef1234567890abcdef12",
  "provider": "openai",
  "expires_at": null,
  "valid": true
}
```

### Validate Key
```bash
curl -s http://localhost:11434/api/keys/validate \
  -H "Content-Type: application/json" \
  -d '{"api_key": "sk-mock-openai-abcdef1234567890abcdef12"}'

# Response:
{
  "valid": true,
  "provider": "openai",
  "expired": false,
  "message": "Key is valid"
}
```

### List Keys
```bash
curl -s http://localhost:11434/api/keys/list | jq '.'

# Response:
{
  "keys": [
    {
      "key": "sk-mock-ope...f12",
      "provider": "openai",
      "created_at": 1704110400,
      "valid": true,
      "expired": false,
      "usage_count": 0
    }
  ],
  "total": 1
}
```

### Revoke Key
```bash
curl -s http://localhost:11434/api/keys/revoke \
  -H "Content-Type: application/json" \
  -d '{"api_key": "sk-mock-openai-abcdef1234567890abcdef12"}'

# Response:
{
  "success": true,
  "message": "Key revoked successfully"
}
```

## Test Cases

### 1. Provider Deployment
- **Test**: `deploy_mock_provider`
- **Verifies**: Docker container starts successfully
- **Success**: Provider responds to health check with `{"status": "ok"}`

### 2. API Key Generation
- **Test**: `generate_api_key`
- **Verifies**: Keys can be generated with proper format
- **Success**: Key matches pattern `^sk-mock-{provider}-[a-f0-9]{24}$`

### 3. API Key Validation
- **Test**: `validate_api_key`
- **Verifies**: Generated keys are recognized as valid
- **Success**: Validation returns `{"valid": true}`

### 4. Deterministic Responses
- **Test**: `deterministic_hello_world`
- **Prompt**: `"Hello world"` (model: `llama2`)
- **Expected**: Response contains `"mock inference provider"`

- **Test**: `deterministic_math`
- **Prompt**: `"What is 2+2?"` (model: `mistral`)
- **Expected**: Response contains `"2 + 2 = 4"`

- **Test**: `deterministic_chat`
- **Prompt**: `"Hello, how are you?"` (chat endpoint)
- **Expected**: Response contains `"doing well"`

### 5. Multi-API Support
- **Test**: `openai_chat`
- **Endpoint**: `/v1/chat/completions`
- **Verifies**: OpenAI-compatible API works correctly
- **Success**: Response matches OpenAI format with deterministic content

### 6. Tool Calling (Agentic)
- **Test**: `agentic_tool_call`
- **Prompt**: `"Search for information about Rust programming language"`
- **Tools**: `web_search` function
- **Expected**: Tool call with `function.name == "web_search"`

### 7. Error Handling
- **Test**: `invalid_model_error`
- **Request**: Model name `"invalid-model"`
- **Expected**: 404 response with error message containing `"not found"`

## Running the Tests

### Run All Tests
```bash
cd e2e-improvements
just e2e --test all
```

### Run Only Inference Tests
```bash
just e2e --test inference
```

### Run With Verbose Output
```bash
just e2e --test inference --verbose
```

### Skip Other Infrastructure
```bash
just e2e --test inference --skip-akash --skip-ethereum
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MOCK_PROVIDER_PORT` | `11434` | Port for mock provider |
| `MOCK_PROVIDER_HOST` | `127.0.0.1` | Host for mock provider |
| `TESTDATA_MODE` | `true` | Enable deterministic responses |
| `MIN_LATENCY_MS` | `0` | Minimum simulated latency |
| `MAX_LATENCY_MS` | `50` | Maximum simulated latency |

## TODO: ERGORS Proxy Integration

The following functionality is **planned but not yet implemented** in the E2E tests:

1. **Configure ERGORS Proxy via gRPC**
   - Endpoint: `/v1/proxy/config` (to be implemented)
   - Action: Store API keys and routing rules in running ERGORS nodes

2. **Route Requests Through ERGORS**
   - Endpoint: `/proxy/ollama/api/generate` (to be verified)
   - Action: Send inference requests through ERGORS proxy
   - Verify: Request is properly routed to mock provider with correct API key

3. **Verify Proxy Capture**
   - Action: Check that ERGORS captures request/response for observability
   - Endpoint: Query proxy session history (to be implemented)

Once the ERGORS proxy gRPC endpoints are available, uncomment the TODO sections in `tests/e2e/tests/inference.sh` to enable full end-to-end proxy routing tests.

## Architecture Notes

### Why Docker Instead of Akash Deployment?

For E2E tests, we use Docker locally instead of deploying to Akash because:
- **Speed**: Docker starts in <5 seconds vs Akash deployment ~2-5 minutes
- **Reliability**: No dependency on provider availability or bidding
- **Determinism**: Consistent test environment
- **Cost**: No AKT costs during development/testing

The mock provider is **designed** for Akash deployment in production workflows, but E2E tests prioritize speed and reliability.

### Testdata.json Strategy

The `testdata.json` file contains ~50 predefined request-response pairs organized by API:
- Ollama (generate, chat, tags, embeddings)
- OpenAI (completions, chat, models, embeddings)
- TGI (generate, info)
- Agentic (execute, tool-calls)
- API Keys (generate, validate, list, revoke)
- System (health, metrics, root)

This approach provides:
- **100% deterministic** responses for testing
- **Network connectivity** verification without LLM variability
- **Error scenario** testing with predefined failures
- **Performance** testing with known response sizes

## Troubleshooting

### Mock Provider Won't Start
```bash
# Check if port is already in use
lsof -ti :11434

# View container logs
docker logs ergors-e2e-mock-provider

# Pull latest image manually
docker pull ghcr.io/permissionlessweb/mock-inference-provider:latest
```

### Tests Fail With "Connection Refused"
```bash
# Verify provider is running
curl http://localhost:11434/health

# Check Docker container status
docker ps | grep mock-provider

# Restart with verbose logging
docker run -d -p 11434:11434 \
  -e TESTDATA_MODE=true \
  -e RUST_LOG=debug \
  ghcr.io/permissionlessweb/mock-inference-provider:latest
```

### Non-Deterministic Responses
```bash
# Ensure testdata mode is enabled
docker inspect ergors-e2e-mock-provider | jq '.[0].Config.Env' | grep TESTDATA_MODE

# Should show: "TESTDATA_MODE=true"
```

## References

- Mock Provider Source: `docker/mock-inference-provider/`
- Mock Provider README: `docker/mock-inference-provider/README.md`
- Testdata JSON: `docker/mock-inference-provider/testdata.json`
- ERGORS Proxy: `packages/ergors/src/proxy/`
- E2E Test Script: `tests/e2e/tests/inference.sh`
