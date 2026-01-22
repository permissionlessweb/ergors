# Akash Deployment Integration Testing Suite

## Overview

This document describes the comprehensive testing suite for validating the ERGORS Akash deployment workflow. The suite tests the complete 16-step deployment sequence with authz/feegrant integration using Akash's official Kind-based developer environment and a custom mock inference provider.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Integration Test Suite                        │
│  packages/cw-ho/tests/src/akash_integration.rs                  │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  AkashDev     │    │  MockInference│    │  Network      │
│  Environment  │    │  Provider     │    │  Topology     │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ Kind Cluster  │    │ Docker Image  │    │ Grant Request │
│ + Akash Node  │    │ Ollama/OpenAI │    │ Simulation    │
│ + Provider    │    │ /TGI APIs     │    │               │
└───────────────┘    └───────────────┘    └───────────────┘
```

## Components

### 1. Testing Module (`packages/cw-ho/src/deploy/testing/`)

| Module | Description |
|--------|-------------|
| `environment.rs` | `AkashDevEnvironment` - Kind cluster and Akash node/provider lifecycle |
| `mock_inference.rs` | `MockInferenceProvider` - In-process inference API simulation |
| `wallet.rs` | `TestWalletManager` - Pre-funded test accounts and grant management |
| `network.rs` | `NetworkTopology` - Multi-node ERGORS network simulation |

### 2. Mock Inference Provider (`docker/mock-inference-provider/`)

Standalone Docker image simulating inference providers without GPU requirements.

| File | Description |
|------|-------------|
| `src/main.rs` | Full inference server implementation |
| `Dockerfile` | Multi-stage build for slim container |
| `docker-compose.yml` | Local testing with multiple instances |
| `deploy.sdl.yaml` | Akash Network deployment template |

### 3. Integration Tests (`packages/cw-ho/tests/src/akash_integration.rs`)

Comprehensive test suite covering:

- Happy path deployment workflows
- Authz grant request/approval cycles
- Feegrant management
- Mock inference API compatibility
- Network topology and partitioning

## Prerequisites

### Required Tools

```bash
# macOS
brew install docker kind kubectl jq

# Verify installations
docker --version
kind --version
kubectl version --client
```

### System Requirements

- Docker Desktop running with 4GB+ memory allocated
- 10GB+ free disk space for container images
- Ports available: 80, 443, 1317, 8443, 11434, 26657

## Quick Start

### Step 1: Setup Development Environment

```bash
# Run the automated setup script
./packages/cw-ho/tests/scripts/setup-akash-dev.sh

# Or with options
./packages/cw-ho/tests/scripts/setup-akash-dev.sh --cluster-name my-test --cleanup
```

This script:

1. Creates a Kind cluster with Akash-compatible configuration
2. Installs nginx ingress controller
3. Deploys Akash node (`ghcr.io/akash-network/node`)
4. Deploys Akash provider (`ghcr.io/akash-network/provider`)
5. Creates test accounts: `validator`, `faucet`, `deployer`, `granter`, `grantee`

### Step 2: Build Mock Inference Provider

```bash
cd docker/mock-inference-provider

# Local build
cargo build --release

# Docker build
docker build -t ergors/mock-inference-provider .

```

### Step 3: Run Integration Tests

```bash
# Run all tests (requires environment setup)
cargo test -p ergors --features testing -- --nocapture

# Run specific test categories
cargo test -p ergors --features testing test_mock_inference -- --nocapture
cargo test -p ergors --features testing test_wallet_manager -- --nocapture
cargo test -p ergors --features testing test_network -- --nocapture

# Run full integration tests (requires Kind cluster)
cargo test -p ergors --features testing test_happy_path -- --nocapture --ignored
```

## Test Categories

### Phase 1: Unit Tests (No External Dependencies)

These tests run without Docker/Kind and validate component logic:

```bash
cargo test -p ergors --features testing
```

| Test | Description |
|------|-------------|
| `test_mock_inference_ollama_api` | Validates Ollama API compatibility |
| `test_mock_inference_openai_api` | Validates OpenAI API compatibility |
| `test_mock_inference_tgi_api` | Validates TGI API compatibility |
| `test_wallet_manager_creation` | Tests wallet creation and funding |
| `test_wallet_manager_authz` | Tests authz grant management |
| `test_wallet_manager_feegrant` | Tests feegrant allowance management |
| `test_network_topology_init` | Tests network node initialization |
| `test_network_grant_acceptance_modes` | Tests grant approval workflows |
| `test_network_node_status` | Tests node online/offline states |
| `test_network_partitioning` | Tests network partition simulation |

### Phase 2: Integration Tests (Requires Kind Cluster)

These tests require the Akash development environment:

```bash
# Setup environment first
./packages/cw-ho/tests/scripts/setup-akash-dev.sh

# Run integration tests
cargo test -p ergors --features testing -- --ignored --nocapture
```

| Test | Description |
|------|-------------|
| `test_happy_path_deployment` | Full 16-step workflow execution |
| `test_deployment_with_grant_request` | Workflow with authz/feegrant requests |
| `test_authz_workflow_integration` | Complete grant request cycle |
| `test_mock_inference_deployment_simulation` | Deployed inference endpoint testing |

## Mock Inference Provider API Reference

### Ollama API (Default Port: 11434)

```bash
# List models
curl http://localhost:11434/api/tags

# Generate text
curl http://localhost:11434/api/generate \
  -d '{"model":"llama2","prompt":"Hello world","stream":false}'

# Chat
curl http://localhost:11434/api/chat \
  -d '{"model":"llama2","messages":[{"role":"user","content":"Hi"}]}'

