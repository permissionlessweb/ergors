# PROJECT 02: Encrypted API Key Storage with Node Identity

tell me about - finish implmenting llm provider trait in library with clear definition for the macros in comments of library due to complexity of debugging macros
    - refactor the custody soft-kms for use of environment variables, refactor by adding to the config an env_policy that is dedicated for the env variables that are registered as a policy to the nodes for storing api keys to use in storage.
     , it makes sense access to storage for retrieval of env key via substore constant reserved for provider occurs after the authentication middleware storage prompt, and occur a single storage request for key in batch as node may have multiple request to api with key use so we shuld document prpraration for local epoch use between actions in tree roots
    - Provider prompt: merge prompts with format penumbra uses for TxPlan. finish merging the prompt trait to be able to define

> wire in signature middleware for cli call in run sub cmd with ,

─────────────────────────────────────────────────────────
> lets implement the custody client within our auth
  middleware such that we run checks for keys able to
  access endpoints reigstered in the cnardium storage,
  we want to use the existing storage client and dedicated
  
## Multi-goal acomplishing action

we already have prepped a custody client server model for use when authorizaing actions to be compatible with various offline/external signing methods. We can make a custom dedicated API Key storage that interafaces with a dedicated layer of the jmt we use for storage (cnardium) by storaging the encrypted keys to its storage and then when prompts come in we can have this server implement this. Since we are going to have. By defining the use of the prompt note we are generating, we can wire in the wallet and plan instructions for creating the objects and strucutre to pass. THis requires access to the custody defintioints , which we still need to interface in with a  client so we can power this, so lets use this goal by the solution allowing the soft-kms to access teh jmt app state for read and writing encrypted api keys to a dedicated layer so we can access them for various actions

## Context

The ERGORS system currently loads API keys from a JSON file located in the home directory (`~/api-keys.json`) and environment variables. We need to migrate to an encrypted storage solution using the node's identity keys for encryption/decryption.

## Current State

### File Locations

- **API Keys JSON**: `~/.ho/api-keys.json` (home directory)
- **Environment file**: `~/.ho/.env` (same directory as api-keys.json)
- **Node identity key management**: `packages/ho-std-keys/src/lib.rs`
- **LLM Key Accessor**: `packages/ergors/src/llm/key_accessor.rs`
- **Storage implementation**: `packages/ergors/src/storage.rs`
- **Cnidarium storage**: Used via `cnidarium::Storage`

### Current JSON Structure

```json
{
  "providers": {
    "anthropic": {
      "api_key": "${ANTHROPIC_API_KEY}",
      "entity": {
        "name": "Anthropic",
        "base_url": "https://api.anthropic.com/v1",
        "models": [...],
        "priority": 1,
        "enabled": true
      }
    }
  }
}
```

### Current Key Loading Flow

1. `EnvKeyAccessor::from_home()` reads `api-keys.json`
2. Resolves `${ENV_VAR}` references to environment variables
3. Caches keys in memory for duration of session
4. Falls back to direct env vars if file not found

## Goal

**Encrypt API keys using node identity keys and store them in the Cnidarium database for network-wide encrypted propagation.**

### Requirements

1. **Encryption**: Use node's Ed25519 identity key (from `ho-std-keys`) to encrypt API keys
2. **Storage**: Store encrypted keys in Cnidarium database with provider metadata
3. **Migration**: On first run, load from `api-keys.json`, encrypt, store in DB, then delete plaintext file
4. **Retrieval**: `ApiKeyMethod` should decrypt keys from DB on-demand
5. **Ephemeral Nature**: Keys are encrypted at rest, only decrypted when needed for API calls
6. **Network Propagation**: Encrypted keys sync across nodes via Cnidarium state replication

### Security Model

- API keys encrypted with node's private key
- Only the node that encrypted can decrypt (or nodes with shared custody)
- Keys never stored in plaintext in database
- Plaintext keys only exist in memory during API calls
- Future: Custody client can manage shared decryption for multi-node scenarios

## Implementation Tasks

### 1. Create Encrypted Storage Schema

Define proto types in `proto/ergors/storage/v1/storage.proto`:

```protobuf
message EncryptedApiKey {
  string provider_name = 1;
  bytes encrypted_key = 2;  // Encrypted with node identity key
  google.protobuf.Timestamp encrypted_at = 3;
  string encryption_method = 4;  // e.g., "ed25519-xchacha20poly1305"
}

message ProviderMetadata {
  string name = 1;
  string base_url = 2;
  repeated string models = 3;
  int32 priority = 4;
  bool enabled = 5;
}
```

### 2. Implement Encryption in `ho-std-keys`

Add encryption methods to `packages/ho-std-keys/src/lib.rs`:

```rust
pub fn encrypt_api_key(
    node_key: &ed25519::SigningKey,
    plaintext_key: &str,
) -> Result<Vec<u8>>;

pub fn decrypt_api_key(
    node_key: &ed25519::SigningKey,
    encrypted_key: &[u8],
) -> Result<String>;
```

### 3. Create Storage-Backed ApiKeyMethod

Implement `StorageKeyAccessor` in `packages/ergors/src/llm/key_accessor.rs`:

```rust
pub struct StorageKeyAccessor {
    storage: Arc<ErgorsStorage>,
    node_key: ed25519::SigningKey,
    cache: Arc<RwLock<HashMap<String, CachedKey>>>,
}

impl StorageKeyAccessor {
    pub async fn new(storage: Arc<ErgorsStorage>, node_key: ed25519::SigningKey) -> Result<Self>;

    pub async fn migrate_from_json(&self, json_path: &Path) -> Result<()>;

    async fn store_encrypted_key(&self, provider: &str, key: &str) -> Result<()>;

    async fn retrieve_encrypted_key(&self, provider: &str) -> Result<Option<String>>;
}
```

### 4. Update Storage Implementation

Add methods to `packages/ergors/src/storage.rs`:

```rust
impl ErgorsStorage {
    pub async fn store_encrypted_api_key(
        &self,
        provider: &str,
        encrypted_key: EncryptedApiKey,
    ) -> Result<()>;

    pub async fn get_encrypted_api_key(
        &self,
        provider: &str,
    ) -> Result<Option<EncryptedApiKey>>;

    pub async fn list_providers_with_keys(&self) -> Result<Vec<String>>;
}
```

### 5. Migration Logic

Create migration utility in `packages/ergors/src/llm/migration.rs`:

```rust
pub async fn migrate_api_keys(
    json_path: &Path,
    storage: Arc<ErgorsStorage>,
    node_key: &ed25519::SigningKey,
) -> Result<()> {
    // 1. Read api-keys.json
    // 2. Resolve environment variables
    // 3. Encrypt each key with node identity
    // 4. Store in Cnidarium with provider metadata
    // 5. Optionally delete or backup json file
    // 6. Return success
}
```

### 6. Update LlmRouter Initialization

Modify `packages/ergors/src/llm/router.rs`:

```rust
impl LlmRouter {
    pub async fn new(
        config: &LlmRouterConfig,
        storage: Arc<ErgorsStorage>,
        node_key: &ed25519::SigningKey,
    ) -> Result<Self> {
        // Create storage-backed key accessor
        let key_accessor = Arc::new(
            StorageKeyAccessor::new(storage.clone(), node_key.clone()).await?
        ) as Arc<dyn ApiKeyMethod>;

        // Check if migration needed
        let json_path = Path::new(&config.api_keys_file);
        if json_path.exists() {
            key_accessor.migrate_from_json(json_path).await?;
        }

        // Continue with router initialization...
    }
}
```

## Testing Strategy

1. **Unit Tests**: Encryption/decryption roundtrip with node keys
2. **Integration Tests**:
   - Store encrypted key → retrieve → decrypt → verify
   - Migration from JSON → encrypted storage
3. **E2E Tests**: Full router initialization with encrypted keys from storage

## Benefits

- ✅ API keys encrypted at rest in database
- ✅ Keys propagate across network encrypted (via Cnidarium replication)
- ✅ No plaintext keys on disk after migration
- ✅ Ephemeral decryption only when needed
- ✅ Foundation for custody client integration
- ✅ Audit trail of key usage via storage operations

## Files to Modify

1. `proto/ergors/storage/v1/storage.proto` - Add encrypted key types
2. `packages/ho-std-keys/src/lib.rs` - Add encryption methods
3. `packages/ergors/src/llm/key_accessor.rs` - Implement `StorageKeyAccessor`
4. `packages/ergors/src/storage.rs` - Add encrypted key storage methods
5. `packages/ergors/src/llm/router.rs` - Update initialization
6. `packages/ergors/src/llm/migration.rs` - Create migration utility (new file)
7. `packages/ergors/src/llm/mod.rs` - Export new types

## Success Criteria

- [ ] API keys loaded from storage instead of JSON file
- [ ] Keys encrypted with node identity (Ed25519)
- [ ] Migration from JSON to encrypted storage works
- [ ] Decryption happens on-demand during API calls
- [ ] No plaintext keys in database
- [ ] Tests pass for encryption, storage, and retrieval
- [ ] Documentation updated

## Future Enhancements

- Custody client integration for shared key decryption across nodes
- Key rotation mechanism
- Multi-signature key access for critical providers
- Audit logging of key access patterns
