# Open Responses API

ERGORS implements the [Open Responses](https://www.openresponses.org) specification, providing a standardized multi-provider LLM interface with unified streaming, tool calling, and item-based responses.

## Routing

ERGORS exposes Open Responses compatibility through two routes:

| Endpoint | Description |
|----------|-------------|
| `POST /v1/responses` | Dedicated Open Responses endpoint. Always returns Open Responses format. |
| `POST /api/prompt` | Existing ERGORS endpoint. Returns Open Responses format when `response_format: "open_responses"` is set. |

Both endpoints accept the same request body and produce identical response formats.

---

## Request Format

### Create Response

```
POST /v1/responses
Content-Type: application/json
```

#### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `model` | string | Model identifier (e.g. `"claude-3-5-sonnet-20241022"`, `"gpt-4o"`) |
| `input` | array | Array of input items (messages) |

#### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `stream` | boolean | `false` | Enable streaming via SSE |
| `instructions` | string | `""` | System prompt / instructions |
| `tools` | array | `[]` | Tool definitions available to the model |
| `tool_choice` | string or object | `"auto"` | Controls tool invocation behavior |
| `allowed_tools` | array of strings | `[]` | Restricts which tools may be called (cache-preserving) |
| `previous_response_id` | string | `null` | Resume conversation from a prior response |
| `truncation` | string | `null` | `"auto"` or `"disabled"` |
| `service_tier` | string | `null` | `"standard"`, `"priority"`, or `"batch"` |

---

## Input Items

Input items form the conversation context sent to the model. Each item has a `role` and `content`.

### Message Item (Text)

```json
{
  "role": "user",
  "content": [
    {
      "type": "input_text",
      "text": "What is the weather in San Francisco?"
    }
  ]
}
```

Content can also be passed as a plain string:

```json
{
  "role": "user",
  "content": "What is the weather in San Francisco?"
}
```

### Roles

| Role | Description |
|------|-------------|
| `user` | User-provided input |
| `assistant` | Model-generated output (for multi-turn) |
| `system` | System-level instructions (alternative to `instructions` field) |

### Multi-turn Conversation

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "input": [
    {
      "role": "user",
      "content": [{"type": "input_text", "text": "My name is Alice."}]
    },
    {
      "role": "assistant",
      "content": [{"type": "output_text", "text": "Hello Alice! How can I help you?"}]
    },
    {
      "role": "user",
      "content": [{"type": "input_text", "text": "What's my name?"}]
    }
  ]
}
```

---

## Response Format

### Non-Streaming Response

```json
{
  "id": "resp_abc123",
  "object": "response",
  "status": "completed",
  "model": "claude-3-5-sonnet-20241022",
  "output": [
    {
      "id": "msg_xyz789",
      "type": "message",
      "status": "completed",
      "role": "assistant",
      "content": [
        {
          "type": "output_text",
          "text": "Hello! How can I help you today?"
        }
      ]
    }
  ],
  "usage": {
    "input_tokens": 12,
    "output_tokens": 9,
    "total_tokens": 21
  }
}
```

### Output Item Types

| Type | Description |
|------|-------------|
| `message` | Model-generated text response |
| `function_call` | Model requesting a tool invocation |
| `function_call_output` | Result of a tool invocation |

---

## Streaming

Set `"stream": true` to receive Server-Sent Events. The response uses `Content-Type: text/event-stream`.

### Event Types

| Event | Description |
|-------|-------------|
| `response.in_progress` | Response generation has started |
| `response.output_item.added` | A new output item was added |
| `response.content_part.added` | A content part was added to an item |
| `response.output_text.delta` | Incremental text token |
| `response.output_text.done` | Text content part is complete |
| `response.content_part.done` | Content part finalized |
| `response.output_item.done` | Output item finalized |
| `response.completed` | Response generation finished |
| `response.failed` | Response generation encountered an error |

### Event Format

Each SSE event follows this structure:

```
event: response.output_text.delta
data: {"type":"response.output_text.delta","sequence_number":5,"output_index":0,"content_index":0,"delta":"Hello"}
```

### Streaming Lifecycle

A typical streaming session emits events in this order:

```
response.in_progress
  response.output_item.added        (message item)
    response.content_part.added     (output_text part)
      response.output_text.delta    (repeated for each token)
      ...
    response.output_text.done
    response.content_part.done
  response.output_item.done
