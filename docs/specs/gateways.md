# Gateways

Communication gateways provide user-facing interfaces for interacting with the ERGORS AI engine. They bridge external platforms (Discord, Nostr, Element, Telegram, etc.) to the LLM routing system, enabling conversations across any supported medium.

---

## Overview

The gateway system provides:

| Feature | Description |
|---------|-------------|
| **Multi-Platform Support** | Unified interface for Discord, Nostr, Element, and custom integrations |
| **Session Continuity** | Per-thread/channel conversation tracking across restarts |
| **Secure Token Storage** | Encrypted credentials using node identity keys |
| **Unified Metrics** | Consistent message tracking across all gateways |
| **Audit Trail** | Access logging for all secret retrievals |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        ERGORS Engine                                 │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    GatewayManager                            │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │    │
│  │  │   Discord    │  │    Nostr     │  │   Element    │  ...  │    │
│  │  │   Gateway    │  │   Gateway    │  │   Gateway    │       │    │
│  │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │    │
│  │         │                 │                 │                │    │
│  │         └────────────────┼─────────────────┘                │    │
│  │                          │                                   │    │
│  │                    ┌─────▼─────┐                             │    │
│  │                    │  Metrics  │                             │    │
│  │                    │  Tracker  │                             │    │
│  │                    └───────────┘                             │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                          │                                           │
│                    ┌─────▼─────┐                                    │
│                    │ LLM Router│                                    │
│                    └───────────┘                                    │
│                          │                                           │
│           ┌──────────────┼──────────────┐                           │
│           ▼              ▼              ▼                           │
│     ┌──────────┐  ┌──────────┐  ┌──────────────┐                   │
│     │ Anthropic│  │  OpenAI  │  │Akash Deploy  │                   │
│     └──────────┘  └──────────┘  └──────────────┘                   │
└─────────────────────────────────────────────────────────────────────┘
```

### Components

| Component | Responsibility |
|-----------|----------------|
| **GatewayManager** | Orchestrates gateway lifecycle, routes events, tracks metrics |
| **GatewayModule** | Platform-specific implementation (Discord, Nostr, etc.) |
| **GatewayMetrics** | Per-gateway message counters and timestamps |
| **ErgorsStorage** | Session persistence, encrypted token storage, config |

---

## Gateway Lifecycle

### 1. Registration

Gateways are registered with the manager on engine startup:

```rust
// In server initialization
let discord = Arc::new(DiscordGateway::from_storage(&storage, node_pubkey).await?);
manager.register(discord).await;
```

### 2. Configuration

Configure gateways via CLI before starting:

```bash
# Set Discord bot token (encrypted)
ergors gateway discord set-token

# Allow specific guilds
ergors gateway discord allow-guild 123456789012345678

# Enable the gateway
ergors gateway enable discord
```

### 3. Startup

When the engine starts, enabled gateways receive a `GatewayContext`:

```rust
GatewayContext {
    router: Arc<LlmRouter>,      // For LLM requests
    storage: Arc<ErgorsStorage>, // For session/config
    config: GatewayConfig,       // Gateway-specific settings
    event_tx: Sender<GatewayEvent>, // For manager communication
}
```

### 4. Message Processing

All gateways follow the same processing pattern:

```
User Message → Gateway → LLM Router → Response → Gateway → User
                   │                        ▲
                   └── MessageProcessed ────┘
                       (metrics tracking)
```

### 5. Shutdown

Gateways are stopped gracefully on engine shutdown:

```bash
# SIGTERM/SIGINT triggers graceful shutdown
kill -TERM $(cat ~/.ergors/ergors.pid)
```

---

## Configuring Gateways

### Via CLI

```bash
# List all gateways
ergors gateway list

# Check gateway status
ergors gateway status discord

