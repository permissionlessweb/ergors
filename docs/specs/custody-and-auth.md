# ERGORS Security: Custody, Cryptography & Authentication

## Agentic Session Context

> **For AI agents:** Quick context for ERGORS security. See implementation in source files below.

| Concept | Type/Trait | Source |
|---------|-----------|--------|
| Node keypair | `NodePrivKey` / `NodePubkey` | [`keys/commonware.rs`](../../packages/ho-std/src/keys/commonware.rs) |
| Custody trait | `NodeIdentityCustody` | [`traits/mod.rs`](../../packages/ho-std/src/traits/mod.rs) |
| Password custody | `PasswordEncryptedCustody` | [`custody/node_identity.rs`](../../packages/ho-std/src/custody/node_identity.rs) |
| Identity storage | `IdentityStorage` | [`storage/identity.rs`](../../packages/ho-std/src/storage/identity.rs) |
| Network integration | `start_network_with_custody` | [`network/manager.rs`](../../packages/cw-ho/src/network/manager.rs) |
| Config helpers | `ErgorsConfig` | [`config.rs`](../../packages/cw-ho/src/config.rs) |

**Security model:** Private keys encrypted at rest (Argon2 + ChaCha20Poly1305), decrypted on-demand with TTL caching.

---

## Overview

ERGORS implements layered security:

| Layer | Purpose | Primitives | Source |
|-------|---------|------------|--------|
| Identity Custody | Secure key storage | Argon2 + ChaCha20Poly1305 | [`custody/`](../../packages/ho-std/src/custody/) |
| API Authentication | Request signing | Ed25519 | [`middleware/`](../../packages/cw-ho/src/middleware/) |
| Transport Encryption | Node communication | X25519 + ChaCha20Poly1305 | [`network/`](../../packages/cw-ho/src/network/) |

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

Config helpers in [`config.rs`](../../packages/cw-ho/src/config.rs):

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

> **Middleware:** [`middleware/auth.rs`](../../packages/cw-ho/src/middleware/auth.rs)

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

---

## 3. Transport Encryption

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

## 4. Network Integration

> **Implementation:** [`network/manager.rs`](../../packages/cw-ho/src/network/manager.rs)

```rust
// Recommended: custody-backed
manifold.start_network_with_custody(&config, &custody).await?;

// Alternative: direct key
manifold.start_network_with_key(&config, private_key).await?;

// Legacy: from NodeIdentity
manifold.start_network(&config).await?;
```

---

## 5. API Key Encryption

> **Implementation:** [`custody/encrypted.rs`](../../packages/ho-std/src/custody/encrypted.rs)

LLM provider keys encrypted with node identity:

```rust
use ho_std::custody::encrypted::{encrypt_with_node_key, decrypt_with_node_key};

let key_bytes = custody.get_key_bytes().await?;
let encrypted = encrypt_with_node_key(&key_bytes, api_key.as_bytes());
let decrypted = decrypt_with_node_key(&key_bytes, &encrypted)?;
```

---

## 6. Security Properties

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

## 7. Best Practices

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

## 8. Future Extensions

| Feature | Status |
|---------|--------|
| Threshold custody | Planned |
| HSM integration | Planned |
| Remote custody (gRPC) | Planned |
| Key rotation | Planned |
| Audit logging | Planned |

---

## Appendix: Source References

All types defined in proto files with generated Rust code:

| Proto | Generated |
|-------|-----------|
| [`storage/v1/storage.proto`](../../proto/ergors/storage/v1/storage.proto) | `ho_std::types::ergors::storage::v1` |
| [`network/v1/network.proto`](../../proto/ergors/network/v1/network.proto) | `ho_std::types::ergors::network::v1` |
| [`orch/v1/orch.proto`](../../proto/ergors/orch/v1/orch.proto) | `ho_std::types::ergors::orch::v1` |
