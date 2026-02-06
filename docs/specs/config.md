# ERGORS Configuration System

## Overview

ERGORS uses a layered configuration system that separates public configuration from sensitive secrets. Configuration is defined using Protocol Buffer definitions for type safety and cross-language compatibility.

```
~/.ergors/
├── config.toml              # Main configuration (TOML serialized from HoConfig proto)
├── node_identity.enc        # Encrypted private key (Argon2 + ChaCha20Poly1305)
├── api-keys.json            # LLM provider API keys (JSON)
├── .env                     # Environment variable overrides
├── ssh/                     # SSH keys derived from node identity
│   ├── id_ed25519           # Private key
│   └── id_ed25519.pub       # Public key
├── data/                    # Persistent storage
│   └── cnidarium/           # State database
└── logs/
    └── engine.log
```

---

## Configuration Hierarchy

Configuration values are resolved in the following order (later sources override earlier):

1. **Proto defaults** - Default values in proto message definitions
2. **config.toml** - Primary configuration file
3. **Environment variables** - Runtime overrides (prefixed with `ERGORS_`)
4. **CLI flags** - Command-line arguments

---

## Core Configuration Types

### HoConfig (Root Configuration)

The root configuration message containing all subsystems.

**Proto Definition:** `ergors.orch.v1.HoConfig`

```protobuf
message HoConfig {
  network.v1.NetworkConfig network = 1;
  network.v1.NodeIdentity identity = 2;
  storage.v1.StorageConfig storage = 3;
  LlmRouterConfig llm = 4;
  string home = 5;
  storage.v1.NodeIdentityCustodyConfig custody = 6;
}
```

**TOML Example:**
```toml
home = "/home/user/.ergors"

[identity]
host = "0.0.0.0"
p2p_port = 26656
api_port = 8080
user = "ergors"
node_type = "development"
# NOTE: private_key should NOT be stored here - use custody instead

[network]
listen_address = "0.0.0.0"
listen_port = 26656
bootstrap_peers = []
enable_discovery = true
connection_timeout_ms = 5000

[network.limits]
max_message_size = 1048576
max_peers = 50

[network.channels]
discovery_buffer = 100
task_buffer = 100
state_buffer = 100
health_buffer = 50

[storage]
data_dir = "/home/user/.ergors/data"
max_size_mb = 1024
enable_compression = true

[llm]
api_keys_file = "/home/user/.ergors/api-keys.json"
timeout_seconds = 30
max_retries = 3
default_entity = 0
default_strategy = "MODEL_SELECTION_STRATEGY_PRIORITY"

[custody]
backend = "password_encrypted"
cache_keys = true
cache_ttl_secs = 300
identity_path = ""  # Uses default: ~/.ergors/node_identity.enc
```

---

## Node Identity & Custody

### Security Model

Private keys are **never** stored in plaintext configuration files. The custody system provides:

1. **Password-based encryption** (default) - Keys encrypted with Argon2id + ChaCha20Poly1305
2. **In-memory caching** - Decrypted keys cached with configurable TTL
3. **Multiple backends** - Support for various custody strategies

### Custody Backends

**Proto Definition:** `ergors.storage.v1.NodeIdentityCustodyConfig`

```protobuf
message NodeIdentityCustodyConfig {
  string backend = 1;           // "password_encrypted", "plaintext", "threshold", "remote"
  bool cache_keys = 2;          // Cache decrypted keys in memory
  uint64 cache_ttl_secs = 3;    // TTL for cached keys (0 = no expiry)
  string identity_path = 4;     // Path to encrypted identity file
  string remote_endpoint = 5;   // For remote custody backend
}
```

| Backend | Description | Security Level |
|---------|-------------|----------------|
| `password_encrypted` | Argon2 + ChaCha20Poly1305 encryption | **Production** |
| `plaintext` | Unencrypted (testing only) | Development |
| `node_key_encrypted` | Encrypted with another node's key | Future |
| `threshold` | M-of-N threshold signature | Future |
| `remote:<endpoint>` | Remote custody service | Future |

### Encrypted Identity File Format

**Proto Definition:** `ergors.storage.v1.EncryptedNodeIdentity`

