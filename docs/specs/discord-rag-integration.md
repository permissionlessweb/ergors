# Discord Gateway RAG Integration

## Goal

Enable per-guild RAG (Retrieval-Augmented Generation) for the Discord gateway:

1. Each Discord guild gets its own isolated embedding namespace
2. Guild admins (specific role) can ingest documents from URLs
3. User prompts automatically retrieve relevant context before LLM call
4. Zero human intervention - storage locations derived from guild ID

---

## Design Principles

1. **No modifications to ergors-rag crate** - Use URI convention for namespace isolation
2. **Role-based access control** - Discord role ID stored in config, checked at runtime
3. **Automatic context injection** - RAG retrieval happens transparently before LLM routing
4. **Audit trail** - Log all ingestion operations with guild/user/source metadata

---

## Namespace Convention

### Storage Key Structure

```
# Chunk storage (existing prefix, namespaced by URI)
rag_chunks/{chunk_id}

# Source index (existing prefix, namespaced by URI)
rag_source_index/discord:guild_{guild_id}/{source_identifier}

# Guild RAG config (new)
gateway/rag_config/{guild_id}
```

### Document URI Convention

All documents ingested via Discord use this URI format:

```
discord:guild_{guild_id}/{source_type}:{identifier}

Examples:
discord:guild_123456789/url:https://docs.example.com/api.md
discord:guild_123456789/file:project-readme.md
discord:guild_123456789/paste:2024-02-01-abc123
```

This enables:

- Query filtering via `source_uri_prefix: "discord:guild_{guild_id}/"`
- Source identification and deletion by guild
- No collision between guilds

---

## Proto Definitions

### File: `proto/ergors/gateway/v1/gateway.proto` (UPDATE)

```protobuf
// Guild-specific RAG configuration
message GuildRagConfig {
  string guild_id = 1;

  // Role ID that can perform ingestion (empty = guild owner only)
  string admin_role_id = 2;

  // Whether RAG context is automatically injected into prompts
  bool auto_context_enabled = 3;

  // Maximum chunks to retrieve for context (default: 3)
  uint32 max_context_chunks = 4;

  // Minimum similarity threshold (default: 0.5)
  float min_similarity = 5;

  // Optional: dedicated embedder endpoint (overrides global)
  string embedder_endpoint = 6;

  // Stats
  uint32 total_documents = 7;
  uint32 total_chunks = 8;
  int64 last_ingestion_at = 9;
}

// Ingest request from Discord
message DiscordRagIngestRequest {
  string guild_id = 1;
  string user_id = 2;

  // Source type: "url" or "content"
  string source_type = 3;

  // For source_type="url": the URL to fetch
  // For source_type="content": ignored
  string url = 4;

  // For source_type="content": the raw content
  // For source_type="url": optional, overrides fetched content
  string content = 5;

  // Human-readable label for this source
  string label = 6;

  // Document type hint (markdown, text, code, etc.)
  string doc_type = 7;
}

message DiscordRagIngestResponse {
  bool success = 1;
  uint32 chunk_count = 2;
  string source_uri = 3;
  string message = 4;
}

// Query context injection result (internal)
message RagContextResult {
  repeated RagContextChunk chunks = 1;
  string guild_id = 2;
  float query_time_ms = 3;
}

message RagContextChunk {
  string content = 1;
  string source_uri = 2;
  float similarity = 3;
}
```

---

## Implementation

### Phase 1: Storage & Config

#### 1.1 Add guild RAG config to storage

**File:** `packages/cw-ho/src/storage.rs`

```rust
const GUILD_RAG_CONFIG_PREFIX: &str = "gateway/rag_config";

impl ErgorsStorage {
    /// Get RAG config for a Discord guild.
    pub async fn get_guild_rag_config(&self, guild_id: &str) -> HoResult<Option<GuildRagConfig>> {
        let key = storage_key(GUILD_RAG_CONFIG_PREFIX, guild_id);
        // ... standard get_raw + deserialize
    }

    /// Store RAG config for a Discord guild.
    pub async fn put_guild_rag_config(&self, config: &GuildRagConfig) -> HoResult<()> {
        let key = storage_key(GUILD_RAG_CONFIG_PREFIX, &config.guild_id);
        // ... standard serialize + put_raw
    }

    /// List all guild RAG configs (for admin).
    pub async fn list_guild_rag_configs(&self) -> HoResult<Vec<GuildRagConfig>> {
        // ... prefix_raw scan
    }

    /// Delete all RAG data for a guild (cleanup).
    pub async fn delete_guild_rag_data(&self, guild_id: &str) -> HoResult<u32> {
        // Delete by source_uri_prefix: "discord:guild_{guild_id}/"
        // Returns count of deleted chunks
    }
}
```