# Enable/disable
ergors gateway enable discord
ergors gateway disable discord
```

### Via gRPC

| RPC Method | Description |
|------------|-------------|
| `ListGateways` | List registered gateways with status and metrics |
| `GetGatewayStatus` | Get detailed status for a specific gateway |
| `EnableGateway` | Enable a gateway (starts on next engine restart) |
| `DisableGateway` | Disable a gateway |

### Configuration Storage

Gateway configurations are stored in Cnidarium with the prefix `gateway_config/`:

```
gateway_config/discord → GatewayConfig {
    gateway_id: "discord",
    gateway_type: "discord",
    enabled: true,
    settings: {
        "bot_token_encrypted": "true",
        "allowed_guild_ids": "123,456",
        "command_prefix": "!",
        "respond_to_mentions": "true",
        "respond_to_dms": "false"
    }
}
```

---

## Security

### Token Encryption

Gateway secrets (bot tokens, API keys) are encrypted using the node's identity key:

| Layer | Method |
|-------|--------|
| Key Derivation | SHA-256(prefix + node_pubkey) |
| Encryption | ChaCha20Poly1305 |
| Nonce | Random 12 bytes per encryption |

**No runtime password required** - secrets are decrypted using the node's Ed25519 public key.

### Audit Logging

Every secret access is logged to Cnidarium:

```protobuf
message SecretAccessLog {
    string secret_id = 1;      // "discord_bot_token"
    Timestamp accessed_at = 2;
    string accessor = 3;       // "discord_gateway"
    string purpose = 4;        // "startup"
    bool success = 5;
    string error = 6;
}
```

Query access logs via storage:

```rust
let logs = storage.list_secret_access_logs(Some("discord_bot_token"), 100).await?;
```

### Guild Authorization

Gateways can restrict access to specific servers/groups:

- **Empty whitelist** = All allowed (permissive default)
- **Non-empty whitelist** = Only listed IDs allowed
- Unauthorized access attempts are logged

---

## Session Management

Each conversation thread maintains its own session:

```
gateway_session/{gateway_id}/{thread_id} → session_id
```

### Session Lifecycle

| Event | Behavior |
|-------|----------|
| First message in thread | Create new session |
| Subsequent messages | Reuse existing session |
| `/clear` command | Create fresh session |
| `/thread` command | Create new Discord thread + session |

### Session Persistence

Sessions survive engine restarts (stored in Cnidarium). The session ID is passed to the LLM router via `PromptContext`:

```rust
PromptContext {
    session_id: "abc123",
    user_id: "discord:123456",
    thread_id: "discord:channel:789",
}
```

---

## Metrics

The GatewayManager tracks per-gateway metrics:

| Metric | Description |
|--------|-------------|
| `messages_processed` | Total messages handled |
| `last_message_timestamp` | Unix timestamp of last message |

### Querying Metrics

**Via CLI:**

```bash
ergors gateway status discord
# Output:
# Connected:   yes
# Messages:    142
# Last Active: 1706835200
```

**Via gRPC:**

```protobuf
message GatewayStatusResponse {
    string gateway_id = 1;
    bool connected = 2;
    uint64 messages_processed = 3;
    int64 last_message_timestamp = 4;
}
```

---

## Available Gateways

### Discord

Full-featured Discord bot with slash commands.

**Features:**
- `/prompt` - Send AI prompts
- `/thread` - Create conversation threads
- `/clear` - Reset conversation
- Guild authorization
- Per-channel session tracking
- Long response chunking (2000 char limit)

**Setup:**

```bash
# 1. Create bot at https://discord.com/developers/applications
# 2. Enable "Message Content Intent"
# 3. Configure
ergors gateway discord set-token
ergors gateway discord allow-guild <guild-id>
ergors gateway enable discord

# 4. Start engine
ergors start

# 5. Invite bot with scopes: bot, applications.commands
```

**Compile Requirement:**

```bash
cargo build --features discord
```

### Nostr (Planned)

Nostr relay integration for decentralized AI access.

### Element/Matrix (Planned)

Matrix protocol integration for encrypted group chats.

---

## Developing Custom Gateways

### 1. Implement the GatewayModule Trait

```rust
use ho_std::traits::gateway::{GatewayModule, GatewayContext, GatewayEvent};

pub struct MyGateway {
    config: MyGatewayConfig,
    connected: AtomicBool,
}

#[async_trait]
impl GatewayModule<LlmRouter, ErgorsStorage> for MyGateway {
    fn gateway_id(&self) -> &str {
        "my-gateway"
    }

    fn name(&self) -> &str {
        "My Custom Gateway"
    }

