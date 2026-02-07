# Edgar Cayce: Akash Documentation Bot

Deploy a Discord chatbot on Akash Network that answers questions about a GitHub repository using RAG.

The engine runs as a container on Akash. Secrets are bootstrapped at runtime via **sentinel mode** — zero secrets in the SDL.

## Requirements

- Ergors CLI installed locally
- Akash wallet funded with ~30 AKT
- AkashML API key ([chatapi.akash.network](https://chatapi.akash.network))
- Discord bot token ([discord.com/developers](https://discord.com/developers/applications))

## SDL Files

| File | Purpose |
|------|---------|
| `sdls/engine/ergors-sentinel.yml` | Engine container (sentinel mode) |
| `sdls/embeddings/qwen.yml` | Qwen3-VL-Embedding-8B for RAG embeddings |
| `sdls/chat/kimi-k2.5.yml` | Kimi-K2.5 for chat completions |

---

## 00. Create Discord Bot

1. [discord.com/developers/applications](https://discord.com/developers/applications) > **New Application**
2. **Bot** settings:
   - Copy the bot token
   - Enable **Message Content Intent**
3. **OAuth2 > URL Generator**:
   - Scopes: `bot`, `applications.commands`
   - Permissions: Send Messages, Send Messages in Threads, Create Public Threads, Embed Links, Read Message History, Use Slash Commands
4. Open the generated URL, select your server, authorize
5. In your server: **Settings > Roles > Create Role** named `Akashic Record Keeper` — assign to users who can manage the knowledge base

---

## 1. Setup Local Client

```bash
ergors init new
ergors keys import-mnemonic --label "Deployer" --chain-id akashnet-2 --make-default
ergors start &
```

Note your admin public key from `~/.ergors/node_identity.enc` (the `public_key` field).

---

## 2. Deploy Engine to Akash

Edit `sdls/engine/ergors-sentinel.yml` — replace `REPLACE_WITH_YOUR_ADMIN_PUBKEY_HEX` with your public key.

```bash
ergors deploy create --sdl sdls/engine/ergors-sentinel.yml --label edgar-cayce --auto
ergors deploy endpoints edgar-cayce
```

Save the HTTP and gRPC endpoints from the output.

### Verify Sentinel Ready

```bash
curl http://<engine-http-endpoint>/sentinel/health
```

Returns the current phase, version, and a per-session X25519 public key:

```json
{"phase":"awaiting_init","version":"0.1.0","session_pubkey":"<hex-encoded 32-byte X25519 pubkey>"}
```

Save the `session_pubkey` — all subsequent sentinel requests must encrypt their JSON body to this key.

---

## 3. Bootstrap via Sentinel

Use the CLI to bootstrap the sentinel. All secrets are entered interactively
(hidden input) and encrypted end-to-end to the sentinel's ephemeral X25519 session key.
The Akash provider proxy sees only ciphertext.

```bash
ergors sentinel bootstrap http://<engine>:8080
```

The command walks through the full handshake:

1. **GET /sentinel/health** — fetches the sentinel's X25519 session pubkey
2. **POST /sentinel/init** — prompts for custody password + optional mnemonic (encrypted)
3. **POST /sentinel/api-keys** — prompts for per-provider API keys (encrypted)
4. **POST /sentinel/activate** — triggers handoff to full server

Interactive prompts (hidden, never logged):

| Prompt | Description |
|--------|-------------|
| Local custody password | Unlocks your admin Ed25519 signing key (or set `ERGORS_CUSTODY_PASSWORD`) |
| Remote custody password | Encrypts the remote node's identity (min 8 chars) |
| Mnemonic | BIP-39 phrase for deterministic key (Enter = generate new) |
| API keys | Per-provider: Anthropic, OpenAI, Akash ML, xAI, plus custom |

For automation / CI, pipe inputs via stdin (one per line):

```bash
printf '%s\n' \
  "remote-custody-pw" \
  "abandon abandon abandon ... about" \
  "sk-ant-xxx" \
  "sk-openai-xxx" \
  "" "" "" \
| ERGORS_CUSTODY_PASSWORD=local-pw ergors sentinel bootstrap http://<engine>:8080
```

### Verify

```bash
curl http://<engine>/health
```

Sentinel is gone. Full server is live.

### Protocol Details (reference)

Each request body is an encrypted envelope (plaintext rejected with 400):

- **Encryption:** X25519 DH → `blake3_derive_key("ergors sentinel v1", shared)` → ChaCha20Poly1305
- **Envelope:** `{"ephemeral_pubkey":"<hex>","nonce":"<hex>","ciphertext":"<hex>"}`
- **Auth:** Ed25519 signature headers over the envelope: `x-signature`, `x-timestamp`, `x-public-key`

---

## 4. Configure Engine

Point your local CLI at the remote engine:

```bash
export ERGORS_GRPC_ADDR=http://<engine-grpc-endpoint>:50051
```

### Import Funded Wallet

```bash
ergors keys import-mnemonic --label "Edgar Cayce" --chain-id akashnet-2 --make-default
```

### Configure Discord

```bash
ergors gateway discord set-token
ergors gateway discord allow-guild <your-guild-id>
ergors gateway enable discord
```

### Restart for Gateway Activation

Gateways register at boot. Add `ERGORS_CUSTODY_PASSWORD=<password>` to the engine SDL env block, then:

```bash
ergors deploy update-deployment edgar-cayce --sdl sdls/engine/ergors-sentinel.yml
```

Container restarts. Verify Discord connected:

```bash
ergors gateway status discord
```

---

## 5. Deploy Inference

```bash
ergors deploy create --sdl sdls/embeddings/qwen.yml --label qwen-embeddings --auto
ergors deploy create --sdl sdls/chat/kimi-k2.5.yml --label kimi-chat --auto
```

Wait for both, then verify:

```bash
ergors deploy info qwen-embeddings
ergors deploy info kimi-chat
curl http://<engine>/v1/models
```

Both should appear as available models.

---

## 6. Configure RAG

```bash
ergors rag configure --endpoint http://<qwen-endpoint>:<port> --model qwen-embeddings
ergors rag status
```

Get the embedding endpoint from `ergors deploy endpoints qwen-embeddings`.

---

## 7. Ingest Documentation

In Discord (requires **Akashic Record Keeper** role):

```
/ingest url:https://github.com/akash-network/website label:akash-docs doc_type:documentation
```

Verify:

```
/ragsources
```

---

## 8. Ask Questions

```
/prompt What are the hardware requirements for running an Akash provider?
```

Other commands:

| Command | Action |
|---------|--------|
| `/prompt <question>` | Ask the bot |
| `/thread <name>` | New conversation thread |
| `/clear` | Reset current session |
| `/ragsources` | List ingested sources |
| `/ragstatus` | RAG stats |

---

## Management

All commands require `ERGORS_GRPC_ADDR` set to the remote engine.

```bash
# Health
ergors deploy info edgar-cayce
ergors deploy info qwen-embeddings
ergors deploy info kimi-chat

# Top up escrow (10 AKT = 10000000 uakt)
ergors deploy topup-escrow edgar-cayce 10000000
ergors deploy topup-escrow qwen-embeddings 10000000
ergors deploy topup-escrow kimi-chat 10000000

# Shutdown (close inference first, then engine from local)
ergors deploy close-deployment kimi-chat
ergors deploy close-deployment qwen-embeddings
unset ERGORS_GRPC_ADDR
ergors deploy close-deployment edgar-cayce
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Sentinel 401/403 | Wrong admin key or bad signature |
| Sentinel 409 | Out-of-phase request — check `GET /sentinel/health` |
| Sentinel 408 | Clock skew >5 min — sync system clock |
| Bot offline after restart | Missing `ERGORS_CUSTODY_PASSWORD` in SDL env |
| `/ingest` denied | User needs Akashic Record Keeper role |
| No bids on deploy | Raise `amount` in SDL pricing section |
| RAG empty results | Run `ergors rag configure` with correct endpoint |
