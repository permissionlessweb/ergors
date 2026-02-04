//! Discord Gateway Implementation
//!
//! Provides Discord bot integration using Poise/Serenity framework.
//! Supports slash commands for AI interaction with per-thread session tracking.
//! Includes per-guild RAG (Retrieval-Augmented Generation) for context injection.

use crate::gateway::crypto::decrypt_gateway_secret;
use crate::storage::ErgorsStorage;
use anyhow::Result;
use async_trait::async_trait;
use ho_std::{
    llm::LlmRouter,
    traits::gateway::{GatewayContext, GatewayEvent, GatewayModule},
    types::ergors::{
        gateway::v1::{DiscordGatewayConfig, GatewayResponse, GuildRagConfig},
        orch::v1::{PromptContext, PromptMessage, PromptRequest},
    },
};
use poise::serenity_prelude as serenity;
use poise::{Framework, FrameworkOptions};
use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

// ============ CONSTANTS ============

/// Maximum content size for URL ingestion (1MB)
const MAX_INGEST_BYTES: u64 = 1_000_000;

/// Maximum context chunks that can be retrieved (hard limit)
const MAX_CONTEXT_CHUNKS_LIMIT: u32 = 10;

/// HTTP request timeout for URL fetching
const URL_FETCH_TIMEOUT_SECS: u64 = 30;

/// Discord message character limit (with buffer for safety)
const DISCORD_MSG_LIMIT: usize = 1990;

/// Maximum characters for display name truncation
const SOURCE_DISPLAY_NAME_LEN: usize = 50;

/// Maximum characters for source list display (longer for /ragsources)
const SOURCE_LIST_DISPLAY_LEN: usize = 60;

/// html2text line width for rendering
const HTML2TEXT_LINE_WIDTH: usize = 100;

/// Allowed content types for ingestion
const ALLOWED_CONTENT_TYPES: &[&str] = &[
    "text/plain",
    "text/html",
    "text/markdown",
    "text/x-markdown",
    "text/css",
    "text/csv",
    "text/xml",
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-yaml",
    "application/yaml",
];

// ============ INTERNAL TYPES (not wire format, no proto needed) ============

/// RAG context retrieved for a prompt (internal only)
#[derive(Debug, Clone)]
struct RagContext {
    chunks: Vec<RagChunk>,
    guild_id: String,
    query_time_ms: f32,
}

/// Single chunk of RAG context (internal only)
#[derive(Debug, Clone)]
struct RagChunk {
    content: String,
    source_uri: String,
    similarity: f32,
}

/// Shared data for all Poise commands
pub struct DiscordData {
    pub storage: Arc<ErgorsStorage>,
    pub router: Arc<LlmRouter>,
    pub allowed_guild_ids: Vec<String>,
    pub event_tx: mpsc::UnboundedSender<GatewayEvent>,
    /// Shared HTTP client for URL fetching (redirects disabled for SSRF protection)
    pub http_client: reqwest::Client,
    /// Shared HTTP client for RAG embedding API calls (redirects allowed for trusted endpoints)
    pub rag_client: reqwest::Client,
    /// RLM service for agentic document exploration (optional)
    #[cfg(feature = "rlm")]
    pub rlm_service: Option<Arc<ergors_rlm::RlmService>>,
}

pub(crate) type Context<'a> = poise::Context<'a, DiscordData, anyhow::Error>;

/// Discord gateway implementation using Poise framework
pub struct DiscordGateway {
    config: Arc<RwLock<DiscordGatewayConfig>>,
    http: Arc<RwLock<Option<Arc<serenity::Http>>>>,
    connected: Arc<AtomicBool>,
}

