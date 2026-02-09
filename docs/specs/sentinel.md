# Sentinel Mode

**Zero-secret deployment for Ergors nodes in headless environments**

## Overview

Sentinel mode enables secure remote bootstrapping of Ergors nodes in environments where you cannot interactively provide secrets (e.g., cloud deployments, Akash, Kubernetes). Instead of embedding sensitive credentials in deployment configurations, you start the node in a locked-down "sentinel" state and remotely initialize it using cryptographically signed requests.

## Activation Conditions

Sentinel mode is **not a separate command or flag**. It is a two-phase startup built into `ergors start`. The node automatically enters sentinel mode when **both** of these conditions are true:

1. **`ERGORS_ADMIN_PUBKEY` environment variable is set** — contains the admin's Ed25519 public key in hex format (64 hex chars)
2. **No `node_identity.enc` file exists** in the node's home directory

If either condition is false, the node starts normally as a full engine server:

| `ERGORS_ADMIN_PUBKEY` set? | `node_identity.enc` exists? | Result |
|---|---|---|
| Yes | No | **Sentinel mode** — lightweight bootstrap server |
| Yes | Yes | Normal startup — identity already provisioned |
| No | No | Normal startup — fails at identity load |
| No | Yes | Normal startup — uses existing identity |

After a successful bootstrap (init → api-keys → activate), the sentinel server shuts down and the same process seamlessly transitions to the full engine server. On subsequent restarts, `node_identity.enc` exists, so sentinel mode is skipped entirely.

## Security Architecture

Sentinel mode uses a multi-layered security approach:

1. **Ed25519 Authentication**: All requests must be signed with your admin private key
2. **X25519 + ChaCha20Poly1305 Encryption**: Secrets are encrypted end-to-end using ephemeral session keys
3. **Timestamp Replay Protection**: Requests older than 5 minutes are rejected
4. **Phase-based State Machine**: Ensures operations happen in the correct order

### Cryptographic Flow

```
┌─────────────┐                                    ┌─────────────┐
│  Local CLI  │                                    │   Sentinel  │
│  (Admin)    │                                    │   Server    │
└──────┬──────┘                                    └──────┬──────┘
       │                                                  │
       │  1. GET /sentinel/health                        │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │  {phase: "awaiting_init",                       │
       │   session_pubkey: "abc123..."}                  │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  2. POST /sentinel/init                         │
       │     Encrypted(custody_password, mnemonic)       │
       │     Signed with admin Ed25519 key               │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │  Creates: node_identity.enc, config.toml        │
       │  {status: "ok"}                                 │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  3. POST /sentinel/api-keys                     │
       │     Encrypted({anthropic: "sk-...", ...})       │
       │     Signed with admin Ed25519 key               │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │  Creates: api-keys.enc                          │
       │  {status: "ok"}                                 │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  4. POST /sentinel/activate                     │
       │     Encrypted({})                               │
       │     Signed with admin Ed25519 key               │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │  Sentinel shuts down, full server starts        │
       │  {status: "ok"}                                 │
       │<─────────────────────────────────────────────────│
```

## Quick Start

### 1. Generate Admin Keypair

First, generate an Ed25519 keypair for signing sentinel requests:

```bash
# Option A: Use OpenSSL
openssl genpkey -algorithm Ed25519 -outform DER -out admin.der
ADMIN_PRIVKEY_HEX=$(tail -c 32 admin.der | xxd -p -c 64)
openssl pkey -in admin.der -inform DER -pubout -outform DER -out admin_pub.der
ADMIN_PUBKEY_HEX=$(tail -c 32 admin_pub.der | xxd -p -c 64)

# Option B: Use ergors keygen (if available)
ergors keys generate --format hex --output admin_keys.json

# Save these securely - you'll need them for all sentinel operations
echo "Admin Private Key: $ADMIN_PRIVKEY_HEX"
echo "Admin Public Key: $ADMIN_PUBKEY_HEX"
```

### 2. Deploy Node in Sentinel Mode

Set the `ERGORS_ADMIN_PUBKEY` environment variable when starting your node. The node will automatically enter sentinel mode if no identity file exists.

**Docker:**
```bash
docker run -d \
  --name ergors-node \
  -p 8080:8080 \
  -e ERGORS_ADMIN_PUBKEY="your_admin_pubkey_hex" \
  -e ERGORS_API_PORT=8080 \
  -v ergors-data:/root/.ergors \
  ergors:latest start
```