response.completed
```

The terminal event is the literal string `[DONE]`.

---

## Tools & Function Calling

### Defining Tools

```json
{
  "model": "gpt-4o",
  "input": [
    {"role": "user", "content": "What's the weather in NYC?"}
  ],
  "tools": [
    {
      "type": "function",
      "name": "get_weather",
      "description": "Get the current weather for a location",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {"type": "string", "description": "City name"}
        },
        "required": ["location"]
      }
    }
  ]
}
```

### Function Call Response

When the model invokes a tool, the response contains a `function_call` item:

```json
{
  "id": "resp_abc123",
  "status": "completed",
  "output": [
    {
      "id": "fc_xyz789",
      "type": "function_call",
      "status": "completed",
      "name": "get_weather",
      "call_id": "call_001",
      "arguments": "{\"location\":\"New York City\"}"
    }
  ]
}
```

### Returning Tool Results

To continue the conversation after a tool call, include the function call output in the next request's input:

```json
{
  "model": "gpt-4o",
  "input": [
    {"role": "user", "content": "What's the weather in NYC?"},
    {
      "type": "function_call",
      "name": "get_weather",
      "call_id": "call_001",
      "arguments": "{\"location\":\"New York City\"}"
    },
    {
      "type": "function_call_output",
      "call_id": "call_001",
      "output": "{\"temperature\": 72, \"condition\": \"sunny\"}"
    }
  ]
}
```

### tool_choice

Controls whether the model should call tools:

| Value | Behavior |
|-------|----------|
| `"auto"` | Model decides whether to use tools (default) |
| `"required"` | Model must call at least one tool |
| `"none"` | Model must not call any tools |
| `{"type": "function", "name": "fn_name"}` | Force a specific tool call |

### allowed_tools

Restricts which tools the model may invoke without changing the `tools` list (preserving cache):

```json
{
  "tools": [
    {"type": "function", "name": "get_weather", "...": "..."},
    {"type": "function", "name": "send_email", "...": "..."}
  ],
  "allowed_tools": ["get_weather"]
}
```

The model sees both tools in context but can only invoke `get_weather`.

---

## Conversation Continuation

Use `previous_response_id` to resume a conversation without resending the full transcript.

### Step 1: Initial Request

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "input": [
    {"role": "user", "content": "My favorite color is blue."}
  ]
}
```

Response:

```json
{
  "id": "resp_first123",
  "status": "completed",
  "output": [{"type": "message", "...": "..."}]
}
```

### Step 2: Follow-up

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "previous_response_id": "resp_first123",
  "input": [
    {"role": "user", "content": "What's my favorite color?"}
  ]
}
```

The server loads the previous request + response context and concatenates it with the new input:

```
previous_response.input + previous_response.output + new input
```

---

## Using via /api/prompt

The existing `/api/prompt` endpoint can return Open Responses format by setting `response_format`:

```json
{
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "model": "claude-3-5-sonnet-20241022",
  "response_format": "open_responses"
}
```

The response will use the same Open Responses JSON structure as `/v1/responses`.

---

## Error Handling

Errors follow the Open Responses error schema:

```json
{
  "error": {
    "type": "invalid_request_error",
    "message": "The 'model' field is required.",
    "param": "model",
    "code": null
  }
}
```

### Error Types

| Type | Status Code | Description |
|------|-------------|-------------|
| `invalid_request_error` | 400 | Request is malformed or missing required fields |
| `not_found_error` | 404 | Requested resource does not exist |
| `model_error` | 500 | Model failed while processing a valid request |
| `too_many_requests` | 429 | Rate limit exceeded |
| `server_error` | 500 | Internal server error |

### Streaming Errors

When an error occurs during streaming, it is emitted as an SSE event followed by `response.failed`:

```
event: error
data: {"error":{"type":"model_error","message":"Model failed to generate response","param":null,"code":null}}
```

---

## Examples

### cURL: Non-Streaming

```bash
curl -X POST http://localhost:8080/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "input": [
      {
        "role": "user",
        "content": [{"type": "input_text", "text": "Hello, how are you?"}]
      }
    ]
  }'
```

### cURL: Streaming

```bash
curl -X POST http://localhost:8080/v1/responses \
  -H "Content-Type: application/json" \
  -N \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "input": [
      {
        "role": "user",
        "content": [{"type": "input_text", "text": "Count from 1 to 5"}]
      }
    ],
    "stream": true
  }'