impl DiscordGateway {
    /// Create a new Discord gateway with configuration
    pub fn new(config: DiscordGatewayConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            http: Arc::new(RwLock::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Load Discord gateway from storage.
    /// If bot_token is encrypted, pass node_pubkey to decrypt it.
    pub async fn from_storage(storage: &ErgorsStorage, node_pubkey: Option<&[u8]>) -> Result<Self> {
        // Try to load Discord-specific config
        if let Some(config) = storage.get_gateway_config("discord").await? {
            // Check if token is encrypted
            let bot_token = if config.settings.get("bot_token_encrypted").map(|s| s == "true").unwrap_or(false) {
                // Load encrypted token
                if let Some(pubkey) = node_pubkey {
                    match storage.get_encrypted_secret("discord_bot_token", "discord_gateway", "startup").await {
                        Ok(Some(secret)) => {
                            match decrypt_gateway_secret(&secret.encrypted_value, &secret.nonce, pubkey)
                                .map_err(|e| anyhow::anyhow!(e))
                            {
                                Ok(token) => {
                                    info!("Decrypted Discord bot token from secure storage");
                                    token
                                }
                                Err(e) => {
                                    error!("Failed to decrypt Discord bot token: {}", e);
                                    String::new()
                                }
                            }
                        }
                        Ok(None) => {
                            warn!("Discord token marked as encrypted but not found in secure storage");
                            String::new()
                        }
                        Err(e) => {
                            error!("Failed to load encrypted Discord token: {}", e);
                            String::new()
                        }
                    }
                } else {
                    warn!("Node pubkey not provided - cannot decrypt Discord token");
                    String::new()
                }
            } else {
                // Backward compatibility: load plaintext token
                let token = config.settings.get("bot_token").cloned().unwrap_or_default();
                if !token.is_empty() {
                    warn!("Loading plaintext Discord token - please reconfigure with encryption for improved security");
                }
                token
            };

            // Parse Discord config from settings
            let discord_config = DiscordGatewayConfig {
                bot_token,
                allowed_guild_ids: config
                    .settings
                    .get("allowed_guild_ids")
                    .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                    .unwrap_or_default(),
                allowed_channel_ids: config
                    .settings
                    .get("allowed_channel_ids")
                    .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                    .unwrap_or_default(),
                command_prefix: config
                    .settings
                    .get("command_prefix")
                    .cloned()
                    .unwrap_or_else(|| "!".to_string()),
                respond_to_mentions: config
                    .settings
                    .get("respond_to_mentions")
                    .map(|s| s == "true")
                    .unwrap_or(true),
                respond_to_dms: config
                    .settings
                    .get("respond_to_dms")
                    .map(|s| s == "true")
                    .unwrap_or(false),
            };
            Ok(Self::new(discord_config))
        } else {
            Ok(Self::new(DiscordGatewayConfig::default()))
        }
    }
}

#[async_trait]
impl GatewayModule<LlmRouter, ErgorsStorage> for DiscordGateway {
    fn gateway_id(&self) -> &str {
        "discord"
    }

    fn name(&self) -> &str {
        "Discord Bot"
    }

    async fn start(&self, ctx: GatewayContext<LlmRouter, ErgorsStorage>) -> ho_std::error::HoResult<()> {
        let config = self.config.read().await;

        if config.bot_token.is_empty() {
            return Err(ho_std::error::HoError::Cfg(
                "Discord bot token not configured".to_string(),
            ));
        }

        let intents = serenity::GatewayIntents::GUILDS | serenity::GatewayIntents::GUILD_MESSAGES;

        let storage = ctx.storage.clone();
        let router = ctx.router.clone();
        let allowed_guild_ids = config.allowed_guild_ids.clone();
        let event_tx = ctx.event_tx.clone();

        // Create shared HTTP client for URL fetching (reuses TCP connections)
        // SECURITY: Disable redirects to prevent SSRF via redirect chains
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(URL_FETCH_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("ERGORS-Bot/1.0 (+https://github.com/ho-rs/ergors)")
            .build()
            .map_err(|e| ho_std::error::HoError::Other(format!("Failed to create HTTP client: {}", e)))?;

        // Create shared HTTP client for RAG embedding API (trusted endpoints)
        let rag_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60)) // Embeddings can be slow
            .user_agent("ERGORS-RAG/1.0")
            .build()
            .map_err(|e| ho_std::error::HoError::Other(format!("Failed to create RAG client: {}", e)))?;

        // Initialize RLM service if feature is enabled
        #[cfg(feature = "rlm")]
        let rlm_service = {
            match ergors_rlm::RlmService::new(2, router.clone()).await {
                Ok(service) => {
                    info!("RLM service initialized successfully");
                    Some(Arc::new(service))
                }
                Err(e) => {
                    warn!("Failed to initialize RLM service: {}. RLM mode will be unavailable.", e);
                    None
                }
            }
        };

        let data = DiscordData {
            storage,
            router,
            allowed_guild_ids,
            event_tx,
            http_client,
            rag_client,
            #[cfg(feature = "rlm")]
            rlm_service,
        };

        let framework = Framework::builder()
            .options(FrameworkOptions {
                commands: vec![
                    prompt(),
                    thread(),
                    clear(),
                    // RAG commands
                    ingest(),
                    ragconfig(),
                    ragsources(),
                    ragdelete(),
                    // RLM commands
                    rlmconfig(),
                ],
                ..Default::default()
            })
            .setup(|ctx, _ready, framework| {
                Box::pin(async move {
                    // Register slash commands globally
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    info!("Discord slash commands registered");
                    Ok(data)
                })
            })
            .build();

        let bot_token = config.bot_token.clone();
        drop(config); // Release lock before spawning

        let client = serenity::ClientBuilder::new(&bot_token, intents)
            .framework(framework)
            .await
            .map_err(|e| ho_std::error::HoError::ChannelError(format!("Discord client error: {}", e)))?;

        // Store HTTP client for send_response
        *self.http.write().await = Some(client.http.clone());

        // Spawn client in background
        let connected = Arc::clone(&self.connected);
        tokio::spawn(async move {
            connected.store(true, Ordering::SeqCst);
            let mut client = client;
            if let Err(e) = client.start().await {
                error!("Discord client error: {}", e);
                connected.store(false, Ordering::SeqCst);
            }
        });

        info!("Discord gateway started");
        Ok(())
    }

    async fn stop(&self) -> ho_std::error::HoResult<()> {
        self.connected.store(false, Ordering::SeqCst);
        *self.http.write().await = None;
        info!("Discord gateway stopped");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn send_response(&self, response: GatewayResponse) -> ho_std::error::HoResult<()> {
        let http = self.http.read().await;
        let http = http.as_ref().ok_or_else(|| {
            ho_std::error::HoError::ChannelError("Discord client not started".to_string())
        })?;

        let channel_id: u64 = response
            .channel_id
            .parse()
            .map_err(|e| ho_std::error::HoError::Other(format!("Invalid channel ID: {}", e)))?;
        let channel = serenity::ChannelId::new(channel_id);

        let mut message = serenity::CreateMessage::new().content(&response.content);

        // Add reply reference if provided
        if !response.reply_to_id.is_empty() {
            let msg_id: u64 = response.reply_to_id.parse().map_err(|e| {
                ho_std::error::HoError::Other(format!("Invalid message ID: {}", e))
            })?;
            message = message.reference_message((channel, serenity::MessageId::new(msg_id)));
        }

        let _ = channel
            .send_message(http.as_ref(), message)
            .await
            .map_err(|e| ho_std::error::HoError::ChannelError(format!("Failed to send message: {}", e)))?;

        Ok(())
    }
}

// ============ SLASH COMMANDS ============

/// Check if guild is authorized (empty whitelist = all allowed)
fn check_guild_authorization(data: &DiscordData, guild_id: Option<serenity::GuildId>) -> Result<(), anyhow::Error> {
    // Empty whitelist means all guilds allowed
    if data.allowed_guild_ids.is_empty() {
        return Ok(());
    }

    let guild_id = guild_id.ok_or_else(|| anyhow::anyhow!("Command must be used in a guild"))?;
    let guild_id_str = guild_id.to_string();

    if data.allowed_guild_ids.contains(&guild_id_str) {
        Ok(())
    } else {
        warn!("Unauthorized guild attempted access: {}", guild_id_str);
        Err(anyhow::anyhow!("This bot is not authorized for this server"))
    }
}

/// Send a prompt to the AI (with optional RAG context injection)
#[poise::command(slash_command, guild_only)]
async fn prompt(
    ctx: Context<'_>,
    #[description = "Your message to the AI"] message: String,
) -> Result<(), anyhow::Error> {
    // Check guild authorization
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    // Defer response - Discord gives us 15 minutes after this
    ctx.defer().await?;

    // Get or create session for this thread/channel
    let guild_id = ctx.guild_id().unwrap().to_string();
    let thread_id = ctx.channel_id().to_string();
    let user_id = ctx.author().id.to_string();
    let session_id = ctx
        .data()
        .storage
        .get_or_create_gateway_session("discord", &thread_id)
        .await?;

    // === UNIFIED CONTEXT INJECTION (RAG or RLM) ===
    let context_result = retrieve_guild_context(ctx.data(), &guild_id, &message).await;

    let response_content = match context_result {
        Some(ContextResult::FinalAnswer(answer)) => {
            // RLM returned final answer - skip LLM call, return directly
            answer
        }
        other => {
            // Get content for LLM (augmented prompt or raw message)
            let content = match other {
                Some(ContextResult::AugmentedPrompt(p)) => p,
                None => message.clone(),
                _ => unreachable!(),
            };

            // Build prompt request for LLM
            let prompt_req = PromptRequest {
                messages: vec![PromptMessage {
                    role: "user".to_string(),
                    content,
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

            // Call LLM router
            let response = match ctx.data().router.handle_request(&prompt_req, "default").await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("LLM router error: {}", e);
                    ctx.say(format!("Error: {}", e)).await?;
                    return Ok(());
                }
            };

            // Join response parts
            response.response.join("")
        }
    };

    // Discord message limit - chunk if needed
    if response_content.len() <= DISCORD_MSG_LIMIT {
        ctx.say(&response_content).await?;
    } else {
        // Send chunked response
        send_chunked_response(&ctx, &response_content).await?;
    }

    // Notify manager for metrics tracking (fire and forget)
    let _ = ctx.data().event_tx.send(GatewayEvent::MessageProcessed {
        gateway_id: "discord".to_string(),
        session_id,
        user_id,
    });

    Ok(())
}

/// Create a new conversation thread
#[poise::command(slash_command, guild_only)]
async fn thread(
    ctx: Context<'_>,
    #[description = "Thread name"] name: Option<String>,
) -> Result<(), anyhow::Error> {
    // Check guild authorization
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    let thread_name = name.unwrap_or_else(|| format!("AI Chat - {}", ctx.author().name));

    // Create Discord thread
    let thread = ctx
        .channel_id()
        .create_thread(
            ctx.http(),
            serenity::CreateThread::new(thread_name.clone())
                .kind(serenity::ChannelType::PublicThread),
        )
        .await?;

    // Create new session for this thread
    let session_id = ctx
        .data()
        .storage
        .create_gateway_session("discord", &thread.id.to_string())
        .await?;

    ctx.say(format!(
        "Created thread: <#{}>\nSession: `{}`",
        thread.id, session_id
    ))
    .await?;
    Ok(())
}

/// Clear conversation history in current thread
#[poise::command(slash_command, guild_only)]
async fn clear(ctx: Context<'_>) -> Result<(), anyhow::Error> {
    // Check guild authorization
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    let thread_id = ctx.channel_id().to_string();

    // Create fresh session
    let session_id = ctx
        .data()
        .storage
        .create_gateway_session("discord", &thread_id)
        .await?;

    ctx.say(format!("Session cleared. New session: `{}`", session_id))
        .await?;
    Ok(())
}

// ============ RAG COMMANDS ============

/// Ingest a URL into this guild's knowledge base (admin only)
#[poise::command(slash_command, guild_only)]
async fn ingest(
    ctx: Context<'_>,
    #[description = "URL to fetch and ingest"] url: String,
    #[description = "Label for this source (optional)"] label: Option<String>,
    #[description = "Document type (markdown, text, code)"] doc_type: Option<String>,
) -> Result<(), anyhow::Error> {
    // Check guild authorization
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    // Check RAG admin role
    check_rag_admin_role(&ctx).await?;

    // Defer - ingestion can take time
    ctx.defer().await?;

    // Dispatch to GitHub ingestion if URL is a GitHub repo
    #[cfg(feature = "github-ingest")]
    {
        if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
            return crate::gateway::github_ingest::ingest_github_repo(&ctx, &url, label, doc_type)
                .await;
        }
    }

    let guild_id = ctx.guild_id().unwrap().to_string();
    let user_id = ctx.author().id.to_string();

    // Validate URL with SSRF protection - returns resolved IP to prevent DNS rebinding
    let validated_url = match validate_ingest_url(&url) {
        Ok(v) => v,
        Err(e) => {
            ctx.say(format!("Invalid URL: {}", e)).await?;
            return Ok(());
        }
    };

    // Fetch URL content using validated IP (prevents DNS rebinding)
    let content = match fetch_url_content(&ctx.data().http_client, &validated_url).await {
        Ok(c) => c,
        Err(e) => {
            ctx.say(format!("Failed to fetch URL: {}", e)).await?;
            log_rag_audit(
                &ctx.data().storage,
                &guild_id,
                &user_id,
                "ingest",
                &url,
                false,
                &e.to_string(),
            ).await;
            return Ok(());
        }
    };

    // Build source URI with guild namespace
    let source_uri = format!("discord:guild_{}/url:{}", guild_id, url);
    let detected_type = doc_type.unwrap_or_else(|| detect_doc_type(&url));

    // Check if RAG is configured globally
    let rag_config = match ctx.data().storage.get_rag_config().await {
        Ok(Some(config)) => config,
        Ok(None) => {
            ctx.say("RAG not configured. Ask the bot admin to run `ergors rag configure`.").await?;
            return Ok(());
        }
        Err(e) => {
            ctx.say(format!("Error checking RAG config: {}", e)).await?;
            return Ok(());
        }
    };

    // Create RAG instance with shared HTTP client for connection pooling
    let rag = match crate::rag::new_remote_with_client(
        &ctx.data().storage,
        ctx.data().rag_client.clone(),
        &rag_config.endpoint,
        &rag_config.model,
        rag_config.dimension as usize,
    ) {
        Ok(r) => r,
        Err(e) => {
            ctx.say(format!("Failed to initialize RAG: {}", e)).await?;
            return Ok(());
        }
    };

    let doc = ergors_rag::Document {
        content,
        uri: source_uri.clone(),
        doc_type: detected_type.clone(),
        tags: vec![format!("guild:{}", guild_id), format!("user:{}", user_id)],
    };

    match rag.ingest(doc, None).await {
        Ok(chunk_ids) => {
            let chunk_count = chunk_ids.len();

            // Update guild RAG config stats
            // Note: Stats are approximate - concurrent ingests may race on read-modify-write.
            // For precise counts, query the actual RAG storage instead.
            let mut guild_config = ctx
                .data()
                .storage
                .get_guild_rag_config(&guild_id)
                .await?
                .unwrap_or_else(|| GuildRagConfig {
                    guild_id: guild_id.clone(),
                    auto_context_enabled: true,
                    max_context_chunks: 3,
                    min_similarity: 0.5,
                    ..Default::default()
                });

            guild_config.total_documents += 1;
            guild_config.total_chunks += chunk_count as u32;
            guild_config.last_ingestion_at = chrono::Utc::now().timestamp();
            ctx.data().storage.put_guild_rag_config(&guild_config).await?;

            // Audit log
            log_rag_audit(
                &ctx.data().storage,
                &guild_id,
                &user_id,
                "ingest",
                &source_uri,
                true,
                &format!("{} chunks", chunk_count),
            ).await;

            let display_name = label.unwrap_or_else(|| {
                url.split('/')
                    .next_back()
                    .unwrap_or(&url)
                    .chars()
                    .take(SOURCE_DISPLAY_NAME_LEN)
                    .collect()
            });

            ctx.say(format!(
                "Ingested **{}** chunks from `{}`\n\
                 Type: `{}`\n\
                 Source URI: `{}`",
                chunk_count, display_name, detected_type, source_uri
            ))
            .await?;
        }
        Err(e) => {
            log_rag_audit(
                &ctx.data().storage,
                &guild_id,
                &user_id,
                "ingest",
                &source_uri,
                false,
                &e.to_string(),
            ).await;
            ctx.say(format!("Failed to ingest: {}", e)).await?;
        }
    }

    Ok(())
}

/// Configure RAG settings for this guild
#[poise::command(slash_command, guild_only)]
async fn ragconfig(
    ctx: Context<'_>,
    #[description = "Role that can ingest documents"] admin_role: Option<serenity::Role>,
    #[description = "Auto-inject context into prompts (true/false)"] auto_context: Option<bool>,
    #[description = "Max context chunks (1-10)"] max_chunks: Option<u32>,
    #[description = "Min similarity threshold (0.0-1.0)"] min_similarity: Option<f32>,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    // Only guild owner or admin can change config
    check_guild_owner_or_admin(&ctx).await?;

    let guild_id = ctx.guild_id().unwrap().to_string();

    // Get or create config
    let mut config = ctx
        .data()
        .storage
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
    let mut changed = false;
    if let Some(role) = admin_role {
        config.admin_role_id = role.id.to_string();
        changed = true;
    }
    if let Some(auto) = auto_context {
        config.auto_context_enabled = auto;
        changed = true;
    }
    if let Some(max) = max_chunks {
        config.max_context_chunks = max.clamp(1, MAX_CONTEXT_CHUNKS_LIMIT);
        changed = true;
    }
    if let Some(sim) = min_similarity {
        config.min_similarity = sim.clamp(0.0, 1.0);
        changed = true;
    }

    if changed {
        ctx.data().storage.put_guild_rag_config(&config).await?;
    }

    // Display current config
    let admin_role_display = if config.admin_role_id.is_empty() {
        "Guild owner only".to_string()
    } else {
        format!("<@&{}>", config.admin_role_id)
    };

    ctx.say(format!(
        "**RAG Configuration**\n\
         Admin role: {}\n\
         Auto-context: {}\n\
         Max chunks: {}\n\
         Min similarity: {:.0}%\n\
         ---\n\
         Documents: ~{} | Chunks: ~{}",
        admin_role_display,
        if config.auto_context_enabled { "enabled" } else { "disabled" },
        config.max_context_chunks,
        config.min_similarity * 100.0,
        config.total_documents,
        config.total_chunks
    ))
    .await?;

    Ok(())
}

/// Configure RLM settings for this guild
#[poise::command(slash_command, guild_only)]
async fn rlmconfig(
    ctx: Context<'_>,
    #[description = "Mode (static, rlm, hybrid)"] mode: Option<String>,
    #[description = "Max RLM iterations (default: 10)"] max_iterations: Option<u32>,
    #[description = "Max sub-LLM calls (default: 50)"] max_sub_calls: Option<u32>,
) -> Result<(), anyhow::Error> {
    use ho_std::types::ergors::gateway::v1::RagMode;

    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    // Only guild owner or admin can change config
    check_guild_owner_or_admin(&ctx).await?;

    let guild_id = ctx.guild_id().unwrap().to_string();

    // Get or create config
    let mut config = ctx
        .data()
        .storage
        .get_guild_rag_config(&guild_id)
        .await?
        .unwrap_or_else(|| GuildRagConfig {
            guild_id: guild_id.clone(),
            auto_context_enabled: true,
            max_context_chunks: 3,
            min_similarity: 0.5,
            mode: RagMode::Static as i32,
            rlm_max_iterations: 10,
            rlm_max_sub_calls: 50,
            ..Default::default()
        });

    // Apply updates
    let mut changed = false;
    if let Some(mode_str) = mode {
        let new_mode = match mode_str.to_lowercase().as_str() {
            "static" => RagMode::Static,
            "rlm" => RagMode::Rlm,
            "hybrid" => RagMode::Hybrid,
            _ => {
                ctx.say("Invalid mode. Use: static, rlm, or hybrid").await?;
                return Ok(());
            }
        };
        config.mode = new_mode as i32;
        changed = true;
    }

    if let Some(iters) = max_iterations {
        config.rlm_max_iterations = iters.clamp(1, 50);
        changed = true;
    }

    if let Some(calls) = max_sub_calls {
        config.rlm_max_sub_calls = calls.clamp(1, 200);
        changed = true;
    }

    if changed {
        ctx.data().storage.put_guild_rag_config(&config).await?;
    }

    // Display current config
    let mode_name = match RagMode::try_from(config.mode).unwrap_or(RagMode::Static) {
        RagMode::Static => "Static RAG",
        RagMode::Rlm => "RLM (Agentic)",
        RagMode::Hybrid => "Hybrid (RLM + RAG fallback)",
        _ => "Unknown",
    };

    ctx.say(format!(
        "**RLM Configuration**\n\
         Mode: {}\n\
         Max iterations: {}\n\
         Max sub-LLM calls: {}",
        mode_name, config.rlm_max_iterations, config.rlm_max_sub_calls
    ))
    .await?;

    Ok(())
}

/// List ingested sources for this guild
#[poise::command(slash_command, guild_only)]
async fn ragsources(
    ctx: Context<'_>,
    #[description = "Maximum sources to show"] limit: Option<usize>,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let prefix = format!("discord:guild_{}/", guild_id);
    let limit = limit.unwrap_or(10).min(25);

    let sources = ctx
        .data()
        .storage
        .list_rag_sources_by_prefix(&prefix, limit)
        .await?;

    if sources.is_empty() {
        ctx.say(
            "No documents ingested yet.\n\
             Use `/ingest <url>` to add documents to this guild's knowledge base.",
        )
        .await?;
    } else {
        let mut msg = format!("**Ingested Sources** ({} shown)\n", sources.len());
        for src in &sources {
            // Extract readable name from URI
            let name = src
                .uri
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_prefix("url:"))
                .unwrap_or(&src.uri);
            let short_name: String = name.chars().take(SOURCE_LIST_DISPLAY_LEN).collect();
            msg.push_str(&format!("- `{}` ({} chunks)\n", short_name, src.chunk_count));
        }
        ctx.say(msg).await?;
    }

    Ok(())
}

/// Delete a source from this guild's knowledge base (admin only)
#[poise::command(slash_command, guild_only)]
async fn ragdelete(
    ctx: Context<'_>,
    #[description = "Source URI or URL to delete"] source: String,
) -> Result<(), anyhow::Error> {
    check_guild_authorization(ctx.data(), ctx.guild_id())?;
    check_rag_admin_role(&ctx).await?;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let user_id = ctx.author().id.to_string();
    let expected_prefix = format!("discord:guild_{}/", guild_id);

    // Allow user to pass just the URL or the full source URI
    let source_uri = if source.starts_with(&expected_prefix) {
        source.clone()
    } else if source.starts_with("http") {
        format!("{}url:{}", expected_prefix, source)
    } else {
        ctx.say("Invalid source. Provide the URL or full source URI.").await?;
        return Ok(());
    };

    // Security: only allow deleting sources belonging to this guild
    if !source_uri.starts_with(&expected_prefix) {
        ctx.say("Can only delete sources from this guild.").await?;
        return Ok(());
    }

    ctx.defer().await?;

    let deleted = ctx.data().storage.delete_rag_source(&source_uri).await?;

    if deleted > 0 {
        // Update guild stats
        if let Ok(Some(mut config)) = ctx.data().storage.get_guild_rag_config(&guild_id).await {
            config.total_documents = config.total_documents.saturating_sub(1);
            config.total_chunks = config.total_chunks.saturating_sub(deleted as u32);
            if let Err(e) = ctx.data().storage.put_guild_rag_config(&config).await {
                warn!("Failed to update guild RAG stats after delete: {}", e);
            }
        }

        log_rag_audit(
            &ctx.data().storage,
            &guild_id,
            &user_id,
            "delete",
            &source_uri,
            true,
            &format!("{} chunks deleted", deleted),
        ).await;

        ctx.say(format!("Deleted {} chunks from `{}`", deleted, source_uri))
            .await?;
    } else {
        ctx.say(format!("Source not found: `{}`", source_uri)).await?;
    }

    Ok(())
}

// ============ HELPER FUNCTIONS ============

/// Extract display name from source URI (DRY helper)
fn source_display_name(uri: &str) -> &str {
    uri.rsplit('/')
        .next()
        .and_then(|s| s.strip_prefix("url:"))
        .unwrap_or(uri)
}

// ============ SECURITY FUNCTIONS ============

/// Validated URL with resolved IP address for safe fetching.
///
/// Contains both the original URL (for Host header) and the resolved IP
/// to prevent DNS rebinding attacks (TOCTOU).
#[derive(Debug)]
struct ValidatedUrl {
    /// Original URL for display and Host header
    original: String,
    /// Scheme (http or https)
    scheme: String,
    /// Resolved IP address to connect to
    resolved_ip: IpAddr,
    /// Port to connect to
    port: u16,
    /// Path and query string
    path_and_query: String,
    /// Original host for Host header
    host: String,
}

impl ValidatedUrl {
    /// Build URL using resolved IP for the actual request.
    /// This prevents DNS rebinding attacks.
    fn ip_url(&self) -> String {
        let ip_host = match self.resolved_ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{}]", v6),
        };
        format!("{}://{}:{}{}", self.scheme, ip_host, self.port, self.path_and_query)
    }
}

