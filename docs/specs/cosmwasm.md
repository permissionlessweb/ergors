# CosmWasm VM Integration Specification

## Overview

This specification outlines the complete integration of CosmWasm VM into the ERGORS system, enabling each individual node to instantiate and execute smart contracts as isolated "mini-chains".

**Core Focus: Node-Wide State Synchronization**
The primary friction point is ensuring contract writes complete before state queries occur. This specification implements node-wide synchronization to guarantee:

- All contract writes are immediately committed and visible to subsequent reads
- Cross-contract queries always see consistent state
- No stale reads possible within the same node
- Partial state preservation on failures (standard CosmWasm behavior)

**Architecture Principles:**

- **Node Isolation**: Each ERGORS node maintains its own VM instance and contract ecosystem
- **Synchronization Scope**: Node-wide (not per-contract) since contracts can query other contracts
- **State Consistency**: Immediate commit of all state changes with proper read-write barriers

## Core Architecture

### Mini-Chain Concept

Each ERGORS node operates as an independent CosmWasm execution environment where:

- Contracts are isolated per node with deterministic addressing
- State changes are committed atomically using Cnidarium's StateDelta
- Cross-contract calls are supported within the same node
- Gas metering prevents resource exhaustion
- Storage is verifiable and cryptographically secure

### Key Components

#### 1. WasmRuntime (packages/ho-std/src/wasm/runtime.rs)

High-level API providing contract lifecycle management:

- `store_code()` - Upload and validate WASM bytecode
- `instantiate_contract()` - Create contract instances
- `execute_contract()` - Handle mutable contract calls
- `query_contract()` - Handle read-only contract queries

#### 2. CnidariumStorage (packages/ho-std/src/wasm/backend.rs)

CosmWasm Storage trait implementation using Cnidarium:

- Thread-safe state access with Arc<Mutex<>>
- Atomic state updates via StateDelta
- Gas metering for storage operations
- Contract-specific state isolation

#### 3. WasmVmBackend (packages/ho-std/src/wasm/backend.rs)

CosmWasm BackendApi implementation:

- Address validation and canonicalization
- Cryptographic operations
- Querier for cross-contract queries

#### 4. ErgorsAppState Integration (packages/cw-ho/src/lib.rs)

Runtime inclusion in application state:

```rust
#[cfg(feature = "cw")]
pub struct ErgorsAppState {
    // ... existing fields
    pub wasm: Arc<WasmRuntime>,
}
```

## Implementation Plan

### Phase 1: Node-Wide State Synchronization

#### **1.1 Node-Wide Atomic Operations**

**Architecture:** Each ERGORS node maintains its own isolated CosmWasm VM instance with node-wide state synchronization.

**Key Design Decisions:**

- **Scope**: Node-wide synchronization (not per-contract) since contracts can query other contracts
- **State Policy**: Leave partial state on execution failures (CosmWasm standard behavior)
- **Isolation**: Each node operates independently with its own contract ecosystem

**Implementation Pattern:**

```rust
pub struct NodeWasmRuntime {
    cache: Arc<WasmCache>,
    state_lock: Arc<RwLock<()>>, // Node-wide synchronization
    // ... other fields
}

impl NodeWasmRuntime {
    /// Execute contract with node-wide state consistency
    pub async fn execute_contract_node_atomic(
        &self,
        state: &mut CnidariumStorage,
        contract_address: String,
        // ... other params
    ) -> HoResult<ContractResult<Response>> {
        // Acquire node-wide lock for state consistency
        let _node_lock = self.state_lock.write().await;

        // Execute contract and apply StateDelta immediately
        let result = self.execute_contract_with_immediate_commit(
            state, contract_address, /*...*/
        ).await?;

        // StateDelta is applied and committed within the lock
        Ok(result)
    }

    /// Query contract with node-wide consistency guarantee
    pub async fn query_contract_node_consistent(
        &self,
        state: &CnidariumStorage, // Read-only for queries
        contract_address: String,
        msg: Vec<u8>,
    ) -> HoResult<ContractResult<Binary>> {
        // Shared read lock allows concurrent queries
        let _node_lock = self.state_lock.read().await;

        // Query always sees latest committed state
        self.query_contract_latest(state, contract_address, msg).await
    }
}
```

#### **1.2 Read-Write Consistency Model**

