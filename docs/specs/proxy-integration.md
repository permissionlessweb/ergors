# LLM Proxy Integration Specification

This document specifies how to configure CLI tools (opencode, Claude Code) to route through the ERGORS proxy engine for prompt/response capture and retention.

## Architecture Overview

```
┌─────────────────┐     ┌─────────────────────────┐     ┌─────────────────┐
│   CLI Tool      │     │    ERGORS Proxy         │     │  Upstream API   │
│  (opencode,     │────▶│  localhost:8080         │────▶│  (Anthropic,    │
│   Claude Code)  │     │                         │     │   OpenAI)       │
│                 │◀────│  - Capture prompts      │◀────│                 │
└─────────────────┘     │  - Store sessions       │     └─────────────────┘
                        │  - SSE passthrough      │
                        └─────────────────────────┘
```

## Prerequisites

1. ERGORS server running with proxy endpoints enabled
2. API keys configured (either in ERGORS or passed through from CLI tools)

### Starting ERGORS Server

```bash
# Start ERGORS with default configuration
cd /path/to/CW-AGENT
cargo run --package ergors

# Server will listen on configured address (default: 0.0.0.0:8080)
```

---

## OpenCode Configuration (Priority)

OpenCode is an open-source Go-based AI coding assistant that supports extensive configuration options.

### Method 1: Environment Variables (Recommended)

Set environment variables before launching opencode:

```bash
# For Anthropic models (Claude)
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_API_KEY="your-anthropic-key"

# For OpenAI models
export OPENAI_BASE_URL="http://localhost:8080"
export OPENAI_API_KEY="your-openai-key"

# Launch opencode
opencode
```

### Method 2: Project Configuration (`opencode.json`)

Create an `opencode.json` in your project root:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "anthropic": {
      "options": {
        "baseURL": "http://localhost:8080",
        "apiKey": "{env:ANTHROPIC_API_KEY}",
        "timeout": 300000
      }
    },
    "openai": {
      "options": {
        "baseURL": "http://localhost:8080",
        "apiKey": "{env:OPENAI_API_KEY}",
        "timeout": 300000
      }
    }
  },
  "model": "anthropic/claude-sonnet-4-5",
  "small_model": "anthropic/claude-haiku-4-5"
}
```

### Method 3: Global Configuration

Create or edit `~/.config/opencode/opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "ergors-anthropic": {
      "npm": "@ai-sdk/anthropic",
      "name": "ERGORS Anthropic Proxy",
      "options": {
        "baseURL": "http://localhost:8080",
        "apiKey": "{env:ANTHROPIC_API_KEY}"
      },
      "models": {
        "claude-sonnet-4-5": {
          "name": "Claude 4.5 Sonnet (via ERGORS)",
          "limit": {
            "context": 200000,
            "output": 64000
          }
        },
        "claude-opus-4-5": {
          "name": "Claude Opus 4.5 (via ERGORS)",
          "limit": {
            "context": 200000,
            "output": 32000
          }
        }
      }
    },
    "ergors-openai": {
      "npm": "@ai-sdk/openai",
      "name": "ERGORS OpenAI Proxy",
      "options": {
        "baseURL": "http://localhost:8080",
        "apiKey": "{env:OPENAI_API_KEY}"
      },
      "models": {
        "gpt-4o": {
          "name": "GPT-4o (via ERGORS)",
          "limit": {
            "context": 128000,
            "output": 16384
          }
        }
      }
    }
  },
  "model": "ergors-anthropic/claude-sonnet-4-5"
}
```

### Method 4: Custom Provider via `/connect`

1. Run opencode and use `/connect`
2. Select "Other" provider
3. Enter provider ID: `ergors`
4. Enter API key (will be passed through to upstream)
5. Configure in `opencode.json` as shown above

### OpenCode Shell Script Wrapper

Create a wrapper script `~/bin/opencode-ergors`:

```bash
#!/bin/bash
# opencode-ergors - Launch opencode through ERGORS proxy

# Proxy configuration
export ANTHROPIC_BASE_URL="${ERGORS_URL:-http://localhost:8080}"
export OPENAI_BASE_URL="${ERGORS_URL:-http://localhost:8080}"

# Ensure API keys are set
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo "Warning: ANTHROPIC_API_KEY not set"
fi

if [ -z "$OPENAI_API_KEY" ]; then
    echo "Warning: OPENAI_API_KEY not set"
fi

# Launch opencode with all arguments passed through
exec opencode "$@"
```

Make executable: `chmod +x ~/bin/opencode-ergors`

---

## Claude Code Configuration

Claude Code supports custom API endpoints through environment variables and settings.

### Method 1: Environment Variables

```bash
# Set proxy endpoint
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_API_KEY="your-anthropic-key"

