# ERGORS Security: Custody, Cryptography & Authentication

## Agentic Session Context

> **For AI agents:** Quick context for ERGORS security. See implementation in source files below.

| Concept | Type/Trait | Source |
|---------|-----------|-------- |
| Node keypair | `NodePrivKey` / `NodePubkey` | [`keys/commonware.rs`](../../packages/ho-std/src/keys/commonware.rs) |
| Custody trait | `NodeIdentityCustody` | [`traits/mod.rs`](../../packages/ho-std/src/traits/mod.rs) |
| Password custody | `PasswordEncryptedCustody` | [`custody/node_identity.rs`](../../packages/ho-std/src/custody/node_identity.rs) |
| Identity storage | `IdentityStorage` | [`storage/identity.rs`](../../packages/ho-std/src/storage/identity.rs) |
| Network integration | `start_network_with_custody` | [`network/manager.rs`](../../packages/ergors/src/network/manager.rs) |
| Config helpers | `ErgorsConfig` | [`config.rs`](../../packages/ergors/src/config.rs) |
| Contract authenticators | `contract_auth_middleware` | [`auth/middleware.rs`](../../packages/ergors/src/auth/middleware.rs) |
| Authenticator handlers | `handle_*_authenticator` | [`auth/handlers.rs`](../../packages/ergors/src/auth/handlers.rs) |

**Security model:** Private keys encrypted at rest (Argon2 + ChaCha20Poly1305), decrypted on-demand with TTL caching.

---

## Overview

ERGORS implements layered security:

| Layer | Purpose | Primitives | Source |
|-------|---------|------------|--------|
| Identity Custody | Secure key storage | Argon2 + ChaCha20Poly1305 | [`custody/`](../../packages/ho-std/src/custody/) |
| API Authentication | Request signing | Ed25519 | [`middleware/`](../../packages/ergors/src/middleware/) |
| Transport Encryption | Node communication | X25519 + ChaCha20Poly1305 | [`network/`](../../packages/ergors/src/network/) |

---

## 1. Node Identity Custody

### Why Custody Matters

Every node has an Ed25519 keypair used for:

- Network authentication
- Message signing
- SSH key derivation (git operations)
- API key encryption

### Custody Backends

#### Password-Encrypted (Production)

> **Implementation:** [`custody/node_identity.rs:PasswordEncryptedCustody`](../../packages/ho-std/src/custody/node_identity.rs)

```rust
use ho_std::custody::PasswordEncryptedCustody;
use ho_std::storage::identity::EncryptedIdentityBuilder;

let custody = PasswordEncryptedCustody::new("~/.ergors/node_identity.enc");

// First-time setup
if !custody.exists() {
    let metadata = EncryptedIdentityBuilder::new()
        .user("ergors")
        .host("127.0.0.1")
        .p2p_port(26969)
        .build();
    custody.create_identity("password", Some(metadata))?;
}

// Usage: unlock -> use -> lock
custody.unlock("password").await?;
let sig = custody.sign_ed25519(Some(b"ns"), b"msg").await?;
custody.lock().await;
```

**Encryption:** Argon2id (2MB memory, 1 iter, 4 parallel) + ChaCha20Poly1305 AEAD

#### Plaintext (Testing Only)

> **Implementation:** [`custody/node_identity.rs:PlaintextCustody`](../../packages/ho-std/src/custody/node_identity.rs)

```rust
use ho_std::custody::PlaintextCustody;

let custody = PlaintextCustody::generate();  // Always unlocked
let sig = custody.sign_ed25519(None, b"test").await?;
```

### The NodeIdentityCustody Trait

> **Definition:** [`traits/mod.rs`](../../packages/ho-std/src/traits/mod.rs)

All backends implement:

| Method | Description |
|--------|-------------|
| `public_key()` | Get public key (no unlock needed) |
| `get_private_key()` | Get private key (requires unlock) |
| `sign_ed25519(ns, msg)` | Sign with optional namespace |
| `export_ssh_keys(dir)` | Generate OpenSSH keys |
| `is_unlocked()` | Check cache status |
| `lock()` | Clear cached key |
| `get_key_bytes()` | Raw 32-byte key for derived encryption |

### Configuration

> **Proto:** [`storage/v1/storage.proto:NodeIdentityCustodyConfig`](../../proto/ergors/storage/v1/storage.proto)

```toml
[custody]
backend = "password_encrypted"  # plaintext | threshold | remote:endpoint
cache_keys = true
cache_ttl_secs = 300
identity_path = "~/.ergors/node_identity.enc"
```

Config helpers in [`config.rs`](../../packages/ergors/src/config.rs):

```rust
let custody = config.create_password_custody();
custody.unlock(&password).await?;
```