/// Validate URL for ingestion - prevents SSRF attacks.
///
/// Returns a ValidatedUrl containing the resolved IP to use for the actual
/// request, preventing DNS rebinding (TOCTOU) attacks.
///
/// Blocks:
/// - Non-HTTP(S) schemes
/// - Localhost and loopback addresses
/// - Private IP ranges (10.x, 172.16-31.x, 192.168.x)
/// - Link-local addresses (169.254.x, fe80::/10)
/// - Unique local addresses (fc00::/7)
/// - Cloud metadata endpoints
/// - IPv4-mapped IPv6 addresses pointing to blocked ranges
fn validate_ingest_url(url_str: &str) -> Result<ValidatedUrl, &'static str> {
    let parsed = reqwest::Url::parse(url_str).map_err(|_| "Invalid URL format")?;

    // Only allow HTTP(S)
    let scheme = match parsed.scheme() {
        "http" => "http".to_string(),
        "https" => "https".to_string(),
        _ => return Err("Only HTTP and HTTPS URLs are allowed"),
    };

    let host = parsed.host_str().ok_or("URL has no host")?.to_string();

    // Block known metadata/internal hostnames
    let blocked_hosts = [
        "localhost",
        "127.0.0.1",
        "::1",
        "[::1]",
        "0.0.0.0",
        "metadata.google.internal",
        "metadata.google.com",
        "169.254.169.254",
    ];

    let host_lower = host.to_lowercase();
    if blocked_hosts.iter().any(|&b| host_lower == b) {
        return Err("Internal/metadata URLs are not allowed");
    }

    // Block .internal, .local, and .localhost TLDs
    if host_lower.ends_with(".internal")
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".localhost")
    {
        return Err("Internal domain URLs are not allowed");
    }

    let port = parsed.port().unwrap_or(if scheme == "https" { 443 } else { 80 });
    let socket_addr = format!("{}:{}", host, port);

    // Resolve hostname and validate ALL returned IPs
    let addrs: Vec<_> = socket_addr
        .to_socket_addrs()
        .map_err(|_| "Failed to resolve hostname")?
        .collect();

    if addrs.is_empty() {
        return Err("Hostname resolved to no addresses");
    }

    // Check ALL resolved IPs - if any are private/blocked, reject
    for addr in &addrs {
        if is_private_or_loopback(addr.ip()) {
            return Err("URL resolves to private/loopback IP");
        }
    }

    // Use first resolved IP for the request
    let resolved_ip = addrs[0].ip();

    // Build path and query
    let path_and_query = match parsed.query() {
        Some(q) => format!("{}?{}", parsed.path(), q),
        None => parsed.path().to_string(),
    };

    Ok(ValidatedUrl {
        original: url_str.to_string(),
        scheme,
        resolved_ip,
        port,
        path_and_query,
        host,
    })
}

