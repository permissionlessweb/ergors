
# Ergors

<!-- [![Lint Status](https://github.com/permissionlessweb/ergors/actions/workflows/lint.yaml/badge.svg)](https://github.com/permissionlessweb/ergors/actions/workflows/lint.yaml)
[![Test Status](https://github.com/permissionlessweb/ergors/workflows/tests.yaml/badge.svg)](https://github.com/permissionlessweb/ergors/actions/workflows/tests.yaml) -->

<!-- https://en.wikipedia.org/wiki/Ergodicity -->
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___

<div align="center">

+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑\
**REDUCE CREATIVE FRICTION THROUGH INTELLIGENT AUTOMATION FOR PUBLIC GOODS**.\
*GOAL: Amplify human creativity through removing obstacles between alignment of intention and manifestation.*\
+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑
</div>

___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
<div align="center">

A **sovereign**, **verifiable**, **private**, and **programmable** LLM orchestration engine.

Each node is a self-owned computational vertex with cryptographic identity, deterministic state, and embedded smart contract execution. No external dependencies for trust. No reliance on centralized infrastructure. Your team, your keys, your data.

*Engines create engines, strands become webs, tasks become distribution—accountably.*
</div>

___

## Core Principles

| Principle | What It Means |
|-----------|---------------|
| **Sovereign** | Self-owned cryptographic identity (Ed25519). Nodes operate independently without permission from external authorities. |
| **Verifiable** | Deterministic storage via Cnidarium/JMT with Merkle proofs. Every state transition is auditable. |
| **Private** | Transport encryption (X25519 + ChaCha20-Poly1305), password-encrypted key custody, ZK commitment framework. |
| **Programmable** | CosmWasm VM integration for smart contracts as isolated mini-chains. Contract-based authenticators. |

___

## Core Features

### Cryptographic Identity & Custody

Every node generates and manages its own Ed25519 keypair with password-encrypted storage (Argon2 + ChaCha20-Poly1305). Keys derive SSH credentials for git operations and encrypt API secrets. No plaintext keys at rest.

→ *[Custody & Auth Spec](./docs/specs/custody-and-auth.md)*

### CosmWasm Smart Contracts

Each node runs an embedded CosmWasm VM, enabling smart contracts as programmable state machines. Store, instantiate, execute, and query contracts via HTTP API. Contracts can be configured as server authentication middleware, store and configure SDL templates, or implement custom logic during runtime of the engine for iterative, programmable feature enhancements (or not!).

→ *[CosmWasm Spec](./docs/specs/cosmwasm.md)*

### P2P Network (Tetrahedral Mesh)

Nodes form a fully-connected mesh topology using Commonware P2P. using Ed25519-signed messages, Nodes discover peers, exchange capabilities, and coordinate without central servers. *There are lots of fun iteration to implement here ::)* 

→ *[Network Spec](./docs/specs/network.md)*

### Deterministic Storage

Cnidarium provides ACID-compliant, snapshot-based state management with Jellyfish Merkle Tree (JMT) verification. Prefix-based multistore for logical separation. Every write is atomic and auditable.

→ *[Storage Spec](./docs/specs/storage.md)*

### Verifiable RAG

Retrieval-Augmented Generation with cryptographic provenance. BLAKE3 content hashes, HNSW vector indexing, and optional JMT proofs. Query results include verification status and source attribution.

→ *[RAG Spec](./docs/specs/rag.md)*

### Multi-LLM Orchestration

Route requests across multiple LLM providers (OpenAI, Anthropic, Ollama, Akash, etc.) via macro-based provider system. Golden ratio resource allocation. Fractal task decomposition. Möbius sandloop feedback cycles for continuous refinement.

→ *[Orchestration Spec](./docs/specs/orch.md)*

### Transport Encryption

All node-to-node communication uses X25519 ephemeral key exchange with ChaCha20-Poly1305 AEAD. Three-message handshake with Ed25519 signatures. Forward secrecy for every session. (not sure if this is accurate as of right now, but thats the goal!)

→ *[Privacy Spec](./docs/specs/privacy.md)*

## Packages

| Package | Binary | Description |
|---------|--------|-------------|
| [`ergors`](packages/ergors/) | `ergors` | Node engine - network, storage, orchestration |
| [`ho-std`](packages/ho-std/) | — | Shared library - types, traits, custody |
| [`ergors-proto`](./proto/) | — | Proto definitions & code generation |

## Documentation

| Resource | Description |
|----------|-------------|
| [Specs](./docs/specs/) | Technical specifications |
| [Custody & Auth](./docs/specs/custody-and-auth.md) | Security, key management, encryption |
| [Network](./docs/specs/network.md) | P2P networking |
| [Storage](./docs/specs/storage.md) | Cnidarium state management |

## Quickstart

### Prerequisites

```sh
cargo install just  # Task runner
```

### Install

```sh
just install  # Builds and installs ergors to ~/.cargo/bin
```

### Initialize & Run

```sh
ergors init           # Create node identity, config, and data directories
ergors init llms      # Configure LLM provider API keys
ergors start          # Start the engine
```

## Development

We use [just](https://github.com/casey/just) as our task runner. Run `just help` to see all available commands.

### Common Workflows

```sh
# Development
just dev init         # Run engine commands in dev mode
just dev start        # Start engine with RUST_BACKTRACE=1
just cli <args>       # Run CLI in dev mode
just watch            # Rebuild on file changes (requires cargo-watch)

# Building
just build            # Debug build
just build-release    # Release build
just proto            # Regenerate proto types

# Quality
just check            # Quick syntax check (cargo chec)
just clippy           # Lint with clippy
just fmt              # Format code
just test             # Run all tests

# CI
just ci               # Full pipeline: fmt, clippy, test, build
just ci-quick         # Quick check without tests
```

### Installation Commands

| Command | Description |
|---------|-------------|
| `just install` | Build release + install `ergors` to PATH |
| `just install-engine` | Install only the engine |
| `just install-cli` | Install only the CLI |
| `just uninstall` | Remove installed binaries |
| `just which` | Show installed binary locations |

### Package-Specific Commands

```sh
just build-pkg ergors         # Build specific package (debug)
just build-pkg ergors release # Build specific package (release)
just test-pkg ho-std          # Test specific package
```

### Utilities

```sh
just env              # Show environment info
just version          # Show binary versions
just clean            # Remove build artifacts
just rebuild          # Clean + release build
just doc-open         # Build and open documentation
```

## Environment Variables

### RUST_LOG

> [for a dedicated list of environment variables and their defaults check here.](./packages/ho-std/src/constants.rs)

Controls the logging level for the entire application. This is the standard Rust tracing environment variable.

**Levels** (from least to most verbose):

* `error` - Only errors
* `warn` - Warnings and errors
* `info` - Informational messages, warnings, and errors (default)
* `debug` - Debug information plus all above
* `trace` - Trace-level debugging plus all above

**Examples**:

```bash
# Basic levels
export RUST_LOG=info          # Default - general operational logs
export RUST_LOG=debug          # Detailed debugging information
export RUST_LOG=trace          # Very verbose trace-level logging

# Module-specific levels
export RUST_LOG=ergors=debug,tower_http=info    # Debug for ergors, info for tower_http
export RUST_LOG=ergors::server=trace            # Trace only server module

# Target specific components
export RUST_LOG=ergors::middleware=debug        # Debug middleware operations
export RUST_LOG=ergors::storage=trace           # Trace storage operations
```

## Testing Library

Tests using orchestration servers are essentially scripts that can be used to verify integrity of logic, including its deployments, upgrades and migrations. we have a dedicated library and tooling specifically for this purpose.

### Mock Server

### Mock Inference Provider

* static responses from prompt requests
  * completions
  * prompts
  * toolcalling
  * api calls
  * mpc servers
  * embeddings

### Authentication Testing

* key gen siging libary
* custody middleware integration tests (reference penumbra testing library)
* integration test library

## DEPENDENCIES

We have ported into this workspace existing designs from the following code-bases:

* [penumbra](https://github.com/penumbra-zone/penumbra)
* [commonware](https://commonware.xyz/)
* [cosmwasm](https://github.com/cosmwasm/cosmwasm)

THANK YOU to the contributors of these, go show some support to their projects