### Encrypted Identity File Format

> **Proto:** [`storage/v1/storage.proto:EncryptedNodeIdentity`](../../proto/ergors/storage/v1/storage.proto)

```json
{
  "public_key": "<32-bytes-hex>",
  "encrypted_private_key": "<encrypted-blob>",
  "encryption_method": "argon2id-chacha20poly1305-v1",
  "version": 1,
  "metadata": { "user": "...", "host": "...", "p2p_port": 26969 }
}
```

---

## 2. API Authentication

### Request Signing

> **Middleware:** [`middleware/auth.rs`](../../packages/ergors/src/middleware/auth.rs)

| Field | Purpose |
|-------|---------|
| `timestamp` | Unix ms - prevents replay (±5min window) |
| `nonce` | 32-byte random - prevents replay |
| `signature` | Ed25519 over `namespace ‖ timestamp ‖ nonce ‖ sha256(payload)` |
| `public_key` | Sender identity |

### Protected Endpoints

| Endpoint | Purpose |
|----------|---------|
| `/orchestrate/bootstrap` | Node bootstrap |
| `/orchestrate/fractal` | Task delegation |
| `/network/topology` | Network state |
| `/auth/*` | Authenticator management |

---

## 3. Contract-Based Middleware Authenticators

ERGORS supports programmable authentication via CosmWasm contracts. Nodes can register custom authenticator contracts for specific API endpoints, enabling flexible access control beyond static Ed25519 signature verification.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        API Request                               │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│              contract_auth_middleware                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 1. Extract endpoint path (normalize to label)            │   │
│  │ 2. Lookup authenticator contract in storage              │   │
│  │ 3. If contract exists → query contract for authorization │   │
│  │ 4. If no contract → proceed (fallback to Ed25519 auth)   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                │
            ┌───────────────────┴───────────────────┐
            ▼                                       ▼
┌─────────────────────┐               ┌─────────────────────────┐
│  Contract Query     │               │  Standard Auth Layer    │
│  {"is_allowed":     │               │  (Ed25519 signatures)   │
│   {"address": ...}} │               │                         │
└─────────────────────┘               └─────────────────────────┘
```

### Authenticator Registry

> **Storage:** [`storage.rs`](../../packages/ergors/src/storage.rs)

Each node maintains an authenticator registry in Cnidarium storage:

| Key Pattern | Value | Description |
|-------------|-------|-------------|
| `authenticators/{endpoint_label}` | Contract address | Maps endpoint to authenticator contract |
| `authenticators/metadata/{endpoint_label}` | JSON metadata | Description, created_at timestamp |

```rust
// Storage operations
storage.put_authenticator("api/prompts", "ergors1abc...").await?;
storage.get_authenticator("api/prompts").await?;  // Some("ergors1abc...")
storage.list_authenticators().await?;  // Vec<(label, address)>
storage.delete_authenticator("api/prompts").await?;
```

### Contract Interface

Authenticator contracts must implement this query interface:

**Query:**

```json
{"is_allowed": {"address": "ergors{node_id}_{pubkey_hash}"}}
```

**Response:**

```json
{"allowed": true}
```

The caller address is deterministically generated from:

- Node's public key (first 8 hex chars as node identifier)
- Caller's public key hash (first 20 bytes of SHA256)

Format: `ergors{node_id}_{caller_pubkey_hash}`

### Management API

> **Handlers:** [`auth/handlers.rs`](../../packages/ergors/src/auth/handlers.rs)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/auth/register` | POST | Register authenticator for endpoint |
| `/auth/list` | GET | List all registered authenticators |
| `/auth/check` | GET | Check if address is authorized |
| `/auth/{endpoint_label}` | DELETE | Remove authenticator |

**Register Request:**

```json
{
  "endpoint_label": "api/prompts",
  "contract_address": "ergors1abc...",
  "description": "Whitelist for prompt API"
}
```

**Register Response:**

```json
{
  "success": true,
  "message": "Authenticator registered for endpoint 'api/prompts'",
  "entry": {
    "endpoint_label": "api/prompts",
    "contract_address": "ergors1abc...",
    "description": "Whitelist for prompt API",
    "created_at": "2024-01-15T10:30:00Z",
    "active": true
  }
}
```

**List Query Parameters:**

- `endpoint_prefix` - Filter by endpoint prefix
- `limit` - Pagination limit (default 100)
- `offset` - Pagination offset

**Check Query Parameters:**

- `endpoint_label` - Endpoint to check
- `address` - Address to verify

### Provided Contract Templates

#### cw-middleware-auth

> **Source:** [`contracts/cw-middleware-auth/`](../../contracts/cw-middleware-auth/)