**Guarantees:**

- All writes are immediately committed and visible to subsequent reads
- Cross-contract queries always see consistent state
- No stale reads possible within the same node
- Partial state preservation on failures (standard CosmWasm behavior)

#### **1.3 Synchronization Barriers**

**Files:** packages/ho-std/src/wasm/runtime.rs

**Implementation:**

- Node-wide RwLock for read-write synchronization
- Write operations take exclusive lock, blocking all other operations
- Read operations take shared lock, allowing concurrent queries
- Immediate StateDelta application within lock scope

#### **1.4 Enable WasmRuntime in Application State**

**Files:** packages/cw-ho/src/lib.rs, packages/cw-ho/src/server.rs

**Changes:**

- Update WasmRuntime to include node-wide synchronization primitives
- Add conditional compilation with `#[cfg(feature = "cw")]`
- Initialize WasmRuntime in Server::new() with proper cache directory and synchronization setup

### Phase 2: Mini-Chain Isolation & Security

#### **2.1 Node-Scoped Contract Isolation**

**Design:** Each node maintains completely independent contract ecosystems with proper namespacing.

**Implementation:**

- Node-specific contract addressing: `ergors{node_id}_{contract_hash}`
- Isolated state storage per node
- Cross-node contract discovery through network protocols (future phase)

#### **2.2 Contract Address Generation**

**Files:** packages/ho-std/src/wasm/runtime.rs

**Implementation:**

- Deterministic addressing using SHA256: node_id + code_id + creator + label
- Collision-resistant with 20-byte truncated hash
- Human-readable prefix for easy identification

```rust
fn generate_contract_address(
    &self,
    code_id: u64,
    creator: &str,
    label: &str,
    node_id: &str,
) -> HoResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(node_id.as_bytes());
    hasher.update(code_id.to_le_bytes());
    hasher.update(creator.as_bytes());
    hasher.update(label.as_bytes());
    let hash = hasher.finalize();

    Ok(format!("ergors{}_{}", node_id, hex::encode(&hash[..20])))
}
```

#### 2.2 Gas Metering & Limits

**Files:** packages/ho-std/src/wasm/runtime.rs, packages/ho-std/src/wasm/backend.rs

**Configuration:**

```rust
pub struct GasLimits {
    pub instantiate: u64,  // Default: 100_000_000
    pub execute: u64,      // Default: 50_000_000
    pub query: u64,        // Default: 10_000_000
    pub migrate: u64,      // Default: 75_000_000
}
```

#### 2.3 Storage Isolation

**Files:** packages/ho-std/src/wasm/state_ext.rs

**Key Structure:**

```
wasm/
├── code/{code_id}/
│   ├── bytecode
│   ├── info
│   └── hash
├── contracts/{contract_address}/
│   ├── info
│   ├── state/{key} -> value
│   └── code_id
└── config/
    └── next_code_id
```

### Phase 3: HTTP API Integration

#### 3.1 Message Routing

**Files:** packages/cw-ho/src/cosmwasm.rs

**Supported Messages:**

- `MsgStoreCode` - Upload contract code
- `MsgInstantiateContract` - Create contract instances
- `MsgExecuteContract` - Execute contract methods
- `MsgMigrateContract` - Upgrade contract code
- `MsgUpdateAdmin` - Change contract admin

#### 3.2 Error Handling

**Standard Response Format:**

```json
{
  "error": {
    "code": "WASM_ERROR",
    "message": "Contract execution failed: gas exhausted",
    "details": { ... }
  }
}
```

#### 3.3 Gas Reporting

**Response Enhancement:**

```json
{
  "result": { ... },
  "gas_used": 1500000,
  "gas_limit": 50000000
}
```

### Phase 4: Cross-Contract Communication

#### 4.1 IBC-Like Messaging

**Implementation:**

- Contract-to-contract calls within the same node
- Event emission and subscription system
- Sub-message execution with reply handling

#### 4.2 Permission System

**Access Control:**

- Contract admin permissions
- Instantiate permissions per code ID
- Execution permissions based on caller address

### Phase 5: Testing & Validation

#### 5.1 Unit Tests

- Contract instantiation and execution
- Gas limit enforcement
- State isolation verification
- Address generation determinism

#### 5.2 Integration Tests