### Phase 2: Discord Slash Commands

#### 2.1 Add RAG admin commands

**File:** `packages/cw-ho/src/gateway/discord.rs`

```rust
/// Ingest a URL into this guild's knowledge base (admin only)
#[poise::command(slash_command, guild_only)]
async fn ingest(
    ctx: Context<'_>,
    #[description = "URL to fetch and ingest"] url: String,
    #[description = "Label for this source"] label: Option<String>,
    #[description = "Document type (markdown, text, code)"] doc_type: Option<String>,
) -> Result<(), anyhow::Error> {
    // 1. Check guild authorization (existing)
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    // 2. Check RAG admin role
    check_rag_admin_role(&ctx).await?;

    // 3. Defer (ingestion can be slow)
    ctx.defer().await?;

    // 4. Fetch URL content
    let content = fetch_url_content(&url).await?;

    // 5. Build source URI
    let guild_id = ctx.guild_id().unwrap().to_string();
    let source_uri = format!("discord:guild_{}/url:{}", guild_id, url);

    // 6. Ingest via RAG
    let result = ctx.data().rag_ingest_for_guild(
        &guild_id,
        &content,
        &source_uri,
        &doc_type.unwrap_or_else(|| detect_doc_type(&url)),
        vec![format!("guild:{}", guild_id), format!("user:{}", ctx.author().id)],
    ).await?;

    // 7. Report result
    if result.success {
        ctx.say(format!(
            "Ingested {} chunks from `{}`\nSource: `{}`",
            result.chunk_count,
            label.unwrap_or_else(|| url.clone()),
            result.source_uri
        )).await?;
    } else {
        ctx.say(format!("Failed to ingest: {}", result.message)).await?;
    }

    Ok(())
}

/// Configure RAG settings for this guild (admin only)
#[poise::command(slash_command, guild_only)]
async fn ragconfig(
    ctx: Context<'_>,
    #[description = "Role that can ingest documents"] admin_role: Option<serenity::Role>,
    #[description = "Auto-inject context into prompts"] auto_context: Option<bool>,
    #[description = "Max context chunks (1-10)"] max_chunks: Option<u32>,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;
    check_guild_owner_or_admin(&ctx).await?;  // Only owner/admin can change config

    let guild_id = ctx.guild_id().unwrap().to_string();

    // Get or create config
    let mut config = ctx.data().storage
        .get_guild_rag_config(&guild_id)
        .await?
        .unwrap_or_else(|| GuildRagConfig {
            guild_id: guild_id.clone(),
            auto_context_enabled: true,
            max_context_chunks: 3,
            min_similarity: 0.5,
            ..Default::default()
        });

    // Apply updates
    if let Some(role) = admin_role {
        config.admin_role_id = role.id.to_string();
    }
    if let Some(auto) = auto_context {
        config.auto_context_enabled = auto;
    }
    if let Some(max) = max_chunks {
        config.max_context_chunks = max.clamp(1, 10);
    }

    ctx.data().storage.put_guild_rag_config(&config).await?;

    ctx.say(format!(
        "RAG config updated:\n\
         - Admin role: {}\n\
         - Auto-context: {}\n\
         - Max chunks: {}",
        if config.admin_role_id.is_empty() { "guild owner only" } else { &config.admin_role_id },
        config.auto_context_enabled,
        config.max_context_chunks
    )).await?;

    Ok(())
}

/// List ingested sources for this guild
#[poise::command(slash_command, guild_only)]
async fn ragsources(ctx: Context<'_>) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let prefix = format!("discord:guild_{}/", guild_id);

    let sources = ctx.data().storage.list_rag_sources_by_prefix(&prefix, 20).await?;

    if sources.is_empty() {
        ctx.say("No documents ingested yet.\nUse `/ingest <url>` to add documents.").await?;
    } else {
        let mut msg = format!("**Ingested Sources** ({} total)\n", sources.len());
        for src in sources.iter().take(10) {
            msg.push_str(&format!("- `{}` ({} chunks)\n", src.uri, src.chunk_count));
        }
        if sources.len() > 10 {
            msg.push_str(&format!("... and {} more", sources.len() - 10));
        }
        ctx.say(msg).await?;
    }

    Ok(())
}

/// Delete a source from this guild's knowledge base (admin only)
#[poise::command(slash_command, guild_only)]
async fn ragdelete(
    ctx: Context<'_>,
    #[description = "Source URI to delete"] source_uri: String,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;
    check_rag_admin_role(&ctx).await?;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let expected_prefix = format!("discord:guild_{}/", guild_id);

    // Security: only allow deleting sources belonging to this guild
    if !source_uri.starts_with(&expected_prefix) {
        return Err(anyhow::anyhow!("Can only delete sources from this guild"));
    }

    let deleted = ctx.data().storage.delete_rag_source(&source_uri).await?;
    ctx.say(format!("Deleted {} chunks from `{}`", deleted, source_uri)).await?;

    Ok(())
}
```

