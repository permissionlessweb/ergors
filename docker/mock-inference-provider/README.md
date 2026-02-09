# Mock Inference Provider

Simulates Ollama, OpenAI, Anthropic, and TGI APIs for testing Akash deployments without GPU resources.

All responses are **deterministic** — every valid request returns a fixed canonical response. Token counts are consistent: **10 input, 25 output** across all providers.

See **[CONTRACTS.md](CONTRACTS.md)** for exact request/response pairs per endpoint.

## Supported APIs

| Provider | Endpoints |
|----------|-----------|
| Ollama | `/api/generate`, `/api/chat`, `/api/tags`, `/api/pull`, `/api/show`, `/api/embeddings` |
| OpenAI | `/v1/completions`, `/v1/chat/completions`, `/v1/models`, `/v1/embeddings` |
| Anthropic | `/v1/messages` |
| TGI | `/generate`, `/generate_stream`, `/info` |
| API Keys | `/api/keys/generate`, `/api/keys/validate`, `/api/keys/list`, `/api/keys/revoke` |
| System | `/health`, `/metrics`, `/` |

## API Reference

| Provider | Method | Endpoint | Request JSON | Response JSON |
|----------|--------|----------|-------------|---------------|
| Anthropic | POST | `/v1/messages` | `{"model":"claude-3-haiku-20240307","messages":[{"role":"user","content":"Hello"}],"max_tokens":1024}` | `{"id":"msg-mock-001","type":"message","role":"assistant","content":[{"type":"text","text":"This is a mock response from the Anthropic Messages API."}],"model":"...","stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":25}}` |
| OpenAI | POST | `/v1/chat/completions` | `{"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"Hello"}]}` | `{"id":"chatcmpl-mock-001","object":"chat.completion","created":1700000000,"model":"...","choices":[{"index":0,"message":{"role":"assistant","content":"This is a mock response from the OpenAI Chat API."},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":25,"total_tokens":35}}` |
| OpenAI | POST | `/v1/completions` | `{"model":"text-davinci-003","prompt":"Hello"}` | `{"id":"cmpl-mock-001","object":"text_completion","created":1700000000,"model":"...","choices":[{"text":"This is a mock response from the OpenAI Completions API.","index":0,"logprobs":null,"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":25,"total_tokens":35}}` |
| OpenAI | GET | `/v1/models` | — | `{"object":"list","data":[{"id":"llama2:latest","object":"model","created":1700000000,"owned_by":"organization"},...]}`|
| OpenAI | POST | `/v1/embeddings` | `{"model":"text-embedding-ada-002","input":"Hello"}` | `{"object":"list","data":[{"object":"embedding","embedding":[0.0,...],"index":0}],"model":"...","usage":{"prompt_tokens":10,"total_tokens":10}}` |
| Ollama | POST | `/api/generate` | `{"model":"llama2","prompt":"Hello","stream":false}` | `{"model":"llama2","created_at":"2024-01-01T00:00:00Z","response":"This is a mock response from the Ollama Generate API.","done":true,"context":[1,2,3],"total_duration":150000000,"load_duration":10000000,"prompt_eval_count":10,"prompt_eval_duration":50000000,"eval_count":25,"eval_duration":90000000}` |
| Ollama | POST | `/api/chat` | `{"model":"llama2","messages":[{"role":"user","content":"Hello"}],"stream":false}` | `{"model":"llama2","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"This is a mock response from the Ollama Chat API."},"done":true,"total_duration":150000000,"load_duration":10000000,"prompt_eval_count":10,"eval_count":25}` |
| Ollama | GET | `/api/tags` | — | `{"models":[{"name":"llama2","model":"llama2:latest","size":4000000000,...},...]}`|
| Ollama | POST | `/api/pull` | `{"name":"llama2"}` | `{"status":"success","digest":"sha256:..."}` |
| Ollama | POST | `/api/show` | `{"name":"llama2"}` | `{"modelfile":"FROM llama2","parameters":"...","details":{...},"model_info":{...}}` |
| Ollama | POST | `/api/embeddings` | `{"model":"llama2","prompt":"Hello"}` | `{"model":"llama2","embeddings":[[0.0,...]]}` |
| TGI | POST | `/generate` | `{"inputs":"Hello","parameters":{"max_new_tokens":100}}` | `{"generated_text":"This is a mock response from the TGI API.","details":{"finish_reason":"eos_token","generated_tokens":25,"seed":42}}` |
| TGI | POST | `/generate_stream` | `{"inputs":"Hello","parameters":{"max_new_tokens":100}}` | SSE stream of token events |
| TGI | GET | `/info` | — | `{"model_id":"mock-model","model_dtype":"float16","max_input_length":4096,"max_total_tokens":8192,"version":"1.4.0",...}` |
| API Keys | POST | `/api/keys/generate` | `{"provider":"openai","valid":true,"expiry_seconds":3600}` | `{"api_key":"sk-mock-openai-000000000000000000000000","provider":"openai","expires_at":1700003600,"valid":true}` |
| API Keys | POST | `/api/keys/validate` | `{"api_key":"sk-mock-openai-000000000000000000000000"}` | `{"valid":true,"provider":"openai","expired":false,"message":"Key is valid"}` |
| API Keys | GET | `/api/keys/list` | — | `{"keys":[{"key":"...","provider":"openai","valid":true,"expired":false,...}],"total":1}` |
| API Keys | POST | `/api/keys/revoke` | `{"api_key":"sk-mock-openai-000000000000000000000000"}` | `{"success":true,"message":"Key revoked successfully"}` |
| System | GET | `/health` | — | `{"status":"ok"}` |
| System | GET | `/metrics` | — | `{"total_requests":0,"models_loaded":4}` |
| System | GET | `/` | — | `{"name":"Mock Inference Provider","version":"...","endpoints":{...}}` |

**Note**: Anthropic `/v1/messages` requires `x-api-key` header. Missing/empty key returns 401.

## Quick Start

```bash
# Local
cargo run -- --port 11434

# Docker
docker build -t mock-inference-provider .
docker run -p 11434:11434 mock-inference-provider
```

## Configuration

| Env / Flag | Default | Description |
|-----------|---------|-------------|
| `PORT` / `-p` | 11434 | Listen port |
| `HOST` | 0.0.0.0 | Bind address |
| `MIN_LATENCY_MS` | 50 | Min simulated latency (ms) |
| `MAX_LATENCY_MS` | 200 | Max simulated latency (ms) |
| `ERROR_RATE` | 0.0 | Probability of 500 error (0.0–1.0) |
| `MODEL_NAME` | llama2 | Default model name |
| `VERBOSE` / `-v` | false | Debug logging |

## Testing Scenarios

```bash
# Zero latency for fast tests
cargo run -- --min-latency-ms 0 --max-latency-ms 0

# 30% error rate for resilience testing
cargo run -- --error-rate 0.3

# High latency for timeout testing
cargo run -- --min-latency-ms 2000 --max-latency-ms 5000
```
