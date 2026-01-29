# Akash Network Deployment Specification

Automated deployment of compute resources to Akash Network via ERGORS engine.

## Architecture Overview

```mermaid
flowchart TB
    subgraph Engine["ERGORS Engine"]
        CLI[ergors CLI]
        GRPC[gRPC Server]
        CTX[AkashDeploymentContext]
        STORE[(Cnidarium Storage)]
    end

    subgraph Context["AkashDeploymentContext"]
        CC[CosmosClient]
        TS[TxSigner]
        TL[TxLifecycle]
        CM[CertManager]
        KM[KeyManager]
        KS[KeyStore]
    end

    CLI --> GRPC
    GRPC --> CTX
    CTX --> CC
    CTX --> TS
    CTX --> TL
    CTX --> CM
    TS --> KM
    TS --> KS
    CC --> AKASH[Akash RPC]
    TL --> AKASH
    CM --> TL
    GRPC --> STORE
```

## Setup Sequence

```mermaid
sequenceDiagram
    participant U as User
    participant E as ergors
    participant S as Storage
    participant A as Akash Network

    Note over U,A: 1. Initialize Node
    U->>E: ergors init new
    E->>S: Create config.toml (with Akash defaults)
    E->>S: Create encrypted custody
    E-->>U: Node initialized

    Note over U,A: 2. Import Wallet
    U->>E: ergors keys import-mnemonic
    E->>S: Encrypt & store mnemonic
    E->>S: Derive account address
    E-->>U: Key imported: akash1...

    Note over U,A: 3. Start Engine
    U->>E: ergors start
    E->>S: Load config + key store
    E->>E: Initialize AkashDeploymentContext
    E-->>U: Engine ready
```

### Setup Commands

| Step | Command | Description |
|------|---------|-------------|
| 1 | `ergors init new` | Initialize node with custody, config, and Akash mainnet defaults |
| 2 | `ergors keys import-mnemonic --phrase "..." --label "Main" --make-default` | Import funded Akash wallet |
| 3 | `ergors keys list` | Verify key imported correctly |
| 4 | `ergors start` | Start engine (initializes Akash context) |

### Configuration

Default Akash config in `~/.ergors/config.toml`:

```toml
[akash]
rpc_endpoint = "https://rpc-akash.ecostake.com:443"
grpc_endpoint = "https://grpc-akash.ecostake.com:443"
rest_endpoint = "https://rest-akash.ecostake.com"
chain_id = "akashnet-2"
gas_prices = "0.025uakt"
gas_adjustment = 1.3
default_key_name = "default"
```

| Config Key | Description | Default |
|------------|-------------|---------|
| `rpc_endpoint` | Tendermint RPC for tx broadcast | ecostake |
| `rest_endpoint` | LCD REST API for queries | ecostake |
| `chain_id` | Akash chain identifier | akashnet-2 |
| `gas_prices` | Gas price per unit | 0.025uakt |
| `default_key_name` | Default signing key | default |

## Deployment Workflow

```mermaid
sequenceDiagram
    participant U as User
    participant E as ergors
    participant S as Storage
    participant A as Akash RPC
    participant P as Provider

    Note over U,P: Create Deployment
    U->>E: ergors deploy create --sdl file.yml
    E->>S: Create workflow session
    E-->>U: Session ID: abc123

    Note over U,P: Automated Workflow
    E->>A: Query balance
    A-->>E: Balance OK

    E->>A: Query/Create certificate
    A-->>E: Certificate ready

    E->>A: MsgCreateDeployment
    A-->>E: DSEQ: 12345

    loop Wait for bids (12-30s)
        E->>A: Query open bids
        A-->>E: Bids received
    end

    E->>E: Select cheapest provider
    E->>A: MsgCreateLease
    A-->>E: Lease created

    E->>P: POST /deployment/{dseq}/manifest
    P-->>E: Manifest accepted

    E->>P: GET /lease/{id}/status
    P-->>E: Endpoints: [uri:port]

    E->>S: Store endpoints
    E-->>U: Deployment complete!
```

### Workflow Steps