#### 2.2 Role checking helper

```rust
/// Check if user has RAG admin role for this guild.
async fn check_rag_admin_role(ctx: &Context<'_>) -> Result<(), anyhow::Error> {
    let guild_id = ctx.guild_id()
        .ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;

    // Get guild config
    let config = ctx.data().storage
        .get_guild_rag_config(&guild_id.to_string())
        .await?;

    // If no config or no admin role, require guild owner
    let required_role_id = match config {
        Some(c) if !c.admin_role_id.is_empty() => c.admin_role_id,
        _ => {
            // No admin role configured - check if user is guild owner
            let guild = guild_id.to_partial_guild(ctx.http()).await?;
            if guild.owner_id == ctx.author().id {
                return Ok(());
            }
            return Err(anyhow::anyhow!(
                "RAG admin role not configured. Only guild owner can ingest.\n\
                 Use `/ragconfig admin_role:@YourRole` to set an admin role."
            ));
        }
    };

    // Check if user has the required role
    let member = ctx.author_member().await
        .ok_or_else(|| anyhow::anyhow!("Could not get member info"))?;

    let role_id: u64 = required_role_id.parse()
        .map_err(|_| anyhow::anyhow!("Invalid role ID in config"))?;

    if member.roles.contains(&serenity::RoleId::new(role_id)) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("You need the RAG admin role to perform this action"))
    }
}
```

### Phase 3: Context Injection

#### 3.1 Modify prompt handler to inject RAG context

**File:** `packages/cw-ho/src/gateway/discord.rs`

Update the `prompt` slash command:

```rust
#[poise::command(slash_command, guild_only)]
async fn prompt(
    ctx: Context<'_>,
    #[description = "Your message to the AI"] message: String,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let thread_id = ctx.channel_id().to_string();
    let user_id = ctx.author().id.to_string();

    // Get session
    let session_id = ctx.data().storage
        .get_or_create_gateway_session("discord", &thread_id)
        .await?;

    // === RAG CONTEXT INJECTION ===
    let rag_context = retrieve_guild_rag_context(
        ctx.data(),
        &guild_id,
        &message,
    ).await;

    // Build prompt with context
    let augmented_prompt = if let Some(ref rag) = rag_context {
        if !rag.chunks.is_empty() {
            format!(
                "Use the following context to help answer the question.\n\n\
                 ## Context\n{}\n\n\
                 ## Question\n{}",
                rag.chunks.iter()
                    .map(|c| format!("Source: {}\n{}", c.source_uri, c.content))
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n"),
                message
            )
        } else {
            message.clone()
        }
    } else {
        message.clone()
    };

    // Build prompt request
    let prompt_req = PromptRequest {
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: augmented_prompt,
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

    // Call LLM
    let response = ctx.data().router.handle_request(&prompt_req, "default").await?;

    // Format response with RAG attribution
    let mut response_content = response.response.join("\n");

    if let Some(ref rag) = rag_context {
        if !rag.chunks.is_empty() {
            response_content.push_str("\n\n---\n*Sources used:*");
            for chunk in &rag.chunks {
                // Extract readable source name from URI
                let source_name = chunk.source_uri
                    .split('/')
                    .last()
                    .unwrap_or(&chunk.source_uri);
                response_content.push_str(&format!("\n- {}", source_name));
            }
        }
    }

    // Truncate if needed (TODO: chunk properly)
    if response_content.len() > 1990 {
        response_content = format!("{}...", &response_content[..1990]);
    }

    ctx.say(&response_content).await?;
    Ok(())
}

/// Retrieve RAG context for a guild's prompt.
async fn retrieve_guild_rag_context(
    data: &DiscordData,
    guild_id: &str,
    query: &str,
) -> Option<RagContextResult> {
    // Get guild config
    let config = data.storage.get_guild_rag_config(guild_id).await.ok()??;

    // Check if auto-context is enabled
    if !config.auto_context_enabled {
        return None;
    }

    // Get global RAG config for embedder
    let rag_config = data.storage.get_rag_config().await.ok()??;

    // Create RAG instance
    let rag = crate::rag::new_remote(
        &data.storage,
        &rag_config.endpoint,
        &rag_config.model,
        rag_config.dimension as usize,
    ).ok()?;

    // Query with guild filter
    let start = std::time::Instant::now();
    let options = ergors_rag::QueryOptions {
        filters: Some(ergors_rag::MetadataFilters {
            source_uri_prefix: Some(format!("discord:guild_{}/", guild_id)),
            ..Default::default()
        }),
        ..Default::default()
    };

    let results = rag.query(
        query,
        config.max_context_chunks as usize,
        options,
    ).await.ok()?;

    // Filter by similarity threshold
    let chunks: Vec<RagContextChunk> = results.iter()
        .filter(|r| r.similarity >= config.min_similarity)
        .map(|r| RagContextChunk {
            content: r.chunk.content.clone(),
            source_uri: r.chunk.source_uri.clone().unwrap_or_default(),
            similarity: r.similarity,
        })
        .collect();

    Some(RagContextResult {
        chunks,
        guild_id: guild_id.to_string(),
        query_time_ms: start.elapsed().as_secs_f32() * 1000.0,
    })
}
```