/// Check if an IP address is private, loopback, link-local, or otherwise blocked.
///
/// Covers:
/// - IPv4: loopback (127.x), private (10.x, 172.16-31.x, 192.168.x),
///         link-local (169.254.x), broadcast, unspecified
/// - IPv6: loopback (::1), unspecified (::), link-local (fe80::/10),
///         unique local (fc00::/7), IPv4-mapped addresses (::ffff:x.x.x.x)
fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()           // 127.0.0.0/8
                || ipv4.is_private()      // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || ipv4.is_link_local()   // 169.254.0.0/16
                || ipv4.is_broadcast()    // 255.255.255.255
                || ipv4.is_unspecified()  // 0.0.0.0
                || ipv4.octets()[0] == 0  // 0.0.0.0/8 (current network)
        }
        IpAddr::V6(ipv6) => {
            let segments = ipv6.segments();

            ipv6.is_loopback()       // ::1
                || ipv6.is_unspecified() // ::
                // fe80::/10 - Link-local
                || (segments[0] & 0xffc0) == 0xfe80
                // fc00::/7 - Unique local (private)
                || (segments[0] & 0xfe00) == 0xfc00
                // Check for IPv4-mapped addresses (::ffff:x.x.x.x)
                || ipv6.to_ipv4_mapped().map(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                }).unwrap_or(false)
                // Also check IPv4-compatible (deprecated but still exists)
                || ipv6.to_ipv4().map(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                }).unwrap_or(false)
        }
    }
}