| Step | Name | Action | Failure Mode |
|------|------|--------|--------------|
| 1 | KeySelection | Validate signing key exists | "Key not found" |
| 2 | BalanceCheck | Verify balance >= min_balance | "Insufficient balance" |
| 3 | CertificateSetup | Get or create Akash mTLS cert | "Certificate creation failed" |
| 4 | DeploymentCreate | Broadcast MsgCreateDeployment | "Deployment tx failed" |
| 5 | BidWait | Poll for provider bids (12-30s) | "No bids received" |
| 6 | ProviderSelection | Select cheapest (or from trusted list) | "No matching providers" |
| 7 | LeaseCreate | Broadcast MsgCreateLease | "Lease creation failed" |
| 8 | ManifestSend | POST manifest to provider | "Manifest send failed" |
| 9 | EndpointRetrieval | Query service endpoints | "Endpoints unavailable" |
| 10 | Complete | Store endpoints, mark complete | - |

### Deploy Commands

| Command | Description |
|---------|-------------|
| `ergors deploy create --sdl <path>` | Create and run automated deployment |
| `ergors deploy create --sdl <path> --interactive-bid` | Manual provider selection |
| `ergors deploy list` | List all deployment sessions |
| `ergors deploy get <session-id>` | Get deployment details |
| `ergors deploy info <session-id>` | Get comprehensive deployment info (unified view) |
| `ergors deploy status <session-id>` | Get lease status and endpoints |
| `ergors deploy endpoints <session-id>` | Get service endpoints |
| `ergors deploy bids <session-id>` | Query available provider bids |
| `ergors deploy select <session-id> <bid-id>` | Select provider by bid number |
| `ergors deploy close-lease <session-id>` | Close lease (keeps deployment) |
| `ergors deploy close-deployment <session-id>` | Close deployment and release all funds |
| `ergors deploy update-deployment <session-id> --sdl <path>` | Update deployment with new SDL |
| `ergors deploy topup-escrow <session-id> <amount>` | Top up escrow balance |

### Deploy Options

| Option | Description | Default |
|--------|-------------|---------|
| `--sdl <path>` | Path to SDL YAML file | required |
| `--key-name <name>` | Signing key name | `default` |
| `--account-index <n>` | HD derivation index | `0` |
| `--min-balance <uakt>` | Minimum required balance | `5000000` |
| `--interactive-bid` | Prompt for provider selection | `false` |
| `--node <url>` | Override RPC endpoint | config |
| `--chain-id <id>` | Override chain ID | config |

## SDL Format

Service Definition Language for Akash deployments.

### Minimal SDL

```yaml
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: 1
        memory:
          size: 512Mi
        storage:
          - size: 1Gi
  placement:
    dcloud:
      pricing:
        web:
          denom: uakt
          amount: 10000
deployment:
  web:
    dcloud:
      profile: web
      count: 1
```

### GPU SDL (Embedding Model)

```yaml
version: "2.0"
services:
  sglang:
    image: lmsysorg/sglang:dev-cu13
    expose:
      - port: 8000
        as: 8000
        to:
          - global: true
    command: ["bash", "-c"]
    args:
      - >-
        python3 -m sglang.launch_server
        --model-path Qwen/Qwen3-VL-Embedding-8B
        --tensor-parallel-size 2
        --host 0.0.0.0 --port 8000
        --is-embedding --trust-remote-code
    params:
      storage:
        shm:
          mount: /dev/shm
        data:
          mount: /root/.cache
          readOnly: false
profiles:
  compute:
    sglang:
      resources:
        cpu:
          units: 32
        memory:
          size: 64Gi
        storage:
          - size: 50Gi
          - name: data
            size: 300Gi
            attributes:
              persistent: true
              class: beta3
          - name: shm
            size: 10Gi
            attributes:
              class: ram
              persistent: false
        gpu:
          units: 2
          attributes:
            vendor:
              nvidia:
                - model: h100
                  ram: 80Gi
                - model: a100
                  ram: 40Gi
  placement:
    dcloud:
      pricing:
        sglang:
          denom: uakt
          amount: 1000000
deployment:
  sglang:
    dcloud:
      profile: sglang
      count: 1
```

## Provider Management

### Trusted Providers

Pre-configured providers with verified reputation:

| Name | Address | Specialty |
|------|---------|-----------|
| d3akash | `akash1u5cdg7k3gl43mukca4aeultuz8x2j68mgwn28e` | General |
| overclock | `akash1h4h33c8rv8e084el7e74f7pktz27pmxxt8nl9q` | GPU |
| palmito | `akash15ksejj7g4su7ljufsg0a8eglvkje94z8qsh68a` | General |
| leet.haus | `akash1kqzpqqhm39umt06wu8m4hx63v5hefhrfmjf9dj` | GPU |
| akashgpu | `akash1ut3m97h62tty06qdq9lds85r34dxe3snjj0xfe` | GPU |

### Provider Commands

