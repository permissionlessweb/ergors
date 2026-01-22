# ERGORS Key Management & Custody

## Overview

ERGORS employs a defense-in-depth approach to key management, separating sensitive cryptographic material from configuration data. The system distinguishes between two categories of secrets:

1. **Node Identity Keys** - Ed25519 keypairs used for network authentication, message signing, and peer identification
2. **API Keys** - Provider credentials for external services (LLM providers, cloud APIs)

Both are protected through the **custody system**, which ensures secrets are never stored in plaintext configuration files.

---

## Design Principles

### Separation of Concerns

```
┌─────────────────────────────────────────────────────────────┐
│                    Configuration Layer                       │
│  config.toml - Public settings, network params, metadata    │
│  (Safe to inspect, version control, share)                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      Custody Layer                           │
│  Encrypted storage for all sensitive cryptographic material │
│  (Password-protected, never in plaintext)                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Runtime Memory                           │
│  Decrypted keys cached temporarily with configurable TTL    │
│  (Cleared on lock, timeout, or shutdown)                    │
└─────────────────────────────────────────────────────────────┘
```

### Zero Plaintext Storage

Private keys and API credentials are **never** written to disk in readable form:

- Node identity private keys → Encrypted with password-derived key
- API keys → Stored in dedicated file with environment variable references
- SSH keys → Derived on-demand from custody-protected identity

### Authentication Boundary

The custody system creates a clear authentication boundary at startup:

```
                    ┌──────────────────┐
                    │   Engine Start   │
                    └────────┬─────────┘
                             │
                             ▼
                    ┌──────────────────┐
                    │ Password Prompt  │◄── User Authentication
                    └────────┬─────────┘
                             │
                             ▼
                    ┌──────────────────┐
                    │  Custody Unlock  │
                    └────────┬─────────┘
                             │
                             ▼
                    ┌──────────────────┐
                    │  Network Start   │◄── Keys now available
                    └──────────────────┘
```

---

## Node Identity Keys

### Purpose

The node identity keypair serves multiple functions:

| Function | Description |
|----------|-------------|
| **Network Authentication** | Proves node identity to peers during P2P handshake |
| **Message Signing** | Signs outbound messages for integrity verification |
| **Peer Identification** | Public key serves as unique node identifier |
| **SSH Authentication** | Derived SSH keys for git operations |

### Security Model

**Encryption at Rest**
- Algorithm: Argon2id key derivation + ChaCha20-Poly1305 AEAD
- The password never touches disk; only the derived encryption key operates on data
- Salt stored alongside ciphertext for key re-derivation

**Memory Protection**
- Decrypted keys held in memory only while needed
- Configurable cache TTL (default: 5 minutes)
- Explicit lock clears all cached material
- Process termination zeroes sensitive memory

**Key Derivation Parameters**
- Memory cost: 2 GiB (resistant to GPU attacks)
- Time cost: 1 iteration
- Parallelism: 4 lanes
- Output: 256-bit encryption key

### Lifecycle

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Create    │────►│   Store     │────►│    Use      │
│  (once)     │     │ (encrypted) │     │ (unlocked)  │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                    ┌─────────────┐            │
                    │   Rotate    │◄───────────┘
                    │ (re-encrypt)│
                    └─────────────┘
```

1. **Creation** - New keypair generated with cryptographically secure randomness
2. **Storage** - Private key encrypted immediately, never exists in plaintext on disk
3. **Usage** - Unlocked on-demand, cached briefly, auto-locked on timeout
4. **Rotation** - Password change re-encrypts without exposing plaintext

---

## Ephemeral Keys

### Purpose

Ephemeral keys provide short-lived cryptographic material for:

| Use Case | Description |
|----------|-------------|
| **Provider Key Derivation** | Derive per-request encryption keys for API credentials |
| **Node Communication** | Session-specific encryption between peers |
| **Request Signing** | One-time signatures that can't be replayed |

### Security Properties

- **Short TTL** - 1 hour default lifetime (configurable)
- **Memory-only** - Never persisted to disk
- **Auto-zeroization** - Memory cleared on expiration or drop
- **Scope isolation** - Keys scoped to specific purposes

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     EphemeralKeyManager                          │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Key Cache (RwLock)                      │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │ key_id → EphemeralKey { material, created, scope }  │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Background Cleanup Task                       │  │
│  │  (Runs every 60s, removes expired keys, zeroes memory)    │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Derivation

Provider keys are derived using HKDF from the master ephemeral key:

```rust
// Derive a provider-specific ephemeral key
let key = manager.derive_and_store_provider_key(
    provider: "anthropic",
    context: &request_context,
).await?;