# Launch Claude Code
claude
```

### Method 2: Settings File (`~/.claude/settings.json`)

Edit or create `~/.claude/settings.json`:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:8080",
    "ANTHROPIC_API_KEY": "your-anthropic-key"
  }
}
```

### Method 3: Per-Session with `--settings` Flag

```bash
claude --settings '{"env":{"ANTHROPIC_BASE_URL":"http://localhost:8080"}}'
```

### Claude Code Shell Wrapper

Create `~/bin/claude-ergors`:

```bash
#!/bin/bash
# claude-ergors - Launch Claude Code through ERGORS proxy

export ANTHROPIC_BASE_URL="${ERGORS_URL:-http://localhost:8080}"

# API key can be passed through (ERGORS will forward it)
# or ERGORS can use its own configured key

exec claude "$@"
```

---

## API Key Handling

The ERGORS proxy supports two authentication modes:

### 1. Passthrough Mode (Default)

API keys from CLI tool requests are forwarded to upstream providers:

```
CLI Tool → (x-api-key: sk-xxx) → ERGORS → (x-api-key: sk-xxx) → Anthropic
```

Configuration: No additional setup needed. CLI tools send their API keys.

### 2. Configured Mode

ERGORS uses its own configured API keys, ignoring client-provided keys:

```
CLI Tool → (any key) → ERGORS → (configured key) → Anthropic
```

Set in ERGORS environment:
```bash
export ANTHROPIC_API_KEY="your-ergors-managed-key"
export OPENAI_API_KEY="your-ergors-managed-key"
```

CLI tools can use placeholder keys when ERGORS manages authentication.

---

## Proxy Endpoints

| Endpoint | Format | Used By |
|----------|--------|---------|
| `POST /v1/messages` | Anthropic Messages API | Claude Code, opencode (Anthropic models) |
| `POST /v1/chat/completions` | OpenAI Chat API | opencode (OpenAI models) |
| `GET /api/proxy/sessions` | Query captured sessions | Admin/debugging |
| `GET /api/proxy/sessions/:id` | Get specific session | Admin/debugging |

---

## Session Capture & Querying

### Viewing Captured Sessions

```bash
# Query all sessions
curl http://localhost:8080/api/proxy/sessions

# Query with filters
curl "http://localhost:8080/api/proxy/sessions?client_type=1&limit=10"

# Get specific session
curl http://localhost:8080/api/proxy/sessions/session-id-here

# Include streaming chunks
curl "http://localhost:8080/api/proxy/sessions/session-id?include_chunks=true"
```

### Client Type Values

| Value | Client |
|-------|--------|
| 0 | Unspecified |
| 1 | Claude Code |
| 2 | opencode |
| 3 | Cursor |
| 4 | Custom |

### API Format Values

| Value | Format |
|-------|--------|
| 0 | Unspecified |
| 1 | Anthropic |
| 2 | OpenAI |

---

## Verification

### Test Anthropic Proxy

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: $ANTHROPIC_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-sonnet-4-5-20241022",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Say hello"}]
  }'
```

### Test OpenAI Proxy

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{
    "model": "gpt-4o",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Say hello"}]
  }'
```

### Verify Session Captured

```bash
curl http://localhost:8080/api/proxy/sessions?limit=1
```

---

## Troubleshooting

### Common Issues

**1. "No API key provided" error**
- Ensure API key is set in environment or headers
- Check ERGORS logs for key extraction issues

**2. Connection refused**
- Verify ERGORS is running: `curl http://localhost:8080/health`
- Check firewall rules

**3. Timeout errors**
- Increase timeout in CLI tool config
- Check ERGORS upstream connectivity

**4. Streaming not working**
- Ensure `stream: true` in request body
- Check Content-Type headers

### Debug Logging

Enable verbose logging in ERGORS:
```bash
RUST_LOG=debug cargo run --package ergors
```

---

## Security Considerations

1. **Local deployment**: Run ERGORS on localhost to avoid exposing API keys
2. **TLS**: For production, configure ERGORS with TLS certificates
3. **Authentication**: Protect `/api/proxy/sessions` endpoints with auth
4. **Key rotation**: Regularly rotate API keys in ERGORS configuration

---

## References

- [OpenCode Configuration](https://opencode.ai/docs/config/)
- [OpenCode Providers](https://opencode.ai/docs/providers/)
- [Claude Code Network Config](https://docs.claude.com/en/docs/claude-code/network-config)
- [Claude Code Custom Endpoints (GitHub Issue)](https://github.com/anthropics/claude-code/issues/216)
