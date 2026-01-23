# ERGORS Specification

This document serves as the central index for all ERGORS specification documents. Each spec details a distinct subsystem of the engine — from orchestration and networking to storage and security.

---

## Core Architecture

| Spec | Description |
|------|-------------|
| [Orchestration](./specs/orch.md) | CosmicOrchestrator component coordinating AI agents, managing distributed multi-LLM workflows across the P2P network. |
| [Storage](./specs/storage.md) | Cnidarium-based key-value store with multistore architecture, prefix-based indexing, and append-only audit trails. |
| [Network](./specs/network.md) | Tetrahedral mesh topology with Commonware P2P networking, Ed25519 node identity, and peer discovery. |
| [Configuration](./specs/config.md) | Layered configuration system separating public config from sensitive secrets using Protocol Buffer definitions. |

## Agents & Workflows

| Spec | Description |
|------|-------------|
| [Agents](./specs/agents.md) | Agentic workflow structure and management — recursive design, distributed orchestration, deterministic state. |
| [Workflows](./specs/workflows.md) | Multi-agent task orchestration with geometric-guided coordination across the node network. |
| [Scripting](./specs/scripting.md) | Meta-programming framework for generating and executing code within the orchestration layer. |

## API & Routing

| Spec | Description |
|------|-------------|
| [API Server](./specs/api-server.md) | Open Responses specification implementation for standardized multi-provider LLM interface with streaming and tool calling. |
| [Open Responses](./OPEN_RESPONSES.md) | Request/response format guide for the `/v1/responses` endpoint and Open Responses compatibility. |
| [API Authentication](./API.md) | Endpoint reference, authentication requirements, and request examples. |
| [Proxy Integration](./specs/proxy-integration.md) | Configuring CLI tools to route through the ERGORS proxy for prompt/response capture and retention. |
| [CLI & Engine Separation](./specs/cli-engine-separation.md) | Architecture separating the `ergors` daemon from `ergors-cli`, communicating via gRPC. |

## Security & Privacy

| Spec | Description |
|------|-------------|
| [Custody & Auth](./specs/custody-and-auth.md) | Identity custody with encrypted key storage, API authentication via Ed25519, and transport encryption. |
| [Key Management](./specs/key-management.md) | Defense-in-depth key management — node identity keys vs API keys, custody system protection. |
| [Privacy](./specs/privacy.md) | Cryptographic custody, encrypted transports, and zero-knowledge commitments for prompts and actions. |
| [Trustlessness](./specs/trustlessness.md) | Accountable action logging, state transition hashing, and recursive proofs of knowledge. |

## Smart Contracts & Interoperability

| Spec | Description |
|------|-------------|
| [CosmWasm](./specs/cosmwasm.md) | CosmWasm VM integration enabling nodes to instantiate and execute smart contracts as isolated mini-chains. |
| [Git Workspaces](./specs/git-workspaces.md) | Git-based workspace management for coordinating project files across the distributed node network. |

## Operations & Observability

| Spec | Description |
|------|-------------|
| [Logs](./specs/logs.md) | Structured logging via Rust's `tracing` ecosystem with configurable verbosity and error trace control. |

## Deployment

| Spec | Description |
|------|-------------|
| [Akash Deployment](./specs/bootstrap/akash-deployment.md) | Deployment configuration and SDL templates for running ERGORS nodes on Akash Network. |
| [Akash Testing](./specs/bootstrap/akash-deployment-testing.md) | Testing procedures for validating Akash deployment configurations. |
| [Grant Request System](./specs/bootstrap/grant-request-system.md) | Grant request system for provisioning node resources. |

---

## Quick Reference

```
docs/
├── API.md                  # Endpoint reference & auth guide
├── OPEN_RESPONSES.md       # Open Responses format & routing
├── QUICKSTART.md           # Getting started
├── spec.md                 # This file (index)
└── specs/
    ├── agents.md           # Agentic workflows
    ├── api-server.md       # Open Responses API spec
    ├── bootstrap/
    │   ├── akash-deployment.md
    │   ├── akash-deployment-testing.md
    │   └── grant-request-system.md
    ├── cli-engine-separation.md
    ├── config.md           # Configuration system
    ├── cosmwasm.md         # Smart contract VM
    ├── custody-and-auth.md # Security & identity
    ├── git-workspaces.md   # Workspace management
    ├── key-management.md   # Key custody
    ├── logs.md             # Observability
    ├── network.md          # P2P networking
    ├── orch.md             # Orchestration engine
    ├── privacy.md          # Privacy primitives
    ├── proxy-integration.md# LLM proxy routing
    ├── scripting.md        # Scripting framework
    ├── storage.md          # Cnidarium storage
    ├── trustlessness.md    # Verifiable execution
    └── workflows.md        # Task workflows
```