# Embeddings
curl http://localhost:11434/api/embeddings \
  -d '{"model":"llama2","prompt":"Hello world"}'
```

### OpenAI API

```bash
# List models
curl http://localhost:11434/v1/models

# Completions
curl http://localhost:11434/v1/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama2","prompt":"Hello","max_tokens":100}'

# Chat completions
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama2","messages":[{"role":"user","content":"Hello"}]}'
```

### TGI API

```bash
# Server info
curl http://localhost:11434/info

# Generate
curl http://localhost:11434/generate \
  -d '{"inputs":"What is ML?","parameters":{"max_new_tokens":100}}'
```

### Agentic Endpoints

```bash
# Execute with tool calls
curl http://localhost:11434/api/agentic/execute \
  -d '{
    "model":"llama2",
    "prompt":"Search for Akash Network info",
    "tools":[{"name":"web_search","description":"Search the web","parameters":{}}]
  }'

# View recorded tool calls
curl http://localhost:11434/api/agentic/tool-calls
```

## Configuration Options

### Mock Inference Provider

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `PORT` | 11434 | HTTP server port |
| `HOST` | 0.0.0.0 | Bind address |
| `MIN_LATENCY_MS` | 50 | Minimum response latency |
| `MAX_LATENCY_MS` | 200 | Maximum response latency |
| `ERROR_RATE` | 0.0 | Simulated error rate (0.0-1.0) |
| `MODEL_NAME` | llama2 | Default model name |
| `RUST_LOG` | info | Log level |

### Setup Script Options

```bash
./setup-akash-dev.sh [options]

Options:
  --cluster-name NAME    Kind cluster name (default: akash-dev)
  --with-gpu             Enable GPU support
  --skip-provider        Skip provider deployment
  --cleanup              Delete existing environment first
  --help                 Show help message
```

## Test Scenarios

### Scenario 1: Happy Path Deployment

Tests the complete 16-step workflow:

```
[1/16] KeySelection        → Select deployment key
[2/16] BalanceCheck        → Verify AKT balance
[3/16] GrantRequest        → (Skip if funded)
[4/16] GrantWait           → (Skip if funded)
[5/16] AuthzSetup          → Verify permissions
[6/16] FeegrantSetup       → Check fee allowances
[7/16] SdlConfiguration    → Configure SDL template
[8/16] CertificateSetup    → Setup Akash certificate
[9/16] DeploymentCreate    → Submit deployment TX
[10/16] BidWait            → Wait for provider bids
[11/16] ProviderSelection  → Select best provider
[12/16] LeaseCreate        → Create lease
[13/16] ManifestSend       → Send manifest to provider
[14/16] EndpointRetrieval  → Get service endpoints
[15/16] EndpointTesting    → Verify connectivity
[16/16] Complete           → Deployment ready
```

### Scenario 2: Grant Request Workflow

Tests authz/feegrant request from unfunded account:

1. Grantee submits request to granter node
2. Granter evaluates based on acceptance mode:
   - `AcceptAll`: Auto-approve
   - `RejectAll`: Auto-reject
   - `Whitelist`: Check requester pubkey
   - `Manual`: Queue for approval
3. On approval, granter broadcasts MsgGrant/MsgGrantAllowance
4. Grantee proceeds with deployment using granted permissions

### Scenario 3: Network Partitioning

Tests resilience to network issues:

1. Initialize network with multiple nodes
2. Create partition separating nodes
3. Verify nodes in same partition can communicate
4. Verify cross-partition communication fails
5. Heal partition and verify recovery

## Troubleshooting

### Docker Not Running

```bash
# macOS
open -a Docker

# Linux
sudo systemctl start docker
```

### Kind Cluster Issues

```bash
# Delete and recreate
kind delete cluster --name akash-dev
./setup-akash-dev.sh --cleanup
```

### Pod Not Starting

```bash
# Check pod status
kubectl get pods -n akash-services

# View logs
kubectl logs -n akash-services -l app=akash-node
kubectl logs -n akash-services -l app=akash-provider
```

### Port Conflicts

```bash
# Find process using port
lsof -i :11434

# Kill process
kill -9 <PID>
```

## Cleanup

```bash
# Stop mock inference provider
docker stop $(docker ps -q --filter ancestor=ergors/mock-inference-provider)

# Delete Kind cluster
kind delete cluster --name akash-dev

# Remove Docker images
docker rmi ergors/mock-inference-provider
```

## File Locations

```
CW-AGENT/
├── docker/
│   └── mock-inference-provider/
│       ├── Cargo.toml
│       ├── src/main.rs
│       ├── Dockerfile
│       ├── docker-compose.yml
│       ├── deploy.sdl.yaml
│       └── README.md
├── packages/cw-ho/
│   ├── src/deploy/testing/
│   │   ├── mod.rs
│   │   ├── environment.rs
│   │   ├── mock_inference.rs
│   │   ├── wallet.rs
│   │   └── network.rs
│   └── tests/
│       ├── src/akash_integration.rs
│       └── scripts/setup-akash-dev.sh
└── docs/specs/
    └── akash-deployment-testing-plan.md
```

## Success Criteria

- [x] Mock inference provider supports Ollama, OpenAI, TGI APIs
- [x] Testing module provides environment, wallet, network simulation
- [x] Integration tests cover happy path and error scenarios
- [x] Setup script automates Kind cluster and Akash deployment
- [x] Docker image can be deployed to Akash Network
- [ ] CI/CD pipeline integration (future)
- [ ] Performance benchmarking (future)