```protobuf
message EncryptedNodeIdentity {
  bytes public_key = 1;              // Always plaintext (for identification)
  bytes encrypted_private_key = 2;   // ChaCha20Poly1305 encrypted blob
  google.protobuf.Timestamp encrypted_at = 3;
  string encryption_method = 4;      // "argon2id-chacha20poly1305-v1"
  bytes kdf_salt = 5;
  string kdf_params = 6;             // JSON: {"memory_cost":2097152,"time_cost":1,"parallelism":4}
  uint32 version = 7;
  NodeIdentityMetadata metadata = 8;
}
```

**JSON Example (`node_identity.enc`):**
```json
{
  "public_key": "base64...",
  "encrypted_private_key": "base64...",
  "encrypted_at": "2024-01-15T10:30:00Z",
  "encryption_method": "argon2id-chacha20poly1305-v1",
  "kdf_salt": "",
  "kdf_params": "{\"memory_cost\":2097152,\"time_cost\":1,\"parallelism\":4}",
  "version": 1,
  "metadata": {
    "user": "ergors",
    "host": "node-1.example.com",
    "p2p_port": 26656,
    "api_port": 8080,
    "node_type": "coordinator"
  }
}
```

### Unlocking Custody

The engine requires the custody password at startup:

```bash
# Environment variable (CI/automated environments)
export ERGORS_CUSTODY_PASSWORD="your-secure-password"
ergors start

# Interactive prompt (terminal)
ergors start
# Enter custody password: ********
```

### Migration from Plaintext Keys

If `config.toml` contains a plaintext `private_key`, the server automatically:

1. Prompts for a new custody password
2. Encrypts the key to `node_identity.enc`
3. Starts the network with the custody-backed identity
4. Logs a warning to remove `private_key` from config

---

## LLM Provider Configuration

### API Keys File

**Proto Definition:** `ergors.orch.v1.ApiKeysJson`

API keys are stored separately in `api-keys.json` for security isolation.

```json
{
  "metadata": {
    "version": "1.0",
    "description": "LLM provider API keys"
  },
  "providers": {
    "anthropic": {
      "api_key": "sk-ant-...",
      "entity": {
        "name": "anthropic",
        "base_url": "https://api.anthropic.com",
        "models": ["claude-3-5-sonnet-20241022", "claude-3-opus-20240229"],
        "default_model": "claude-3-5-sonnet-20241022",
        "priority": 1,
        "enabled": true
      }
    },
    "openai": {
      "api_key": "sk-...",
      "entity": {
        "name": "openai",
        "base_url": "https://api.openai.com/v1",
        "models": ["gpt-4o", "gpt-4-turbo"],
        "default_model": "gpt-4o",
        "priority": 2,
        "enabled": true
      }
    }
  },
  "global_settings": {
    "default_timeout_seconds": 30,
    "max_retry_attempts": 3,
    "fallback_enabled": true
  }
}
```

### LLM Router Configuration

**Proto Definition:** `ergors.orch.v1.LlmRouterConfig`

```protobuf
message LlmRouterConfig {
  string api_keys_file = 1;
  repeated LlmEntity entities = 2;
  ModelSelectionStrategy default_strategy = 3;
  uint64 timeout_seconds = 4;
  uint32 max_retries = 5;
  uint32 default_entity = 6;
}
```

### Model Selection Strategies

| Strategy | Description |
|----------|-------------|
| `PRIORITY` | Always use highest priority available provider |
| `ROUND_ROBIN` | Cycle through available providers |
| `GOLDEN_RATIO` | Weighted selection based on golden ratio (1.618) |
| `LOAD_BALANCED` | Select based on current provider load |

---

## Network Configuration

### Node Identity

**Proto Definition:** `ergors.network.v1.NodeIdentity`

```protobuf
message NodeIdentity {
  string host = 1;
  uint32 p2p_port = 2;
  uint32 api_port = 3;
  string user = 4;
  HostOS os = 5;
  uint32 ssh_port = 6;
  string node_type = 7;
  optional bytes public_key = 8;
}
```

### Node Types

| Type | Description |
|------|-------------|
| `coordinator` | Orchestrates task distribution across the network |
| `executor` | Executes assigned tasks from coordinators |
| `referee` | Validates execution results |
| `development` | Local development mode (all capabilities) |