    async fn start(&self, ctx: GatewayContext<LlmRouter, ErgorsStorage>) -> HoResult<()> {
        // Initialize connection
        // Store ctx.router, ctx.storage, ctx.event_tx for later use
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> HoResult<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn send_response(&self, response: GatewayResponse) -> HoResult<()> {
        // Send response back to user via your platform
        Ok(())
    }
}
```

### 2. Message Processing Pattern

**Option A: Event-Based (Simple)**

Send `GatewayEvent::MessageReceived` and let the manager handle routing:

```rust
let msg = GatewayMessage {
    gateway_id: "my-gateway".to_string(),
    channel_id: channel.to_string(),
    thread_id: thread.to_string(),
    sender_id: user.to_string(),
    sender_name: username.clone(),
    content: message_text,
    timestamp: Utc::now().timestamp(),
    metadata: HashMap::new(),
    reply_to_id: String::new(),
};

event_tx.send(GatewayEvent::MessageReceived(msg))?;
// Manager calls your send_response() with the LLM reply
```

**Option B: Direct Routing (Custom UX)**

Call the router directly for custom response handling (e.g., chunking, embeds):

```rust
// 1. Get session
let session_id = storage
    .get_or_create_gateway_session("my-gateway", &thread_id)
    .await?;

// 2. Build request
let request = PromptRequest {
    messages: vec![PromptMessage {
        role: "user".to_string(),
        content: user_message,
        ..Default::default()
    }],
    model: "default".to_string(),
    context: Some(PromptContext {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        thread_id: thread_id.clone(),
    }),
    ..Default::default()
};

// 3. Route to LLM
let response = router.handle_request(&request, "default").await?;

// 4. Handle response (custom formatting, chunking, etc.)
send_my_custom_response(&response).await?;

// 5. Notify manager for metrics
event_tx.send(GatewayEvent::MessageProcessed {
    gateway_id: "my-gateway".to_string(),
    session_id,
    user_id,
})?;
```

### 3. Register Your Gateway

Add to `server.rs` initialization:

```rust
#[cfg(feature = "my-gateway")]
{
    let my_gw = Arc::new(MyGateway::from_storage(&storage).await?);
    manager.register(my_gw).await;
}
```

### 4. Add Feature Flag

In `Cargo.toml`:

```toml
[features]
my-gateway = ["dep:my-gateway-sdk"]
```

---

## Events

The gateway system uses typed events for manager communication:

```rust
pub enum GatewayEvent {
    /// Message received - triggers LLM processing
    MessageReceived(GatewayMessage),

    /// Message processed - metrics only (no LLM call)
    /// For gateways that handle routing internally
    MessageProcessed {
        gateway_id: String,
        session_id: String,
        user_id: String,
    },

    /// Connection state changed
    ConnectionStateChanged {
        gateway_id: String,
        connected: bool,
    },

    /// Error occurred
    Error {
        gateway_id: String,
        error: String,
    },
}
```

---

## Troubleshooting

### Gateway Not Starting

```bash
# Check if enabled
ergors gateway list

# Check logs
RUST_LOG=ergors::gateway=debug ergors start
```

### Token Decryption Failed

```
ERROR Failed to decrypt Discord bot token: Decryption failed
```

**Cause:** Node identity changed since token was encrypted.

**Fix:** Re-configure the token:

```bash
ergors gateway discord set-token
```

### Unauthorized Guild

```
WARN Unauthorized guild attempted access: 123456789
```

**Cause:** Guild not in allowlist.

**Fix:** Add the guild:

```bash
ergors gateway discord allow-guild 123456789
```

### Bot Not Responding

1. Check bot is connected: `ergors gateway status discord`
2. Verify slash commands registered (check Discord server)
3. Ensure bot has required permissions in the guild
4. Check engine logs for errors

---

## Proto Definitions

Gateway types are defined in `proto/ergors/gateway/v1/gateway.proto`:

```protobuf
message GatewayConfig {
    string gateway_id = 1;
    string gateway_type = 2;
    bool enabled = 3;
    map<string, string> settings = 4;
}

message GatewayMessage {
    string gateway_id = 1;
    string channel_id = 2;
    string thread_id = 3;
    string sender_id = 4;
    string sender_name = 5;
    string content = 6;
    int64 timestamp = 7;
    map<string, string> metadata = 8;
    string reply_to_id = 9;
}

message GatewayResponse {
    string gateway_id = 1;
    string channel_id = 2;
    string recipient_id = 3;
    string content = 4;
    string reply_to_id = 5;
    repeated Embed embeds = 6;
    repeated Attachment attachments = 7;
}

message DiscordGatewayConfig {
    string bot_token = 1;
    repeated string allowed_guild_ids = 2;
    repeated string allowed_channel_ids = 3;
    string command_prefix = 4;
    bool respond_to_mentions = 5;
    bool respond_to_dms = 6;
}
```

Encrypted secrets are stored using `proto/ergors/storage/v1/storage.proto`:

```protobuf
message EncryptedSecret {
    string secret_id = 1;
    string secret_type = 2;
    string label = 3;
    bytes encrypted_value = 4;
    bytes nonce = 5;
    string encryption_method = 6;
    Timestamp created_at = 7;
    Timestamp last_accessed_at = 8;
    uint64 access_count = 9;
    map<string, string> metadata = 10;
}
```
