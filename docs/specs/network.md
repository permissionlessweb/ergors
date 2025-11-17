# Networking Implementation Specification for ERGORS

This specification defines a minimal networking implementation for ergors using commonware libraries, avoiding the fractal complexity that occurred in the previous implementation while maintaining the essential tetrahedral topology.

- [NodeIdentity](packages/ho-proto-rs/src/types/ergors/network/v1.rs)
- [network types](./packages/ho-proto-rs/src/types/ergors/network/v1.rs)

## Overview

The ERGORS (Life Creativity Engine) project is a distributed multi-LLM agent orchestration platform designed to reduce creative friction through intelligent automation for public goods. Central to this vision is a robust networking layer that enables peer-to-peer (P2P) communication across a tetrahedral mesh of nodes, ensuring seamless coordination and state synchronization. The networking implementation, found in `packages/ho-core/src/network/`, comprises two primary systems: a legacy WebSocket-based P2P network for compatibility and a modern Commonware-based network leveraging authenticated discovery protocols from `commonware-cryptography`. This specification details the networking implementation as reflected in the current compiled binary, capturing the sacred geometry principles—tetrahedral connectivity, golden ratio resource allocation, Möbius strip sandloop feedback, and fractal task scaling—that underpin its design.

**Intention**: Networking in ERGORS is the digital nervous system of a tetrahedral organism, connecting nodes as vertices in a fully-connected mesh. Inspired by nature's resilient structures, it balances fast-path messaging with state finality using the golden ratio, ensuring continuous feedback loops and recursive scalability for a harmonious, distributed creativity engine.

## Architecture Decision

After analyzing the commonware libraries and the existing implementation, we will use:

1. **commonware-p2p**: For authenticated peer-to-peer communication with encryption
2. **commonware-cryptography**: For Ed25519-based node identities
3. **commonware-broadcast**: For efficient message dissemination across the network
4. **commonware-collector**: For request-response patterns between nodes

