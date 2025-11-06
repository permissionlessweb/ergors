# CW-HO Minimal Networking Implementation Specification

## Overview

This specification defines a minimal networking implementation for cw-ho using commonware libraries, avoiding the fractal complexity that occurred in the previous implementation while maintaining the essential tetrahedral topology.

## Architecture Decision

After analyzing the commonware libraries and the existing implementation, we will use:

1. **commonware-p2p**: For authenticated peer-to-peer communication with encryption
2. **commonware-cryptography**: For Ed25519-based node identities
3. **commonware-broadcast**: For efficient message dissemination across the network
4. **commonware-collector**: For request-response patterns between nodes

## Core Design Principles

1. **Simplicity First**: Remove fractal patterns, golden ratio calculations, and Möbius loops
2. **Essential Features Only**: Focus on basic node discovery, messaging, and topology management
3. **Leverage Commonware**: Use existing commonware patterns without overengineering
4. **Clear Separation**: Keep networking concerns isolated from application logic

## Network Topology

### Simplified Tetrahedral Model

```
     Coordinator
      /   |   \
     /    |    \
Executor--+--Referee
     \    |    /
      \   |   /
     Development
```

Each node type connects to all other types, forming a complete graph with 4 vertices and 6 edges.

## Implementation Components

### 1. Node Identity (commonware-cryptography)

```rust
use commonware_cryptography::ed25519::{PrivateKey, PublicKey};

pub struct NodeIdentity {
    private_key: PrivateKey,
    public_key: PublicKey,
    node_type: NodeType,
    node_id: String,
}
```

### 2. Network Manager (commonware-p2p)

```rust
pub struct CwHoNetworkManifold {
    identity: NodeIdentity,
    p2p_network: Network<PublicKey>,
    broadcast: Broadcast<PublicKey>,
    collector: Collector<PublicKey>,
    peers: HashMap<PublicKey, PeerInfo>,
}
```

### 3. Message Types

```rust
#[derive(Serialize, Deserialize)]
pub enum NetworkMessage {
    // Discovery
    NodeAnnounce { 
        node_type: NodeType,
        capabilities: Vec<String>,
    },
    
    // Task Coordination
    TaskRequest {
        task_id: Uuid,
        task_type: String,
        payload: Vec<u8>,
    },
    TaskResponse {
        task_id: Uuid,
        status: TaskStatus,
        result: Option<Vec<u8>>,
    },
    
    // State Sync
    StateRequest {
        version: u64,
    },
    StateUpdate {
        version: u64,
        updates: Vec<StateOp>,
    },
    
    // Health
    Ping { timestamp: u64 },
    Pong { timestamp: u64 },
}
```

## Network Channels

Using commonware-p2p channels for different message types:

1. **Channel 0**: Discovery and topology management
2. **Channel 1**: Task coordination
3. **Channel 2**: State synchronization
4. **Channel 3**: Health checks

## Bootstrap Process

1. Load node identity from environment/config
2. Initialize commonware-p2p with bootstrap peers
3. Start discovery protocol on Channel 0
4. Announce node capabilities
5. Build peer topology map

## Message Flow Patterns

### 1. Broadcast Pattern (commonware-broadcast)

- Node announcements
- State updates
- Network-wide notifications

### 2. Request-Response Pattern (commonware-collector)

- Task assignment/execution
- State queries
- Health checks

### 3. Direct Messaging (commonware-p2p)

- Point-to-point task coordination
- Peer-specific state sync

## Configuration

```toml
[network]
node_type = "executor"
listen_addr = "0.0.0.0:3000"
bootstrap_peers = [
    "12D3KooWExample1...:3001",
    "12D3KooWExample2...:3002"
]

[network.limits]
max_message_size = 1048576  # 1MB
max_peers = 50
connection_timeout = 30

[network.channels]
discovery_buffer = 100
task_buffer = 1000
state_buffer = 500
health_buffer = 50
```

## API Interface

```rust
impl CwHoNetworkManifold {
    /// Initialize and start the network
    pub async fn new(config: NetworkConfig) -> HoResult<Self>;
    
    /// Send a message to specific node type(s)
    pub async fn send_to_role(&self, role: NodeType, msg: NetworkMessage) -> HoResult<()>;
    
    /// Broadcast a message to all peers
    pub async fn broadcast(&self, msg: NetworkMessage) -> HoResult<()>;
    
    /// Request data with timeout (using collector)
    pub async fn request(&self, peer: PublicKey, req: NetworkMessage, timeout: Duration) -> HoResult<NetworkMessage>;
    
    /// Get current network topology
    pub fn get_topology(&self) -> NetworkTopology;
    
    /// Subscribe to network events
    pub fn subscribe(&self) -> Receiver<NetworkEvent>;
}
```

## Error Handling

```rust
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("P2P error: {0}")]
    P2P(#[from] commonware_p2p::Error),
    
    #[error("Broadcast error: {0}")]
    Broadcast(#[from] commonware_broadcast::Error),
    
    #[error("Collector timeout")]
    CollectorTimeout,
    
    #[error("No peers with role: {0}")]
    NoPeersForRole(String),
    
    #[error("Message serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

## Testing Strategy

1. **Unit Tests**: Test individual components in isolation
2. **Integration Tests**: Use commonware's simulated network for testing
3. **Local Cluster**: Run 4-node local cluster for tetrahedral testing

## Migration Path

1. Create new `network` module alongside existing implementation
2. Implement minimal features first
3. Add integration tests
4. Switch main application to use new implementation
5. Remove old implementation after verification

## Performance Targets

- Node discovery: < 5 seconds
- Message latency: < 100ms (local network)
- Throughput: 1000 messages/second per channel
- Memory usage: < 100MB per node

## Security Considerations

1. All messages authenticated via Ed25519 signatures (built into commonware-p2p)
2. Encrypted connections between peers (provided by commonware-p2p)
3. Rate limiting per channel to prevent spam
4. Peer reputation tracking for misbehavior detection

## Next Steps

1. Implement `NodeIdentity` wrapper around commonware-cryptography
2. Create `CwHoNetworkManifold` with basic p2p functionality
3. Add broadcast layer for efficient message dissemination
4. Integrate collector for request-response patterns
5. Build integration tests with 4-node topology
6. Benchmark and optimize based on performance targets