Controls who can update the authenticator registry itself. Only the coordinator or explicitly authorized addresses can modify registry entries.

**InstantiateMsg:**

```json
{
  "coordinator": "ergors1coordinator...",
  "initial_authorized": ["ergors1admin1...", "ergors1admin2..."]
}
```

**ExecuteMsg:**

```json
// Add authorized updater
{"authorize": {"address": "ergors1new..."}}

// Remove authorized updater
{"revoke": {"address": "ergors1old..."}}

// Transfer coordinator role
{"transfer_coordinator": {"new_coordinator": "ergors1newcoord..."}}
```

**QueryMsg:**

```json
// Check if address can update registry
{"is_authorized": {"address": "ergors1check..."}}

// List all authorized addresses
{"list_authorized": {}}
```

#### cw-auth

> **Source:** [`contracts/cw-auth/`](../../contracts/cw-auth/)

Whitelist-based endpoint authentication supporting both allowlist mode (default deny) and blocklist mode (default allow).

**InstantiateMsg:**

```json
{
  "admin": "ergors1admin...",
  "description": "API access whitelist",
  "initial_whitelist": ["ergors1user1...", "ergors1user2..."],
  "default_allow": false
}
```

**ExecuteMsg:**

```json
// Add address to whitelist
{"add_address": {"address": "ergors1new..."}}

// Remove address from whitelist
{"remove_address": {"address": "ergors1old..."}}

// Update admin
{"update_admin": {"new_admin": "ergors1newadmin..."}}

// Toggle default behavior
{"set_default_allow": {"allow": true}}
```

**QueryMsg:**

```json
// Middleware query - check authorization
{"is_allowed": {"address": "ergors1check..."}}

// Admin query - list whitelist
{"list_addresses": {"start_after": null, "limit": 100}}

// Get contract config
{"config": {}}
```

### Middleware Flow

> **Implementation:** [`auth/middleware.rs`](../../packages/ergors/src/auth/middleware.rs)

```rust
pub async fn contract_auth_middleware(
    State(state): State<ErgorsAppState>,
    request: Request,
    next: Next,
) -> Response {
    // 1. Normalize endpoint path
    let endpoint_label = normalize_endpoint_path(request.uri().path());

    // 2. Extract caller's public key from header
    let public_key = extract_header(&headers, "x-public-key");

    // 3. Check for registered authenticator
    match state.s.get_authenticator(&endpoint_label).await {
        Ok(Some(contract)) => {
            // 4. Generate caller address
            let caller_address = generate_caller_address(&public_key, &state);

            // 5. Query authenticator contract
            let allowed = query_authenticator_contract(
                &state, &contract, &caller_address
            ).await?;

            if allowed {
                next.run(request).await  // Proceed
            } else {
                forbidden_response("Access denied by authenticator contract")
            }
        }
        Ok(None) => next.run(request).await,  // No contract, proceed
        Err(_) => next.run(request).await,    // Fail open on lookup errors
    }
}
```

### Contract Deployment

Contracts can be deployed during node startup via configuration or manually using the ContractManager:

> **Manager:** [`contracts/manager.rs`](../../packages/ergors/src/contracts/manager.rs)

```rust
use crate::contracts::ContractManager;

// Deploy auth registry updater (coordinator only)
let address = contract_manager.deploy_auth_registry_updater(
    wasm_bytes,
    coordinator_address,
).await?;

// Deploy whitelist authenticator for specific endpoint
let address = contract_manager.deploy_whitelist_authenticator(
    wasm_bytes,
    "api/prompts",
    admin_address,
    Some("Prompt API access control".into()),
    Some(vec!["ergors1user1...".into()]),
).await?;
```

### Configuration

Add to `config.toml` for automatic deployment:

```toml
[[cosmwasm.initial_contracts]]
name = "auth_registry_updater"
wasm_path = "contracts/auth_registry_updater.wasm"
init_msg = '{"coordinator": "${NODE_ADDRESS}"}'
deploy_on_node_types = ["coordinator"]
required = true

[[cosmwasm.initial_contracts]]
name = "whitelist_auth_prompts"
wasm_path = "contracts/whitelist_authenticator.wasm"
init_msg = '{"admin": "${NODE_ADDRESS}", "default_allow": false}'
deploy_on_node_types = ["coordinator"]
required = false
```

### Security Considerations

| Aspect | Behavior |
|--------|----------|
| Missing public key | Returns 401 if contract registered, proceeds if not |
| Contract query failure | Returns 500 (fail closed for security) |
| Authenticator lookup failure | Proceeds (fail open for availability) |
| No authenticator registered | Falls through to standard Ed25519 auth |

---

## 4. Transport Encryption

