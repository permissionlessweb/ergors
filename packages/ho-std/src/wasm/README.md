# CosmWasm-VM Integration for ERGORS

This module provides a complete CosmWasm-VM integration with Cnidarium storage backend, enabling smart contract execution on ERGORS nodes.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ERGORS Server Layer                      │
│  (Axum Router, LlmRouter, Network Node)                    │
└──────────────────┬──────────────────────────────────────────┘
                   │
         ┌─────────┴─────────┐
         │                   │
    ┌────▼────┐      ┌──────▼──────┐
    │   LLM   │      │  WASM-VM    │
    │ Routing │      │   Runtime   │
    └────┬────┘      └──────┬──────┘
         │                  │
         │          ┌───────┴────────┐
         │          │                │
         │    ┌─────▼─────┐   ┌─────▼──────┐
         │    │  CosmWasm │   │   Smart    │
         │    │  Backend  │   │  Contract  │
         │    │  (Custom) │   │  Manager   │
         │    └─────┬─────┘   └─────┬──────┘
         │          │               │
    ┌────▼──────────▼───────────────▼─────┐
    │      Cnidarium Storage (JMT)        │
    │  ┌──────────┬──────────┬──────────┐ │
    │  │   LLM    │ Encrypted│  WASM    │ │
    │  │ Configs  │   Keys   │  State   │ │
    │  └──────────┴──────────┴──────────┘ │
    └─────────────────────────────────────┘
```

## Components

### 1. Storage Layer (`state_keys.rs` + `state_ext.rs`)

Provides Cnidarium storage integration with a hierarchical key structure:

**Key Structure:**

```
wasm/config                           → WasmConfig (global settings)
wasm/code/{code_id}                  → WASM bytecode
wasm/code_info/{code_id}             → CodeInfo (metadata)
wasm/code_hash/{hash}                → code_id (lookup by hash)
wasm/contract/{address}              → ContractInfo
wasm/contract_by_code/{code_id}/{idx} → contract_address (index)
wasm/state/{contract_address}/{key}   → contract state value
wasm/contract_config/{contract_address} → ContractConfig (retention policy)
```

**Extension Traits:**

- `WasmVmCnidariumStateWrite`: Async read operations for WASM data
- `WasmVmCnidariumStateWrite`: Write operations for WASM data

**Usage Example:**

```rust
use crate::wasm::state_ext::{WasmVmCnidariumStateWrite, WasmVmCnidariumStateWrite};

// Read code
let code = state.get_wasm_code(code_id).await?;

// Write contract info
state.put_wasm_contract_info(&address, &contract_info);
```

### 2. CosmWasm Backend (`backend.rs`)

Custom Backend implementation for cosmwasm-vm that bridges with Cnidarium storage.

**Key Components:**

#### `WasmVmBackend`

- Implements `BackendApi` and `Api` traits from cosmwasm-vm
- Address validation, canonicalization, and humanization
- Signature verification (ed25519 supported, secp256k1 stub)

#### `CnidariumStorage`

- Implements `Storage` trait from cosmwasm-vm
- Uses trait objects with unsafe lifetime extension for CosmWasm VM compatibility
- Backed by Cnidarium state implementations for transactional writes
- Gas tracking with atomic counters for thread safety
- Iterator support for range scans

**Gas Costs:**

```rust
const GAS_COST_READ: u64 = 100;
const GAS_COST_WRITE: u64 = 200;
const GAS_COST_REMOVE: u64 = 150;
const GAS_COST_SCAN: u64 = 50;
const GAS_COST_NEXT: u64 = 25;
```

#### `CnidariumQuerier`

- Implements `Querier` trait for cross-contract queries
- Currently returns empty responses (stub for production expansion)

### Implementation Details

#### Trait Object Architecture

The integration uses trait objects with unsafe lifetime extension to bridge the gap between CosmWasm-VM's synchronous `Storage` trait and Cnidarium's async state management:

```rust
// WasmStorageState trait provides object-safe methods
pub trait WasmStorageState: std::fmt::Debug + Send + 'static {
    async fn get_contract_state_dyn(&self, contract_address: &str, key: &[u8]) -> HoResult<Option<Vec<u8>>>;
    // ... other methods
}