- Multi-contract interactions
- Cross-contract calls
- State persistence across restarts
- Concurrent execution safety

## Contract Lifecycle & Deployment

### Initial Contract Upload

Contracts are deployed via the **ContractManager** (`packages/cw-ho/src/contracts/manager.rs`), which provides:

- **Named contract resolution** - Contracts are referenced by name (e.g., `"identity_registry"`)
- **Automatic coordinator deployment** - Required contracts deployed on coordinator startup
- **Existence checks** - Skip deployment if contract already exists

#### Startup Deployment Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Coordinator Node Startup                      │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
                   ┌─────────────────────┐
                   │ Check node_type ==  │
                   │   "coordinator"     │
                   └──────────┬──────────┘
                              │
                    ┌─────────┴─────────┐
                    │ No                │ Yes
                    ▼                   ▼
            ┌───────────────┐  ┌───────────────────┐
            │ Skip deploy   │  │ Check contract    │
            │ (regular node)│  │ already exists?   │
            └───────────────┘  └─────────┬─────────┘
                                         │
                               ┌─────────┴─────────┐
                               │ Yes               │ No
                               ▼                   ▼
                       ┌─────────────────┐  ┌─────────────────┐
                       │ Skip (already   │  │ Upload WASM +   │
                       │ deployed)       │  │ Instantiate     │
                       └─────────────────┘  └─────────────────┘
```

#### Code Example: Deploying a Contract

```rust
use crate::contracts::{ContractManager, ProviderConfig};

// Create manager with storage and runtime
let manager = ContractManager::new(storage, wasm_runtime, node_id);

// Check if already deployed
if !manager.contract_exists("identity_registry").await? {
    // Upload WASM bytecode
    let code_id = manager.upload_contract(&wasm_bytes, "identity_registry").await?;

    // Instantiate with init message
    let init_msg = IdentityRegistryInstantiateMsg {
        coordinator: coordinator_pubkey,
        providers: vec![
            ProviderConfig {
                name: "anthropic".to_string(),
                ownership: "shared".to_string(),
                threshold: Some(2),
                total_shares: Some(3),
            },
        ],
    };

    let address = manager.instantiate_contract(code_id, "identity_registry", &init_msg).await?;
}
```

#### Default Contracts

| Contract | Purpose | Deployed By |
|----------|---------|-------------|
| `identity_registry` | Node identity verification, key share tracking | Coordinator on fresh DB |

### Cross-Node Contract Communication

Nodes can query and execute contracts on **other nodes** via P2P network messages:

#### Network Channel 4: Contract Operations

```
┌──────────────┐          Channel 4          ┌──────────────┐
│   Node A     │ ─────────────────────────► │   Node B     │
│              │     ContractQuery           │              │
│  Query B's   │                             │  Execute on  │
│  contract    │ ◄───────────────────────── │  local VM    │
│              │     QueryResponse           │              │
└──────────────┘                             └──────────────┘
```

#### Message Types

```protobuf
message ContractQuery {
  string target_node_id = 1;      // Node hosting the contract
  string contract_name = 2;        // Named contract reference
  bytes query_msg = 3;             // JSON-encoded query
  bytes sender_pubkey = 4;         // For access control
}

