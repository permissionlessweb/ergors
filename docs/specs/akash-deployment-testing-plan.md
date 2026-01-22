# Akash Deployment Integration Testing Suite Plan

## Overview
Create a comprehensive testing suite that validates the engine's Akash deployment workflow execution, focusing on the 16-step deployment sequence with full authz/feegrant integration using Akash's exact developer environment. The suite will test real Akash infrastructure while using a mock inference provider for agentic request simulation.

## Core Testing Approach

### 1. Real Akash Infrastructure Testing
- **Environment**: Use Akash's exact Docker-based developer environment (`ghcr.io/akash-network/node` + `ghcr.io/akash-network/provider`)
- **Infrastructure**: Full Kind cluster + Akash chain containerization
- **Providers**: Real Akash providers in dev environment for authentic testing
- **Workflow Validation**: End-to-end testing of the 16-step deployment sequence

### 2. Authz Workflow Integration
- **Grant Requests**: Test requesting authz/feegrants from nodes in the ERGORS network
- **Multi-Node Authz**: Validate grant approval workflows across network participants
- **Feegrant Management**: Test fee allowance setup and consumption
- **Permission Validation**: Ensure proper authz execution permissions

### 3. Mock Inference Provider
Create a dedicated Docker image (`ergors/mock-inference-provider`) that:
- Simulates Ollama/vLLM/TGI API responses
- Handles agentic requests with tool calls and actions
- Provides realistic latency and error scenarios
- Enables testing without GPU requirements
- Supports multiple inference model types

## Testing Components

### Phase 1: Happy Path Validation
**Primary Test Scenarios:**
- Complete deployment workflow (KeySelection → Complete)
- Authz grant request and approval cycle
- Feegrant setup and validation
- Provider selection and lease creation
- Manifest deployment and endpoint verification
- Mock inference provider connectivity testing

**Test Structure:**
```rust
#[tokio::test]
async fn test_happy_path_deployment_with_authz() {
    // Start Akash dev environment
    let env = AkashDevEnvironment::start().await?;

    // Setup mock inference provider
    let mock_provider = MockInferenceProvider::start().await?;

    // Create workflow with authz request
    let workflow = create_workflow_with_grant_request(&env).await?;

    // Execute full deployment sequence
    let result = workflow.run_to_completion().await?;

    // Validate deployment and inference connectivity
    assert_deployment_successful(&result).await?;
    assert_inference_provider_responsive(&mock_provider).await?;
}
```

### Phase 2: Authz Network Integration
**Network Node Testing:**
- Grant requests to ERGORS network nodes
- Multi-node approval workflows
- Feegrant delegation across nodes
- Authz permission validation

### Phase 3: Advanced Scenarios
**Additional Test Coverage:**
- Deployment updates and scaling
- Multiple concurrent deployments
- Provider failover scenarios
- Network connectivity issues

## Implementation Structure

### 1. Test Script Approach
Create `packages/cw-ho/tests/akash-integration-test.rs` as the main test runner:
- Self-contained script for manual execution
- Environment setup and teardown automation
- Comprehensive logging and error reporting
- CI/CD ready for future integration

### 2. Environment Management
**Akash Dev Environment Setup:**
```bash
# packages/cw-ho/tests/setup-akash-dev.sh
#!/bin/bash
# - Start Kind cluster
# - Deploy Akash node and provider
# - Configure test wallets
# - Setup mock inference provider
# - Initialize test network topology
```

**Test Execution:**
```bash
# Run integration tests
cd packages/cw-ho
cargo test --test akash_integration -- --nocapture
```

### 3. Test Utilities
- `AkashDevEnvironment`: Docker environment lifecycle management
- `MockInferenceProvider`: Simulated inference workload container
- `TestWalletManager`: Pre-funded test accounts with known balances
- `NetworkTopology`: Multi-node ERGORS network simulation

### 4. Mock Inference Provider Implementation
**Docker Image Structure:**
```
ergors/mock-inference-provider/
├── Dockerfile
├── src/
│   ├── server.rs      # HTTP API server
│   ├── models.rs      # Inference model simulation
│   └── agentic.rs     # Tool call and action simulation
└── tests/
    └── integration.rs
```

**API Endpoints:**
- `/api/generate` (Ollama-compatible)
- `/v1/completions` (OpenAI-compatible)
- `/generate` (TGI-compatible)
- Custom agentic endpoints for tool calls

## Success Criteria
- ✅ Full 16-step deployment workflow executes successfully
- ✅ Authz grant requests work across ERGORS network nodes
- ✅ Feegrant allowances properly configured and consumed
- ✅ Provider selection and lease creation functions correctly
- ✅ Mock inference provider responds to agentic requests
- ✅ Comprehensive error logging and debugging information
- ✅ Test script runs reliably in Akash dev environment

## Key Technical Decisions
1. **Real Infrastructure**: Using actual Akash containers for authentic testing
2. **Mock Inference**: Custom Docker image to avoid GPU requirements
3. **Script-First**: Dedicated test script before CI/CD integration
4. **Happy Path Priority**: Focus on successful workflows before failure scenarios
5. **Network Integration**: Authz testing includes real ERGORS network interactions