// Blanket implementation for any StateRead + StateWrite type
impl<T> WasmStorageState for T
where
    T: StateRead + StateWrite + std::fmt::Debug + Send + 'static,
{ /* ... */ }

// CnidariumStorage uses unsafe lifetime extension
pub struct CnidariumStorage {
    state: &'static mut dyn WasmStorageState, // Extended lifetime
    // ...
}
```

#### Safety Considerations

- **Lifetime Extension**: Uses `std::mem::transmute` to extend state reference lifetimes to `'static`
- **Caller Responsibility**: Storage instances must not outlive their backing state references
- **Thread Safety**: Gas counters use `AtomicU64` for safe concurrent access

**Usage Example:**

```rust
use crate::wasm::backend::{WasmVmBackend, CnidariumStorage, CnidariumQuerier};

// SAFETY: The storage instance must not outlive the state reference
let storage = unsafe { CnidariumStorage::new(contract_address, state) };
let api = WasmVmBackend;
let querier = CnidariumQuerier;

let backend = Backend { api, storage, querier };
```

### 3. WasmRuntime (`runtime.rs`)

High-level API for managing the complete contract lifecycle.

**Configuration:**

```rust
pub const DEFAULT_INSTANTIATE_GAS: u64 = 100_000_000;
pub const DEFAULT_EXECUTE_GAS: u64 = 50_000_000;
pub const DEFAULT_QUERY_GAS: u64 = 10_000_000;
pub const DEFAULT_MEMORY_LIMIT: usize = 32 * 1024 * 1024; // 32 MB
pub const MAX_WASM_CODE_SIZE: usize = 800 * 1024; // 800 KB
```

**Core Methods:**

#### `store_code`

Upload and validate WASM bytecode.

```rust
pub async fn store_code<S>(
    &self,
    state: &mut S,
    wasm_code: Vec<u8>,
    creator: String,
) -> HoResult<u64>
```

**Process:**

1. Validates code size (max 800KB)
2. Computes SHA256 hash
3. Checks for duplicate (by hash)
4. Validates WASM via Cache
5. Stores code and metadata
6. Returns `code_id`

#### `instantiate_contract`

Create a new contract instance.

```rust
pub async fn instantiate_contract<S>(
    &self,
    state: &mut S,
    code_id: u64,
    creator: String,
    admin: Option<String>,
    label: String,
    msg: Vec<u8>,
    funds: Vec<Coin>,
) -> HoResult<(String, Vec<u8>)>
```

**Process:**

1. Fetches WASM code by `code_id`
2. Generates deterministic contract address
3. Creates CosmWasm `Env` and `MessageInfo`
4. Initializes custom Backend with Cnidarium storage
5. Creates VM Instance with gas limits
6. Calls `instantiate` entrypoint
7. Stores `ContractInfo`
8. Returns `(contract_address, response_data)`

**Address Generation:**

```rust
fn generate_contract_address(code_id, creator, label) -> String {
    SHA256(code_id || creator || label)[0..20] → "ergors{hex}"
}
```

#### `execute_contract`

Execute a mutable contract operation.

```rust
pub async fn execute_contract<S>(
    &self,
    state: &mut S,
    contract_address: String,
    sender: String,
    msg: Vec<u8>,
    funds: Vec<Coin>,
) -> HoResult<Vec<u8>>
```

**Process:**

1. Fetches contract and code
2. Creates execution environment
3. Initializes Backend with writable state
4. Creates VM Instance
5. Calls `execute` entrypoint
6. Returns response data
7. **State changes are persisted via StateDelta**