message ContractExecute {
  string target_node_id = 1;
  string contract_name = 2;
  bytes execute_msg = 3;
  bytes sender_pubkey = 4;
  bytes signature = 5;             // Proves sender identity
}
```

#### Query Flow

```rust
// Node A queries Node B's contract
let response = network.query_remote_contract(
    target_node: "node_b",
    contract: "identity_registry",
    msg: QueryMsg::GetNodeInfo { node_id: "node_c" },
).await?;
```

#### Permission Model

Cross-node contract calls follow these rules:

1. **Queries** - Read-only, allowed by default
2. **Executions** - Require signature verification
3. **Admin operations** - Only allowed for contract admin

```rust
// Verify sender has permission
fn verify_cross_node_execute(
    sender_pubkey: &[u8],
    contract_admin: &[u8],
    msg: &ExecuteMsg,
) -> bool {
    match msg {
        // Admin-only operations
        ExecuteMsg::UpdateConfig { .. } => sender_pubkey == contract_admin,
        // Public operations with rate limiting
        ExecuteMsg::RegisterNode { .. } => true,
        // Restricted by contract logic
        _ => verify_in_contract(sender_pubkey, msg),
    }
}
```

## Configuration

### Environment Variables

```bash
# CosmWasm Configuration
COSMWASM_ENABLED=true
COSMWASM_CACHE_DIR=./data/wasm_cache
COSMWASM_MEMORY_LIMIT=33554432  # 32MB
COSMWASM_INSTANTIATE_GAS=100000000
COSMWASM_EXECUTE_GAS=50000000
COSMWASM_QUERY_GAS=10000000
```

### Cargo Features

```toml
[features]
default = []
cw = ["cosmwasm-vm", "cosmwasm-std"]
```

## Security Considerations

### 1. Resource Limits

- Strict gas metering prevents infinite loops
- Memory limits prevent excessive allocation
- Code size validation prevents oversized uploads

### 2. Isolation

- Contract state completely isolated per address
- No direct filesystem access
- Controlled network access through querier

### 3. Validation

- WASM bytecode validation on upload
- Address format validation
- Permission checks for privileged operations

## Performance Optimizations

### 1. Caching Strategy

- Compiled WASM modules cached in memory and disk
- Frequently accessed contracts kept in memory
- LRU eviction for cache management

### 2. State Optimization

- Lazy state loading for queries
- Batch state updates for efficiency
- Compression for large state values

### 3. Gas Optimization

- Efficient gas tracking with AtomicU64
- Early termination on gas exhaustion
- Configurable gas limits per operation type

## Migration Path

### From Current State

1. Enable `cw` feature flag
2. Initialize WasmRuntime in server startup
3. Add cosmwasm routes to HTTP server
4. Test with simple contracts
5. Gradually enable advanced features

### Backward Compatibility

- Non-cw builds continue to work unchanged
- Optional feature maintains clean separation
- Configuration defaults prevent breaking changes

## Success Metrics

### **Functional**

- ✅ Contract upload, instantiation, execution, and querying
- ✅ **Node-wide state synchronization**: All writes committed before reads
- ✅ **Cross-contract consistency**: Queries see latest committed state across all contracts
- ✅ **No stale reads**: Impossible to read uncommitted state changes
- ✅ Gas metering and limits enforcement
- ✅ Partial state preservation on failures (CosmWasm standard)

### **Performance**

- ✅ Sub-second contract instantiation
- ✅ Efficient gas tracking (<5% synchronization overhead)
- ✅ Memory usage within configured limits
- ✅ Concurrent queries allowed, exclusive writes for consistency

### **Security**

- ✅ Complete node-level contract isolation
- ✅ Resource limit enforcement
- ✅ Address collision resistance within node namespace
- ✅ Thread-safe state operations with proper locking

## Risk Mitigation

### 1. Compilation Issues

- Use trait objects for dynamic dispatch
- Proper lifetime management
- Comprehensive error handling

### 2. Runtime Stability

- Extensive testing before production deployment
- Gradual feature rollout
- Monitoring and alerting for anomalies

### 3. Security Vulnerabilities

- Regular security audits
- Gas limit tuning based on real-world usage
- Input validation for all contract inputs

## Key Design Decisions

### **State Synchronization vs Geometric Memory**

**Decision**: Focus on state synchronization rather than geometric memory allocation.

**Rationale**:

- CosmWasm's linear memory model is compatible with ERGORS' geometric design
- The actual friction point is state consistency, not memory allocation patterns
- Geometric memory allocation would add unnecessary complexity for imported CosmWasm contracts
- ERGORS-native contracts can implement geometric patterns at the application level

### **Node-Wide vs Per-Contract Synchronization**

**Decision**: Node-wide synchronization scope.

**Rationale**:

- Contracts can query other contracts within the same node
- Ensures cross-contract consistency guarantees
- Simpler to implement than per-contract isolation
- Acceptable performance trade-off for correctness

### **Partial State Preservation**

**Decision**: Leave partial state on execution failures.

**Rationale**:

- Maintains compatibility with standard CosmWasm behavior
- Allows for manual cleanup and error recovery
- Avoids complex rollback logic that could introduce new bugs
- Contract developers can implement their own error handling

This specification provides a concrete, implementable plan for complete CosmWasm VM integration, addressing the core state synchronization friction points while maintaining compatibility with ERGORS' architectural principles.</content>
<parameter name="filePath">docs/specs/cosmwasm.md
