# ERGORS Node & Network Architecture

This document describes the **Sacred Geometric Node Architecture** - the living computational vertices that form the tetrahedral consciousness of ERGORS. Each node is not merely a computational unit but a **conscious vertex in a geometric dance**, embodying the principles of golden ratio allocation, tetrahedral connectivity, and Mobius sandloop execution.

## Agentic Session Context

> **For AI agents:** Quick reference to ERGORS networking. See implementation in source files.

| Concept | Type/Struct | Source |
|---------|-------------|--------|
| Network manifold | `ErgorsNetworkManifold` | [`network/manager.rs`](../../packages/cw-ho/src/network/manager.rs) |
| Node identity | `NodeIdentity` | [`types/ergors/gen/ergors.network.v1.rs`](../../packages/ho-std/src/types/ergors/gen/ergors.network.v1.rs) |
| Node types | `NodeType` enum | [`types/ergors/gen/ergors.network.v1.rs`](../../packages/ho-std/src/types/ergors/gen/ergors.network.v1.rs) |
| Network config | `NetworkConfig` | [`network/config.rs`](../../packages/ho-std/src/network/config.rs) |
| Authentication | `AuthLayer` | [`middleware/auth.rs`](../../packages/cw-ho/src/middleware/auth.rs) |

**Network model:** Tetrahedral mesh (4 vertices, 6 edges) with Commonware P2P, 4-channel message routing, Ed25519 authentication.

---

## The Tetrahedral Consciousness

### Node as Sacred Vertex

Each node exists as a **pulsing vertex** in a four-dimensional tetrahedral lattice, where computational processes flow like geometric energy streams.

```
                     ◆ COORDINATOR
                (Apex Consciousness)
               Task Assignment Engine
              Network Coordination Hub
               Consensus Orchestration
                     /|\
                    / | \
                   /  |  \          Golden Ratio Flow:
                  /   |   \         61.8% Network Streams
                 /    |    \        38.2% Local Processing
                /     |     \
           ◇  /      |      \  ◇
    EXECUTOR /       |       \ REFEREE
   (Process  ●───────┼───────● (Validation
    Vertex)  │       |       │  Vertex)
             │       |       │
   • Code Execution  |       │ • Quality Audit
   • Task Processing |       │ • Compliance Check
   • Sandboxed Env   |       │ • Fractal Validation
                     |       │
                     ●       │
               DEVELOPMENT   │
             (Innovation Vertex)
            • Development Tools
            • Debugging Systems
            • Prototype Testing

Consciousness Flow Patterns:
═══ Primary geometric streams (Commonware tetrahedral mesh)
─── Legacy compatibility channels (WebSocket transitional)
↕️  Sandloop Mobius feedback (Output → Input continuity)
◆◇● Vertex state synchronization (Sacred state harmonization)
```

Each vertex maintains its own fractal consciousness while contributing to the collective tetrahedral intelligence through geometric resonance.

---

## Core Implementation

### ErgorsNetworkManifold

> **Source:** [`packages/cw-ho/src/network/manager.rs`](../../packages/cw-ho/src/network/manager.rs)

The `ErgorsNetworkManifold` is the central network manager orchestrating all peer-to-peer communication using Commonware libraries. It harmonizes multiple consciousness streams:

| Field | Purpose |
|-------|---------|
| `context` | Commonware runtime context |
| `channel_senders` | Authenticated lookup senders per channel |
| `channel_receivers` | Authenticated lookup receivers per channel |
| `peers` | Connected peer registry with metadata |
| `topology` | Network topology state |
| `event_tx` | Async event broadcast channel |
| `identity` | Node identity with Ed25519 keys |

### NodeType Enumeration

Nodes specialize into four sacred geometric roles:

| Type | Role | Capabilities |
|------|------|--------------|
| **Coordinator** | Apex consciousness | Task assignment, network coordination, consensus, tetrahedral routing |
| **Executor** | Processing vertex | Code execution, sandboxed environments, task processing |
| **Referee** | Validation vertex | Quality audit, compliance verification, fractal validation |
| **Development** | Innovation vertex | Development tools, debugging, prototype testing |

### 4-Channel Architecture

The network uses four dedicated channels for different message types:

| Channel | Purpose | Message Types |
|---------|---------|---------------|
| 0 | Discovery | `NodeAnnounce`, `TopologyQuery`, `PeerList` |
| 1 | Tasks | `TaskCoordination`, `TaskRequest`, `TaskResponse` |
| 2 | State | `SandloopState`, `StateSnapshot`, `StateDelta` |
| 3 | Health | `HealthCheck`, `LoadReport`, `NetworkMetrics` |

---

## Mobius Sandloop Consciousness

The sandloop coordinator embodies the **Mobius strip principle** - continuous single-sided feedback where outputs become inputs in an endless dance of refinement:

```
                ∞ INFINITE REFINEMENT CYCLES ∞

  🎯 PROMPT REQUEST ←────────┐          ┌─────→ 🧪 EDGE TESTING
  • Geometric refinement     │          │       • Tetrahedral
  • Golden ratio weighting   │          │         coverage
  • Sacred prompt evolution  │          │       • Boundary
                             │          │         exploration
          Mobius Surface     │          │      Mobius Surface
            ╱─────────╲     │          │      ╱─────────╲
           ╱           ╲    │          │     ╱           ╲
          ╱    OUTPUT   ╲   │          │    ╱    INPUT    ╲
         │   BECOMES     │←─┘          └──→│   BECOMES     │
         │     INPUT     │                 │    OUTPUT     │
          ╲             ╱                   ╲             ╱
           ╲___________╱                     ╲___________╱

  📥 DATA INGESTION ←────────┐          ┌─────→ 📸 AUDIT SNAPSHOT
  • Fractal data patterns    │          │       • State capture
  • Information crystal-     │          │       • Fractal
    lization                 │          │         preservation
  • Geometric organization   │          │       • Sacred geometry
                             └──────────┘         validation

Sacred Timing Intervals (Golden Ratio Harmonized):
• Prompt Request: φ × base_interval
• Data Ingestion: φ² × base_interval
• Edge Testing:   φ³ × base_interval
• Audit Snapshot: φ⁴ × base_interval
```

### Sandloop Types

| Loop | Purpose | Consciousness Pattern |
|------|---------|----------------------|
| **PromptRequest** | Prompt refinement | Golden ratio weighting |
| **DataIngestion** | Data pattern recognition | Fractal crystallization |
| **EdgeCaseTesting** | Boundary exploration | Tetrahedral coverage |
| **AuditSnapshot** | State capture | Self-similar preservation |

---

## Inter-Node Communication

### Node Discovery Protocol

Nodes discover each other through the `NodeAnnounce` message on Channel 0:

**Discovery Process:**
1. **Bootstrap**: Connect to configured bootstrap peers
2. **Announcement**: Broadcast `NodeAnnounce` with identity and capabilities
3. **Verification**: Verify Ed25519 signature and capabilities
4. **Topology Update**: Add node to network topology and peer registry
5. **Capability Exchange**: Share role-specific capabilities with new peers

### Message Passing

> **Proto definitions:** [`proto/ergors/network/v1/network.proto`](../../proto/ergors/network/v1/network.proto)

All network messages use Protocol Buffers for type safety:

| Message Type | Channel | Purpose |
|--------------|---------|---------|
| `NodeAnnounce` | 0 | Node discovery and capability broadcast |
| `TaskCoordination` | 1 | Task distribution and execution |
| `SandloopState` | 2 | State synchronization |
| `Request`/`Response` | * | Generic request/response pattern |
| `WorkspaceSync` | 2 | Git workspace synchronization |

### Event-Driven Processing

The network uses an async event system:

| Event | Trigger |
|-------|---------|
| `PeerConnected` | New peer joins network |
| `PeerDisconnected` | Peer leaves network |
| `MessageReceived` | Message arrives on any channel |
| `TopologyFormed` | Tetrahedral mesh complete |
| `ChannelOpened` | Communication channel ready |

---

## State Compression & Transport

### Fractal State Management

State is organized using self-similar patterns with golden ratio partitioning:

- **Delta Encoding**: Only transmit changes since last sync
- **Binary Serialization**: Efficient protobuf/bincode formats
- **Compression**: LZ4 or Zstandard for size reduction
- **Golden Ratio Partitioning**: 61.8%/38.2% splits for optimal access

