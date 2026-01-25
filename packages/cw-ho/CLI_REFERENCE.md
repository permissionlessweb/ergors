# ERGORS Engine CLI Reference

Main ERGORS engine daemon with HTTP API and gRPC management server.

```bash
ergors [OPTIONS] <COMMAND>
```

## Global Options

| Flag | Description | Default | Env Var |
|------|-------------|---------|---------|
| `--home <PATH>` | Home directory for configuration and data | `~/.ergors` | `NODE_DATA_PATH` |
| `--log-level <LEVEL>` | Log level (trace, debug, info, warn, error) | `info` | - |

## Command Groups

| Group | Description | Commands |
|-------|-------------|----------|
| `start` | Start the engine daemon | - |
| `init` | Initialize configuration and setup | new, llms, providers, unsafe-wipe, migrate |
| `config` | Manage configuration values | set, get, list, init |
| `manage-auth` | User authentication management | register, revoke |
| `keys` | Manage Cosmos funding keys | import-mnemonic, list, delete, set-default |

---

## Start Command

| Command | Description | Options | Example |
|---------|-------------|---------|---------|
| `start` | Start engine (HTTP API + gRPC server) | `--grpc-port <PORT>` - gRPC management port (default: `50051`, env: `ERGORS_GRPC_PORT`) | `ergors start --grpc-port 60051` |

**Notes:**

- Starts HTTP API server for LLM proxying and data capture
- Starts gRPC management server for remote control via ergors-cli
- Creates PID file to prevent multiple instances
- Handles SIGTERM/SIGINT for graceful shutdown

---

## Init Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `init new` | Initialize new node with full setup | Auto-generates:<br>- Encrypted node identity (Ed25519)<br>- SSH keys from custody<br>- API key encryption<br>- Sample .env file | `ergors init new` |
| `init llms` | Configure LLM provider API keys | Prompts for API keys:<br>- Anthropic (Claude)<br>- OpenAI (GPT)<br>- Ollama (local)<br>- Grok (xAI)<br>- Akash ML<br>Saves to `api-keys.toml` | `ergors init llms` |
| `init providers` | Configure provider key sharing | Sets per-provider ownership:<br>- `shared` - Shamir secret sharing<br>- `local` - Node-only<br>Configures k-of-n threshold | `ergors init providers` |
| `init unsafe-wipe` | Delete all data in home directory | **DESTRUCTIVE** - Removes all config and data | `ergors init unsafe-wipe` |
| `init migrate` | Migrate from major versions | (TODO: not implemented) | `ergors init migrate` |

**Init New Details:**

1. Creates encrypted custody (password-protected Ed25519 key)
2. Generates SSH keys from custody for git operations
3. Prompts for LLM API keys and encrypts with custody password
4. Writes sample `.env` file from template
5. Password must be at least 8 characters
6. Password can be set via `ERGORS_CUSTODY_PASSWORD` env var for non-interactive setup

**Provider Sharing:**

- Default: Anthropic/OpenAI → `shared` (2-of-3 threshold)
- Default: Ollama → `local` (no sharing)
- Shared keys distributed via Shamir secret sharing from coordinator
- Threshold format: `k-of-n` (e.g., `2-of-3` means 2 shares needed from 3 total)

---

## Config Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `config init` | Initialize minimal valid configuration | `--node-type <TYPE>` - coordinator, executor, referee, development (default: `development`)<br>`--api-port <PORT>` - gRPC/API port (default: `50051`)<br>`--p2p-port <PORT>` - P2P port (default: `26656`)<br>`--with-sdl-contract` - Deploy SDL template contract on startup<br>`--sdl-wasm-path <PATH>` - Path to SDL WASM file (required if --with-sdl-contract) | `ergors config init --node-type executor --api-port 50051 --p2p-port 26656` |
| `config set <KEY> <VALUE>` | Set configuration value | `<KEY>` - Dot-separated path (e.g., `network.listen_port`)<br>`<VALUE>` - Value (type validated) | `ergors config set network.listen_port 9090` |
| `config get <KEY>` | Get configuration value | `<KEY>` - Dot-separated path | `ergors config get identity.node_type` |
| `config list` | List all configuration keys and types | - | `ergors config list` |

