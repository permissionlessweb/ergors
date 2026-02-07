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
## Spawn Test Network Only
./scripts/spawn-test-network.sh --keep-running
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

## TODO

### DEPLOYMENT

- trusted/limited runtime: ensure functionliaty for our minimized trusted runtime so that we can first allow an admin to configure the sensitive information before actually starting the engine.

#### PHASE 1: simple deployment to provider via akash workflow automation

- confirm actual bidding and accepting bid + manifest digest + provider response workflow is occuring (try default workflow first, check provider configurations for local enviroment to ensure provider will actually bid on msgs (wybot))
- confirm ability to route msgs through the engine to the deployed provider

### INFERENCE PROVIDERS

#### Phase 1: Simple API KEY USAGE

- ensure test exist that use a mock inference provider (must create api keys to allow us to test authenticated access (our mock inference providers should have this support))
-

### COSMWASM

### BOOTSTRAPPING

#### PHASE 1: simple two device bootstrapping

- akash testing: spec/plan out how to actually implement e2e test for bootstrapping a node-engine from a node engine using akash. should we just use live testing situation (may be best to have reproducible script for this)
- ssh testing: specify an available endpoint to boostrap/ssh into, perform tests where we provide api key to endpoint to make calls to a mock inference provider endpoint we have for our testing scenario and shared the endpoint on bootstrapping

### AUTHENTICATION

- node identity: will have to scope out what live test can be done here. primary overlap in testing with bootstrapping, as secure channel needs to use node pk authentication
- key rotation:

### STORAGE

- test network state compression: fill up network state with many different various configurations and files. benchmark and

### Benchmarking
