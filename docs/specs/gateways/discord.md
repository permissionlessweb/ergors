# Edgar: Discord Bot Setup Guide

Set up Edgar, the Ergors Discord bot, to bring AI chat and document-powered Q&A to your Discord server.

## Prerequisites

- A running Ergors engine with at least one LLM provider configured
- Discord account with "Manage Server" permission on your target server
- The engine binary built with `--features discord` (included by default)

## 1. Create the Bot in Discord Developer Portal

1. Go to https://discord.com/developers/applications
2. Click **New Application**, give it a name (e.g., "Edgar"), click **Create**
3. Go to **Bot** in the left sidebar
4. Click **Reset Token**, copy the token (you'll need it in step 3)
5. Under **Privileged Gateway Intents**, enable **Message Content Intent**
6. Go to **OAuth2 > URL Generator** in the left sidebar
7. Under **Scopes**, check: `bot`, `applications.commands`
8. Under **Bot Permissions**, check: `Send Messages`, `Read Message History`, `Use Slash Commands`
9. Copy the generated URL at the bottom and open it in your browser
10. Select your server and click **Authorize**

## 2. Find Your Guild (Server) ID

1. In Discord, go to **User Settings > Advanced** and enable **Developer Mode**
2. Right-click your server name in the sidebar
3. Click **Copy Server ID** — this is your guild ID (e.g., `123456789012345678`)

## 3. Configure on Ergors Engine

**Quick setup** (recommended — configures token, adds guild, enables gateway):

```bash
ergors gateway discord setup <GUILD_ID>
# Prompts for bot token (hidden input)
```

Or with token inline (useful for automation):

```bash
ergors gateway discord setup <GUILD_ID> --token <BOT_TOKEN>
```

**Manual step-by-step** (if you prefer):

```bash
# Set bot token (prompts interactively)
ergors gateway discord set-bot-token

# Add your guild to the allowlist
ergors gateway discord allow-guild <GUILD_ID>

# Enable the gateway (starts the bot immediately if engine is running)
ergors gateway enable discord
```

If the engine isn't running yet, start it:

```bash
ergors start
```

## 4. Set Admin Roles

In Discord, use the `/edgar config` command to set which role can manage documents:

```
/edgar config admin_role:@DocumentAdmins
```

Without an admin role configured, only the server owner can ingest/delete documents.

## 5. Verify It Works

In any channel where the bot is present:

```
/edgar prompt message:Hello, are you working?
```

You should get an AI response. If not, see Troubleshooting below.

## Available Commands

All bot commands live under `/edgar`:

| Command | Description | Admin Only |
|---------|-------------|------------|
| `/edgar prompt <message>` | Send a message to the AI | No |
| `/edgar thread [name]` | Create a new conversation thread | No |
| `/edgar clear` | Clear conversation history in current thread | No |
| `/edgar ingest <url> [label] [doc_type]` | Ingest a URL or GitHub repo | Yes |
| `/edgar sources [limit]` | List ingested documents | No |
| `/edgar delete <source>` | Remove a document from the knowledge base | Yes |
| `/edgar config [admin_role] [auto_context] [max_chunks] [min_similarity]` | Configure RAG settings | Yes |
| `/edgar rlm [mode] [max_iterations] [max_sub_calls]` | Set reasoning mode (static/rlm/hybrid) | Yes |

## Document Ingestion

### Ingest a URL

```
/edgar ingest url:https://docs.example.com/api-reference label:api-docs
```

### Ingest a GitHub Repository

```
/edgar ingest url:https://github.com/owner/repo label:repo-docs
/edgar ingest url:https://github.com/owner/repo/tree/main doc_type:documentation
```

**Doc type options:**
- `documentation` / `docs` — Standard preset (docs + source code)
- `code` — Source files only
- `minimal` — Almost everything (excludes binaries)
- Default: Standard preset

GitHub repos are shallow-cloned, filtered (no `node_modules`, binaries, etc.), and each file is ingested individually for precise retrieval.

### List and Delete Sources

```
/edgar sources
/edgar delete https://docs.example.com/api-reference
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Bot doesn't respond to `/edgar` | Slash commands not registered | Restart the engine — commands register on startup |
| "This bot is not authorized for this server" | Guild not in allowlist | `ergors gateway discord allow-guild <GUILD_ID>` |
| "Discord bot token not configured" | Token missing or decryption failed | Re-run `ergors gateway discord set-bot-token` |
| `token_configured: false` in config | Bug in older versions checking wrong key | Update to latest version with the fix |
| "RAG not configured" on `/edgar ingest` | Global RAG not set up | Run `ergors rag configure` on the engine |
| "Missing required RAG admin role" | User lacks permission | Server owner sets role: `/edgar config admin_role:@Role` |
| Bot is "offline" in Discord | Gateway not enabled or engine not running | `ergors gateway enable discord` then `ergors start` |
| "Invalid URL" on ingest | SSRF protection triggered | Use a public URL (private IPs, localhost, metadata endpoints are blocked) |

## Engine CLI Reference

All `ergors gateway` commands for Discord:

```
ergors gateway discord setup <guild-id> [--token <TOKEN>]   # Quick setup
ergors gateway discord set-bot-token [--token <TOKEN>]      # Set/update bot token
ergors gateway discord allow-guild <guild-id>               # Add guild to allowlist
ergors gateway discord deny-guild <guild-id>                # Remove guild from allowlist
ergors gateway discord config [--json]                      # Show configuration
ergors gateway discord register                             # Show registered commands

ergors gateway list [--json]                                # List all gateways
ergors gateway status discord                               # Show gateway status
ergors gateway enable discord                               # Enable gateway
ergors gateway disable discord                              # Disable gateway
```

## Test Mode

For development and staging, bypass LLM calls while still testing auth, sessions, and document ops:

```bash
export ERGORS_GATEWAY_TEST_MODE=1
ergors start
```

In test mode:
- `/edgar prompt` returns a canned test response (no LLM call)
- `/edgar ingest` works normally (documents are actually stored)
- All permission checks are still enforced