### Network Configuration

**Proto Definition:** `ergors.network.v1.NetworkConfig`

```protobuf
message NetworkConfig {
  NodeType node_type = 1;
  repeated string bootstrap_peers = 2;
  repeated string known_peers = 3;
  uint32 listen_port = 4;
  string listen_address = 5;
  uint32 connection_timeout_ms = 7;
  bool enable_discovery = 8;
  NetworkLimits limits = 9;
  ChannelConfig channels = 10;
}
```

---

## Storage Configuration

**Proto Definition:** `ergors.storage.v1.StorageConfig`

```protobuf
message StorageConfig {
  string data_dir = 1;
  uint32 max_size_mb = 2;
  bool enable_compression = 3;
}
```

The storage layer uses **Cnidarium** for persistent state with:
- Jellyfish Merkle Tree for verifiable state
- RocksDB backend for durability
- Snapshot-based pruning

---

## CosmWasm Configuration

**Proto Definition:** `ergors.orch.v1.CosmwasmConfig`

```protobuf
message CosmwasmConfig {
  bool enabled = 1;
  string cache_dir = 2;
  uint64 memory_limit = 3;
  CosmwasmGasLimits gas_limits = 4;
  repeated ContractDeployment initial_contracts = 5;
}

message CosmwasmGasLimits {
  uint64 instantiate = 1;  // Default: 100,000,000
  uint64 execute = 2;      // Default: 50,000,000
  uint64 query = 3;        // Default: 10,000,000
  uint64 migrate = 4;      // Default: 75,000,000
}
```

### Contract Deployment Configuration

**Proto Definition:** `ergors.orch.v1.ContractDeployment`

```protobuf
message ContractDeployment {
  string name = 1;           // Unique name for resolution
  string wasm_path = 2;      // Path to WASM binary
  bytes wasm_bytes = 3;      // Alternative: embedded WASM
  string label = 4;          // Contract instance label
  string init_msg = 5;       // JSON instantiation message
  string admin = 6;          // Admin address (default: coordinator)
  bool required = 7;         // Fail startup if deployment fails
  repeated string deploy_on_node_types = 8;  // Node types to deploy on
  ContractConfig config = 9;
}

message ContractConfig {
  bool skip_if_exists = 1;   // Default: true
  ContractMigration migration = 2;
  map<string, string> metadata = 3;
}
```

### TOML Configuration Example

```toml
[cosmwasm]
enabled = true
cache_dir = "/home/user/.ergors/data/wasm_cache"
memory_limit = 33554432  # 32MB

[cosmwasm.gas_limits]
instantiate = 100000000
execute = 50000000
query = 10000000
migrate = 75000000

# Deploy identity registry on coordinator startup
[[cosmwasm.initial_contracts]]
name = "identity_registry"
wasm_path = "contracts/identity_registry.wasm"
label = "identity_registry"
init_msg = '''
{
  "coordinator": "ergors1...",
  "providers": [
    {"name": "anthropic", "ownership": "shared", "threshold": 2, "total_shares": 3},
    {"name": "ollama", "ownership": "local"}
  ]
}
'''
required = true
deploy_on_node_types = ["coordinator"]

[cosmwasm.initial_contracts.config]
skip_if_exists = true

# Deploy a custom contract on all nodes
[[cosmwasm.initial_contracts]]
name = "custom_contract"
wasm_path = "contracts/custom.wasm"
label = "custom_v1"
init_msg = '{"version": "1.0"}'
required = false
deploy_on_node_types = ["coordinator", "executor", "referee"]
```

### Deployment Behavior

| Configuration | Behavior |
|--------------|----------|
| `enabled = false` | No contracts deployed, CosmWasm VM not initialized |
| `enabled = true` | Deploys contracts based on `initial_contracts` |
| `deploy_on_node_types = []` | Only coordinators deploy (default) |
| `deploy_on_node_types = ["executor"]` | Only executors deploy |
| `required = true` | Startup fails if deployment fails |
| `required = false` | Warning logged, startup continues |
| `skip_if_exists = true` | Skip if contract name already deployed |

### WASM Loading Priority

1. **`wasm_bytes`** - Embedded bytes (base64 in proto, hex in TOML)
2. **`wasm_path`** - File path (absolute or relative to home directory)