#### `query_contract`

Read-only contract query.

```rust
pub async fn query_contract<S>(
    &self,
    state: &S,
    contract_address: String,
    msg: Vec<u8>,
) -> HoResult<Vec<u8>>
```

**Process:**

1. Fetches contract and code
2. Creates query environment
3. Initializes Backend (state changes discarded)
4. Creates VM Instance with query gas limit
5. Calls `query` entrypoint
6. Returns response data

## Usage Examples

### Complete Contract Lifecycle

```rust
use ho_std::wasm::WasmRuntime;
use std::path::PathBuf;

// Initialize runtime
let cache_dir = PathBuf::from("./data/wasm_cache");
let runtime = WasmRuntime::new(cache_dir)?;

// 1. Upload Code
let wasm_bytecode = std::fs::read("./contract.wasm")?;
let code_id = runtime
    .store_code(&mut state, wasm_bytecode, "creator_address".to_string())
    .await?;

println!("Uploaded code with ID: {}", code_id);

// 2. Instantiate Contract
let init_msg = serde_json::json!({
    "count": 0
});
let init_msg_bytes = serde_json::to_vec(&init_msg)?;

let (contract_address, init_response) = runtime
    .instantiate_contract(
        &mut state,
        code_id,
        "creator".to_string(),
        Some("admin".to_string()),
        "my-counter".to_string(),
        init_msg_bytes,
        vec![], // no funds
    )
    .await?;

println!("Contract instantiated at: {}", contract_address);

// 3. Execute Contract
let exec_msg = serde_json::json!({
    "increment": {}
});
let exec_msg_bytes = serde_json::to_vec(&exec_msg)?;

let exec_response = runtime
    .execute_contract(
        &mut state,
        contract_address.clone(),
        "sender".to_string(),
        exec_msg_bytes,
        vec![],
    )
    .await?;

println!("Execute response: {:?}", exec_response);

// 4. Query Contract
let query_msg = serde_json::json!({
    "get_count": {}
});
let query_msg_bytes = serde_json::to_vec(&query_msg)?;

let query_response = runtime
    .query_contract(&state, contract_address.clone(), query_msg_bytes)
    .await?;

let count: u64 = serde_json::from_slice(&query_response)?;
println!("Current count: {}", count);
```

### Integration with ErgorsStorage

```rust
use cw_ho::ErgorsStorage;
use ho_std::wasm::WasmRuntime;

// Initialize storage
let data_dir = PathBuf::from("./data");
let storage = ErgorsStorage::new(data_dir.clone(), vec!["wasm".to_string()]).await?;

// Initialize WASM runtime
let cache_dir = data_dir.join("wasm_cache");
let runtime = WasmRuntime::new(cache_dir)?;

// Use latest snapshot for reads
let snapshot = storage.cs.latest_snapshot();

// Use StateDelta for writes
let mut delta = cnidarium::StateDelta::new(snapshot);

// Execute operations
let code_id = runtime.store_code(&mut delta, wasm_code, creator).await?;

// Commit changes
storage.cs.commit(delta).await?;
```

## Feature Flag

The CosmWasm-VM integration is gated behind the `cw` feature flag:

```toml
[features]
cw = ["cosmwasm-vm"]
```

**Enable with:**

```bash
cargo build --features cw
```

**Check availability:**

```rust
#[cfg(feature = "cw")]
{
    // CosmWasm features available
}
```

## Storage Patterns

### Deterministic State Updates

All contract state updates are deterministic:

1. State changes accumulate in `StateDelta`
2. Changes are committed atomically
3. JMT ensures verifiable state roots

### Gas Tracking

Gas is tracked with atomic counters for thread safety:

```rust
// SAFETY: The storage instance must not outlive the state reference
let storage = unsafe { CnidariumStorage::new(address, state) };
// Execute contract...
let gas_used = storage.gas_used();
```

