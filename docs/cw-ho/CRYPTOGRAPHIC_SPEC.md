# CW-HO Cryptographic Security Specification

## Overview

This specification defines the cryptographic security architecture for CW-HO (Commonware Helper Orchestration) nodes, implementing two primary security layers:

1. **API Authentication Layer**: Public key-based authentication for API endpoint access
2. **Transport Encryption Layer**: ChaCha20-Poly1305 encrypted communication between nodes

The implementation leverages the battle-tested cryptographic primitives from the commonware stream library, specifically their X25519 + ChaCha20-Poly1305 implementation.

## 1. API Authentication Layer

### 1.1 Overview

API endpoints requiring authentication will verify that requests originate from authorized nodes within the network using Ed25519 signature verification.

### 1.2 Implementation Details

#### Public Key Registry

- Each node maintains a registry of authorized public keys within the network
- Keys are stored in the `NodeIdentity` configuration structure
- Registry is populated from:
  - Network discovery during bootstrap
  - Static configuration for initial coordinator nodes
  - Runtime updates via secure network consensus

#### Request Signing

```rust
// API request structure
pub struct AuthenticatedRequest<T> {
    pub payload: T,
    pub timestamp: u64,           // Unix timestamp in milliseconds
    pub nonce: [u8; 32],         // Random nonce to prevent replay
    pub signature: Signature,     // Ed25519 signature
    pub public_key: PublicKey,   // Sender's public key
}

// Signature covers: namespace || timestamp || nonce || payload_hash
let message = [
    NAMESPACE,
    &timestamp.to_be_bytes(),
    &nonce,
    &sha256(payload)
].concat();
```

#### Verification Process

1. **Timestamp Validation**: Ensure request timestamp is within synchrony bound (±5 minutes)
2. **Nonce Tracking**: Maintain LRU cache of recent nonces to prevent replay attacks
3. **Public Key Authorization**: Verify sender's public key exists in authorized registry
4. **Signature Verification**: Validate Ed25519 signature against request payload

### 1.3 Protected Endpoints

The following API endpoints require authentication:

- `/orchestrate/bootstrap` - Node bootstrap operations
- `/orchestrate/fractal` - Fractal hoe creation
- `/orchestrate/prune` - Storage pruning operations
- `/network/topology` - Network topology queries (read-only but sensitive)

## 2. Inter-Node Transport Encryption

### 2.1 Overview

All communication between nodes uses the commonware stream library's implementation of X25519 key exchange with ChaCha20-Poly1305 AEAD encryption.

### 2.2 Cryptographic Primitives

#### Key Exchange

- **Algorithm**: X25519 Elliptic Curve Diffie-Hellman
- **Key Size**: 32 bytes (256 bits)
- **Libraries**: `x25519-dalek` via commonware implementation

#### Symmetric Encryption

- **Algorithm**: ChaCha20-Poly1305 AEAD
- **Key Size**: 32 bytes (256 bits)
- **Nonce Size**: 12 bytes (96 bits)
- **Authentication Tag**: 16 bytes (128 bits)

### 2.3 Handshake Protocol

The implementation follows commonware's 3-message handshake:

#### Message 1: Dialer Hello

```rust
pub struct Hello {
    pub recipient: PublicKey,      // Expected listener's public key
    pub ephemeral: PublicKey,      // Dialer's ephemeral X25519 key
    pub timestamp: u64,            // Current timestamp
    pub signature: Signature,      // Ed25519 signature by dialer's static key
}
```

#### Message 2: Listener Hello + Confirmation

```rust
pub struct ListenerResponse {
    pub hello: Hello,              // Listener's hello message
    pub confirmation: Confirmation, // Proof of shared secret derivation
}
```

#### Message 3: Dialer Confirmation

```rust
pub struct Confirmation {
    pub tag: [u8; 16],            // Authentication tag proving shared secret
}
```

### 2.4 Key Derivation

Following commonware's HKDF-SHA256 implementation:

```rust
// Base key derivation parameters
const BASE_KDF_PREFIX: &[u8] = b"commonware-stream/KDF/v1/";
const NAMESPACE: &[u8] = b"cw-ho/v1/";

// Directional traffic keys
let salt = SHA256(BASE_KDF_PREFIX || NAMESPACE || hello_transcript);
let shared_secret = x25519(dialer_ephemeral_secret, listener_ephemeral_public);

// HKDF-Expand to derive 4 keys:
// - d2l_traffic: Dialer-to-Listener traffic encryption
// - l2d_traffic: Listener-to-Dialer traffic encryption  
// - d2l_confirmation: Dialer-to-Listener handshake confirmation
// - l2d_confirmation: Listener-to-Dialer handshake confirmation
```

### 2.5 Nonce Management

- **Counter-based nonces**: 12-byte nonces derived from monotonic counters
- **Counter initialization**: Starts at 0 for each connection
- **Counter overflow**: Connection terminates and re-establishes (>2^96 messages)
- **Direction separation**: Separate counters for each traffic direction

### 2.6 Message Format