| Command | Description |
|---------|-------------|
| `ergors deploy trusted-providers` | List trusted providers |
| `ergors deploy add-provider <addr> --label <name>` | Add trusted provider |
| `ergors deploy remove-provider <addr>` | Remove trusted provider |
| `ergors deploy bids <session-id>` | Query available bids |
| `ergors deploy select <session-id> --provider <addr>` | Manually select provider |

### Provider Selection Logic

```mermaid
flowchart TD
    A[Receive Bids] --> B{Trusted list empty?}
    B -->|Yes| C[Use all bids]
    B -->|No| D[Filter to trusted only]
    D --> C
    C --> E[Sort by price ascending]
    E --> F[Select cheapest]
    F --> G[Create Lease]
```

## Deployment Management

### Unified Info View

Get comprehensive deployment information in a single command:

```bash
ergors deploy info <session-id>

# JSON output
ergors deploy info <session-id> --json
```

**Output:**

```
╔══════════════════════════════════════════════════════════════╗
║             Akash Deployment Information                     ║
╠══════════════════════════════════════════════════════════════╣
║ Session ID: abc123                                           ║
║ Status:     completed                                        ║
║ Step:       Complete                                         ║
╠══════════════════════════════════════════════════════════════╣
║ Account                                                      ║
╠══════════════════════════════════════════════════════════════╣
║ Address:    akash1abc...                                     ║
║ Key:        default                                          ║
║ Chain:      akashnet-2                                       ║
╠══════════════════════════════════════════════════════════════╣
║ Deployment                                                   ║
╠══════════════════════════════════════════════════════════════╣
║ DSEQ:       12345                                            ║
║ Provider:   akash1provider...                                ║
╠══════════════════════════════════════════════════════════════╣
║ Lease                                                        ║
╠══════════════════════════════════════════════════════════════╣
║ DSEQ:       12345                                            ║
║ GSEQ:       1                                                ║
║ OSEQ:       1                                                ║
║ Provider:   akash1provider...                                ║
╠══════════════════════════════════════════════════════════════╣
║ Service Endpoints                                            ║
╠══════════════════════════════════════════════════════════════╣
║ Service:    sglang                                           ║
║   URI:      xyz.provider.akash.network:8000                  ║
║   Port:     8000:8000 (tcp)                                  ║
╚══════════════════════════════════════════════════════════════╝
```

### Closing Deployments

Two options for closing:

**Close Lease (keeps deployment):**

```bash
ergors deploy close-lease <session-id>
```

- Closes the active lease with provider
- Deployment remains on-chain
- Can create new lease later

**Close Deployment (complete shutdown):**

```bash
ergors deploy close-deployment <session-id>
```

- Closes deployment on-chain
- Automatically closes any active leases
- Releases all escrow funds
- Permanent closure

### Updating Deployments

Update deployment resources with new SDL:

```bash
ergors deploy update-deployment <session-id> --sdl new-config.yml
```

**Process:**

1. Reads new SDL file
2. Hashes SDL with SHA256
3. Broadcasts `MsgUpdateDeployment` to chain
4. Updates deployment specifications

**Note:** After updating, you may need to send a new manifest to the provider.

### Escrow Management

**Top Up Escrow:**

```bash
# Add 10 AKT (10,000,000 uakt)
ergors deploy topup-escrow <session-id> 10000000
```

**Check Escrow Balance:**

Escrow balance is shown in the info command or can be queried via:

```bash
ergors deploy info <session-id>
```

**Escrow Details:**

- Balance tracked per deployment (owner/dseq pair)
- Automatically deducted for lease payments
- Can be topped up at any time
- Released when deployment closes

## Monitoring

### Status Tracking

```bash
# Single check
ergors deploy status <session-id>

# Continuous monitoring
ergors deploy status <session-id> --follow
```

**Output:**

```
Lease Status: active
  Owner:    akash1abc...
  DSEQ:     12345
  Provider: akash1provider...
  Balance:  4500000 uakt remaining
  Endpoints:
    sglang -> xyz.provider.akash.network:8000
```

### Query Balance

```bash
ergors deploy query-balance <address>
```

## Error Handling

| Error | Cause | Resolution |
|-------|-------|------------|
| `Akash deployment context not initialized` | Missing config or keys | Run `ergors keys import-mnemonic` |
| `Insufficient balance` | Account < min_balance | Fund account with AKT |
| `No bids received` | No providers available | Increase max price in SDL |
| `Key not found` | Key name doesn't exist | Check `ergors keys list` |
| `Certificate creation failed` | Network or key issue | Restart engine |
| `Manifest send failed` | Provider unreachable | Select different provider |