**Akash SDL:**
```yaml
services:
  ergors:
    image: ergors:latest
    command: ["start"]
    env:
      - ERGORS_ADMIN_PUBKEY=your_admin_pubkey_hex
      - ERGORS_API_PORT=8080
    expose:
      - port: 8080
        as: 80
        to:
          - global: true
```

**Kubernetes:**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: ergors-node
spec:
  containers:
  - name: ergors
    image: ergors:latest
    args: ["start"]
    env:
    - name: ERGORS_ADMIN_PUBKEY
      valueFrom:
        secretKeyRef:
          name: ergors-admin
          key: pubkey
    - name: ERGORS_API_PORT
      value: "8080"
    ports:
    - containerPort: 8080
```

**Direct (for testing):**
```bash
ERGORS_ADMIN_PUBKEY="your_admin_pubkey_hex" \
ERGORS_API_PORT=8080 \
ergors --home ~/.ergors-sentinel start
```

### 3. Bootstrap the Node

From your local machine, use the `ergors sentinel bootstrap` command to initialize the node:

```bash
# Interactive mode (prompts for secrets)
ergors sentinel bootstrap \
  --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
  http://your-node-address:8080

# The CLI will interactively prompt for:
# - Remote custody password (min 8 chars)
# - Mnemonic phrase (or press Enter to generate new)
# - Anthropic API key
# - OpenAI API key
# - Akash ML API key
# - xAI/Grok API key
# - Custom provider names (Enter when done)
```

**Non-interactive mode (for automation):**
```bash
# Pipe inputs via stdin
printf '%s\n' \
  "my-strong-custody-password" \
  "" \
  "sk-ant-api-key-here" \
  "sk-openai-key-here" \
  "" \
  "" \
  "" \
| ergors sentinel bootstrap \
    --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
    http://your-node-address:8080
```

## Command Reference

### `ergors sentinel bootstrap`

Orchestrates the complete sentinel handshake: fetches session key, encrypts secrets, sends signed requests.

**Usage:**
```bash
ergors sentinel bootstrap [OPTIONS] <URL>
```

**Arguments:**
- `<URL>` - Sentinel HTTP endpoint (e.g., `http://host:8080`)

**Options:**
- `--admin-privkey-hex <HEX>` - Ed25519 private key for signing (64 hex chars)
  - **Required** for remote bootstrapping
  - Keep this secret! Anyone with this key can configure your node
  - Alternative: Use `ERGORS_ADMIN_PRIVKEY` environment variable

**Input Sequence (stdin):**

The command prompts for the following in order:

1. **Remote custody password** - Encrypts node identity and API keys (min 8 chars)
2. **Mnemonic phrase** - BIP-39 phrase for deterministic key derivation (or empty to generate new)
3. **Anthropic API key** - For Claude models (or empty to skip)
4. **OpenAI API key** - For GPT models (or empty to skip)
5. **Akash ML API key** - For Akash ML providers (or empty to skip)
6. **xAI/Grok API key** - For Grok models (or empty to skip)
7. **Custom provider names** - Loop until empty input

**Examples:**

```bash
# Generate new identity, configure Claude + OpenAI
ergors sentinel bootstrap \
  --admin-privkey-hex "abc123..." \
  http://node.example.com:8080

# Import existing mnemonic (deterministic keys)
printf '%s\n' \
  "my-password" \
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about" \
  "sk-ant-key" \
  "" "" "" "" \
| ergors sentinel bootstrap \
    --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
    http://node.example.com:8080

# Use environment variable for admin key
export ERGORS_ADMIN_PRIVKEY="your-admin-privkey-hex"
ergors sentinel bootstrap http://localhost:8080
```

**Exit Codes:**
- `0` - Bootstrap completed successfully
- `1` - Network error, authentication failure, or validation error
- `2` - Invalid arguments or missing required options

## Sentinel HTTP API

When a node starts in sentinel mode, it exposes a minimal HTTP API on the configured port (default 8080).

### Authentication

All endpoints except `/sentinel/health` require Ed25519 signatures:

**Request Headers:**
```
x-public-key: <admin_pubkey_hex>
x-timestamp: <unix_timestamp>
x-signature: <ed25519_signature_hex>
```

