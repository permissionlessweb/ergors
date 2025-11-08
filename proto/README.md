# Ergors Protocol Buffer Definitions

This package contains Protocol Buffer definitions for the CW-HO (CommonWare Host Orchestrator) system, organized into two main layers following the sacred geometry principles:

## 📁 Structure

```
storage: related to storage layout and struct definitions
orchestration: related to 
```

## 📦 Layers

### Storage Layer (`storage/v1/`)

### Orchestration Layer (`orchestration/v1/`)

### Network Layer (`network/v1/`)

## 🛠 Usage

```rust
use cw_hoe_proto::{
    StoragePromptRequest, StoragePromptResponse,
    CosmicTask, NetworkMessage, NodeType,
    constants::GOLDEN_RATIO
};

// Create a storage request
let request = StoragePromptRequest {
    provider: "akash_chat".to_string(),
    model: "DeepSeek-R1-0528".to_string(),
    messages: vec![...],
    temperature: Some(0.7),
    max_tokens: Some(2048),
    context: Some(...),
};

// Create a cosmic task following fractal requirements
let task = CosmicTask {
    id: uuid::Uuid::new_v4().to_string(),
    task_type: OrchestrateTask::Recursive as i32,
    fractal_requirements: Some(FractalRequirements {
        recursion_depth: 3,
        golden_ratio_compliance: true,
        fractal_coherence: GOLDEN_RATIO - 1.0, // ≈ 0.618
        ..Default::default()
    }),
    ..Default::default()
};
```

## 🔄 Generation

Proto files are built automatically via `build.rs` using `prost-build`. To regenerate:

```bash
cd packages/ergors
cargo build
```

For external tooling (buf, protoc):

```bash
# Using buf (recommended)
buf generate

# Using protoc directly  
protoc --rust_out=src hoe/**/*.proto
```

## 🎯 Design Principles

1. **Backwards Compatibility**: Versioned packages (`v1`, future `v2`, etc.)
2. **Sacred Geometry**: Constants and patterns embedded in field defaults
3. **Ergonomic Rust**: Serde traits, type aliases, and re-exports
4. **Network Efficiency**: Optimized for tetrahedral P2P communication
5. **Fractal Scalability**: Self-similar patterns across all abstraction levels

## 🔮 Future Extensions

- **gRPC Services**: Add service definitions for network APIs
- **Validation**: Custom proto validators for golden ratio compliance
- **Compression**: Specialized encoding for fractal data structures
- **Streaming**: Real-time sandloop state synchronization protocols