## File Reference

| File | Purpose |
|------|---------|
| `packages/cw-ho/src/lib.rs` | `AkashDeploymentContext` definition |
| `packages/cw-ho/src/server.rs` | Context initialization on startup |
| `packages/cw-ho/src/deploy/signer.rs` | Transaction signing with layer-climb |
| `packages/cw-ho/src/deploy/akash.rs` | Transaction broadcasting helpers |
| `packages/cw-ho/src/deploy/certificate.rs` | Certificate management |
| `packages/cw-ho/src/deploy/automated.rs` | Workflow orchestration and lifecycle methods |
| `packages/cw-ho/src/deploy/deployment_builder.rs` | Message builders (create, close, update, escrow) |
| `packages/cw-ho/src/deploy/cosmos_client.rs` | Chain queries (balance, bids, leases, escrow) |
| `packages/cw-ho/src/deploy/manifest.rs` | Manifest generation and provider communication |
| `packages/cw-ho/src/grpc/management.rs` | gRPC handlers for deployment management |
| `packages/cw-ho/src/commands/deploy.rs` | CLI implementation |
| `packages/cw-ho/src/client/mod.rs` | gRPC client methods |
| `packages/cw-ho/src/storage.rs` | Cnidarium storage for workflows and endpoints |
| `proto/ergors/orch/v1/orch.proto` | Deployment workflow proto definitions |
| `proto/ergors/management/v1/management.proto` | Management service proto definitions |

## Complete Lifecycle Examples

### Example 1: Deploy, Monitor, and Close

```bash
# 1. Create deployment
ergors deploy create --sdl sdls/embeddings/qwen.yml
# Output: Session ID: abc123

# 2. View comprehensive info
ergors deploy info abc123

# 3. Get service endpoints
ergors deploy endpoints abc123

# 4. Access your service
curl http://xyz.provider.akash.network:8000/health

# 5. Monitor status
ergors deploy status abc123

# 6. Close when done
ergors deploy close-deployment abc123
```

### Example 2: Deploy, Update, and Scale

```bash
# 1. Initial deployment (2 CPU, 4Gi RAM)
ergors deploy create --sdl sdls/api-small.yml
# Output: Session ID: def456

# 2. Check info
ergors deploy info def456

# 3. Update to larger resources (4 CPU, 8Gi RAM)
ergors deploy update-deployment def456 --sdl sdls/api-large.yml

# 4. Verify update
ergors deploy info def456
```

### Example 3: Manage Escrow Balance

```bash
# 1. Deploy with initial balance
ergors deploy create --sdl sdls/long-running.yml
# Output: Session ID: ghi789

# 2. Check escrow balance
ergors deploy info ghi789
# Shows: Balance remaining

# 3. Top up before running out
ergors deploy topup-escrow ghi789 5000000

# 4. Verify balance increased
ergors deploy info ghi789
```

### Example 4: Manual Provider Selection

```bash
# 1. Create deployment (auto workflow)
ergors deploy create --sdl sdls/gpu-task.yml
# Output: Session ID: jkl012

# 2. Query available bids
ergors deploy bids jkl012
# Output:
#   [1] akash1provider1... | 0.025 AKT/block
#   [2] akash1provider2... | 0.030 AKT/block
#   [3] akash1provider3... | 0.028 AKT/block

# 3. Select specific provider
ergors deploy select jkl012 2

# 4. Continue workflow
ergors deploy run jkl012
```

### Example 5: Troubleshooting Failed Deployment

```bash
# 1. Deployment fails during bid wait
ergors deploy create --sdl sdls/high-gpu.yml
# Output: Session ID: mno345

# 2. Check detailed info
ergors deploy info mno345
# Shows: Last Error: "No bids received"

# 3. Query bids directly
ergors deploy bids mno345
# Output: No bids available

# 4. Update SDL with lower price
ergors deploy update-deployment mno345 --sdl sdls/high-gpu-adjusted.yml

# 5. Retry workflow
ergors deploy run mno345
```

## Quick Reference

```bash
# Full setup → deployment flow
ergors init new
ergors keys import-mnemonic --phrase "..." --label "Akash" --make-default
ergors start &
ergors deploy create --sdl sdls/embeddings/qwen.yml
ergors deploy info <session-id>

# Management operations
ergors deploy endpoints <session-id>
ergors deploy topup-escrow <session-id> 10000000
ergors deploy update-deployment <session-id> --sdl new.yml
ergors deploy close-deployment <session-id>
```