**Signature Calculation:**
```
message = body_bytes || timestamp_bytes
hash = blake3(message)
signature = ed25519_sign(admin_privkey, hash)
```

**Timestamp Validation:**
- Must be within 5 minutes of server time
- Prevents replay attacks

### Encryption

Request bodies for `/sentinel/init`, `/sentinel/api-keys`, and `/sentinel/activate` are encrypted using the sentinel's ephemeral X25519 session key (returned in `/sentinel/health`).

**Encryption:**
```
1. Fetch session_pubkey from /sentinel/health
2. Generate ephemeral X25519 keypair
3. Derive shared secret: x25519(client_privkey, session_pubkey)
4. Encrypt payload: chacha20poly1305(shared_secret, plaintext)
5. Send: {ciphertext, nonce, client_pubkey}
```

### Endpoints

#### `GET /sentinel/health`

Check sentinel phase and get session public key.

**Authentication:** None (public endpoint)

**Response:**
```json
{
  "phase": "awaiting_init",
  "version": "0.1.0",
  "session_pubkey": "abc123..."
}
```

**Phases:**
- `awaiting_init` - Waiting for initial configuration
- `awaiting_api_keys` - Identity created, waiting for API keys
- `awaiting_activation` - Ready to activate and hand off to full server
- `activating` - Shutting down sentinel, starting full server

#### `POST /sentinel/init`

Create node identity and configuration.

**Authentication:** Required (Ed25519 signature)

**Request Body (encrypted):**
```json
{
  "custody_password": "min-8-chars",
  "mnemonic": "optional-bip39-phrase",
  "node_type": "orchestrator",
  "api_port": 8080,
  "p2p_port": 50100,
  "host": "0.0.0.0"
}
```

**Response:**
```json
{
  "status": "ok"
}
```

**Side Effects:**
- Creates `node_identity.enc` (encrypted Ed25519 keypair)
- Creates `config.toml` (node configuration)
- Advances phase to `awaiting_api_keys`

**Errors:**
- `400` - Password too short (< 8 chars) or invalid mnemonic
- `401` - Invalid signature or stale timestamp
- `409` - Wrong phase (not awaiting_init)
- `500` - File system error

#### `POST /sentinel/api-keys`

Store encrypted LLM provider API keys.

**Authentication:** Required (Ed25519 signature)

**Request Body (encrypted):**
```json
{
  "api_keys": {
    "anthropic": "sk-ant-...",
    "openai": "sk-...",
    "custom_provider": "key-..."
  }
}
```

**Response:**
```json
{
  "status": "ok"
}
```

**Side Effects:**
- Creates `api-keys.enc` (encrypted key store)
- Advances phase to `awaiting_activation`

**Errors:**
- `400` - Empty api_keys map
- `401` - Invalid signature or stale timestamp
- `409` - Wrong phase (not awaiting_api_keys)
- `500` - Encryption or file system error

#### `POST /sentinel/activate`

Finalize configuration and hand off to full server.

**Authentication:** Required (Ed25519 signature)

**Request Body (encrypted):**
```json
{}
```

**Response:**
```json
{
  "status": "ok"
}
```

**Side Effects:**
- Signals sentinel shutdown
- Sets `ERGORS_CUSTODY_PASSWORD` environment variable
- Full server starts in the same process
- Sentinel endpoints become unavailable

**Errors:**
- `401` - Invalid signature or stale timestamp
- `409` - Wrong phase (not awaiting_activation)

## File Outputs

After successful bootstrap, the following files are created in the node's home directory:

### `node_identity.enc`

Encrypted node identity containing the Ed25519 keypair and metadata.

**Format:** JSON (encrypted with custody password)
```json
{
  "encryption_method": "argon2id-chacha20poly1305-v1",
  "salt": "...",
  "nonce": "...",
  "ciphertext": "...",
  "public_key": [63, 125, 8, 48, ...],
  "metadata": {
    "user": "ergors",
    "host": "0.0.0.0",
    "p2p_port": 50100,
    "api_port": 8080,
    "node_type": "orchestrator"
  }
}
```

**Permissions:** `0600` (owner read/write only)

### `config.toml`

Node configuration in TOML format.

**Example:**
```toml
[identity]
user = "ergors"
host = "0.0.0.0"
api_port = 8080
p2p_port = 50100
node_type = "orchestrator"
public_key = ""

[network]
listen_address = "0.0.0.0"
bootstrap_peers = []

[storage]
data_dir = "/root/.ergors/memories"

[llm]
api_keys_file = "/root/.ergors/api-keys.json"
timeout_seconds = 60
```