/// Check if user has RAG admin role for this guild.
async fn check_rag_admin_role(ctx: &Context<'_>) -> Result<(), anyhow::Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;

    // Get guild RAG config
    let config = ctx
        .data()
        .storage
        .get_guild_rag_config(&guild_id.to_string())
        .await?;

    // If no config or no admin role, require guild owner/admin
    let required_role_id = match config {
        Some(c) if !c.admin_role_id.is_empty() => c.admin_role_id,
        _ => {
            // No admin role configured - check if user is guild owner or has ADMINISTRATOR
            return check_guild_owner_or_admin(ctx).await;
        }
    };

    // Check if user has the required role
    let member = ctx
        .author_member()
        .await
        .ok_or_else(|| anyhow::anyhow!("Could not get member info"))?;

    let role_id: u64 = required_role_id
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid role ID in config"))?;

    if member.roles.contains(&serenity::RoleId::new(role_id)) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "You need the RAG admin role to perform this action"
        ))
    }
}

/// Check if user is guild owner or has ADMINISTRATOR permission.
async fn check_guild_owner_or_admin(ctx: &Context<'_>) -> Result<(), anyhow::Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| anyhow::anyhow!("Not in a guild"))?;

    // Get guild info
    let guild = guild_id
        .to_partial_guild(ctx.http())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get guild info: {}", e))?;

    // Check if user is owner
    if guild.owner_id == ctx.author().id {
        return Ok(());
    }

    // Check if user has ADMINISTRATOR permission
    let member = ctx
        .author_member()
        .await
        .ok_or_else(|| anyhow::anyhow!("Could not get member info"))?;

    // Try cache first, but handle cache miss explicitly
    let permissions = match member.permissions(ctx.cache()) {
        Ok(perms) => perms,
        Err(_) => {
            // Cache miss - fetch permissions from API
            // Get member's roles and compute permissions manually
            debug!("Cache miss for member permissions, fetching from API");
            match guild_id.member(ctx.http(), ctx.author().id).await {
                Ok(fetched_member) => {
                    // Check role permissions
                    for role_id in &fetched_member.roles {
                        if let Some(role) = guild.roles.get(role_id) {
                            if role.permissions.administrator() {
                                return Ok(());
                            }
                        }
                    }
                    // No admin role found
                    serenity::Permissions::empty()
                }
                Err(e) => {
                    warn!("Failed to fetch member permissions: {}", e);
                    // Conservative: deny if we can't verify
                    serenity::Permissions::empty()
                }
            }
        }
    };

    if permissions.administrator() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Only the guild owner or administrators can perform this action"
        ))
    }
}

