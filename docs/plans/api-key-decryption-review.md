Excellent! Now I have a complete understanding. Let me create a comprehensive summary document:

Summary: Custody System for Encrypted API Keys

Based on my thorough search of the codebase, here's a complete breakdown of how the custody system works for storing
and retrieving encrypted API keys:

1. Custody Trait API (packages/ho-std/src/traits/mod.rs, lines 105-155)

The NodeIdentityCustody trait is the core interface for custody-backed key management:

```rs
# [async_trait]
pub trait NodeIdentityCustody: Send + Sync {
    fn backend(&self) -> NodeIdentityCustodyBackend;
    fn public_key(&self) -> HoResult<NodePubkey>;
    async fn get_private_key(&self) -> HoResult<NodePrivKey>;
    async fn sign_ed25519(
        &self,
        namespace: Option<&[u8]>,
        message: &[u8],
    ) -> HoResult<ed25519::Signature>;
    async fn export_ssh_keys(&self, ssh_dir: &Path) -> HoResult<()>;
    fn is_unlocked(&self) -> bool;
    async fn lock(&self);
    async fn get_key_bytes(&self) -> HoResult<[u8; 32]>;
}
```

Backend Types (enum NodeIdentityCustodyBackend, lines 90-103):

- Plaintext - insecure, testing only
- PasswordEncrypted (default) - ChaCha20Poly1305 + Argon2
- NodeKeyEncrypted - encrypted using node's own key (for API keys etc.)
- Threshold - distributed key shares
- RemoteCustody(String) - gRPC remote service

1. PasswordEncryptedCustody Implementation (packages/ho-std/src/custody/node_identity.rs)

This is the main custody backend for production use:

pub struct PasswordEncryptedCustody {
    storage: IdentityStorage,
    password: Arc<RwLock<Option<String>>>,
}

Key Methods:

- unlock(password: &str) - caches password after verification
- get_private_key() - requires unlock, returns NodePrivKey
- get_key_bytes() - returns raw 32-byte key for encryption operations
- lock() - clears cached password
- is_unlocked() - checks if password is cached

1. Encrypted API Key Storage (packages/ho-std/src/llm/encrypted_keys.rs)

The EncryptedApiKeyManager handles encrypted storage of provider API keys:

pub struct EncryptedApiKeyManager {
    derived_key: Option<[u8; 32]>,      // Derived from password
    salt: [u8; 32],                      // Argon2id salt
    cache: HashMap<String, String>,      // Decrypted keys cache
}

Encryption Scheme:

- KDF: Argon2id (2^16 memory, 2 time cost, 2 parallelism in production)
- Encryption: ChaCha20Poly1305 with 12-byte nonce
- Storage Format: Protocol Buffers (EncryptedApiKeyStore proto message)

Key Methods:

- unlock(password: &str) - derives key from password + salt
- encrypt_key(provider, api_key) - produces EncryptedApiKey
- decrypt_key(encrypted) - caches result for performance
- create_store(api_keys_map) - creates encrypted store
- load_store(store) - batch decrypts all keys
- serialize_store() / deserialize_store() - Protocol Buffer conversion

Storage Location: api-keys.enc (encrypted binary file)

1. Sentinel Bootstrap Flow (packages/ergors/src/sentinel.rs)

The Sentinel server orchestrates API key initialization during headless deployment:

Phase 1: Initialization (/sentinel/init)

- Creates custody with password-encrypted identity
- Saves config to config.toml

Phase 2: API Key Storage (/sentinel/api-keys)

- Receives encrypted API keys via X25519 + ChaCha20Poly1305 transport
- Stores to api-keys.enc using EncryptedApiKeyManager
- Sets restrictive file permissions (0o600 on Unix)

Phase 3: Activation (/sentinel/activate)

- Signals handoff to full Ergors server
- Returns custody password to caller

Encryption Transport:

- Uses X25519 Diffie-Hellman for key agreement
- Derives ChaCha20Poly1305 key via blake3::derive_key(SENTINEL_KDF_CONTEXT, shared_secret)
- Context: "ergors sentinel v1"

1. ProxyRouter Configuration (packages/ergors/src/proxy/router.rs, lines 500-559)

The router struct provides provider configuration and routing:

# [derive(Debug, Clone)]

pub struct ProxyRouter {
    config: ProxyRouterConfig,
    client: Client,
}

# [derive(Debug, Clone)]

pub struct RouteTarget {
    pub base_url: String,
    pub api_key: Option<String>,
    pub provider_type: i32,
}

Provider Configuration (InferenceProviderConfig proto):
pub struct InferenceProviderConfig {
    pub provider_id: String,           // "openai", "anthropic", etc.
    pub base_url: String,               // API endpoint URL
    pub api_key_ref: String,            // Custody reference: "env://{VAR}" or "custody://{key_id}"
    pub provider_type: i32,             // Anthropic, OpenAI, Ollama, etc.
    pub enabled: bool,
    pub display_name: String,
    pub metadata: HashMap<String, String>,
    // ... timeout, concurrency limits, timestamps
}

The provider_to_route_target Method (lines 535-559):
fn provider_to_route_target(&self, provider: &InferenceProviderConfig) -> Result<RouteTarget> {
    if !provider.enabled {
        return Err(anyhow!("Provider '{}' is disabled", provider.provider_id));
    }

    // Resolve API key (support both direct key and key references)
    let api_key = if !provider.api_key_ref.is_empty() {
        // TODO: Implement custody key resolution
        // For now, treat api_key_ref as env var reference
        if let Some(env_ref) = provider.api_key_ref.strip_prefix("env://") {
            std::env::var(env_ref).ok()
        } else {
            Err(anyhow!(
                "API key reference not yet supported: {}",
                provider.api_key_ref
            ));
        }
    };

    Ok(RouteTarget {
        base_url: provider.base_url.clone(),
        api_key,
        provider_type: provider.provider_type,
    })
}

IMPORTANT NOTE: This method currently has a logic bug:

- Line 541 checks !provider.api_key_ref.is_empty() but the code expects api_key to be Option<String>
- The else block (lines 547-550) returns Err() which won't compile
- The code should properly return Option<String> or handle the error case differently
- TODO comment (line 542) indicates custody key resolution is not yet implemented

1. API Key Storage & Retrieval Pattern

During Sentinel Bootstrap:
API Keys (plaintext) → Sentinel receives encrypted request
                    → EncryptedApiKeyManager.unlock(password)
                    → EncryptedApiKeyManager.create_store(api_keys_map)
                    → serialize_store() → api-keys.enc file

During Server Startup (packages/ergors/src/server.rs, lines 698-770):

1. Load api-keys.enc from disk
2. Create EncryptedApiKeyManager from store
3. Call unlock(custody_password)
4. Call load_store() → HashMap<provider, decrypted_key>
5. Cache decrypted keys for routing decisions