```

### cURL: With Tools

```bash
curl -X POST http://localhost:8080/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "input": [
      {"role": "user", "content": "What is 25 * 4?"}
    ],
    "tools": [
      {
        "type": "function",
        "name": "calculator",
        "description": "Perform arithmetic calculations",
        "parameters": {
          "type": "object",
          "properties": {
            "expression": {"type": "string"}
          },
          "required": ["expression"]
        }
      }
    ]
  }'
```

### cURL: Conversation Continuation

```bash
# First request
RESP_ID=$(curl -s -X POST http://localhost:8080/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "input": [{"role": "user", "content": "Remember: the secret word is banana."}]
  }' | jq -r '.id')

# Follow-up using previous_response_id
curl -X POST http://localhost:8080/v1/responses \
  -H "Content-Type: application/json" \
  -d "{
    \"model\": \"claude-3-5-sonnet-20241022\",
    \"previous_response_id\": \"$RESP_ID\",
    \"input\": [{\"role\": \"user\", \"content\": \"What is the secret word?\"}]
  }"
```

### Python

```python
import requests

# Non-streaming
response = requests.post("http://localhost:8080/v1/responses", json={
    "model": "claude-3-5-sonnet-20241022",
    "input": [
        {
            "role": "user",
            "content": [{"type": "input_text", "text": "Explain quantum computing briefly."}]
        }
    ]
})

result = response.json()
for item in result["output"]:
    if item["type"] == "message":
        for part in item["content"]:
            print(part["text"])
```

```python
import requests

# Streaming
response = requests.post("http://localhost:8080/v1/responses", json={
    "model": "claude-3-5-sonnet-20241022",
    "input": [{"role": "user", "content": "Write a haiku about rust."}],
    "stream": True
}, stream=True)

for line in response.iter_lines():
    if line:
        decoded = line.decode("utf-8")
        if decoded.startswith("data: ") and decoded != "data: [DONE]":
            import json
            event = json.loads(decoded[6:])
            if event.get("type") == "response.output_text.delta":
                print(event["delta"], end="", flush=True)
print()
```

### JavaScript

```javascript
// Non-streaming
const response = await fetch('http://localhost:8080/v1/responses', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    model: 'claude-3-5-sonnet-20241022',
    input: [
      { role: 'user', content: [{ type: 'input_text', text: 'Hello!' }] }
    ]
  })
});

const result = await response.json();
console.log(result.output);
```

```javascript
// Streaming with EventSource-like parsing
const response = await fetch('http://localhost:8080/v1/responses', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    model: 'claude-3-5-sonnet-20241022',
    input: [{ role: 'user', content: 'Tell me a joke.' }],
    stream: true
  })
});

const reader = response.body.getReader();
const decoder = new TextDecoder();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;

  const chunk = decoder.decode(value, { stream: true });
  for (const line of chunk.split('\n')) {
    if (line.startsWith('data: ') && line !== 'data: [DONE]') {
      const event = JSON.parse(line.slice(6));
      if (event.type === 'response.output_text.delta') {
        process.stdout.write(event.delta);
      }
    }
  }
}
```

---

## Provider Routing

ERGORS routes requests to the appropriate LLM provider based on the `model` field:

| Model Pattern | Provider |
|---------------|----------|
| `claude-*`, `anthropic/*` | Anthropic |
| `gpt-*`, `o1-*`, `o3-*` | OpenAI |
| Other | Configured default provider |

The Open Responses layer transparently handles provider-specific protocol translation (Anthropic Messages API, OpenAI Chat Completions) and normalizes responses into the unified Open Responses format.

---

## Comparison: ERGORS vs Open Responses Format

| Feature | ERGORS (`/api/prompt`) | Open Responses (`/v1/responses`) |
|---------|------------------------|----------------------------------|
| Input field | `messages` | `input` |
| System prompt | `system` | `instructions` |
| Response body | `PromptResponse` proto | Open Responses JSON |
| Streaming events | Provider-native SSE | Semantic Open Responses events |
| Tool calls | `tool_calls` array | `function_call` items |
| Conversation state | Managed externally | `previous_response_id` |

Both formats are fully supported. Use `/v1/responses` for spec-compliant Open Responses clients, or set `response_format: "open_responses"` on `/api/prompt` for gradual migration.