// ============ PROMPT HELPER FUNCTIONS ============

/// Build an augmented prompt with RAG context
fn build_augmented_prompt(message: &str, rag: Option<&RagContext>) -> String {
    match rag {
        Some(ctx) if !ctx.chunks.is_empty() => {
            let context_str = ctx
                .chunks
                .iter()
                .map(|c| {
                    format!("[Source: {}]\n{}", source_display_name(&c.source_uri), c.content)
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");

            format!(
                "Use the following context to help answer the question. \
                 If the context is not relevant, ignore it.\n\n\
                 ## Context\n{}\n\n## Question\n{}",
                context_str, message
            )
        }
        _ => message.to_string(),
    }
}

/// Format response with source attribution
fn format_response_with_sources(response_parts: &[String], rag: Option<&RagContext>) -> String {
    let mut content = response_parts.join("\n");

    if let Some(ctx) = rag {
        if !ctx.chunks.is_empty() {
            content.push_str("\n\n---\n*Sources:*");
            for chunk in &ctx.chunks {
                content.push_str(&format!(
                    "\n- {} ({:.0}%)",
                    source_display_name(&chunk.source_uri),
                    chunk.similarity * 100.0
                ));
            }
        }
    }

    content
}

/// Log a guild RAG audit event with error handling
pub(crate) async fn log_rag_audit(
    storage: &ErgorsStorage,
    guild_id: &str,
    user_id: &str,
    operation: &str,
    source_uri: &str,
    success: bool,
    message: &str,
) {
    if let Err(e) = storage
        .log_guild_rag_audit(guild_id, user_id, operation, source_uri, success, message)
        .await
    {
        warn!(
            "Failed to log RAG audit for guild {}: {} (op={}, success={})",
            guild_id, e, operation, success
        );
    }
}

/// Send a chunked response for messages exceeding Discord's limit
async fn send_chunked_response(ctx: &Context<'_>, content: &str) -> Result<(), anyhow::Error> {
    let mut remaining = content;
    let mut is_first = true;

    while !remaining.is_empty() {
        let chunk_len = remaining
            .char_indices()
            .take_while(|(i, _)| *i < DISCORD_MSG_LIMIT)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(remaining.len().min(DISCORD_MSG_LIMIT));

        let (chunk, rest) = remaining.split_at(chunk_len);

        if is_first {
            ctx.say(chunk).await?;
            is_first = false;
        } else {
            ctx.channel_id()
                .say(ctx.http(), chunk)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to send chunk: {}", e))?;
        }

        remaining = rest;
    }

    Ok(())
}

/// Context retrieval result distinguishing final answers from augmented prompts
enum ContextResult {
    /// Final answer ready for user (from RLM) - skip LLM call
    FinalAnswer(String),
    /// Augmented prompt for LLM (from RAG)
    AugmentedPrompt(String),
}

/// Retrieve context for guild (RLM or RAG based on config)
async fn retrieve_guild_context(
    data: &DiscordData,
    guild_id: &str,
    query: &str,
) -> Option<ContextResult> {
    use ho_std::types::ergors::gateway::v1::RagMode;

    let config = match data.storage.get_guild_rag_config(guild_id).await {
        Ok(Some(c)) => c,
        _ => return None,
    };

    match RagMode::try_from(config.mode).unwrap_or(RagMode::Static) {
        RagMode::Rlm => {
            // RLM mode - call embedded RLM service (returns final answer)
            #[cfg(feature = "rlm")]
            {
                if let Some(answer) = query_rlm_service(data, guild_id, query, &config).await {
                    return Some(ContextResult::FinalAnswer(answer));
                }
            }
            #[cfg(not(feature = "rlm"))]
            {
                warn!("RLM mode requested but feature is not enabled. Falling back to RAG.");
            }

            // Fallback to RAG if RLM fails or not available
            let rag_ctx = retrieve_guild_rag_context(data, guild_id, query).await?;
            Some(ContextResult::AugmentedPrompt(build_augmented_prompt(query, Some(&rag_ctx))))
        }
        RagMode::Static => {
            // Existing RAG logic (returns augmented prompt)
            let rag_ctx = retrieve_guild_rag_context(data, guild_id, query).await?;
            Some(ContextResult::AugmentedPrompt(build_augmented_prompt(query, Some(&rag_ctx))))
        }
        RagMode::Hybrid => {
            // Try RLM first (final answer), fallback to RAG (augmented prompt)
            #[cfg(feature = "rlm")]
            {
                if let Some(answer) = query_rlm_service(data, guild_id, query, &config).await {
                    return Some(ContextResult::FinalAnswer(answer));
                }
            }

            // Fallback to RAG
            let rag_ctx = retrieve_guild_rag_context(data, guild_id, query).await?;
            Some(ContextResult::AugmentedPrompt(build_augmented_prompt(query, Some(&rag_ctx))))
        }
        _ => {
            // Default to static RAG
            let rag_ctx = retrieve_guild_rag_context(data, guild_id, query).await?;
            Some(ContextResult::AugmentedPrompt(build_augmented_prompt(query, Some(&rag_ctx))))
        }
    }
}

/// Query RLM service for document-based answer
#[cfg(feature = "rlm")]
async fn query_rlm_service(
    data: &DiscordData,
    guild_id: &str,
    query: &str,
    config: &ho_std::types::ergors::gateway::v1::GuildRagConfig,
) -> Option<String> {
    use ergors_rlm::RlmQuery;

    let rlm_service = data.rlm_service.as_ref()?;

    debug!("Querying RLM service for guild {}", guild_id);

    // Load documents from storage
    let prefix = format!("discord:guild_{}/", guild_id);

    // Load documents using shared utility (with HTTP client reuse)
    let proto_documents = match crate::grpc::load_documents_by_prefix(&data.storage, &prefix, 100, Some(data.rag_client.clone())).await {
        Ok(docs) => docs,
        Err(e) => {
            warn!("Failed to load documents for RLM: {}", e);
            return None;
        }
    };

    if proto_documents.is_empty() {
        debug!("No documents found for RLM query");
        return None;
    }

    debug!("RLM querying {} documents", proto_documents.len());

    // Convert proto documents to RLM documents
    let documents: Vec<ergors_rlm::Document> = proto_documents
        .into_iter()
        .map(|d| ergors_rlm::Document {
            source_uri: d.source_uri,
            content: d.content,
            doc_type: d.doc_type,
            tags: d.tags,
            ingested_at: d.ingested_at,
        })
        .collect();

    // Execute RLM query
    let rlm_query = RlmQuery {
        query: query.to_string(),
        guild_id: guild_id.to_string(),
        max_iterations: config.rlm_max_iterations,
        max_sub_calls: config.rlm_max_sub_calls,
    };

    match rlm_service.query(rlm_query, documents).await {
        Ok(response) => {
            debug!(
                "RLM response: {} iterations, {} sub-LLM calls",
                response.iterations, response.sub_llm_calls
            );

            // Format with source attribution
            let mut content = response.answer;
            if !response.source_uris.is_empty() {
                content.push_str("\n\n---\n*Sources:*");
                for uri in response.source_uris {
                    content.push_str(&format!("\n- {}", source_display_name(&uri)));
                }
            }

            Some(content)
        }
        Err(e) => {
            warn!("RLM query failed: {}", e);
            None
        }
    }
}

/// Retrieve RAG context for a guild's prompt.
///
/// Returns None if RAG is not configured/enabled, with explicit logging for errors.
///
/// Uses the shared `rag_client` from DiscordData for HTTP connection pooling.
async fn retrieve_guild_rag_context(
    data: &DiscordData,
    guild_id: &str,
    query: &str,
) -> Option<RagContext> {
    // Get guild RAG config - explicit error handling
    let config = match data.storage.get_guild_rag_config(guild_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            debug!("No RAG config for guild {}", guild_id);
            return None;
        }
        Err(e) => {
            warn!("Failed to get RAG config for guild {}: {}", guild_id, e);
            return None;
        }
    };

    // Check if auto-context is enabled
    if !config.auto_context_enabled {
        debug!("RAG auto-context disabled for guild {}", guild_id);
        return None;
    }

    // Get global RAG config for embedder - explicit error handling
    let rag_config = match data.storage.get_rag_config().await {
        Ok(Some(c)) => c,
        Ok(None) => {
            debug!("No global RAG config - cannot retrieve context");
            return None;
        }
        Err(e) => {
            warn!("Failed to get global RAG config: {}", e);
            return None;
        }
    };

    // Create RAG instance with shared HTTP client
    let rag = match crate::rag::new_remote_with_client(
        &data.storage,
        data.rag_client.clone(),
        &rag_config.endpoint,
        &rag_config.model,
        rag_config.dimension as usize,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to create RAG instance for guild {}: {}", guild_id, e);
            return None;
        }
    };

    // Query with guild filter and verification (to get full chunk content)
    let start = std::time::Instant::now();
    let options = ergors_rag::QueryOptions {
        verify: true, // Need full content, not just preview
        filters: ergors_rag::MetadataFilters {
            source_uri_prefix: Some(format!("discord:guild_{}/", guild_id)),
            ..Default::default()
        },
        ..Default::default()
    };

    let result = match rag
        .query(query, config.max_context_chunks as usize, options)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("RAG query failed for guild {}: {}", guild_id, e);
            return None;
        }
    };

    // Extract chunks (use internal types, not proto)
    let chunks: Vec<RagChunk> = match result {
        ergors_rag::QueryResult::Verified(verified_chunks) => {
            verified_chunks
                .into_iter()
                .filter(|c| c.similarity >= config.min_similarity)
                .map(|c| RagChunk {
                    content: c.content,
                    source_uri: c.provenance.source_uri,
                    similarity: c.similarity,
                })
                .collect()
        }
        ergors_rag::QueryResult::Standard(search_results) => {
            // Fallback: use preview from metadata (less ideal but works)
            search_results
                .into_iter()
                .filter(|r| r.similarity >= config.min_similarity)
                .map(|r| RagChunk {
                    content: r.metadata.preview,
                    source_uri: String::new(), // Not available in standard results
                    similarity: r.similarity,
                })
                .collect()
        }
    };

    let query_time_ms = start.elapsed().as_secs_f32() * 1000.0;

    if !chunks.is_empty() {
        debug!(
            "RAG retrieved {} chunks for guild {} in {:.1}ms",
            chunks.len(),
            guild_id,
            query_time_ms
        );
    }

    Some(RagContext {
        chunks,
        guild_id: guild_id.to_string(),
        query_time_ms,
    })
}