// Key is automatically scoped and will be cleaned up
```

### Default Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `ttl` | 3600s (1 hour) | Key lifetime |
| `cleanup_interval` | 60s | How often cleanup runs |
| `max_keys` | 1000 | Maximum cached keys |

---

## API Keys

### Purpose

API keys authenticate the node to external services:

| Provider Type | Examples |
|---------------|----------|
| **LLM Providers** | Anthropic, OpenAI, local Ollama |
| **Cloud Platforms** | Akash, AWS, Phala |
| **External APIs** | Custom integrations |

### Security Model

**Storage Strategy**
- Dedicated `api-keys.json` file separate from main config
- Keys can be literal values or environment variable references
- File excluded from version control by default

**Environment Variable Pattern**
```
api_key: "${ANTHROPIC_API_KEY}"
```
- Actual secret stored in environment or `.env` file
- Config file contains only the reference
- Enables secret injection from external secret managers

**Access Control**
- File permissions restricted to owner (600)
- Loaded once at startup, not re-read
- Provider connections use keys directly, never logged

### Provider Isolation

Each provider's credentials are scoped independently:

```
┌─────────────────────────────────────────┐
│              api-keys.json              │
├─────────────────────────────────────────┤
│  anthropic:                             │
│    api_key: "${ANTHROPIC_API_KEY}"      │
│    ────────────────────────────────     │
│  openai:                                │
│    api_key: "${OPENAI_API_KEY}"         │
│    ────────────────────────────────     │
│  akash:                                 │
│    api_key: "${AKASH_API_KEY}"          │
└─────────────────────────────────────────┘
```

Compromise of one provider's key does not expose others.

---

## API Key Distribution

For multi-node deployments, API keys can be securely distributed from the coordinator to other nodes using **Shamir Secret Sharing** or **Direct Encryption**.

### Ownership Models

```
┌─────────────────────────────────────────────────────────────────┐
│                     Provider Ownership Types                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SHARED (Anthropic, OpenAI)          LOCAL (Ollama)             │
│  ┌─────────────────────────┐         ┌─────────────────────────┐│
│  │ Coordinator holds key   │         │ Each node has own key   ││
│  │ Distributes via Shamir  │         │ Not distributed         ││
│  │ n-of-m threshold        │         │ Local use only          ││
│  └─────────────────────────┘         └─────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Shamir Secret Sharing (Shared Providers)

For shared providers like Anthropic and OpenAI, the coordinator splits the API key:

```
                        ┌─────────────────┐
                        │  Coordinator    │
                        │  API Key: sk-...│
                        └────────┬────────┘
                                 │
                    Shamir Split (2-of-3)
                                 │
          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
          ▼                      ▼                      ▼
    ┌───────────┐          ┌───────────┐          ┌───────────┐
    │  Share 1  │          │  Share 2  │          │  Share 3  │
    │  Node A   │          │  Node B   │          │  Node C   │
    └───────────┘          └───────────┘          └───────────┘

    Any 2 shares can reconstruct the key.
    No single node can access the key alone.
```

#### Distribution Flow

1. **Node Bootstrap**: Node authenticates to coordinator with challenge-response
2. **Identity Verification**: Coordinator verifies node's Ed25519 signature
3. **Share Generation**: Coordinator generates Shamir shares for approved nodes
4. **Encrypted Transfer**: Each share encrypted with recipient's derived decaf377-ka key
5. **Local Storage**: Node stores encrypted share in ephemeral memory

#### Configuration (providers.toml)

```toml
[[providers]]
name = "anthropic"
ownership = 1  # SHARED
[providers.sharing_config]
mode = 2       # SHAMIR
threshold = 2
total_shares = 3

[[providers]]
name = "openai"
ownership = 1  # SHARED
[providers.sharing_config]
mode = 2       # SHAMIR
threshold = 2
total_shares = 3

[[providers]]
name = "ollama"
ownership = 2  # LOCAL
[providers.sharing_config]
mode = 1       # DIRECT
threshold = 1
total_shares = 1
```

### Direct Mode (Local Providers)

For local providers like Ollama, each node manages its own key:

```rust
// Direct mode: 1-to-1 encrypted transfer (no splitting)
let config = SecretSharingConfig {
    mode: KeySharingMode::Direct,
    threshold: 1,
    total_shares: 1,
};
```

### Network Protocol (Channel 4)

Key distribution uses a dedicated P2P channel with rate limiting:

```
Channel 4: Key Sharing
├── Rate limit: 10 messages/second
├── Message types:
│   ├── KeyShareRequest  - Node requests its shares
│   ├── KeyShareResponse - Coordinator sends encrypted shares
│   ├── KeyRevocation    - Coordinator revokes a node's shares
│   └── KeyHeartbeat     - Node confirms shares still valid
```

### Default Behavior

| Provider | Ownership | Sharing Mode | Default Threshold |
|----------|-----------|--------------|-------------------|
| `anthropic` | Shared | Shamir | 2-of-3 |
| `openai` | Shared | Shamir | 2-of-3 |
| `ollama` | Local | Direct | 1-of-1 |
| Custom | Configurable | Configurable | User-defined |

### Security Guarantees

1. **Threshold Security**: No single node compromise exposes shared API keys
2. **Forward Secrecy**: Ephemeral keys used for share encryption
3. **Revocation**: Coordinator can revoke shares for compromised nodes
4. **Auditability**: All share distributions logged with timestamps

---

## Custody Backends

The custody system supports multiple backend implementations for different security requirements:

| Backend | Security Level | Use Case |
|---------|----------------|----------|
| **Password Encrypted** | Production | Standard deployments with human operators |
| **Environment Variable** | Automation | CI/CD pipelines, containerized deployments |
| **Plaintext** | Development | Local testing only (not for production) |
| **Threshold** | High Security | Multi-party key management (future) |
| **Remote Custody** | Enterprise | Hardware security modules, vault services (future) |

### Backend Selection

The active backend is determined by configuration:

```
[custody]
backend = "password_encrypted"
```

For automated deployments, the password can be injected:

```bash
export ERGORS_CUSTODY_PASSWORD="..."
ergors start
```

---

## Operational Security

### Startup Flow

1. Engine loads public configuration (`config.toml`)
2. Custody backend initialized based on config
3. **Authentication checkpoint**: Password required to proceed
4. Private key decrypted and cached
5. Network layer initialized with authenticated identity
6. API keys loaded for provider connections

### Runtime Behavior

- Keys remain cached for configured TTL
- Cache refreshed on use (sliding expiration)
- Explicit `lock` command clears all cached secrets
- Graceful shutdown clears memory before exit

### Failure Modes

| Scenario | Behavior |
|----------|----------|
| Wrong password | Unlock fails, engine does not start |
| Corrupted identity file | Error reported, manual recovery required |
| Missing API keys | Warning logged, affected providers disabled |
| Cache timeout | Next operation triggers re-authentication |

---

## Migration Path

### From Plaintext Keys

Existing deployments with plaintext keys in `config.toml` are automatically migrated:

1. Engine detects plaintext `private_key` in config
2. User prompted to create custody password
3. Key encrypted and stored in custody
4. Original plaintext key should be manually removed from config
5. Future startups use custody exclusively

### Key Rotation

To rotate the node identity (new keypair):

1. Stop the engine
2. Delete or rename existing `node_identity.enc`
3. Start engine - new identity created automatically
4. Update any systems that reference the old public key

To change the custody password (same keypair):

1. Unlock custody with current password
2. Use password change operation
3. Key re-encrypted with new password
4. Old password immediately invalid

---

## File Reference

| File | Contents | Protection |
|------|----------|------------|
| `config.toml` | Public configuration | None (safe to share) |
| `node_identity.enc` | Encrypted private key | Password encryption |
| `api-keys.json` | Provider credentials | File permissions + env vars |
| `.env` | Environment secrets | File permissions |
| `ssh/id_ed25519` | Derived SSH private key | File permissions (600) |
| `ssh/id_ed25519.pub` | SSH public key | None (safe to share) |

---

## Security Recommendations

1. **Use strong passwords** - Minimum 12 characters, avoid dictionary words
2. **Protect the home directory** - Restrict access to the ERGORS data directory
3. **Use environment variables** - Prefer `${VAR}` references over literal API keys
4. **Rotate credentials** - Change custody password and API keys periodically
5. **Secure backups** - Encrypted identity file is safe to backup; protect password separately
6. **Monitor access** - Review logs for unexpected unlock attempts
7. **Limit plaintext exposure** - Never echo, log, or display decrypted keys

---

## Threat Model

### Protected Against

- Disk theft (encrypted at rest)
- Config file exposure (no secrets in config)
- Memory dumps after lock (keys cleared)
- Brute force (Argon2 memory-hard KDF)
- Credential stuffing (unique per-provider keys)

### Not Protected Against

- Compromised host with root access (memory readable while unlocked)
- Weak passwords (user responsibility)
- Stolen password (authentication bypass)
- Side-channel attacks on running process

For high-security deployments requiring protection against these threats, consider threshold custody or hardware security modules (HSM) when available.