### `api-keys.enc`

Encrypted API key store.

**Format:** Binary (encrypted with custody password)
- Uses same encryption as node identity
- Decrypted in-memory only
- Never written to disk in plaintext

## Security Best Practices

### Admin Key Management

**Generate Once, Store Securely:**
```bash
# Generate keypair
openssl genpkey -algorithm Ed25519 -outform DER -out admin.der
ADMIN_PRIVKEY=$(tail -c 32 admin.der | xxd -p -c 64)
ADMIN_PUBKEY=$(openssl pkey -in admin.der -inform DER -pubout -outform DER | tail -c 32 | xxd -p -c 64)

# Store in password manager or encrypted vault
echo "ADMIN_PRIVKEY=$ADMIN_PRIVKEY" >> ~/.ergors-admin-keys.env
chmod 600 ~/.ergors-admin-keys.env

# For production: use hardware security module (HSM) or cloud KMS
```

**Never:**
- ❌ Commit admin keys to version control
- ❌ Include in deployment manifests
- ❌ Share via insecure channels
- ❌ Reuse across environments

**Always:**
- ✅ Generate unique keys per deployment
- ✅ Store in encrypted secrets manager
- ✅ Rotate keys periodically
- ✅ Use separate keys for dev/staging/prod

### Network Security

**TLS Termination:**

For production deployments, always use TLS:

```bash
# Deploy behind reverse proxy with TLS
# Nginx example:
server {
    listen 443 ssl;
    server_name node.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

**Firewall Rules:**

Restrict sentinel port access during bootstrap:

```bash
# Only allow from your IP during setup
ufw allow from YOUR_IP_ADDRESS to any port 8080

# After bootstrap, lock down to authenticated endpoints only
# (full server has its own authentication layer)
```

### Credential Management

**Custody Password Strength:**

Use a strong, unique password for the custody encryption:

```bash
# Generate strong password
CUSTODY_PASSWORD=$(openssl rand -base64 32)

# Store securely (not in shell history)
export HISTFILE=/dev/null
```

**API Key Rotation:**

API keys are encrypted at rest. To rotate:

```bash
# 1. Stop the node
ergors stop

# 2. Re-run sentinel bootstrap with new keys
# (requires redeployment in sentinel mode)

# 3. Or use provider management commands (post-bootstrap)
ergors provider update anthropic --api-key "new-key"
```

## Troubleshooting

### Sentinel Won't Start

**Symptom:** Node starts normally instead of sentinel mode

**Cause:** Identity file already exists

**Solution:**
```bash
# Remove existing identity to enable sentinel mode
rm ~/.ergors/node_identity.enc

# Or use a fresh home directory
ERGORS_ADMIN_PUBKEY="..." ergors --home /tmp/new-node start
```

### Authentication Failures

**Symptom:** `401 Unauthorized` errors during bootstrap

**Causes & Solutions:**

1. **Wrong admin key:**
   ```bash
   # Verify pubkey matches
   echo $ADMIN_PUBKEY_HEX
   # Check server logs for expected pubkey
   ```

2. **Clock skew (> 5 minutes):**
   ```bash
   # Sync system time
   sudo ntpdate -s time.nist.gov
   # Or adjust server clock
   ```

3. **Invalid signature:**
   ```bash
   # Verify key format (64 hex chars)
   echo $ADMIN_PRIVKEY_HEX | wc -c  # Should be 65 (64 + newline)
   ```

### Connection Refused

**Symptom:** Cannot connect to sentinel endpoint

**Solutions:**
```bash
# Check if node is running
ps aux | grep ergors

# Check if port is listening
lsof -i :8080
netstat -tlnp | grep 8080

# Check firewall
ufw status
iptables -L -n

# Check logs
tail -f ~/.ergors/node.log
```

### Phase Errors (409 Conflict)

**Symptom:** `invalid phase: expected awaiting_init, got awaiting_api_keys`

**Cause:** Trying to repeat a phase that already completed

**Solution:**
```bash
# Check current phase
curl http://node:8080/sentinel/health