During Request Routing:
InferenceProviderConfig.api_key_ref (e.g., "env://OPENAI_API_KEY")
                    → provider_to_route_target()
                    → std::env::var() [env:// prefix]
                    → [TODO: custody:// prefix not implemented]
                    → RouteTarget.api_key

1. Key Storage Paths & File Locations

- Identity: {home_dir}/.ergors/identity.enc (password-encrypted private key)
- Config: {home_dir}/.ergors/config.toml
- API Keys: {home_dir}/.ergors/api-keys.enc (Argon2id + ChaCha20Poly1305 encrypted)
- Permission: 0o600 (owner read/write only)

1. Trait Integration Points

ApiKeyMethod trait (lines 785-799):

# [async_trait]

pub trait ApiKeyMethod: Send + Sync {
    async fn get_key(&self, provider: &str) -> HoResult<Option<String>>;
    async fn set_key(&mut self, provider: &str, key: String) -> HoResult<()>;
    async fn has_key(&self, provider: &str) -> bool;
    async fn available_providers(&self) -> Vec<String>;
}

1. Proto Type Definitions

EncryptedApiKeyStore (packages/ho-std/src/types/ergors/gen/ergors.storage.v1.rs):

- version: u32
- keys: Vec<EncryptedApiKey>
- created_at: Timestamp
- updated_at: Timestamp
- kdf_salt: Vec<u8> (32 bytes)
- kdf_params: String (JSON metadata)

EncryptedApiKey:

- provider_name: String
- encrypted_key: Vec<u8> (ciphertext)
- encrypted_at: Timestamp
- encryption_method: String ("argon2id-chacha20poly1305-v1")
- nonce: Vec<u8> (12 bytes)

1. Security Properties

✅ Strengths:

- Password never stored (derived via Argon2id on each unlock)
- Memory zeroed after lock
- Proper nonce generation (random per key)
- Authenticated encryption (ChaCha20Poly1305 provides AEAD)
- Cache can be cleared on lock
- File permissions restricted (0o600)
- Sentinel uses encrypted transport (X25519 DH + ChaCha20)

⚠️ Current Gaps:

- custody:// API key reference format not implemented in router
- No integration between NodeIdentityCustody and EncryptedApiKeyManager
- Plaintext custody backend exists (should warn in production)
- API key resolution in provider_to_route_target() incomplete

1. Full Context of provider_to_route_target with Surrounding Code

File: /Users/returniflost/CW-AGENT/e2e-improvements/packages/ergors/src/proxy/router.rs

Lines 520-560 (full method context):
// ============= Generic Provider Access =============

/// Get provider configuration by ID
pub fn get_provider(&self, provider_id: &str) -> Option<&InferenceProviderConfig> {
    self.config.providers.get(provider_id)
}

/// Get enabled provider configuration by ID
pub fn get_enabled_provider(&self, provider_id: &str) -> Option<&InferenceProviderConfig> {
    self.get_provider(provider_id).filter(|p| p.enabled)
}

/// Get route target from provider configuration
fn provider_to_route_target(&self, provider: &InferenceProviderConfig) -> Result<RouteTarget> {
    if !provider.enabled {
        return Err(anyhow!("Provider '{}' is disabled", provider.provider_id));
    }

    // Resolve API key (support both direct key and key references)
    let api_key = if !provider.api_key_ref.is_empty() {
        // TODO: Implement custody key resolution
        // For now, treat api_key_ref as env var reference
        if let Some(env_ref) = provider.api_key_ref.strip_prefix("env://") {
            std::env::var(env_ref).ok()
        } else {
            Err(anyhow!(
                "API key reference not yet supported: {}",
                provider.api_key_ref
            ));
        }
    };

    Ok(RouteTarget {
        base_url: provider.base_url.clone(),
        api_key,
        provider_type: provider.provider_type,
    })
}

Lines 564-612 (routing methods that use provider_to_route_target):

- route_anthropic(&self, model: &str) -> RouteTarget
- route_openai(&self, model: &str) -> RouteTarget
- route_ollama(&self, model: &str) -> RouteTarget
- match_model_route(&self, model: &str) -> Result<RouteTarget>

All these methods:

1. Check model-specific routes first (via match_model_route)
2. Fall back to provider-specific lookup (e.g., "anthropic", "openai")
3. Call provider_to_route_target() to resolve configuration
4. Return fallback RouteTarget with default URL if no match

Summary of Current Architecture

The custody system is well-architected for security but partially implemented:

- Encryption layer is solid (Argon2id + ChaCha20Poly1305)
- Bootstrap flow is complete (Sentinel handles encrypted key injection)
- Storage is encrypted at rest with proper permissions
- Missing link: No integration between custody password derivation and API key resolution in the router

The api_key_ref field in InferenceProviderConfig is designed to support multiple formats (env://, custody://) but only
env:// is currently implemented as a fallback. The production path for custody:// resolution is not yet wired into the
router.
