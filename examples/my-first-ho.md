# Getting Started with ERGORS

This guide walks you through setting up ERGORS to proxy LLM requests from [OpenCode](https://github.com/sst/opencode), Claude Code, or similar CLI tools. ERGORS captures all prompts and responses for review while transparently forwarding to your configured LLM provider.

## Prerequisites

- Rust toolchain (1.75+)
- [just](https://github.com/casey/just) task runner
- An LLM API key (Anthropic, OpenAI, or others)
- OpenCode CLI or Claude Code

```bash
# Install just (if not already installed)
cargo install just
```

---

## Quick Start

```bash
# Clone and build
git clone https://github.com/permissionlessweb/ergors.git
cd ergors
just install                    # Builds + installs to ~/.cargo/bin

# Initialize and run
ergors init                     # Create node identity and config
ergors init llms                # Configure LLM API keys
ergors start                    # Start the engine
```

In another terminal, configure your CLI tool:

```bash
export ANTHROPIC_BASE_URL="http://localhost:8080"
opencode                        # Requests now route through ERGORS
```

**Request flow:** `OpenCode → ERGORS (captures) → Anthropic API`

---

## Step-by-Step Setup

### 1. Install ERGORS

```bash
cd /path/to/ergors

# Build and install both binaries to PATH
just install
```

This installs:

- `ergors` — Node engine (HTTP API + gRPC management)

Verify installation:

```bash
just which
# Output:
#   ergors: /Users/you/.cargo/bin/ergors
```

### 2. Initialize Node

```bash
ergors init
```

Creates:

```
~/.ergors/
├── config.toml          # Main configuration
├── node_identity.enc    # Encrypted node keypair
└── data/                # Storage directory
```

### 3. Configure LLM Providers

```bash
ergors init llms
```

This launches an interactive prompt to configure your API keys. Keys are encrypted using your node identity.

**Alternative:** Set environment variables:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
```

### 4. Start the Engine

```bash
ergors start
```

### 5. Verify Health

```bash
curl http://localhost:8080/health
# response should look similar to: {"status":"ok","version":"0.1.0","uptime_seconds":111,"storage_status":"healthy","network_status":"connected (1 peers)"}% 
```

---

## Configure OpenCode UI

OpenCode can be configured to route requests through ERGORS either via environment variables or its settings UI.

### Option A: Environment Variables

```bash
export BASE_URL="http://localhost:8080"
opencode
```

### Option B: OpenCode Settings UI

<!-- SCREENSHOT: OpenCode settings panel -->
> **Screenshot placeholder:** OpenCode main settings panel
>
> ![OpenCode Settings](./screenshots/opencode-settings-panel.png)

1. Open OpenCode
2. Navigate to **Settings** (gear icon or `Cmd+,`)
3. Find the **API Configuration** section

<!-- SCREENSHOT: API endpoint configuration -->
> **Screenshot placeholder:** API endpoint configuration field
>
> ![API Endpoint Config](./screenshots/opencode-api-endpoint.png)

1. Set the **Base URL** to `http://localhost:8080`
2. Leave your API key as-is (ERGORS will use its own configured keys)

<!-- SCREENSHOT: Configured state showing ERGORS URL -->
> **Screenshot placeholder:** Configured state with ERGORS proxy URL
>
> ![Configured State](./screenshots/opencode-configured.png)

1. Save and restart OpenCode

### Option C: Claude Code

For Claude Code CLI:

```bash
export ANTHROPIC_BASE_URL="http://localhost:8080"
claude
```

<!-- SCREENSHOT: Claude Code with proxy configured -->
> **Screenshot placeholder:** Claude Code terminal showing proxy configuration
>
> ![Claude Code Config](./screenshots/claude-code-proxy.png)

---

## Verify Proxy is Working

### Check Captured Sessions

After running some prompts:

```bash
curl "http://localhost:8080/api/proxy/sessions?limit=5"
```

### Session Data Includes

- Full request payload (prompts, system messages)
- Full response (assistant output)
- Token counts (input/output)
- Timing information
- Client detection (OpenCode, Claude Code, curl, etc.)

---

## Development Workflow

Use `just` for development tasks:

```bash
# Run engine in dev mode (with RUST_BACKTRACE)
just dev start

# Run with custom args
just dev init --home /custom/path

# Watch for changes
just watch

# Quick syntax check
just check
```

---

## Engine Management

### CLI Commands

```bash
ergors status              # Check engine status
ergors node info           # View node identity
ergors provider list       # List LLM providers
ergors provider test anthropic  # Test connectivity
```

### Ports Reference

| Port  | Protocol | Purpose |
|-------|----------|---------|
| 8080  | HTTP     | API server (proxy endpoints) |
| 50051 | gRPC     | Management server |
| 26969 | TCP      | P2P networking |

---

## Proxy Endpoints

ERGORS exposes OpenAI and Anthropic-compatible endpoints:

| Endpoint | Format | Description |
|----------|--------|-------------|
| `/v1/messages` | Anthropic | Claude API |
| `/v1/chat/completions` | OpenAI | Chat completions |
| `/api/proxy/sessions` | ERGORS | Query captured sessions |
| `/health` | ERGORS | Health check |

### Direct curl Example

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: $ANTHROPIC_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

---

## Troubleshooting

### Engine Won't Start

```bash
# Check if already running
lsof -i :8080

# Check logs with debug level
just dev start --log-level debug
```

### API Key Errors

```bash
# Re-configure API keys
ergors init llms

# Verify key is set
echo $ANTHROPIC_API_KEY
```

### Proxy Not Forwarding

Test direct upstream connectivity:

```bash
curl -X POST https://api.anthropic.com/v1/messages \
  -H "x-api-key: $ANTHROPIC_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'
```

---

## Screenshots Directory

To add the screenshots referenced above, create:

```
examples/
└── screenshots/
    ├── opencode-settings-panel.png
    ├── opencode-api-endpoint.png
    ├── opencode-configured.png
    └── claude-code-proxy.png
```

Capture these from the respective applications with the proxy configured.

---

## Next Steps

- **Review sessions**: Query `/api/proxy/sessions` to analyze captured prompts
- **Multi-node**: Connect multiple ERGORS nodes for distributed orchestration
- **Custom providers**: Add local Ollama or other LLM backends
- **Secure keys**: Use password-encrypted custody (see [Custody & Auth](../docs/specs/custody-and-auth.md))

---

## Quick Reference

| Task | Command |
|------|---------|
| Install | `just install` |
| Initialize | `ergors init` |
| Configure LLMs | `ergors init llms` |
| Start | `ergors start` |
| Status | `ergors status` |
| Dev mode | `just dev start` |
| Check health | `curl localhost:8080/health` |
| Query sessions | `curl localhost:8080/api/proxy/sessions` |