/// Fetch content from a URL for ingestion.
///
/// Uses the shared HTTP client for connection pooling.
/// Fetch content from a validated URL for ingestion.
///
/// Uses the resolved IP to prevent DNS rebinding attacks.
/// Validates Content-Type before reading body.
async fn fetch_url_content(
    client: &reqwest::Client,
    validated: &ValidatedUrl,
) -> Result<String, anyhow::Error> {
    // Build request using resolved IP to prevent DNS rebinding
    let request = client
        .get(validated.ip_url())
        .header("Host", &validated.host);

    let response = request.send().await?;

    // Check for redirects (should be blocked by client policy, but double-check)
    if response.status().is_redirection() {
        return Err(anyhow::anyhow!(
            "Redirects are not allowed (got {})",
            response.status()
        ));
    }

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "HTTP {} - {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    // Check content length before reading body
    if let Some(len) = response.content_length() {
        if len > MAX_INGEST_BYTES {
            return Err(anyhow::anyhow!(
                "Content too large ({} bytes, max {} bytes)",
                len,
                MAX_INGEST_BYTES
            ));
        }
    }

    // Validate Content-Type BEFORE reading body
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Extract MIME type (ignore charset and other parameters)
    let mime_type = content_type.split(';').next().unwrap_or("").trim();

    // Check if it's a text-based content type we can process
    let is_allowed = ALLOWED_CONTENT_TYPES.contains(&mime_type)
        || mime_type.starts_with("text/")
        || mime_type.is_empty(); // Allow missing content-type (common for raw files)

    if !is_allowed {
        return Err(anyhow::anyhow!(
            "Unsupported content type: '{}'. Only text-based content is allowed.",
            mime_type
        ));
    }

    let is_html = mime_type == "text/html";

    let text = response.text().await?;

    // Additional safety: reject binary content that slipped through
    // Check if content appears to be binary (high ratio of non-printable chars)
    let non_printable_count = text
        .chars()
        .take(1000)
        .filter(|c| !c.is_ascii_graphic() && !c.is_ascii_whitespace())
        .count();
    if non_printable_count > 50 {
        return Err(anyhow::anyhow!(
            "Content appears to be binary, not text"
        ));
    }

    if is_html {
        Ok(html2text::from_read(text.as_bytes(), HTML2TEXT_LINE_WIDTH))
    } else {
        Ok(text)
    }
}

/// Detect document type from URL file extension.
///
/// Only relies on explicit file extensions - domain heuristics are unreliable
/// (github.com can host docs, images, anything).
fn detect_doc_type(url: &str) -> String {
    // Extract path from URL, get last segment
    let path = url.split('?').next().unwrap_or(url);
    let filename = path.rsplit('/').next().unwrap_or("").to_lowercase();

    // Check file extension
    if let Some(ext) = filename.rsplit('.').next() {
        match ext {
            "md" | "mdx" | "markdown" => return "markdown".to_string(),
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "rb" | "php" | "swift" | "kt" | "scala" | "zig" | "hs" => {
                return "code".to_string()
            }
            "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" => {
                return "config".to_string()
            }
            "txt" | "text" => return "text".to_string(),
            "html" | "htm" => return "html".to_string(),
            _ => {}
        }
    }

    // Default to text - let the content speak for itself
    "text".to_string()
}