```rust
// Encrypted message structure
pub struct EncryptedMessage {
    pub length: u32,              // Message length (big-endian)
    pub ciphertext: Vec<u8>,      // ChaCha20 encrypted payload
    pub tag: [u8; 16],           // Poly1305 authentication tag
}

// Nonce derivation (not transmitted)
let nonce = [
    counter.to_be_bytes(),        // 8 bytes: message counter
    [0u8; 4]                     // 4 bytes: padding
];
```

## 3. Security Properties

### 3.1 API Authentication

- **Authentication**: Cryptographically proves request origin
- **Non-repudiation**: Ed25519 signatures provide non-repudiation
- **Replay Protection**: Timestamp + nonce prevents replay attacks
- **Authorization**: Public key registry ensures only authorized nodes can access endpoints

### 3.2 Transport Layer

- **Mutual Authentication**: Both parties prove ownership of static keys
- **Forward Secrecy**: Ephemeral X25519 keys protect past sessions
- **Session Uniqueness**: Transcript binding prevents replay/substitution attacks
- **Confidentiality**: ChaCha20 encryption protects message contents
- **Integrity**: Poly1305 MAC prevents message tampering
- **Session Timeout**: Configurable handshake deadlines prevent resource exhaustion

### 3.3 Not Provided

- **Traffic Analysis Protection**: Message sizes/timing not hidden
- **Future Secrecy**: Static key compromise exposes future sessions
- **0-RTT Support**: No session resumption capability
- **Anonymity**: Node identities visible during handshake

## 4. Configuration Parameters

### 4.1 API Authentication

```rust
pub struct ApiAuthConfig {
    pub namespace: Vec<u8>,              // Application-specific namespace
    pub synchrony_bound: Duration,       // Max clock skew (default: 5 minutes)
    pub nonce_cache_size: usize,         // LRU cache size (default: 10,000)
    pub nonce_cache_ttl: Duration,       // Cache TTL (default: 1 hour)
}
```

### 4.2 Transport Layer

```rust
pub struct TransportConfig {
    pub namespace: Vec<u8>,              // Application namespace
    pub max_message_size: usize,         // Max message size (default: 1MB)
    pub synchrony_bound: Duration,       // Clock skew tolerance (default: 5 minutes)
    pub max_handshake_age: Duration,     // Max handshake message age (default: 5 minutes)
    pub handshake_timeout: Duration,     // Handshake completion timeout (default: 30 seconds)
}
```

## 5. Implementation Architecture

### 5.1 Crate Dependencies

```toml
[dependencies]
commonware-cryptography = { version = "0.1", features = ["ed25519"] }
commonware-stream = "0.1"
x25519-dalek = "2.0"
chacha20poly1305 = "0.10"
ed25519-dalek = "2.0"
sha2 = "0.10"
hkdf = "0.12"
```

### 5.2 Module Structure

```
src/
├── crypto/
│   ├── mod.rs              # Public API
│   ├── api_auth.rs         # API authentication implementation
│   ├── transport.rs        # Transport layer wrapper around commonware
│   ├── keys.rs            # Key management utilities
│   └── config.rs          # Crypto configuration
├── middleware/
│   └── auth.rs            # Axum middleware for API authentication
└── network/
    └── encrypted.rs        # Encrypted network transport integration
```

### 5.3 Integration Points

#### Server Middleware

```rust
// Apply to protected routes
.route("/orchestrate/bootstrap", post(handle_bootstrap))
.layer(axum::middleware::from_fn(crypto_auth_middleware))
```

#### Network Manager

```rust
// Replace plaintext connections with encrypted transport
impl CwHoNetworkManifold {
    async fn connect_peer(&mut self, peer: PeerInfo) -> Result<EncryptedConnection> {
        let config = TransportConfig { /* ... */ };
        let conn = Connection::upgrade_dialer(
            context,
            config,
            tcp_sink,
            tcp_stream,
            peer.public_key
        ).await?;
        Ok(EncryptedConnection::new(conn))
    }
}
```

## 6. Security Considerations

### 6.1 Key Management

- Static node keys should be generated with cryptographically secure randomness
- Private keys must be stored securely (environment variables, secure files)
- Key rotation procedures should be established for long-running deployments
- Ephemeral keys are automatically managed by the transport layer

### 6.2 Network Security

- Namespace must be unique per application to prevent cross-application attacks
- Time synchronization is critical for timestamp validation
- Network partitions may cause legitimate requests to be rejected due to clock skew

### 6.3 Implementation Security

- All cryptographic operations use constant-time implementations
- Sensitive data (keys, plaintexts) are zeroized after use
- Error messages should not leak cryptographic information
- Rate limiting should be applied to prevent DoS attacks on expensive crypto operations

## 7. Testing Strategy

### 7.1 Unit Tests

- Key generation and serialization
- Signature creation and verification
- Encryption/decryption round-trips
- Nonce counter overflow handling
- Error condition handling

### 7.2 Integration Tests

- Full handshake protocol execution
- Authenticated API request/response cycles
- Network partition and recovery scenarios
- Clock skew edge cases
- Concurrent connection establishment

### 7.3 Security Tests

- Replay attack prevention
- Invalid signature rejection
- Unauthorized key rejection
- Man-in-the-middle resistance
- Malformed message handling

This specification provides a comprehensive security architecture that balances strong cryptographic protection with practical implementation considerations for the CW-HO distributed orchestration system.