**Available Config Keys:**

| Section | Key | Type | Description |
|---------|-----|------|-------------|
| **Home** | `home` | string | Home directory path |
| **Identity** | `identity.host` | string | Node hostname/IP |
| | `identity.p2p_port` | u32 | P2P listening port |
| | `identity.api_port` | u32 | API/gRPC port |
| | `identity.user` | string | Username |
| | `identity.os` | i32 | OS type (1=Linux, 2=MacOS, 3=Windows) |
| | `identity.ssh_port` | u32 | SSH port |
| | `identity.node_type` | string | Coordinator, Executor, Referee, Development |
| **Network** | `network.node_type` | i32 | 1=Coordinator, 2=Executor, 3=Referee, 4=Development |
| | `network.listen_port` | u32 | P2P listening port |
| | `network.listen_address` | string | Bind address (e.g., 0.0.0.0) |
| | `network.connection_timeout_ms` | u32 | Connection timeout in ms |
| | `network.enable_discovery` | bool | Enable peer discovery |
| **Storage** | `storage.data_dir` | string | Data directory path |
| | `storage.max_size_mb` | u32 | Maximum storage size in MB |
| | `storage.enable_compression` | bool | Enable data compression |
| **LLM** | `llm.api_keys_file` | string | Path to API keys file |
| | `llm.timeout_seconds` | u64 | Request timeout |
| | `llm.max_retries` | u32 | Maximum retry attempts |
| | `llm.default_strategy` | i32 | Model selection strategy |
| **CosmWasm** | `cosmwasm.enabled` | bool | Enable CosmWasm VM |
| | `cosmwasm.cache_dir` | string | WASM cache directory |
| | `cosmwasm.memory_limit` | u64 | Memory limit in bytes |

---

## Auth Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `manage-auth register` | Register user key pair for API access | `--auth <BASE64_JSON>` - Base64-encoded auth structure | `ergors manage-auth register --auth <base64>` |
| `manage-auth revoke` | Revoke user key pair | `--auth <BASE64_JSON>` - Base64-encoded auth structure | `ergors manage-auth revoke --auth <base64>` |

**Notes:**

- (Implementation incomplete - placeholder for contract-based authentication)
- Will integrate with CosmWasm authenticator contracts for programmable API access control
- Supports Ed25519 signature-based authentication fallback

---

## Keys Commands

Manage Cosmos blockchain funding keys (Akash, Cosmos Hub, etc.) for deployment operations.

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `keys import-mnemonic` | Import BIP-39 mnemonic seed phrase | `--phrase <MNEMONIC>` - 24-word seed phrase (required)<br>`--label <LABEL>` - Human-readable label (required)<br>`--key-name <NAME>` - Internal identifier (default: `default`)<br>`--chain-id <ID>` - Chain ID (default: `akashnet-2`)<br>`--address-prefix <PREFIX>` - Bech32 prefix (default: `akash`)<br>`--make-default` - Set as default key | `ergors keys import-mnemonic --phrase "word1 word2 ..." --label "My Akash Key" --key-name prod --make-default` |
| `keys list` | List all stored keys | Shows: name, label, address, chain ID, default marker | `ergors keys list` |
| `keys delete` | Delete a key by name | `--key-name <NAME>` - Key name to delete (required) | `ergors keys delete --key-name old-key` |
| `keys set-default` | Set a key as the default | `--key-name <NAME>` - Key name to make default (required) | `ergors keys set-default --key-name prod` |

**Security:**

- All mnemonics encrypted with Argon2id + ChaCha20Poly1305
- Password-protected key store
- Mnemonics never persisted in plaintext
- Secure password prompt (hidden input)
- Password confirmation required for new stores
- File permissions set to 0600 (owner read/write only) on Unix

**Example Output:**

```
NAME            LABEL                ADDRESS                                       CHAIN        DEFAULT
--------------------------------------------------------------------------------
prod            My Akash Key         akash1abc123...                                akashnet-2   *
test            Test Key             akash1xyz789...                                akashnet-2
```

---

## Storage

ERGORS uses Cnidarium (JMT-based verifiable storage) for:

- Encrypted key stores (Cosmos mnemonics, API keys)
- LLM request/response capture
- Session state and metadata
- CosmWasm contract state

**Storage Location:** `$HOME/data/` (configurable via `storage.data_dir`)

**Encryption:**

- Custody keys: Password-encrypted Ed25519 (ChaCha20Poly1305)
- API keys: Custody password-encrypted
- Cosmos keys: Argon2id + ChaCha20Poly1305 with separate password

---

## HTTP API Endpoints

When running, the engine exposes:

| Endpoint | Description |
|----------|-------------|
| `/v1/chat/completions` | OpenAI-compatible chat completions (proxies to configured provider) |
| `/v1/messages` | Anthropic-compatible messages API |
| `/health` | Health check endpoint |
| `/metrics` | Prometheus-compatible metrics |

**API Features:**

- Automatic LLM provider routing based on model name
- Request/response capture to Cnidarium storage
- Session management with fractal hierarchy
- Rate limiting via token bucket (configurable)
- Streaming support for both OpenAI and Anthropic formats

---

## gRPC Management Server

Used by `ergors-cli` for remote management:

| Service | Methods |
|---------|---------|
| `ManagementService` | 70+ RPC methods for node control, deployment, network management |

**Default Address:** `0.0.0.0:50051` (configurable via `--grpc-port`)

See `ergors-cli` documentation for available management operations.

---

## Daemon Management

ERGORS uses PID file locking to prevent multiple instances:

**PID File:** `$HOME/ergors.pid`

**Signals:**

- `SIGTERM` / `SIGINT` - Graceful shutdown (releases PID lock, closes storage)
- `SIGHUP` - Reload configuration (not implemented)

**Process Management:**

- Checks for existing process on startup
- Acquires PID lock before initializing
- Releases lock on clean exit or crash
- Auto-cleanup on normal termination

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `NODE_DATA_PATH` | Override default home directory |
| `ERGORS_GRPC_PORT` | Override default gRPC port |
| `ERGORS_CUSTODY_PASSWORD` | Non-interactive custody password (for automation) |
| Provider-specific | `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GROK_API_KEY`, `AKASHML_API_KEY` |

---

## Exit Codes

| Code | Description |
|------|-------------|
| `0` | Success |
| `1` | General error (config load failed, storage error, etc.) |
| Non-zero | Runtime error with message on stderr |

---

## Quick Start

```bash
# 1. Initialize new node (creates custody, SSH keys, API keys)
ergors init new

# 2. (Optional) Import Akash funding key for deployments
ergors keys import-mnemonic \
  --phrase "your 24 word mnemonic here" \
  --label "Akash Main" \
  --chain-id akashnet-2 \
  --make-default

# 3. Start the engine
ergors start

# 4. In another terminal, verify it's running
ergors-cli status
```

---

## Files Created

| File | Description | Permissions |
|------|-------------|-------------|
| `$HOME/config.toml` | Main configuration file | 0644 |
| `$HOME/.env` | Environment template (from `templates/example.env`) | 0644 |
| `$HOME/identity.enc` | Encrypted node identity (custody) | 0600 |
| `$HOME/api-keys.enc` | Encrypted LLM API keys | 0600 |
| `$HOME/providers.toml` | Provider sharing configuration | 0644 |
| `$HOME/ssh/id_ed25519` | SSH private key (from custody) | 0600 |
| `$HOME/ssh/id_ed25519.pub` | SSH public key | 0644 |
| `$HOME/data/` | Cnidarium storage directory | 0755 |
| `$HOME/wasm_cache/` | CosmWasm VM cache | 0755 |
| `$HOME/ergors.pid` | PID file (created on start) | 0644 |

---

## Logging

Logs are written to stderr using `tracing`:

```bash
# Set log level via CLI
ergors --log-level debug start

# Or via environment variable
RUST_LOG=debug ergors start

# Filter by module
RUST_LOG=ergors=debug,cnidarium=info ergors start
```

**Log Levels:**

- `trace` - Very verbose (network packets, state transitions)
- `debug` - Detailed (request/response, storage ops)
- `info` - Normal operations (startup, shutdown, key events)
- `warn` - Warning conditions (retries, degraded state)
- `error` - Error conditions (failures, exceptions)