### State Cleanup

To delete all contract state:

```rust
state.delete_all_contract_state(&contract_address);
```

## Future Enhancements

### 1. Retention Policies (TODO)

Currently, all state is retained indefinitely. Future versions will support:

```rust
pub enum RetentionPolicy {
    LatestOnly,           // Keep only latest state
    Versioned(u64),       // Keep last N versions
    TimeBased(Duration),  // Keep for time period
}
```

### 2. Contract Migration

Add support for `call_migrate_raw`:

```rust
pub async fn migrate_contract(...) -> HoResult<Vec<u8>>
```

### 3. IBC Integration

Support inter-blockchain communication via IBC callbacks:

- `call_ibc_channel_open`
- `call_ibc_channel_connect`
- `call_ibc_channel_close`
- `call_ibc_packet_receive`

### 4. Enhanced Querier

Implement cross-contract and system queries:

```rust
impl Querier for CnidariumQuerier {
    fn query_raw(&self, request: &[u8], gas_limit: u64) -> QuerierResult {
        // Parse request type
        // Route to appropriate handler (bank, staking, wasm, custom)
    }
}
```

### 5. Admin Operations

- `update_admin`: Change contract admin
- `clear_admin`: Remove admin privileges
- `pin_codes` / `unpin_codes`: Cache management

### 6. Signature Verification

Add full secp256k1 support (currently stub):

```rust
fn secp256k1_verify(...) -> Result<bool, VerificationError> {
    // Use k256 or secp256k1 crate
}
```

## Testing

### Unit Tests

```bash
cargo test --package ho-std --lib wasm --features cw
```

### Integration Tests

```bash
cargo test --package ho-std --test wasm_integration --features cw
```

### Example Contract Tests

Use the official CosmWasm testing utilities:

```rust
use cosmwasm_vm::testing::{mock_env, mock_info};
```

## Security Considerations

### Gas Limits

Always enforce gas limits to prevent DoS:

```rust
let options = InstanceOptions {
    gas_limit: DEFAULT_EXECUTE_GAS,
    print_debug: false,
};
```

### Memory Limits

Constrain WASM memory usage:

```rust
Instance::from_module(..., self.memory_limit, options)
```

### Code Size Validation

Reject oversized contracts:

```rust
if wasm_code.len() > MAX_WASM_CODE_SIZE {
    return Err(...);
}
```

### State Isolation

Each contract's state is isolated by address prefix:

```
wasm/state/{contract_address}/*
```

### Admin Controls

Contracts can have optional admin addresses for migrations:

```rust
admin: Some("admin_address".to_string())
```

## Performance

### Caching

- Compiled WASM modules are cached on disk
- Cache directory: `./data/wasm_cache`
- Modules are loaded from cache for subsequent executions


### Storage Optimization

- State keys are hex-encoded for efficient indexing
- Range scans supported via prefix iteration
- Gas costs incentivize minimal state usage

## Troubleshooting

### "WASM code size exceeds maximum"

- Default max: 800 KB
- Optimize contract with `cosmwasm-optimizer`
- Consider splitting into multiple contracts

### "Failed to create instance"

- Check gas limits are sufficient
- Verify memory limits
- Ensure WASM is valid CosmWasm contract

### "Storage read error"

- Verify Cnidarium storage is initialized
- Check contract address exists
- Ensure state was committed properly

### "Iterator not found"

- Iterators are stateful and single-use
- Don't reuse iterator IDs
- Check for concurrent access issues

## References

- [CosmWasm Documentation](https://docs.cosmwasm.com/)
- [CosmWasm-VM Crate](https://crates.io/crates/cosmwasm-vm)
- [Cnidarium Storage](https://github.com/penumbra-zone/penumbra/tree/main/crates/cnidarium)
- [ERGORS Repository](https://github.com/permissionlessweb/ergors)

## License

MIT OR Apache-2.0
