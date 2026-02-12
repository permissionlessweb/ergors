# Edgar Cayce: Discord Knowledge Bot on Akash

Deploy a Discord bot on Akash that answers questions about ingested documents using RLM (agentic code execution against source-of-truth files).

The engine runs as a container on Akash. Secrets are bootstrapped at runtime via **sentinel mode** — zero secrets in the SDL.

## Requirements

- Ergors CLI installed locally
- Akash wallet funded with ~30 AKT
- At least two inference deployments on Akash (or LLM provider API keys)
- Discord bot token ([discord.com/developers](https://discord.com/developers/applications))

## SDL Files

| File | Purpose |
|------|---------|
| `sdls/engine/ergors-sentinel.yml` | Engine container (sentinel mode) |
| `sdls/chat/local-inference.yml` | Chat models for RLM reasoning loops (deploys two services: `glm-flash` and `qwen-coder`) |

---

## 1. Create the Discord Bot

1. [discord.com/developers/applications](https://discord.com/developers/applications) > **New Application**
2. **Bot** tab — copy the token, enable **Message Content Intent**
3. **OAuth2 > URL Generator** — scopes: `bot`, `applications.commands`; permissions: Send Messages, Send Messages in Threads, Create Public Threads, Embed Links, Read Message History, Use Slash Commands
4. Open the generated URL, authorize in your server
5. Create an admin role (e.g. `Akashic Record Keeper`) for users who manage the knowledge base

---

## 2. Initialize Local Client

```bash
ergors init new
ergors keys import-mnemonic --label "Deployer" --prefix akash --default
ergors start &
```

---

## 3. Deploy Engine to Akash

Replace `REPLACE_WITH_YOUR_ADMIN_PUBKEY_HEX` in the SDL with your public key from `~/.ergors/node_identity.enc`.

```bash
ergors deploy create --sdl sdls/engine/ergors-sentinel.yml --label edgar-cayce --auto
ergors deploy endpoints edgar-cayce
```

---

## 4. Bootstrap the Sentinel

All secrets are entered interactively (hidden input), encrypted end-to-end via X25519. The Akash provider sees only ciphertext.

```bash
ergors sentinel bootstrap http://<engine-endpoint>:8080
```

Prompts: local custody password, remote custody password, optional mnemonic, API keys per provider.

Verify the engine is live:

```bash
curl http://<engine-endpoint>/health
```

---

## 5. Deploy Inference Providers

Deploy two inference models — a stronger one for primary reasoning and a faster/cheaper one for sub-queries. Use `--model-map` to tell the engine which upstream model each service actually serves:

```bash
ergors deploy create \
  --sdl sdls/chat/local-inference.yml \
  --label inference-gpu \
  --model-map glm-flash=Qwen/Qwen2.5-Coder-7B-Instruct \
  --model-map qwen-coder=Qwen/Qwen2.5-Coder-7B-Instruct \
  --interactive-bid \
  --min-balance 1000000 \
  --key-name default
```

`--model-map` maps SDL service names to the actual model name the inference server expects. This is critical — without it, the engine would send the service name (e.g. `glm-flash`) as the model identifier to the upstream server, which it wouldn't understand.

Wait for status `completed`, then register the deployed endpoints as providers:

```bash
ergors deploy register-providers inference-gpu
```

This reads the deployment's service endpoints, resolves model names from the workflow's `model_map`, and registers each in both the proxy router and LLM router. Each provider's name matches the service name (`glm-flash`, `qwen-coder`), but the upstream model name (e.g. `Qwen/Qwen2.5-Coder-7B-Instruct`) is substituted in the actual API calls.

**Alternative — manual registration** (if `register-providers` isn't available or you want more control):

```bash
ergors provider add glm-flash --no-key \
  --base-url http://provider.a100.kci.val.akash.pub:31499 \
  --model-name Qwen/Qwen2.5-Coder-7B-Instruct \
  --role rlm-primary

ergors provider add qwen-coder --no-key \
  --base-url http://provider.a100.kci.val.akash.pub:32611 \
  --model-name Qwen/Qwen2.5-Coder-7B-Instruct \
  --role rlm-secondary
```

`--model-name` is essential for self-hosted inference — it tells the engine what model identifier to send upstream.

---

## 6. Test and Assign Providers

Verify each provider is reachable before assigning roles:

```bash
ergors provider test glm-flash
ergors provider test qwen-coder
```

Expected output:

```
glm-flash: OK (243ms)
  URL:   http://provider.a100.kci.val.akash.pub:31499
  Model: Qwen/Qwen2.5-Coder-7B-Instruct
qwen-coder: OK (187ms)
  URL:   http://provider.a100.kci.val.akash.pub:32611
  Model: Qwen/Qwen2.5-Coder-7B-Instruct
```

The test sends a real HTTP request to `/v1/chat/completions` with the correct model name. If a provider is unreachable or returns an error, the output shows the failure reason.

Assign engine roles:

```bash
ergors provider assign glm-flash --role rlm-primary
ergors provider assign qwen-coder --role rlm-secondary
```

- **`rlm-primary`** — drives the root reasoning loop (pick your strongest reasoner)
- **`rlm-secondary`** — used by sandboxed `llm_query()` sub-calls (can be cheaper/faster)

If only `rlm-primary` is assigned, `rlm-secondary` calls automatically fall back to the primary provider.

---

## 7. Verify Provider Roles

```bash
ergors provider roles
```

Expected output:

```
Engine Role Assignments
=======================
  rlm-primary: glm-flash [primary]
  rlm-secondary: qwen-coder [primary]
```

You can reassign roles at any time without restarting:

```bash
ergors provider unassign glm-flash --role rlm-primary
ergors provider assign anthropic --role rlm-primary
```

---

## 8. Configure Discord Gateway

```bash
ergors gateway discord setup <your-guild-id>
```

This prompts for the bot token (hidden), adds the guild to the allowlist, and enables the gateway. Restart the engine to activate:

```bash
ergors deploy update-deployment edgar-cayce --sdl sdls/engine/ergors-sentinel.yml
ergors gateway status discord
```

---

## 9. Set RLM Mode and Admin Role (in Discord)

In your Discord server, use these slash commands:

```
/edgar config admin_role:@Akashic Record Keeper
/edgar rlm mode:rlm
```

- **admin_role** — which Discord role can ingest/delete documents
- **mode:rlm** — switches from static RAG to agentic reasoning loops

By default, RLM routes to providers assigned via `rlm-primary` and `rlm-secondary` engine roles. You can override with explicit model names:

```
/edgar rlm primary_model:claude-sonnet-4-5-20250929
/edgar rlm sub_model:glm-flash
```

When `primary_model` / `sub_model` are empty (default), the engine resolves them through role assignments set in step 6.

---

## 10. Ingest a GitHub Repository

Requires the admin role set above.

```
/edgar ingest url:https://github.com/akash-network/website label:Akash Docs doc_type:documentation
```

The `label` parameter is **required** — it defines the topic category that users select when asking questions. Multiple documents can share the same label to form a topic.

Verify:

```
/edgar sources
```

---

## 11. Ask a Question

```
/edgar ask topic:Akash Docs question:What are the hardware requirements for running an Akash provider?
```

What happens:

1. `/edgar ask` validates the topic exists for this guild
2. Prepends `[Topic: Akash Docs]` to scope the query
3. RLM receives the scoped query, calls `list_documents` and `search_in_document` against `DocumentStorage`
4. The reasoning loop reads matching source files, extracts relevant sections, and synthesizes an answer
5. The response is grounded in the ingested documents — not hallucinated from training data

The `topic` field autocompletes from ingested document labels, so users see exactly what's available.

---

## Command Reference

| Command | Description | Admin |
|---------|-------------|-------|
| `/edgar ask <topic> <question>` | Ask about ingested documents (topic autocompletes) | No |
| `/edgar ingest <url> <label> [doc_type]` | Ingest URL or GitHub repo with topic label | Yes |
| `/edgar sources [limit]` | List ingested documents | No |
| `/edgar delete <source>` | Remove a document | Yes |
| `/edgar thread [name]` | New conversation thread | No |
| `/edgar clear` | Reset current session | No |
| `/edgar config [admin_role]` | Set admin role | Yes |
| `/edgar rlm [mode] [max_iterations] [max_sub_calls] [primary_model] [sub_model]` | Configure RLM mode and models | Yes |

---

## Management

All commands require `ERGORS_GRPC_ADDR` pointing to the remote engine.

```bash
# Check deployments
ergors deploy info edgar-cayce

# Top up escrow (10 AKT)
ergors deploy topup-escrow edgar-cayce 10000000

# Shutdown
unset ERGORS_GRPC_ADDR
ergors deploy close-deployment edgar-cayce
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Sentinel 401/403 | Wrong admin key or bad signature |
| Sentinel 409 | Out-of-phase — check `GET /sentinel/health` |
| Bot offline after restart | Missing `ERGORS_CUSTODY_PASSWORD` in SDL env |
| `/edgar ingest` denied | User needs the configured admin role |
| `/edgar ask` topic not found | Ingest documents with that label first |
| RLM returns generic answers | Verify mode is `rlm` via `/edgar rlm mode:rlm` |
| RLM uses wrong model | Check `ergors provider test <name>` — if Model shows the label instead of the actual model, re-add with `--model-name` |
| `provider test` "not found" | Provider not in runtime — re-add with `provider add --model-name` or `deploy register-providers` |
| `provider test` connection refused | Deployment may still be starting — check `ergors deploy info <label>` |
| "error decoding response body" | Upstream returned unexpected JSON shape — update to latest ergors (uses resilient JSON parsing) |
| `register-providers` registers 0 | `--model-map` keys don't match SDL service names — check diagnostic output for mismatch |
| No bids on deploy | Raise `amount` in SDL pricing section |

---

## TL;DR

```bash
# reset and initialize
ergors init unsafe-wipe
ergors init new
ergors keys import-mnemonic --label default --default --prefix akash --coin-type 118
RUST_LOG=debug ergors start

# setup discord bot
ergors gateway discord setup <guild-id> --token <bot-token>

# deploy inference with per-service model mapping
ergors deploy create \
  --sdl sdls/chat/local-inference.yml \
  --label inference-gpu \
  --model-map glm-flash=Qwen/Qwen2.5-Coder-7B-Instruct \
  --model-map qwen-coder=Qwen/Qwen2.5-Coder-7B-Instruct \
  --interactive-bid --min-balance 1000000 --key-name default

# register deployed endpoints as providers (auto model mapping from --model-map)
ergors deploy register-providers inference-gpu

# OR manually register with explicit model names
# ergors provider add glm-flash --no-key --base-url <url> --model-name Qwen/Qwen2.5-Coder-7B-Instruct --role rlm-primary
# ergors provider add qwen-coder --no-key --base-url <url> --model-name Qwen/Qwen2.5-Coder-7B-Instruct --role rlm-secondary

# test providers are reachable (shows URL, model name, latency)
ergors provider test glm-flash
ergors provider test qwen-coder

# assign engine roles (skip if --role was used during add)
ergors provider assign glm-flash --role rlm-primary
ergors provider assign qwen-coder --role rlm-secondary

# verify
ergors provider roles
```

## What is the workflow we want to use?

**heuristic steps**

- determine what specific tool we should use (based on model)
- load the file (to cache?)

**agentic steps**
- determine what the question is asking
- use the tool to generate code for interacting with the document
- 

