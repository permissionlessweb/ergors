# Discord Gateway Specification

## Overview

The Discord Gateway provides a bot interface for interacting with the Ergors engine through Discord slash commands. It enables AI chat, document ingestion, and RAG (Retrieval-Augmented Generation) capabilities within Discord servers.

## Architecture

### Component Structure

```
Discord Bot (Poise/Serenity)
    ↓
Discord Gateway Module
    ↓
├─→ LLM Router (inference)
├─→ RAG Storage (document retrieval)
├─→ RLM Service (agentic reasoning - optional)
├─→ Session Manager (conversation tracking)
└─→ Gateway Manager (metrics & events)
```

### Key Components

#### 1. **DiscordData** (Shared Context)

```rust
pub struct DiscordData {
    storage: Arc<ErgorsStorage>,           // Cnidarium storage backend
    router: Arc<LlmRouter>,                // LLM provider routing
    allowed_guild_ids: Vec<String>,        // Guild allowlist
    event_tx: UnboundedSender<GatewayEvent>, // Event bus
    http_client: reqwest::Client,          // URL fetching (SSRF protected)
    rag_client: reqwest::Client,           // RAG/embedding API client
    rlm_service: Option<Arc<RlmService>>,  // Agentic reasoning (optional)
    test_mode: bool,                       // Bypass LLM for testing
}
```

#### 2. **Session Management**

- **Session Scope**: Per Discord thread/channel
- **Session ID**: `thread-{channel_id}`
- **Storage**: Cnidarium state machine
- **Lifecycle**: Created on first message, persists across bot restarts

#### 3. **Authentication & Authorization**

**Guild Authorization**:

- Allowlist-based (if configured)
- Validates guild ID before command execution

**Admin Authorization** (for document operations):

- Configurable per-guild admin role
- Falls back to Discord guild owner if not configured
- Required for: `/ingest`, `/ragconfig`, `/ragdelete`, `/rlmconfig`

#### 4. **Context Injection Pipeline**

```
User Message
    ↓
Check RLM Mode Config
    ├─→ [static] → RAG Query → Augmented Prompt → LLM
    ├─→ [rlm]    → RLM Query → Final Answer (skip LLM)
    └─→ [hybrid] → RAG Query → Augmented Prompt → LLM (RLM as fallback)
```

## Features

### Slash Commands

#### Core Commands

| Command | Description | Admin Required |
|---------|-------------|----------------|
| `/prompt <message>` | Send message to AI | No |
| `/thread [name]` | Create new conversation thread | No |
| `/clear` | Clear conversation history | No |

#### Document Management Commands

| Command | Description | Admin Required |
|---------|-------------|----------------|
| `/ingest <url> [label] [doc_type]` | Ingest URL or GitHub repo | Yes |
| `/ragsources [limit]` | List ingested documents | No |
| `/ragdelete <source>` | Delete document by URI | Yes |
| `/ragconfig [admin_role]` | Configure RAG settings | Yes |
| `/rlmconfig [mode]` | Configure RLM mode | Yes |

### Document Ingestion

#### Supported Sources

1. **Regular URLs**:
   - HTTP/HTTPS endpoints
   - Content-type validation (text/html, markdown, json, etc.)
   - SSRF protection (blocks private IPs, DNS rebinding)
   - 1MB size limit

2. **GitHub Repositories**:
   - Auto-detected from URL pattern
   - Shallow clone via `githem` library
   - Smart filtering (excludes binaries, node_modules, etc.)
   - Per-file ingestion for granular retrieval
   - Supports branches, PRs, commits

#### Ingestion Flow

```
URL/GitHub → SSRF Validation → Fetch/Clone → Content Curation
    ↓
Chunk Text → Generate Embeddings → Store in RAG
    ↓
Tag with Guild/Repo/User → Return Success
```

#### GitHub-Specific Features

**Filter Presets** (via `doc_type` parameter):

- `documentation` / `docs`: Standard preset (docs + code)
- `code`: Code-only preset (source files only)
- `minimal`: Minimal filtering (almost everything)

**Metadata Tagging**:

- `guild:{guild_id}` - Guild-scoped isolation
- `repo:{owner/repo}` - Repository identifier
- `user:{user_id}` - Ingestion author

**Example**:

```
/ingest url:https://github.com/cosmology-tech/interchain
        label:interchain-docs
        doc_type:documentation
```

