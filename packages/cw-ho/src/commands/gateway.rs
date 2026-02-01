//! Gateway management CLI commands
//!
//! Commands for managing communication gateways (Discord, Nostr, Element, etc.)

use anyhow::Result;
use clap::Subcommand;

use super::CliContext;
use crate::client::ManagementClient;

/// Gateway management commands
#[derive(Subcommand)]
pub enum GatewayCmd {
    /// List all registered gateways and their status
    List,

    /// Show gateway status and configuration
    Status {
        /// Gateway ID (e.g., "discord", "nostr")
        gateway_id: String,
    },

    /// Enable a gateway
    Enable {
        /// Gateway ID to enable
        gateway_id: String,
    },

    /// Disable a gateway
    Disable {
        /// Gateway ID to disable
        gateway_id: String,
    },

    /// Discord gateway configuration
    #[cfg(feature = "discord")]
    Discord {
        #[command(subcommand)]
        cmd: DiscordCmd,
    },
}

/// Discord-specific gateway commands
#[cfg(feature = "discord")]
#[derive(Subcommand)]
pub enum DiscordCmd {
    /// Set Discord bot token (encrypted via custody)
    SetToken {
        /// Bot token (will prompt interactively if not provided)
        #[arg(long)]
        token: Option<String>,
    },

    /// Add allowed guild ID
    AllowGuild {
        /// Discord guild (server) ID to allow
        guild_id: String,
    },

    /// Remove allowed guild ID
    DenyGuild {
        /// Discord guild (server) ID to remove from allowlist
        guild_id: String,
    },

    /// Show current Discord configuration (token redacted)
    Config,

    /// Register slash commands with Discord
    Register,
}

impl GatewayCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            GatewayCmd::List => {
                let response = client.list_gateways().await?;

                if ctx.json {
                    let gateways: Vec<_> = response
                        .gateways
                        .iter()
                        .map(|g| {
                            serde_json::json!({
                                "gateway_id": g.gateway_id,
                                "name": g.name,
                                "enabled": g.enabled,
                                "connected": g.connected,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({"gateways": gateways}))?
                    );
                } else {
                    println!("Communication Gateways");
                    println!("======================");

                    if response.gateways.is_empty() {
                        println!("No gateways registered.");
                        println!();
                        println!("Available gateways:");
                        println!("  discord - Discord bot integration (requires --features discord)");
                        println!("  nostr   - Nostr relay integration (coming soon)");
                        println!("  element - Matrix/Element integration (coming soon)");
                    } else {
                        for g in &response.gateways {
                            let status = if !g.enabled {
                                "disabled"
                            } else if g.connected {
                                "connected"
                            } else {
                                "disconnected"
                            };
                            println!("  {} ({}) - {}", g.gateway_id, g.name, status);
                        }
                    }
                }
                Ok(())
            }
            GatewayCmd::Status { gateway_id } => {
                let response = client.get_gateway_status(gateway_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "gateway_id": response.gateway_id,
                            "connected": response.connected,
                            "messages_processed": response.messages_processed,
                            "last_message_timestamp": response.last_message_timestamp,
                        }))?
                    );
                } else {
                    println!("Gateway Status: {}", response.gateway_id);
                    println!("====================");
                    println!(
                        "Connected:   {}",
                        if response.connected { "yes" } else { "no" }
                    );
                    println!("Messages:    {}", response.messages_processed);
                    if response.last_message_timestamp > 0 {
                        println!("Last Active: {}", response.last_message_timestamp);
                    }
                }
                Ok(())
            }
            GatewayCmd::Enable { gateway_id } => {
                let result = client.enable_gateway(gateway_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Gateway enabled: {}", gateway_id);
                } else {
                    eprintln!("Failed to enable gateway: {}", result.message);
                }
                Ok(())
            }
            GatewayCmd::Disable { gateway_id } => {
                let result = client.disable_gateway(gateway_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Gateway disabled: {}", gateway_id);
                } else {
                    eprintln!("Failed to disable gateway: {}", result.message);
                }
                Ok(())
            }
            #[cfg(feature = "discord")]
            GatewayCmd::Discord { cmd } => cmd.execute(ctx, client).await,
        }
    }
}

#[cfg(feature = "discord")]
impl DiscordCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            DiscordCmd::SetToken { token } => {
                let bot_token = match token {
                    Some(t) => t.clone(),
                    None => {
                        // Prompt interactively (won't appear in shell history)
                        rpassword::prompt_password("Enter Discord bot token: ")
                            .map_err(|e| anyhow::anyhow!("Failed to read token: {}", e))?
                    }
                };

                if bot_token.is_empty() {
                    return Err(anyhow::anyhow!("Token cannot be empty"));
                }

                let result = client
                    .configure_discord_gateway(&bot_token, None, None)
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Discord bot token configured (encrypted)");
                    println!();
                    println!("Next steps:");
                    println!("  1. Configure allowed guilds: ergors gateway discord allow-guild <guild-id>");
                    println!("  2. Enable the gateway: ergors gateway enable discord");
                } else {
                    eprintln!("Failed to configure token: {}", result.message);
                }
                Ok(())
            }
            DiscordCmd::AllowGuild { guild_id } => {
                let result = client.add_discord_allowed_guild(guild_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Guild added to allowlist: {}", guild_id);
                } else {
                    eprintln!("Failed to add guild: {}", result.message);
                }
                Ok(())
            }
            DiscordCmd::DenyGuild { guild_id } => {
                let result = client.remove_discord_allowed_guild(guild_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Guild removed from allowlist: {}", guild_id);
                } else {
                    eprintln!("Failed to remove guild: {}", result.message);
                }
                Ok(())
            }
            DiscordCmd::Config => {
                let response = client.get_discord_config().await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "token_configured": response.token_configured,
                            "allowed_guild_ids": response.allowed_guild_ids,
                            "allowed_channel_ids": response.allowed_channel_ids,
                            "command_prefix": response.command_prefix,
                            "respond_to_mentions": response.respond_to_mentions,
                            "respond_to_dms": response.respond_to_dms,
                        }))?
                    );
                } else {
                    println!("Discord Gateway Configuration");
                    println!("=============================");
                    println!(
                        "Token:            {}",
                        if response.token_configured {
                            "configured (encrypted)"
                        } else {
                            "not configured"
                        }
                    );
                    println!("Command Prefix:   {}", response.command_prefix);
                    println!("Respond to @:     {}", response.respond_to_mentions);
                    println!("Respond to DMs:   {}", response.respond_to_dms);
                    println!();

                    if response.allowed_guild_ids.is_empty() {
                        println!("Allowed Guilds:   (all guilds)");
                    } else {
                        println!("Allowed Guilds:");
                        for gid in &response.allowed_guild_ids {
                            println!("  - {}", gid);
                        }
                    }

                    if !response.allowed_channel_ids.is_empty() {
                        println!();
                        println!("Allowed Channels:");
                        for cid in &response.allowed_channel_ids {
                            println!("  - {}", cid);
                        }
                    }
                }
                Ok(())
            }
            DiscordCmd::Register => {
                println!("Slash commands are automatically registered when the bot starts.");
                println!();
                println!("Available commands:");
                println!("  /prompt <message> - Send a prompt to the AI");
                println!("  /thread [name]    - Create a new conversation thread");
                println!("  /clear            - Clear conversation history in current thread");
                Ok(())
            }
        }
    }
}
