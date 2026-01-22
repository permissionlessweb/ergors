# Testing

## E2E Deployment Workflow Test

Run the complete ERGORS deployment workflow test:

```bash
./scripts/e2e-test.sh
```

### What It Does

1. **Build ERGORS** - Compiles the ergors binary
2. **Start ERGORS Network** - Spawns coordinator + executor nodes
3. **Setup Akash** - Creates Kind cluster with Akash environment
4. **Build Image** - Builds mock inference provider Docker image
5. **Deploy via ERGORS** - Executes deployment workflow:
   - Executor requests grant from coordinator
   - Coordinator approves grant
   - Executor deploys to Akash
6. **Test Network** - Verifies ERGORS node connectivity
7. **Test Service** - Verifies deployed APIs (Ollama, OpenAI, TGI)
8. **Cleanup** - Stops nodes, deletes cluster

### Options

```bash
./scripts/e2e-test.sh --skip-build     # Use existing ergors binary
./scripts/e2e-test.sh --skip-network   # Use existing ERGORS network
./scripts/e2e-test.sh --skip-akash     # Use existing Kind cluster
./scripts/e2e-test.sh --skip-cleanup   # Keep everything running
./scripts/e2e-test.sh --verbose        # Show detailed output
```

## Spawn Test Network Only

Start an ERGORS network for manual testing:

```bash
./scripts/spawn-test-network.sh --keep-running
```

### Options

```bash
./scripts/spawn-test-network.sh --executors 3     # Number of executor nodes
./scripts/spawn-test-network.sh --with-referee    # Include referee node
./scripts/spawn-test-network.sh --base-port 50200 # Starting port
```

## Prerequisites

- Docker (running)
- kind
- kubectl
- cargo (Rust)

## Test Coverage

**ERGORS Network:**
- Coordinator node starts and accepts connections
- Executor nodes connect to coordinator
- Grant request/approval workflow
- Node health monitoring

**Akash Deployment:**
- Container deploys to Akash provider
- Service is exposed and accessible
- Pod reaches running state
- Deployment scaling works

**Inference APIs (on deployed service):**
- Ollama: `/api/tags`, `/api/generate`, `/api/chat`
- OpenAI: `/v1/models`, `/v1/chat/completions`
- TGI: `/info`, `/generate`
