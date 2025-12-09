# ERGORS Privacy Specification

This specification outlines the privacy mechanisms in ERGORS, focusing on cryptographic custody, encrypted transports, and an upgraded zero-knowledge (ZK) commitment scheme for prompts and orchestration actions. Drawing from Penumbra's action handling patterns, we propose a ZK framework to ensure trustless, verifiable privacy for agentic sessions. This leverages Ed25519 signatures, X25519 key exchange, and a custom ActionHandler-inspired structure for ZK commitments. The spec prioritizes code-based implementation over outdated docs, with data objects presented in MD tables for clarity.

## Overview

ERGORS ensures privacy through:

- **Key Custody**: Secure key management with soft/threshold KMS.
- **Transport Encryption**: ChaCha20-Poly1305 AEAD with X25519.
- **API Authentication**: Ed25519 signatures with nonce/timestamp.
- **ZK Commitments (Upgraded)**: Zero-knowledge proofs for prompts and actions, inspired by Penumbra's ActionHandler and balance commitments, enabling verifiable, private orchestration without revealing sensitive data.

The ZK scheme treats prompts/actions as "actions" with balance-like commitments, proving validity without disclosure. This aligns with ERGORS's trustlessness goals, using recursive proofs for state transitions.

we should implement PromptKey action, which is an action that performs the workflow for scheduling nad handing responses through the app-satete structure penumbra does withe the cosmos-sdk app. we can use the abci layer to granularize the actions to perform, specifically when new prompts join the mempool, the endpoint that will be delegated to retrieve the offline storage will have been chosen, and that the node can retrieve this. we can also refactor to use vote extensions for an offchain-aggregation layer.

these promptaction plans should be like a liquidity pool position in the dex, but with all core encrypted medata of properties associated with notes for privacy but verifiability.  We should use the exact component system used in the app sdk to make use of vote extensionhandlers for injection of offline data. 

## Core Data Objects

Data objects are defined in tables, based on code in packages/ho-std/src/custody/*, src/crypto/*, and proposed ZK extensions.

### Key Types

| Type | Description | Code Location | Fields |
|------|-------------|---------------|--------|
| `SpendKey` | Private key for spending/deriving addresses | ho-std/src/keys/keys/spend.rs | `inner: [u8; 32]` |
| `SeedPhrase` | BIP39-like mnemonic for key derivation | ho-std/src/keys/seed_phrase.rs | `words: Vec<String>` |
| `Address` | Derived public address | ho-std/src/keys/address.rs | `inner: [u8; 20]` |

### Custody Objects

| Type | Description | Code Location | Fields |
|------|-------------|---------------|--------|
| `SoftKms` | Local key management system | ho-std/src/custody/soft_kms/* | `config: SoftKmsConfig` |
| `Threshold` | Distributed key management (DKG) | ho-std/src/custody/threshold/* | `config: ThresholdConfig`, `dkg: Dkg` |
| `EncryptedKey` | Encrypted key storage | ho-std/src/custody/encrypted.rs | `ciphertext: Vec<u8>`, `nonce: [u8; 12]` |

### ZK Commitment Objects (Proposed)

| Type | Description | Inspired By (Penumbra) | Fields |
|------|-------------|------------------------|--------|
| `PromptAction` | ZK-wrapped prompt | Action enum | `prompt: String`, `commitment: ZkCommitment`, `proof: Groth16Proof` |
| `OrchAction` | ZK-wrapped orchestration action | ActionHandler | `action_type: ActionType`, `params: Vec<u8>`, `balance_commit: Commitment` |
| `ActionPlan` | Planning structure with openings | ActionPlan | `action: OrchAction`, `opening: Opening`, `memo_key: Option<PayloadKey>` |

## Current Privacy Mechanisms

### Key Custody

- **SoftKMS**: Local, policy-enforced key storage.
- **Threshold KMS**: Distributed key generation (DKG) and signing.
- Flow: Requests pre-authenticated, keys encrypted.

### Transport Encryption

- **Handshake**: 3-message X25519 with Ed25519 signatures.
- **Encryption**: ChaCha20-Poly1305 AEAD.
- Flow: Dialer/Listener exchange ephemerals, derive keys via HKDF.

### API Authentication

- **Signing**: Ed25519 over namespace || timestamp || nonce || payload_hash.
- **Verification**: Timestamp, nonce cache, public key registry.
- Flow: Middleware checks protected endpoints.

## Upgraded ZK Commitment Scheme

To implement ZK commitments for prompts and orchestration actions, we adapt Penumbra's ActionHandler pattern. Prompts/actions are treated as verifiable "actions" with balance commitments (adapted to "state commitments") proving validity without revealing contents. This enables trustless verification in agentic sessions.

### Requirements

- **Traits to Implement**:
  - `ActionHandler`: For stateless/stateful checks and execution.
  - `IsAction`: For commitment contributions (e.g., state balance sums to zero).
- **Protobuf Schema**: Add to transaction.proto for serialization.
- **Balance Commitment**: Prove net state change is zero via binding signature.

### Implementation Pattern

1. **Define Action Types** (e.g., in ho-std/src/action/mod.rs):
   - Enum: `PromptAction`, `OrchAction`.
   - Add to `Action` enum.

2. **Implement ActionHandler**:
   - `check_stateless`: Validate without state (e.g., proof verification).
   - `check_and_execute`: Stateful validation and execution.

3. **Implement IsAction**:
   - `balance_commitment`: Returns ZK commitment to state change.

4. **Create ActionPlan**:
   - Includes openings for private data.

5. **Register in Top-Level Handler**:
   - In app/action_handler/actions.rs.

6. **Protobuf Schema**:
   - Extend transaction.proto with new messages.

### Transaction Lifecycle for ZK Actions

- **Planning**: Create ActionPlan with private data.
- **Building**: Generate proofs, build Action.
- **Signing**: Binding signature over commitments.
- **Validation**: check_stateless, then check_and_execute.
- **Execution**: Apply state changes.

### Geometric Integration

- **Golden Ratio**: Allocate 61.8% to fast-path ZK proofs, 38.2% to verification.
- **Tetrahedral**: Distribute proofs across node types.
- **Möbius**: Feedback loops for iterative proof refinement.
- **Fractal**: Recursive commitments for nested actions.

### Data Objects in Tables (ZK-Specific)

#### ZkCommitment

| Field | Type | Description |
|-------|------|-------------|
| `inner` | [u8; 32] | Pedersen commitment |
| `blinding` | [u8; 32] | Random blinding factor |

#### Groth16Proof

| Field | Type | Description |
|-------|------|-------------|
| `a` | Point | Proof component A |
| `b` | Point | Proof component B |
| `c` | Point | Proof component C |

### Enforcement and Validation

- **Invariants**: Ensure commitments sum to zero; use Schnorr signatures for proofs.
- **Errors**: InvariantViolation if geometry fails.

## Testing and Validation

- Unit: Commitment round-trips, proof verification.
- Integration: Multi-node ZK action execution.
- Security: Replay resistance, forward secrecy.

This upgraded spec enhances ERGORS privacy with ZK, aligning with trustlessness goals.

## Multi-goal acomplishing action

we already have prepped a custody client server model for use when authorizaing actions to be compatible with various offline/external signing methods. We can make a custom dedicated API Key storage that interafaces with a dedicated layer of the jmt we use for storage (cnidarium) by storaging the encrypted keys to its storage and then when prompts come in we can have this server implement this. Since we are going to have. By defining the use of the prompt note we are generating, we can wire in the wallet and plan instructions for creating the objects and strucutre to pass. THis requires access to the custody defintioints , which we still need to interface in with a  client so we can power