### RAG (Retrieval-Augmented Generation)

#### Configuration

Per-guild RAG settings:

- **Admin Role**: Discord role ID for admin permissions
- **Embedding Model**: Model for generating embeddings
- **Embedding Endpoint**: API endpoint for embeddings
- **Embedding Dimension**: Vector dimension (e.g., 1536 for text-embedding-3-small)
- **Max Context Chunks**: Number of chunks to retrieve (default: 3, max: 10)
- **Min Similarity**: Minimum similarity score (0.0-1.0)

#### Query Flow

```
User Message → Similarity Search → Top-K Chunks → Augment Prompt → LLM
```

**Augmented Prompt Format**:

```
Context from guild knowledge base:
---
Source: {uri}
{chunk_content}
---
Source: {uri}
{chunk_content}
---

User question: {original_message}
```

#### Statistics Tracking

Per-guild metrics:

- Total documents ingested
- Total chunks stored
- Last ingestion timestamp
- Auto-context enabled/disabled

### RLM (Retrieval-Augmented Language Model)

Optional agentic reasoning layer for document exploration.

#### Modes

- **static**: Traditional RAG (similarity search only)
- **rlm**: Pure RLM (agentic document exploration, returns final answer)
- **hybrid**: RLM with LLM fallback

#### RLM Query Flow

```
User Question → RLM Agent → Document Exploration → Code Execution → Final Answer
```

**RLM bypasses LLM** when it can answer directly from documents.

### Test Mode

Environment variable: `ERGORS_GATEWAY_TEST_MODE=1`

**Purpose**: Validate gateway integration without LLM providers.

**Behavior**:

- ✅ All authentication/authorization enforced
- ✅ Document ingestion works normally
- ✅ RAG storage and embeddings generated
- ❌ LLM calls bypassed (test responses returned)

**Use Cases**:

- Testing bot connectivity
- Validating admin permissions
- Debugging gateway configuration
- Development/staging environments

## Security

### SSRF Protection

**URL Ingestion**:

1. Parse URL and resolve hostname to IP
2. Block private/loopback/link-local addresses
3. Block IPv4-mapped IPv6 addresses
4. Fetch using resolved IP (prevents DNS rebinding)

**Blocked IP Ranges**:

- IPv4: 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16
- IPv6: ::1, fe80::/10, fc00::/7, IPv4-mapped addresses

### Token Encryption

- Bot token encrypted via custody password
- Never stored in plaintext
- Encrypted blob stored in cnidarium

### Permission Model

**Guild Isolation**:

- All RAG data scoped per guild
- No cross-guild data access

**Admin Operations**:

- Document ingestion
- Configuration changes
- Document deletion

**User Operations**:

- Prompts and queries
- Viewing sources

## Data Storage

### Storage Backend: Cnidarium

State machine with key-value storage.

#### Keys

**Guild RAG Configuration**:

```
gateway/discord/guild/{guild_id}/rag_config
```

**Session Tracking**:

```
gateway/discord/session/{channel_id}
```

**RAG Documents** (via ergors-rag):

```
rag/chunks/{chunk_hash}
rag/embeddings/{chunk_hash}
```

#### State Persistence

- All state survives bot restarts
- Sessions persist across connections
- RAG documents retained until explicit deletion

## Configuration

### Bot Setup

1. **Create Discord Application**:
   - Visit <https://discord.com/developers/applications>
   - Create new application
   - Navigate to Bot section
   - Enable "Message Content Intent"
   - Copy bot token

2. **Generate Invite Link**:
   - OAuth2 → URL Generator
   - Scopes: `bot`, `applications.commands`
   - Permissions: Send Messages, Read Message History, Use Slash Commands
   - Copy generated URL

3. **Invite Bot to Server**:
   - Use generated URL
   - Select target server
   - Authorize permissions

### Engine Configuration

```bash
# Set bot token (encrypted via custody)
ergors gateway discord set-token
# Enter token when prompted

# Optional: Restrict to specific guilds
ergors gateway discord allow-guild 123456789012345678

# Start gateway
ergors gateway start discord
```

### Guild Configuration (via Discord)

```
# Set admin role for document operations
/ragconfig admin_role:@DocumentAdmins

# Configure RLM mode (optional)
/rlmconfig mode:hybrid
```

## Error Handling

### Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| "Missing required RAG admin role" | User lacks admin permissions | Add user to admin role or use `/ragconfig` |
| "RAG not configured" | Global RAG settings missing | Run `ergors rag configure` on engine |
| "Invalid URL" | SSRF protection triggered | Use public URL, not private/loopback |
| "Failed to fetch URL" | Network error or blocked | Check URL accessibility |
| "Document not found" | Invalid document ID | Verify ID with `/ragsources` |

### Logging

**Log Levels**:

- `debug`: Message processing, session creation
- `info`: Command execution, document ingestion
- `warn`: Non-fatal errors (e.g., failed to fetch URL)
- `error`: Fatal errors (e.g., storage failure)

**Log Location**: `~/.ergors/logs/gateway-discord.log`

## Performance Considerations

### Message Limits

- Discord message limit: 2000 characters
- Buffer: 1990 characters (safety margin)
- Long responses automatically chunked

### Rate Limits

**Discord API**:

- 50 slash commands per second
- 5 requests per second per channel

**Ergors Engine**:

- No built-in rate limits
- Use Discord's rate limiting

### Resource Usage

**Memory**:

- Base: ~100MB (bot framework + connection)
- Per-session: ~1KB (session metadata)
- RAG cache: Variable (depends on document size)

**Network**:

- Idle: Minimal (WebSocket heartbeat)
- Active: Depends on LLM provider latency
- Document ingestion: Depends on repository size

## Development

### Build Requirements

**Features**:

```bash
# Discord gateway (required)
--features discord

# GitHub ingestion (optional but recommended)
--features github-ingest

# RLM support (optional)
--features rlm
```

**Build Command**:

```bash
cargo build --release -p ergors --features discord,github-ingest,rlm
```

### Testing

**Test Mode**:

```bash
export ERGORS_GATEWAY_TEST_MODE=1
ergors gateway start discord
```

**Manual Testing**:

1. Invite bot to test server
2. Configure admin role
3. Test each slash command
4. Verify responses and error handling

**Integration Tests**:

- E2E tests: `tests/e2e/tests/gateway.sh` (if exists)
- Unit tests: `packages/ergors/src/gateway/discord.rs`

## Deployment

### Production Checklist

- [ ] Bot token configured and encrypted
- [ ] Guild allowlist configured (if restricting access)
- [ ] LLM providers configured
- [ ] RAG configuration set globally
- [ ] Admin roles configured per guild
- [ ] Test mode disabled (`unset ERGORS_GATEWAY_TEST_MODE`)
- [ ] Logs monitored
- [ ] Resource usage monitored

### Monitoring

**Key Metrics**:

- Message processing latency
- LLM provider success rate
- RAG query performance
- Document ingestion success rate
- Session creation rate

**Health Checks**:

- Bot online status in Discord
- Gateway status: `ergors gateway status discord`
- Recent logs: `tail -f ~/.ergors/logs/gateway-discord.log`

## Future Enhancements

### Planned Features

- [ ] Message reactions for feedback
- [ ] Thread summarization
- [ ] Voice channel integration
- [ ] Scheduled document refresh
- [ ] Multi-guild document sharing
- [ ] Advanced RLM modes (research, analysis)

### API Extensions

- [ ] Custom slash command registration
- [ ] Webhook support for external triggers
- [ ] REST API for programmatic control
- [ ] GraphQL query interface

## References

### Internal Documentation

- [Gateway Architecture](../architecture/gateway.md)
- [LLM Routing](../llm-routing.md)
- [RAG Setup](../rag-setup.md)
- [RLM Configuration](../rlm-config.md)

### External Dependencies

- **Poise**: Discord bot framework - <https://github.com/serenity-rs/poise>
- **Serenity**: Discord API library - <https://github.com/serenity-rs/serenity>
- **githem**: GitHub repository ingestion - <https://github.com/commonwarexyz/githem>
- **ergors-rag**: RAG implementation - `packages/ergors-rag/`

### Discord Resources

- [Discord Developer Portal](https://discord.com/developers/docs)
- [Slash Commands Guide](https://discord.com/developers/docs/interactions/slash-commands)
- [Gateway Intents](https://discord.com/developers/docs/topics/gateway#gateway-intents)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2024-01-15 | Initial implementation |
| 0.2.0 | 2024-02-10 | Added RLM support |
| 0.3.0 | 2024-02-10 | Added test mode |
| 0.3.1 | 2024-02-10 | Test mode now performs real document ingestion |