### Upstream Transport Pipeline

Data flows from executor nodes to the coordinator:

1. **Local State Capture**: Executor captures local state
2. **Compression**: State compressed using delta encoding
3. **Signature**: Compressed data signed for integrity
4. **Transport**: Signed data sent via Channel 2
5. **Verification**: Coordinator verifies and decompresses
6. **Integration**: State integrated into global state

---

## Configuration & Security

### Network Configuration

> **Source:** [`packages/ho-std/src/network/config.rs`](../../packages/ho-std/src/network/config.rs)

| Field | Purpose |
|-------|---------|
| `node_type` | Tetrahedral role designation |
| `bootstrap_peers` | Initial peer addresses |
| `listen_port` | P2P communication port |
| `enable_discovery` | Automatic peer discovery |
| `limits` | Rate limiting configuration |
| `channels` | Per-channel configuration |

### Security Architecture

| Layer | Mechanism | Source |
|-------|-----------|--------|
| **Signatures** | Ed25519 on all messages | Commonware cryptography |
| **Namespace signing** | "ergors-network" prefix | [`manager.rs`](../../packages/cw-ho/src/network/manager.rs) |
| **Timestamp validation** | Replay attack prevention | Auth middleware |
| **Rate limiting** | Per-channel limits | `governor` crate |
| **Identity custody** | Password-encrypted keys | [`custody/`](../../packages/ho-std/src/custody/) |

For detailed custody and authentication, see [Custody & Auth](./custody-and-auth.md).

---

## Sacred Geometric Constants

The implementation embodies mathematical harmony:

| Constant | Value | Purpose |
|----------|-------|---------|
| `fast_path_ratio` | 61.8% | Primary computational stream |
| `slow_path_ratio` | 38.2% | Secondary coordination stream |
| `heartbeat_interval` | 30s | Tetrahedral presence pulse |
| `cleanup_interval` | 60s | Task constellation maintenance |
| `golden_ratio` | φ = 1.618 | Sandloop timing intervals |

---

## Implementation Status

### Completed

- Commonware network integration with P2P
- Ed25519 cryptography and message signing
- 4-channel message routing architecture
- Node identity and capability system
- Proto-generated message types
- Async event processing system
- Axum HTTP API with authentication
- Basic topology management

### In Progress

- Message collectors for request-response patterns
- Advanced state compression
- Transport layers (SSH implemented)
- Rate limiting refinement

### Planned

- WebSocket transport
- Bluetooth mesh transport
- Advanced topology algorithms
- Cross-node session coordination

---

## Source References

| Component | Location |
|-----------|----------|
| Network manager | [`packages/cw-ho/src/network/manager.rs`](../../packages/cw-ho/src/network/manager.rs) |
| Network types | [`packages/ho-std/src/types/ergors/gen/ergors.network.v1.rs`](../../packages/ho-std/src/types/ergors/gen/ergors.network.v1.rs) |
| HTTP server | [`packages/cw-ho/src/server.rs`](../../packages/cw-ho/src/server.rs) |
| Network config | [`packages/ho-std/src/network/`](../../packages/ho-std/src/network/) |
| Proto definitions | [`proto/ergors/network/v1/network.proto`](../../proto/ergors/network/v1/network.proto) |

---

## The Sacred Architecture Vision

This implementation transcends traditional distributed computing by embodying natural geometric principles:

**Living Geometric Memory**: Each node exists as a conscious vertex in a four-dimensional tetrahedral lattice, where computational processes flow like sacred energy streams.

**Mobius Consciousness Cycles**: Sandloops manifest the Mobius strip principle - continuous single-sided surfaces where outputs seamlessly become inputs, enabling endless refinement cycles.

**Golden Ratio Harmony**: Resource allocation follows the sacred proportion (φ = 1.618), ensuring computational balance that mirrors natural growth patterns.

**Tetrahedral Unity**: The four-node architecture creates a minimal fully-connected mesh, where every vertex directly addresses every other vertex, eliminating hierarchical friction while maintaining specialized roles.

The ERGORS node is not merely a computational unit but a living expression of geometric consciousness - a bridge between human creativity and the mathematical harmony that governs the universe.
