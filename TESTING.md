# Testing

## E2E Testing - Quick Start

### Unified Interface

All E2E tests are now run through a single unified command:

```bash
# Run all test suites
just e2e

# Run specific suite
just e2e inference
just e2e network
just e2e deployment
just e2e api

# List available suites
just e2e list

# Show detailed help
just e2e help

# Run with options
just e2e inference --verbose
just e2e inference --skip-build
just e2e inference --skip-cleanup
```

### Available Test Suites

| Suite | Description | Auto-Skips |
|-------|-------------|------------|
| `all` | All test suites (default) | - |
| `network` | Network setup and connectivity | Akash, Ethereum, Inference |
| `grants` | Grant management and validation | Ethereum, Inference |
| `deployment` | Akash deployment workflows | Ethereum, Inference |
| `security` | Security and permissions | Akash, Ethereum, Inference |
| `contracts` | CosmWasm contract integration | Akash, Ethereum, Inference |
| `api` | gRPC/REST API endpoints | Akash, Ethereum, Inference |
| `bootstrap` | Node bootstrap and P2P transfers | Ethereum, Inference |
| `ethereum` | Ethereum integration | - |
| `inference` | LLM inference proxy routing | Akash, Ethereum |
| `sdl-storage` | SDL storage and retrieval | Akash, Ethereum, Inference |
| `chain-config` | Chain configuration | Akash, Ethereum, Inference |
| `sentinel` | Sentinel mode (standalone) | All |

### Common Options

```bash
--skip-build        # Skip building ergors binary
--skip-contracts    # Skip building CosmWasm contracts
--skip-network      # Skip ERGORS network setup
--skip-akash        # Skip Akash/Kind setup
--skip-cleanup      # Keep everything running after tests
--skip-ethereum     # Skip Ethereum/Anvil setup
--skip-inference    # Skip mock inference provider
--verbose           # Enable verbose output
```

### Examples

```bash
# Development iteration (fast)
just e2e inference --skip-build

# Debug mode (keep running)
just e2e inference --verbose --skip-cleanup

# Quick API validation
just e2e api

# Full integration test
just e2e all --verbose
```

---

## TODO

#### PHASE 1: simple deployment to provider via akash workflow automation

- confirm actual bidding and accepting bid + manifest digest + provider response workflow is occuring (try default workflow first, check provider configurations for local enviroment to ensure provider will actually bid on msgs (wybot))
- confirm ability to route msgs through the engine to the deployed provider

### INFERENCE PROVIDERS

#### Phase 1: Simple API KEY USAGE

- ensure test exist that use a mock inference provider (must create api keys to allow us to test authenticated access (our mock inference providers should have this support))
-

### COSMWASM

- test address resolution
- test permission authentication
- test event attribute action invocation
- test sdl construction/usage deployment

### BOOTSTRAPPING

#### PHASE 1: simple two device bootstrapping

- akash testing: spec/plan out how to actually implement e2e test for bootstrapping a node-engine from a node engine using akash. should we just use live testing situation (may be best to have reproducible script for this)
- ssh testing: specify an available endpoint to boostrap/ssh into, perform tests where we provide api key to endpoint to make calls to a mock inference provider endpoint we have for our testing scenario and shared the endpoint on bootstrapping

### AUTHENTICATION

- node identity: will have to scope out what live test can be done here. primary overlap in testing with bootstrapping, as secure channel needs to use node pk authentication
- key rotation:

### STORAGE

- test network state compression: fill up network state with many different various configurations and files. benchmark and

### RLM & DISCORD BOT

- sh script to automate the entire workflow, using multiple discord accounts to simpulate high frequency use and test edge-cases/ sanity cases for admin only access to funtions,multi-threading effectiveness
we are going to setup our wya of e2e testing the rlm loop for an engine. heres the workflow we will dedicated to
  ensuring our rlm works:
  1. spin up mock inference provider
  2. spin up ergors engine node
  3. configure mock inference api as both primary and secondary rlm-providers by registering it as provider in our
  engine
  4. ingest a github uurl via our document ingestion logic we have
  5. use the endpoint that invokes the rlm response, by proving a specific question being asked about the ingested
  document
  6. ensure that the rlm loop works as expected.

  that is the ultimate workflow goal of the e2e loops. we need to :
  a. add support in our mock-inference provider to deterministacally return responses that minimc the reasoning
  during an rlm loop. it should be able to return pre-generated python code that we can use to test our repl sandbox
  for parsing through relevant information in the ingested document, ensure that the response based on the rlm code
  sent retuyrns whats expected, and then return with a final response.

  this will fleex our rlm workflow loop in our e2e tests, identifying and friction points during the

### Benchmarking
