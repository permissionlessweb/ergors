
## Quick Start

1. **Initialize Configuration**:

   ```bash
   cargo run --bin cw-ho -- init
   ```

2. **Configure API Keys**:
   Edit `api-keys.json` with your LLM provider keys:

   ```json
   {
     "openai": "sk-...",
     "anthropic": "sk-ant-...",
     "grok": "xai-..."
   }
   ```

3. **Start the Service**:

   ```bash
   cargo run --bin cw-ho -- start
   ```

## API Endpoints

### POST /api/prompt

Submit a prompt for processing:

```json
{
  "prompt": "What is the capital of France?",
  "context": {
    "session_id": "user-123",
    "user_id": "john",
    "metadata": {}
  },
  "model": "gpt-3.5-turbo",
  "temperature": 0.7,
  "max_tokens": 1000
}
```

Response:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "prompt": "What is the capital of France?",
  "response": "The capital of France is Paris.",
  "model": "gpt-3.5-turbo",
  "timestamp": "2024-01-01T12:00:00Z",
  "tokens_used": {
    "prompt": 10,
    "completion": 8,
    "total": 18
  },
  "request_duration_ms": 1250
}
```

### GET /api/prompts

Query historical prompts:

```bash
curl "http://localhost:8080/api/prompts?session_id=user-123&limit=10"
```

### GET /health

Service health check:

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "storage_status": "healthy"
}
```