### Handshake Protocol

> **Implementation:** Uses `commonware-stream` library

```
Dialer                              Listener
   |-- Hello(ephem_pk, sig) ----------->|
   |<---- Hello(ephem_pk, sig) ---------|
   |<---- Confirmation(tag) ------------|
   |-- Confirmation(tag) -------------->|
   |==== Encrypted Channel =============|
```

### Key Derivation

HKDF-SHA256 derives 4 directional keys from X25519 shared secret.

---

## 5. Network Integration

> **Implementation:** [`network/manager.rs`](../../packages/ergors/src/network/manager.rs)

```rust
// Recommended: custody-backed
manifold.start_network_with_custody(&config, &custody).await?;

// Alternative: direct key
manifold.start_network_with_key(&config, private_key).await?;

// Legacy: from NodeIdentity
manifold.start_network(&config).await?;
```

---

## 6. API Key Encryption

> **Implementation:** [`custody/encrypted.rs`](../../packages/ho-std/src/custody/encrypted.rs)

LLM provider keys encrypted with node identity:

```rust
use ho_std::custody::encrypted::{encrypt_with_node_key, decrypt_with_node_key};

let key_bytes = custody.get_key_bytes().await?;
let encrypted = encrypt_with_node_key(&key_bytes, api_key.as_bytes());
let decrypted = decrypt_with_node_key(&key_bytes, &encrypted)?;
```

---

## 7. Security Properties

| Protected | Mechanism |
|-----------|-----------|
| Key confidentiality | Argon2 + ChaCha20Poly1305 |
| Authentication | Ed25519 signatures |
| Forward secrecy | Ephemeral X25519 |
| Replay prevention | Timestamps + nonces |
| Integrity | Poly1305 MACs |

| NOT Protected | Notes |
|---------------|-------|
| Traffic analysis | Sizes/timing visible |
| Future secrecy | Static key compromise affects future |
| Anonymity | Identities visible in handshake |

---

## 8. Best Practices

**Operators:**

1. Use password-encrypted custody in production
2. Back up `node_identity.enc` files
3. Rotate passwords with `change_password()`
4. Ensure NTP time sync

**Developers:**

1. Accept `impl NodeIdentityCustody` not concrete types
2. Handle lock/unlock lifecycle
3. Check `is_unlocked()` before operations
4. Use namespaces for signatures

---

## 9. Future Extensions

| Feature | Status |
|---------|--------|
| Contract-based authenticators | **Implemented** |
| Threshold custody | Planned |
| HSM integration | Planned |
| Remote custody (gRPC) | Planned |
| Key rotation | Planned |
| Audit logging | Planned |
| Role-based authenticator contracts | Planned |
| Time-bounded access contracts | Planned |

---

## Appendix: Source References

All types defined in proto files with generated Rust code:

| Proto | Generated |
|-------|-----------|
| [`storage/v1/storage.proto`](../../proto/ergors/storage/v1/storage.proto) | `ho_std::types::ergors::storage::v1` |
| [`network/v1/network.proto`](../../proto/ergors/network/v1/network.proto) | `ho_std::types::ergors::network::v1` |
| [`orch/v1/orch.proto`](../../proto/ergors/orch/v1/orch.proto) | `ho_std::types::ergors::orch::v1` |

### Contract-Based Authentication Sources

| Component | Source |
|-----------|--------|
| Auth middleware | [`auth/middleware.rs`](../../packages/ergors/src/auth/middleware.rs) |
| Auth handlers | [`auth/handlers.rs`](../../packages/ergors/src/auth/handlers.rs) |
| Auth module | [`auth/mod.rs`](../../packages/ergors/src/auth/mod.rs) |
| Storage methods | [`storage.rs`](../../packages/ergors/src/storage.rs) |
| Contract manager | [`contracts/manager.rs`](../../packages/ergors/src/contracts/manager.rs) |
| Auth registry updater | [`contracts/cw-middleware-auth/`](../../contracts/cw-middleware-auth/) |
| Whitelist authenticator | [`contracts/cw-auth/`](../../contracts/cw-auth/) |

### Authenticator Proto Types

From [`orch/v1/orch.proto`](../../proto/ergors/orch/v1/orch.proto):

| Type | Purpose |
|------|---------|
| `RegisterAuthenticatorRequest` | Register endpoint authenticator |
| `RegisterAuthenticatorResponse` | Registration result |
| `ListAuthenticatorsResponse` | List all authenticators |
| `DeleteAuthenticatorResponse` | Deletion result |
| `AuthorizationCheckRequest` | Check address authorization |
| `AuthorizationCheckResponse` | Authorization result |
| `AuthenticatorEntry` | Authenticator registry entry |