# If you need to restart from scratch:
# 1. Stop the node
# 2. Remove created files
rm ~/.ergors/node_identity.enc ~/.ergors/config.toml ~/.ergors/api-keys.enc
# 3. Restart in sentinel mode
```

### Full Server Won't Start After Activation

**Symptom:** Sentinel activates, but `/health` endpoint doesn't respond

**Possible Causes:**

1. **Provider validation failure** (missing required API keys)
   - Check node logs for validation errors
   - Ensure all enabled providers have keys configured

2. **Config validation error**
   - Review `config.toml` for syntax errors
   - Check storage path permissions

3. **Port conflict**
   - Another process may have bound to the port
   - Check with `lsof -i :8080`

**Debug:**
```bash
# Check node logs
tail -f ~/.ergors/node.log

# Look for specific errors
grep -i "error\|failed" ~/.ergors/node.log | tail -20

# Verify files were created
ls -la ~/.ergors/
```

## Advanced Usage

### Mnemonic-based Deterministic Keys

Use a BIP-39 mnemonic for reproducible node identities:

```bash
# Generate mnemonic (externally)
MNEMONIC="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

# Bootstrap with known mnemonic
printf '%s\n' \
  "custody-password" \
  "$MNEMONIC" \
  "sk-ant-key" "" "" "" "" \
| ergors sentinel bootstrap \
    --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
    http://node:8080

# Same mnemonic always produces same node public key
# Useful for:
# - Node migration/recovery
# - Deterministic testing
# - Key escrow/backup
```

### Automated Deployment Pipeline

**CI/CD Integration Example:**

```bash
#!/bin/bash
# deploy-sentinel-node.sh

set -euo pipefail

# Load secrets from vault
ADMIN_PRIVKEY=$(vault kv get -field=privkey secret/ergors/admin)
CUSTODY_PW=$(vault kv get -field=password secret/ergors/custody)
ANTHROPIC_KEY=$(vault kv get -field=key secret/ergors/anthropic)

# Deploy to Akash
akash tx deployment create deployment.yml --from my-wallet

# Wait for deployment to be ready
sleep 30
DEPLOYMENT_URL=$(akash query deployment get ...)

# Bootstrap node non-interactively
printf '%s\n' \
  "$CUSTODY_PW" \
  "" \
  "$ANTHROPIC_KEY" \
  "" "" "" "" \
| ergors sentinel bootstrap \
    --admin-privkey-hex "$ADMIN_PRIVKEY" \
    "$DEPLOYMENT_URL"

echo "✅ Node deployed and bootstrapped at $DEPLOYMENT_URL"
```

### Multi-Node Fleet Management

**Bootstrap multiple nodes with different identities:**

```bash
#!/bin/bash
# bootstrap-fleet.sh

NODES=(
  "http://node1.example.com:8080"
  "http://node2.example.com:8080"
  "http://node3.example.com:8080"
)

for NODE_URL in "${NODES[@]}"; do
  echo "Bootstrapping $NODE_URL..."

  # Each node gets its own password and keys
  CUSTODY_PW=$(openssl rand -base64 24)

  printf '%s\n' \
    "$CUSTODY_PW" \
    "" \
    "$SHARED_ANTHROPIC_KEY" \
    "" "" "" "" \
  | ergors sentinel bootstrap \
      --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
      "$NODE_URL" &
done

wait
echo "✅ All nodes bootstrapped"
```

### Custom Provider Configuration

Add custom LLM providers during bootstrap:

```bash
# Bootstrap with custom provider
printf '%s\n' \
  "custody-password" \
  "" \
  "sk-ant-key" \
  "" \
  "" \
  "" \
  "my-custom-llm" \
  "custom-api-key-here" \
  "" \
| ergors sentinel bootstrap \
    --admin-privkey-hex "$ADMIN_PRIVKEY_HEX" \
    http://node:8080
```

## Implementation Details

### State Machine

Sentinel mode is implemented as a strict state machine to prevent configuration errors:

```
┌────────────────┐
│ awaiting_init  │  (Start state)
└────────┬───────┘
         │ POST /sentinel/init
         │ (creates identity + config)
         ▼
┌────────────────────┐
│ awaiting_api_keys  │
└────────┬───────────┘
         │ POST /sentinel/api-keys
         │ (stores encrypted keys)
         ▼
┌───────────────────────┐
│ awaiting_activation   │
└────────┬──────────────┘
         │ POST /sentinel/activate
         │ (signals handoff)
         ▼