### Contract Resolution

Deployed contracts can be accessed by name:

```rust
// Get contract address by name
let address = contract_manager.get_contract_address("identity_registry").await?;

// Query contract by name
let result: QueryResponse = contract_manager.query_contract(
    "identity_registry",
    &QueryMsg::GetNode { id: "node_1" }
).await?;

// Execute contract by name
contract_manager.execute_contract(
    "identity_registry",
    &ExecuteMsg::Register { ... }
).await?;
```

---

## Environment Variables

All configuration can be overridden via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `ERGORS_HOME` | Home directory path | `~/.ergors` |
| `ERGORS_GRPC_ADDR` | gRPC server address | `http://localhost:50051` |
| `ERGORS_CUSTODY_PASSWORD` | Custody unlock password | *(prompt)* |
| `ANTHROPIC_API_KEY` | Anthropic API key | *(from api-keys.json)* |
| `OPENAI_API_KEY` | OpenAI API key | *(from api-keys.json)* |
| `ANTHROPIC_API_BASE` | Anthropic API base URL | `https://api.anthropic.com` |
| `OPENAI_API_BASE` | OpenAI API base URL | `https://api.openai.com/v1` |

---

## File Constants

Defined in `ho_std::constants`:

```rust
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const LLM_API_KEYS_FILE: &str = "api-keys.json";
pub const ENV_VARIABLES_FILE: &str = ".env";
```

---

## Rust API

### Loading Configuration

```rust
use crate::config::ErgorsConfig;
use ho_std::traits::HoConfigTrait;

// Load from default path
let config = ErgorsConfig::load("~/.ergors/config.toml")?;

// Create new config
let config = ErgorsConfig::new(Utf8Path::new("~/.ergors"));

// Save config
config.save("~/.ergors/config.toml")?;
```

### Using Custody

```rust
use ho_std::custody::PasswordEncryptedCustody;
use ho_std::traits::NodeIdentityCustody;

// Create custody from config
let custody = config.create_password_custody();

// Unlock with password
custody.unlock("your-password").await?;

// Get private key (decrypted)
let private_key = custody.get_private_key().await?;

// Get public key (always available)
let public_key = custody.public_key()?;

// Lock when done
custody.lock().await;
```

### Creating New Identity

```rust
use ho_std::storage::identity::EncryptedIdentityBuilder;

let metadata = EncryptedIdentityBuilder::new()
    .user("ergors")
    .host("node-1.example.com")
    .p2p_port(26656)
    .api_port(8080)
    .node_type("coordinator")
    .build();

custody.create_identity("password", Some(metadata))?;
```

---

## Security Considerations

1. **Never commit secrets** - `api-keys.json`, `node_identity.enc`, and `.env` should be in `.gitignore`
2. **Use strong custody passwords** - Minimum 8 characters, enforced by the CLI
3. **Rotate API keys regularly** - Update `api-keys.json` periodically
4. **Secure file permissions** - Encrypted identity should be `600` (owner read/write only)
5. **Environment variable hygiene** - Prefer file-based secrets over env vars in production
6. **Cache TTL** - Set appropriate TTL for key caching based on security requirements

---

## Proto Type Reference

| Type | Package | Description |
|------|---------|-------------|
| `HoConfig` | `ergors.orch.v1` | Root configuration |
| `NodeIdentity` | `ergors.network.v1` | Node identity metadata |
| `NetworkConfig` | `ergors.network.v1` | P2P network settings |
| `StorageConfig` | `ergors.storage.v1` | Storage layer settings |
| `LlmRouterConfig` | `ergors.orch.v1` | LLM provider routing |
| `NodeIdentityCustodyConfig` | `ergors.storage.v1` | Custody backend config |
| `EncryptedNodeIdentity` | `ergors.storage.v1` | Encrypted key storage |
| `ApiKeysJson` | `ergors.orch.v1` | API keys file format |
| `CosmwasmConfig` | `ergors.orch.v1` | CosmWasm VM configuration |
| `ContractDeployment` | `ergors.orch.v1` | Contract deployment specification |
| `CosmwasmGasLimits` | `ergors.orch.v1` | Gas limits for WASM operations |