### Phase 4: URL Fetching

#### 4.1 URL content fetcher

```rust
/// Fetch content from a URL for ingestion.
async fn fetch_url_content(url: &str) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("ERGORS-Bot/1.0")
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to fetch URL: HTTP {}",
            response.status()
        ));
    }

    // Check content type
    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain");

    if content_type.contains("text/html") {
        // Convert HTML to markdown for better chunking
        let html = response.text().await?;
        Ok(html2text::from_read(html.as_bytes(), 80))
    } else {
        // Plain text, markdown, etc.
        Ok(response.text().await?)
    }
}

/// Detect document type from URL.
fn detect_doc_type(url: &str) -> String {
    if url.ends_with(".md") || url.contains("readme") {
        "markdown".to_string()
    } else if url.ends_with(".rs") || url.ends_with(".py") || url.ends_with(".js") {
        "code".to_string()
    } else if url.ends_with(".json") || url.ends_with(".yaml") || url.ends_with(".toml") {
        "config".to_string()
    } else {
        "text".to_string()
    }
}
```

---

## Command Reference

### User Commands

| Command | Description | Access |
|---------|-------------|--------|
| `/prompt <message>` | Send prompt (auto-includes RAG context) | All users |
| `/ragsources` | List ingested sources for this guild | All users |

### Admin Commands

| Command | Description | Access |
|---------|-------------|--------|
| `/ingest <url>` | Fetch and ingest URL content | RAG admin role |
| `/ragconfig` | Configure RAG settings | Guild owner |
| `/ragdelete <uri>` | Delete ingested source | RAG admin role |

---

## Storage Layout

```
gateway/rag_config/
├── 123456789012345678          # GuildRagConfig (JSON)
├── 234567890123456789
└── ...

rag_chunks/
├── {chunk_uuid_1}              # VerifiableChunk (includes source_uri)
├── {chunk_uuid_2}
└── ...

rag_source_index/
├── discord:guild_123456789/url:https://...  -> [chunk_ids]
├── discord:guild_123456789/url:https://...  -> [chunk_ids]
└── ...
```

---

## Security Considerations

1. **Namespace isolation** - Guilds can only query/delete their own namespace (enforced by URI prefix)
2. **Role-based access** - Only designated role can ingest documents
3. **URL validation** - Validate URLs before fetching (no internal/localhost)
4. **Size limits** - Cap fetched content at 1MB, chunk count at 100
5. **Rate limiting** - Max ingestions per guild per hour (configurable)
6. **Audit logging** - All ingestion operations logged with user/guild/source

---

## Dependencies

```toml
# packages/cw-ho/Cargo.toml

[dependencies]
reqwest = { version = "0.11", features = ["json"], optional = true }
html2text = { version = "0.6", optional = true }

[features]
discord = ["serenity", "poise", "reqwest", "html2text"]
```

---

## Implementation Order

### Phase 1: Foundation

1. Add `GuildRagConfig` to proto
2. Add guild RAG config storage methods
3. Regenerate proto types

### Phase 2: Admin Commands

4. Add `/ragconfig` command
2. Add `/ingest` command with URL fetching
3. Add `/ragsources` and `/ragdelete` commands
4. Implement role checking

### Phase 3: Context Injection

8. Add `retrieve_guild_rag_context()` function
2. Modify `/prompt` to inject context
3. Add source attribution to responses

### Phase 4: Polish

11. Add rate limiting
2. Add audit logging
3. Update CLI_REFERENCE.md
4. Integration testing

---

## Verification

```bash
# Build
cargo chec -p ergors --features discord

# Test flow (in Discord)
/ragconfig admin_role:@Moderators auto_context:true
/ingest url:https://docs.example.com/api.md label:"API Docs"
/ragsources
/prompt "How do I authenticate with the API?"
# -> Response should reference API Docs content
/ragdelete source_uri:discord:guild_xxx/url:https://docs.example.com/api.md
```