┌──────────────┐
│  activating  │  (Terminal state)
└──────────────┘
         │
         ▼
   (Sentinel shuts down,
    full server starts)
```

### Encryption Details

**X25519 Key Exchange:**
```rust
// Server generates ephemeral keypair on startup
let session_privkey = X25519Secret::random();
let session_pubkey = X25519PublicKey::from(&session_privkey);

// Client derives shared secret
let client_privkey = X25519Secret::random();
let shared_secret = client_privkey.diffie_hellman(&session_pubkey);

// Both sides use same shared secret for ChaCha20Poly1305
```

**ChaCha20Poly1305 AEAD:**
```rust
let key = Key::from_slice(&shared_secret.as_bytes()[..32]);
let cipher = ChaCha20Poly1305::new(key);
let nonce = Nonce::from_slice(random_12_bytes);
let ciphertext = cipher.encrypt(nonce, plaintext)?;
```

**Signature Verification:**
```rust
// Server verifies Ed25519 signatures
let message = [body_bytes, timestamp_bytes].concat();
let hash = blake3::hash(&message);
verifier.verify(None, &hash, &signature)?;
```

### Process Lifecycle

```rust
// main.rs (simplified)
fn start() {
    let admin_pubkey = env::var("ERGORS_ADMIN_PUBKEY").ok();
    let identity_exists = home.join("node_identity.enc").exists();

    if admin_pubkey.is_some() && !identity_exists {
        // Run sentinel server (blocks until activation)
        let custody_password = SentinelServer::new(pubkey, home)
            .run()
            .await?;

        // Sentinel done, set password for full server
        env::set_var("ERGORS_CUSTODY_PASSWORD", custody_password);
    }

    // Start full server (same process)
    Runner::new(RuntimeConfig::default()).start(|ctx| async move {
        let config = ErgorsConfig::load(home.join("config.toml"))?;
        let server = Server::new(config, ctx).await?;
        server.run(shutdown_signal).await?;
    });
}
```

## FAQ

**Q: Can I change the admin key after deployment?**

A: No. The admin pubkey is only used during initial sentinel bootstrapping. After activation, the node uses its own Ed25519 identity for authentication. You would need to redeploy in sentinel mode with a new admin key.

**Q: What happens if I lose the custody password?**

A: The custody password encrypts `node_identity.enc` and `api-keys.enc`. Without it, these files cannot be decrypted and the node cannot start. You would need to redeploy and bootstrap from scratch. For deterministic recovery, use a BIP-39 mnemonic during initial bootstrap.

**Q: Can I bootstrap the same node twice?**

A: No. Once `node_identity.enc` exists, the node will start normally (not in sentinel mode). To re-bootstrap, you must delete the identity file and restart with `ERGORS_ADMIN_PUBKEY` set.

**Q: Is the admin private key stored on the node?**

A: No. The admin key is only used by the local `ergors sentinel bootstrap` command to sign requests. The node only knows the admin public key for signature verification during sentinel mode.

**Q: Can I use sentinel mode in local development?**

A: Yes, but it's simpler to use `ergors init` for local development. Sentinel mode is designed for remote/headless deployments where you cannot interactively provide secrets.

**Q: What ports does sentinel mode use?**

A: Sentinel uses the same port as the full server (configured via `ERGORS_API_PORT`, default 8080). The sentinel HTTP API replaces the full server API until activation completes.

**Q: How long does the session key last?**

A: The ephemeral X25519 session key exists only in memory during the sentinel server's lifetime. It is generated on sentinel startup and destroyed when the sentinel shuts down (after activation).

**Q: Can multiple clients bootstrap the same node?**

A: Only if they have the same admin private key. However, once a phase completes (e.g., `awaiting_init` → `awaiting_api_keys`), that phase cannot be repeated. The state machine only moves forward.

**Q: What happens if bootstrap fails partway through?**

A: Check the current phase with `GET /sentinel/health`. You can continue from that phase. If you need to start over, stop the node, delete created files, and restart in sentinel mode.

## See Also

- [Node Identity Management](./custody-and-auth.md) - Understanding node keypairs and custody
- [Configuration Management](./config.md) - Managing node configuration
- [Deployment Workflows](./workflows.md) - Deploying nodes to various platforms
- [Key Management](./key-management.md) - Comprehensive key management guidelines
