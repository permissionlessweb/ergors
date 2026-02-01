//! Discord Gateway Implementation
//!
//! Provides Discord bot integration using Poise/Serenity framework.
//! Supports slash commands for AI interaction with per-thread session tracking.

use crate::gateway::crypto::decrypt_gateway_secret;
use crate::storage::ErgorsStorage;
use anyhow::Result;
use async_trait::async_trait;
use ho_std::{
    llm::LlmRouter,
    traits::gateway::{GatewayContext, GatewayEvent, GatewayModule},
    types::ergors::{
        gateway::v1::{DiscordGatewayConfig, GatewayResponse},
        orch::v1::{PromptContext, PromptMessage, PromptRequest},
    },
};
use poise::serenity_prelude as serenity;
use poise::{Framework, FrameworkOptions};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

/// Shared data for all Poise commands
pub struct DiscordData {
    pub storage: Arc<ErgorsStorage>,
    pub router: Arc<LlmRouter>,
    pub allowed_guild_ids: Vec<String>,
    pub event_tx: mpsc::UnboundedSender<GatewayEvent>,
}

type Context<'a> = poise::Context<'a, DiscordData, anyhow::Error>;

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

        let data = DiscordData {
            storage,
            router,
            allowed_guild_ids,
            event_tx,
        };

        let framework = Framework::builder()
            .options(FrameworkOptions {
                commands: vec![prompt(), thread(), clear()],
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

/// Send a prompt to the AI
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
    let thread_id = ctx.channel_id().to_string();
    let user_id = ctx.author().id.to_string();
    let session_id = ctx
        .data()
        .storage
        .get_or_create_gateway_session("discord", &thread_id)
        .await?;

    // Build prompt request with session context
    let prompt_req = PromptRequest {
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: message.clone(),
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

    // Call LLM router directly - no async event dance
    let response = match ctx.data().router.handle_request(&prompt_req, "default").await {
        Ok(resp) => resp,
        Err(e) => {
            error!("LLM router error: {}", e);
            ctx.say(format!("Error: {}", e)).await?;
            return Ok(());
        }
    };

    // Join response parts
    let response_content = response.response.join("\n");

    // Discord message limit is 2000 chars - chunk if needed
    const MAX_MSG_LEN: usize = 1990;
    if response_content.len() <= MAX_MSG_LEN {
        ctx.say(&response_content).await?;
    } else {
        // Send first chunk as reply
        let mut remaining = response_content.as_str();
        let mut is_first = true;

        while !remaining.is_empty() {
            let chunk_len = remaining
                .char_indices()
                .take_while(|(i, _)| *i < MAX_MSG_LEN)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(remaining.len().min(MAX_MSG_LEN));

            let (chunk, rest) = remaining.split_at(chunk_len);

            if is_first {
                ctx.say(chunk).await?;
                is_first = false;
            } else {
                // Follow-up messages go to the channel
                ctx.channel_id()
                    .say(ctx.http(), chunk)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to send chunk: {}", e))?;
            }

            remaining = rest;
        }
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