```
┌────────────────────────────────────────────────────────────────────────────┐
│                🌟 SACRED GEOMETRY NETWORKING PRINCIPLES 🌟                │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  ┌─────────────────────────┐    Golden Ratio    ┌───────────────────────┐  │
│  │ TETRAHEDRAL CONNECTIVITY│ ←── 61.8% / 38.2% ──│ FAST-PATH / FINALITY  │  │
│  │   (4-Vertex Mesh)       │                     │   Resource Allocation │  │
│  │ Coordinator ●──┼──● Executor          61.8%  │ Messaging (Fast-Path) │  │
│  │     │       \ / \      │                      │                       │  │
│  │     │        ×   ×     │                      │ 38.2% State Finality  │  │
│  │     │       / \ /      │                      │ (Consensus & Snapshot)│  │
│  │ Referee ●──┼──● Development                  └───────────────────────┘  │
│  └─────────────────────────┘                                               │
│  ┌─────────────────────────┐    Möbius Strip    ┌───────────────────────┐  │
│  │   FRACTAL TASK SCALING  │ ←── Feedback Loop ──│ SANDLOOP EXECUTION    │  │
│  │ (Self-Similar APIs)     │                     │ (Output Feeds Input)  │  │
│  │ Task(n) = φ * Task(n-1) │                     │ Iteration 1 → Input 2 │  │
│  │   where φ = 1.618       │                     │   Continuous Flow     │  │
│  └─────────────────────────┘                     └───────────────────────┘  │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

**Explanation of ASCII Art**:

- **Tetrahedral Connectivity**: Illustrated as a 4-vertex mesh (Coordinator, Executor, Referee, Development), ensuring each node type directly communicates with others, forming a minimal fully-connected topology.
- **Golden Ratio Allocation**: Depicted as a split of 61.8% for fast-path messaging (immediate communication) and 38.2% for state finality (consensus and snapshots), optimizing network performance.
- **Fractal Task Scaling**: Represented as recursive task structures, where each sub-task inherits self-similar API contracts, scaled by the golden ratio (φ ≈ 1.618).
- **Möbius Strip Feedback**: Visualized as sandloop execution where outputs seamlessly feed back as inputs, ensuring continuous refinement without visible breaks, akin to a single-sided loop.

## Core Design Principles

1. **Simplicity First**: Remove fractal patterns, golden ratio calculations, and Möbius loops
2. **Essential Features Only**: Focus on basic node discovery, messaging, and topology management
3. **Leverage Commonware**: Use existing commonware patterns without overengineering
4. **Clear Separation**: Keep networking concerns isolated from application logic

### NetworkManager (Legacy WebSocket P2P)

The `NetworkManager` implements a temporary WebSocket-based P2P network for compatibility during migration to a more advanced system. It manages peer connections and message broadcasting with the following components:

- **Node Identity**: A `node_id` string to uniquely identify the node within the network.
- **P2P Port**: A `p2p_port` for listening to incoming WebSocket connections.
- **Peers**: A thread-safe `HashMap` wrapped in `RwLock` to store `PeerInfo` (peer ID, node ID, last seen, endpoint, capabilities) for connected peers.
- **Event Channel**: A `broadcast::Sender` and `Receiver` for network events like peer connections, disconnections, and message receipts.

**Intention**: This legacy system acts as a bridge, maintaining tetrahedral connectivity during the transition to Commonware. It ensures nodes can communicate as vertices in a mesh, preserving the geometric foundation while evolving towards a more secure and efficient protocol.

### CommonwareNetworkManager (Tetrahedral Mesh Topology)

The `CommonwareNetworkManager` represents the modern networking layer using `commonware-cryptography` for authenticated discovery and P2P communication. It embodies sacred geometry with the following components:

- **NetworkConfig**: A configuration struct defining `node_role` (Coordinator, Executor, Referee, Development), `node_id`, listen and dialable addresses, bootstrap peers, max message size, namespace, data directory, and `GoldenRatioConfig` for resource allocation.
- **GoldenRatioConfig**: Configures fast-path messaging (61.8%) and state finality (38.2%) ratios, plus dial and gossip frequencies based on golden ratio timing (e.g., 618ms for dial frequency).
- **NodeIdentity and Signer**: Uses `NodeIdentity` for unique identification and a `PrivateKey` (Ed25519) for cryptographic signing, ensuring secure peer interactions.
- **Network Handle and Oracle**: Manages the Commonware runtime and peer registration for tetrahedral mesh participation.
- **Event and Message Channels**: Broadcasts `NetworkEvent` (peer connections, messages, topology formation) and manages `TetrahedralMessage` routing via internal channels.
- **Topology State and Metrics**: Tracks `NetworkTopology` (lists of node roles and connectivity matrix) and `GoldenRatioMetrics` for sandloop performance, ensuring geometric balance.

**Intention**: This implementation is the digital realization of a tetrahedral crystal, where each node role forms a vertex in a fully-connected mesh, secured by cryptography. The golden ratio allocation optimizes communication speed versus consensus, mirroring natural efficiencies, while fractal and Möbius principles ensure scalable tasks and continuous feedback.

### Message Types and Events

- **AgentMessage (Legacy)**: Includes `NodeAnnouncement`, `TaskAssignment`, `TaskStatusUpdate`, `StateSync`, `SandloopTrigger`, and `Heartbeat`, facilitating basic P2P coordination.
- **TetrahedralMessage (Commonware)**: Encompasses `NodeCapabilities`, `TaskCoordination`, `SandloopState` (with `GoldenRatioMetrics`), `FractalSync`, and `TetrahedralPing`, tailored for tetrahedral mesh interactions.
- **NetworkEvent (Both Systems)**: Covers peer connections/disconnections, message receipts, tetrahedral topology formation, and sandloop iterations, providing visibility into network state.

**Intention**: Messages are the lifeblood of tetrahedral connectivity, carrying tasks, states, and feedback across nodes. They reflect fractal self-similarity—each message type scales from individual to network-wide impact—and Möbius continuity by enabling iterative sandloop updates.

## Networking Implementation Details

### Legacy WebSocket P2P (NetworkManager)

- **Initialization (`new` Method)**: Creates a `NetworkManager` with a node ID, P2P port, and known peers, attempting connections to specified peer addresses.
- **Startup (`start` Method)**: Binds a WebSocket listener on the P2P port, spawns a task to handle incoming connections, and announces the node to the network with capabilities.
- **Connection Handling**: Manages incoming (`handle_connection`) and outgoing (`handle_outgoing_peer_messages`) WebSocket connections, tracking peers and broadcasting events for connections/disconnections.
- **Messaging**: Supports `broadcast` and `send_to_peer` for message propagation, currently logging actions as placeholders for full implementation.
- **Node Announcement**: Broadcasts a `NodeAnnouncement` with node type and capabilities, establishing tetrahedral presence.
- **Shutdown**: Gracefully disconnects peers, though full cleanup is a TODO item.

**Intention**: The legacy system prioritizes simplicity and compatibility, ensuring tetrahedral connectivity during migration. It acts as a scaffold, allowing nodes to announce their roles and capabilities, laying the groundwork for a geometrically precise network.

### Commonware Tetrahedral Mesh (CommonwareNetworkManager)

- **Initialization (`new` Method)**: Sets up a `CommonwareNetworkManager` with a `NetworkConfig`, loading or generating a `NodeIdentity` and Ed25519 `PrivateKey` for secure communication.
- **Startup (`start` Method)**: Initiates the Commonware runtime with golden ratio-customized dial and gossip frequencies, registers the node in peer set 0, sets up a tetrahedral message channel (ID 1), and spawns tasks for message handling and forwarding.
- **Capability Announcement (`announce_node_capabilities`)**: Broadcasts `NodeCapabilities` with role-specific capabilities (e.g., "workflow-distribution" for Coordinator), establishing tetrahedral presence.
- **Messaging**: Supports `broadcast_tetrahedral_message` for network-wide communication and `send_to_role` for role-specific targeting, using internal channels to route `TetrahedralMessage`.
- **Sandloop Execution (`start_sandloop`)**: Initiates Möbius feedback loops with `SandloopState` messages, tracking golden ratio metrics for performance analysis.
- **Fractal State Sync (`sync_fractal_state`)**: Broadcasts `FractalSync` messages for recursive state synchronization, limited to a depth of 10 for safety.
- **Tetrahedral Health Check (`check_tetrahedral_health`)**: Verifies connectivity across all node roles, emitting a `TetrahedralTopologyFormed` event when complete.
- **Topology Updates**: Dynamically updates `NetworkTopology` based on received messages, tracking node roles for connectivity matrix maintenance.
- **Shutdown**: Aborts the network runtime handle, ensuring graceful termination.

**Intention**: The Commonware system is a geometric masterpiece, implementing tetrahedral connectivity with cryptographic security. Golden ratio timing optimizes network interactions, while fractal sync and Möbius sandloops ensure state consistency and iterative refinement, aligning with ERGORS's vision of a recursive, nature-inspired tool-building system.

### Coordination Utilities

- **State Synchronization (`coordinate_state_sync`)**: Broadcasts `StateSync` messages in the legacy system to initiate fractal-like state updates across nodes.
- **Best Node Selection (`find_best_node_for_task`)**: Selects optimal nodes for tasks based on capabilities and golden ratio-weighted fitness (considering time since last seen), ensuring balanced tetrahedral workload distribution.

**Intention**: Coordination utilities embody fractal scalability—state sync operates at multiple depths, while node selection uses golden ratio weighting to mirror natural balance, reducing friction by matching tasks to the most suitable vertices.

## Integration with ERGORS System

- **Orchestrator Integration**: Both `NetworkManager` and `CommonwareNetworkManager` are integrated into the `Orchestrator` (in `node/mod.rs`), started during node runtime, and used for event handling (`handle_legacy_network_event`, `handle_commonware_event`) to manage tasks, sandloops, and peer connectivity.
- **CLI Access**: Networking is indirectly accessed via `start` and `deploy` commands in `main.rs`, enabling node and network initialization with P2P configurations.
- **State Persistence**: Network events and topology states are stored via `StateManager`, though full Cnidarium integration for deterministic snapshots is pending.

**Intention**: Networking integrates as the connective tissue of ERGORS's tetrahedral organism, linking nodes to the orchestration layer. It supports the recursive workspace-as-API philosophy by enabling programmable interactions across distributed environments.

## Limitations and Future Work

- **Legacy System Completion**: `NetworkManager` messaging and peer handling are placeholders (e.g., TODOs in `handle_peer_messages`), awaiting full WebSocket implementation or complete migration.
- **Commonware Bootstrap**: Bootstrap peer handling in `CommonwareNetworkManager` lacks conversion from legacy known peers, indicating incomplete setup for initial discovery.
- **State Backend**: Current reliance on Sled via `StateManager` for network state, with plans for Cnidarium to enhance deterministic fractal snapshots.
- **Transport Layer**: Ethernet transport initialization in `main.rs` and `transports.rs` contains `todo!()` placeholders, signaling incomplete network transport mechanisms.
- **Advanced Features**: Full load balancing in `send_to_role` and detailed golden ratio metrics calculation are future enhancements for optimizing tetrahedral coordination.

**Intention**: These limitations are natural steps in a fractal evolution, where each iteration refines the network towards geometric perfection. Migration ensures continuity while reaching for a future where tetrahedral connectivity and golden ratio efficiency are fully realized.

## API and Interaction Points

- **Legacy Networking**: `NetworkManager` offers `start`, `connect_to_peer`, `broadcast`, `send_to_peer`, `subscribe_events`, and `get_peers` for basic P2P interactions.
- **Commonware Networking**: `CommonwareNetworkManager` provides `start`, `broadcast_tetrahedral_message`, `send_to_role`, `start_sandloop`, `sync_fractal_state`, `check_tetrahedral_health`, and `subscribe_events` for tetrahedral mesh coordination.
- **Coordination Utilities**: `coordinate_state_sync` and `find_best_node_for_task` for state and task distribution logic.

**Intention**: These APIs are the digital pathways of a tetrahedral network, designed for accessibility and programmability. They reflect ERGORS's ethos of transparency and reusability, enabling nodes to interact as vertices in a sacred geometric structure.

## Conclusion

The networking implementation in ERGORS, spanning `packages/ho-core/src/network/`, represents a dual-layered system of legacy WebSocket P2P and modern Commonware tetrahedral mesh networking. By embedding sacred geometry principles—tetrahedral connectivity across four node roles, golden ratio allocation for messaging versus finality (61.8%/38.2%), Möbius strip sandloop feedback, and fractal task scaling—it ensures balanced, scalable, and secure communication across distributed nodes. The ASCII art above visualizes these principles, capturing the geometric harmony that drives ERGORS's vision of reducing creative friction through intelligent automation. This specification reflects the current state of the binary, providing a clear blueprint for understanding and extending the networking system as ERGORS evolves towards full Commonware integration and enhanced geometric optimizations.

**Final Intention**: Networking in ERGORS is the digital embodiment of a golden spiral, connecting nodes in a tetrahedral dance of communication and coordination. Each message, each connection, flows with the elegance of nature's patterns, building a resilient P2P fabric that empowers public goods creation through distributed agent orchestration.

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

## API Interface

## Error Handling

## Testing Strategy

1. **Unit Tests**: Test individual components in isolation
2. **Integration Tests**: Use commonware's simulated network for testing
3. **Local Cluster**: Run 4-node local cluster for tetrahedral testing (starship)
4. **CI Integration**: WIre in ci intergration using starship/interchaintests

## Security Considerations

1. All messages authenticated via Ed25519 signatures (built into commonware-p2p)
2. Encrypted connections between peers (provided by commonware-p2p)
3. Rate limiting per channel to prevent spam

### 1. Node Identity (commonware-cryptography)

### 2. Network Manager (commonware-p2p)

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
