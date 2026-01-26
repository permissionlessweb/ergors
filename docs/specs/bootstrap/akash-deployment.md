# Akash Network Deployment Specification

Deploy inference providers (Ollama, vLLM, TGI) to Akash Network for use during agentic LLM sessions.

## Overview

The ERGORS engine provides a streamlined workflow for deploying compute resources on Akash Network. This specification covers the end-user experience for:

1. Setting up cosmos-compatible keys
2. Configuring deployment parameters
3. Selecting providers
4. Managing deployments
5. Using deployed endpoints

## Prerequisites

- ERGORS node running (`ergors server`)
- Funded Akash account (minimum 5 AKT recommended)
- Node unlocked with custody password

## Quick Start

```bash
# 1. Import or generate a cosmos key
ergors akash key import --name deployer --mnemonic "your 24 word mnemonic..."
# or
ergors akash key generate --name deployer

# 2. Check balance
ergors akash balance --key deployer

# 3. Deploy an inference provider
ergors akash deploy ollama --key deployer --gpu 1 --memory 16Gi

# 4. Check deployment status
ergors akash status --session <session-id>

# 5. Use the endpoint
curl http://<endpoint>/api/generate -d '{"model": "llama2", "prompt": "Hello"}'
```

## Key Management

### Generate New Key

Creates a new 24-word BIP-39 mnemonic and derives a cosmos-sdk compatible keypair.

```bash
ergors akash key generate --name <key-name> [--hd-index 0]
```

**Output:**

```
Key 'deployer' created successfully
Address: akash1abc123...
HD Path: m/44'/118'/0'/0/0

IMPORTANT: Save this mnemonic securely:
abandon abandon abandon ... art
```

### Import Existing Key

Import an existing mnemonic phrase.

```bash
ergors akash key import --name <key-name> --mnemonic "<24 words>"
```

**Security Notes:**

- Mnemonics are encrypted with Argon2id + ChaCha20Poly1305
- Stored in Cnidarium merkle tree storage
- Zeroized from memory after use
- Never transmitted over network

### List Keys

```bash
ergors akash key list
```

**Output:**

```
Name        Address                                    HD Index  Chain
deployer    akash1abc123def456...                      0         akashnet-2
backup      akash1xyz789...                            1         akashnet-2
```

### Derive Additional Accounts

Derive additional accounts from the same mnemonic for concurrent deployments.

```bash
ergors akash key derive --name deployer --index 1
```

## Deployment Workflow

### 1. Create Deployment Session

```bash
ergors akash deploy <provider-type> [options]
```

**Provider Types:**

- `ollama` - Ollama inference server
- `vllm` - vLLM OpenAI-compatible server
- `tgi` - HuggingFace Text Generation Inference
- `custom` - Custom SDL template

**Common Options:**

| Option | Description | Default |
|--------|-------------|---------|
| `--key` | Key name for signing | required |
| `--cpu` | CPU units | 4 |
| `--memory` | Memory size | 16Gi |
| `--storage` | Storage size | 50Gi |
| `--gpu` | GPU count | 1 |
| `--replicas` | Instance count | 1 |
| `--model` | Model to preload | - |
| `--max-price` | Max bid price (uakt) | 50000 |
| `--trusted-only` | Only use trusted providers | false |

**Grant Request Options (for unfunded accounts):**

| Option | Description | Default |
|--------|-------------|---------|
| `--request-grant-from` | Granter node pubkey or address | - |
| `--request-grant` | Request from any available granter | false |
| `--grant-duration` | Requested grant duration | 24h |
| `--spend-limit` | Requested spend limit | 5000000uakt |
| `--grant-purpose` | Purpose/reason for the grant | - |

**Example:**

```bash
ergors akash deploy vllm \
  --key deployer \
  --gpu 1 \
  --memory 32Gi \
  --model meta-llama/Llama-2-7b-chat-hf \
  --trusted-only
```

**Example with Grant Request:**

```bash
# Deploy with authz/feegrant from another node
ergors akash deploy ollama \
  --key deployer \
  --request-grant-from <granter-node-id> \
  --grant-duration 24h \
  --spend-limit 5000000uakt \
  --grant-purpose "Deploy Ollama for inference testing"
```

### 2. Workflow Steps

The deployment proceeds through these automated steps:

```
[1/16] KeySelection        - Validating selected key
[2/16] BalanceCheck        - Checking AKT balance (minimum 5 AKT)
[3/16] GrantRequest        - Requesting authz/feegrant (if --request-grant-from used)
[4/16] GrantWait           - Waiting for grant approval
[5/16] AuthzSetup          - Verifying deployment permissions
[6/16] FeegrantSetup       - Checking fee allowances
[7/16] SdlConfiguration    - Configuring SDL template
[8/16] CertificateSetup    - Verifying Akash certificate
[9/16] DeploymentCreate    - Submitting deployment transaction
[10/16] BidWait            - Waiting for provider bids (~60s)
[11/16] ProviderSelection  - Selecting best provider
[12/16] LeaseCreate        - Creating lease with provider
[13/16] ManifestSend       - Sending deployment manifest
[14/16] EndpointRetrieval  - Getting service endpoints
[15/16] EndpointTesting    - Verifying connectivity
[16/16] Complete           - Deployment ready
```

**Note:** Steps 3-4 (GrantRequest/GrantWait) are only executed when `--request-grant-from` or `--request-grant` flags are used. For funded accounts with existing permissions, these steps are skipped automatically.

### 3. Monitor Progress

```bash
# Watch deployment progress
ergors akash status --session <session-id> --watch

# View detailed logs
ergors akash logs --session <session-id>
```

**Status Output:**

```
Session: a1b2c3d4-e5f6-7890-abcd-ef1234567890
Status: Running
Step: [9/14] ProviderSelection

Key: deployer (akash1abc123...)
Chain: akashnet-2

Provider Bids:
  akash1provider1... - 15000 uakt - Score: 95 (trusted)
  akash1provider2... - 12000 uakt - Score: 78
  akash1provider3... - 10000 uakt - Score: 65

Selected: akash1provider1... (best reputation)
```

## SDL Templates

### Built-in Templates

**Ollama:**

```yaml
services:
  ollama:
    image: ollama/ollama:${OLLAMA_VERSION:latest}
    expose:
      - port: ${EXPOSE_PORT:11434}
        as: 80
        to:
          - global: true
    env:
      - OLLAMA_HOST=0.0.0.0

profiles:
  compute:
    ollama:
      resources:
        cpu:
          units: ${CPU:4}
        memory:
          size: ${MEMORY:16Gi}
        storage:
          - size: ${STORAGE:50Gi}
        gpu:
          units: ${GPU_COUNT:1}
          attributes:
            vendor:
              nvidia:
```

**vLLM:**

```yaml
services:
  vllm:
    image: vllm/vllm-openai:${VLLM_VERSION:latest}
    expose:
      - port: ${EXPOSE_PORT:8000}
        as: 80
        to:
          - global: true
    env:
      - MODEL=${MODEL_NAME:meta-llama/Llama-2-7b-chat-hf}
    args:
      - --model
      - ${MODEL_NAME}
      - --tensor-parallel-size
      - "${TENSOR_PARALLEL:1}"

profiles:
  compute:
    vllm:
      resources:
        cpu:
          units: ${CPU:8}
        memory:
          size: ${MEMORY:32Gi}
        gpu:
          units: ${GPU_COUNT:1}
```

### Custom Templates

Create a custom SDL with variables:

```yaml
# my-template.sdl.yaml
services:
  myservice:
    image: ${IMAGE}
    expose:
      - port: ${PORT:8080}
        as: 80
        to:
          - global: true
    env:
      - API_KEY=${API_KEY_SECRET}
```

Deploy with custom template:

```bash
ergors akash deploy custom \
  --key deployer \
  --template my-template.sdl.yaml \
  --var IMAGE=myregistry/myimage:latest \
  --var PORT=3000 \
  --var API_KEY_SECRET=sk-xxx
```

### Variable Syntax

| Syntax | Description |
|--------|-------------|
| `${VAR}` | Required variable |
| `${VAR:default}` | Variable with default value |

## Provider Selection

### Trusted Providers

The following providers are pre-configured as trusted:

| Provider | Address | Specialty |
|----------|---------|-----------|
| d3akash | `akash1u5cdg7k3gl43mukca4aeultuz8x2j68mgwn28e` | General |
| overclock | `akash1h4h33c8rv8e084el7e74f7pktz27pmxxt8nl9q` | GPU |
| palmito | `akash15ksejj7g4su7ljufsg0a8eglvkje94z8qsh68a` | General |
| leet.haus | `akash1kqzpqqhm39umt06wu8m4hx63v5hefhrfmjf9dj` | GPU |
| akashgpu | `akash1ut3m97h62tty06qdq9lds85r34dxe3snjj0xfe` | GPU |

### Selection Criteria

```bash
ergors akash deploy ollama \
  --key deployer \
  --min-reputation 80 \        # Minimum reputation score (0-100)
  --max-price 30000 \          # Maximum price in uakt
  --trusted-only \             # Only trusted providers
  --min-uptime 95 \            # Minimum uptime percentage
  --reputation-weight 0.7      # Weight reputation vs price (0-1)
```

### Reputation Scoring

Provider scores are calculated from:

| Factor | Weight | Description |
|--------|--------|-------------|
| Success Rate | 35% | Successful / Total deployments |
| Uptime | 30% | Average uptime percentage |
| Response Time | 20% | Average response latency |
| Trusted Bonus | 15% | Bonus for trusted providers |

## Managing Deployments

### List Active Deployments

```bash
ergors akash list
```

**Output:**

```
Session ID                            Status     Provider              Endpoint                    Created
a1b2c3d4-e5f6-7890-abcd-ef1234567890  Running    akash1provider1...    http://xyz.akash.network    2h ago
b2c3d4e5-f6a7-8901-bcde-f23456789012  Complete   akash1provider2...    http://abc.akash.network    1d ago
```

### Get Deployment Details

```bash
ergors akash info --session <session-id>
```

**Output:**

```
Session: a1b2c3d4-e5f6-7890-abcd-ef1234567890
Status: Complete

Deployment:
  DSEQ: 12345
  GSEQ: 1
  OSEQ: 1
  Provider: akash1provider1...

Resources:
  CPU: 4 units
  Memory: 16Gi
  Storage: 50Gi
  GPU: 1x NVIDIA

Endpoints:
  ollama: http://xyz.provider.akash.network:80

Test Results:
  ollama: OK (response: 234ms)

Cost:
  Price: 15000 uakt/block
  Escrowed: 5.0 AKT
  Spent: 0.5 AKT
```

### Close Deployment

```bash
ergors akash close --session <session-id>
```

### Update Deployment

```bash
ergors akash update --session <session-id> \
  --cpu 8 \
  --memory 32Gi
```

## Using Deployed Endpoints

### Ollama

```bash
# Pull a model
curl http://<endpoint>/api/pull -d '{"name": "llama2"}'

# Generate text
curl http://<endpoint>/api/generate -d '{
  "model": "llama2",
  "prompt": "Why is the sky blue?"
}'

# Chat
curl http://<endpoint>/api/chat -d '{
  "model": "llama2",
  "messages": [{"role": "user", "content": "Hello!"}]
}'
```

### vLLM (OpenAI Compatible)

```bash
curl http://<endpoint>/v1/completions -d '{
  "model": "meta-llama/Llama-2-7b-chat-hf",
  "prompt": "Hello, how are you?",
  "max_tokens": 100
}'

curl http://<endpoint>/v1/chat/completions -d '{
  "model": "meta-llama/Llama-2-7b-chat-hf",
  "messages": [{"role": "user", "content": "Hello!"}]
}'
```

### TGI

```bash
curl http://<endpoint>/generate -d '{
  "inputs": "What is deep learning?",
  "parameters": {"max_new_tokens": 100}
}'
```

## Integration with Agentic Sessions

Deployed endpoints are automatically registered for use in agentic LLM sessions:

```bash
# Start an agentic session with Akash-deployed provider
ergors session start --provider akash:<session-id>

# Or reference by endpoint directly
ergors session start --endpoint http://xyz.akash.network
```

The workflow manager tracks endpoint availability and can automatically failover to backup deployments.

## Troubleshooting

### Common Issues

**Insufficient Balance:**

```
Error: Account has no AKT balance. Please fund the account first.
```

Solution: Send AKT to your account address.

**No Bids Received:**

```
Error: No bids received after 12 attempts
```

Solution: Increase `--max-price` or reduce resource requirements.

**Provider Selection Failed:**

```
Error: No providers match the selection criteria
```

Solution: Relax criteria (lower `--min-reputation`, remove `--trusted-only`).

**Deployment Failed:**

```
Error: Workflow failed at step DeploymentCreate: insufficient funds
```

Solution: Ensure sufficient AKT for escrow (typically 5 AKT minimum).

### View Logs

```bash
# Workflow logs
ergors akash logs --session <session-id>

# Provider logs (after lease created)
ergors akash provider-logs --session <session-id>
```

### Retry Failed Step

```bash
ergors akash retry --session <session-id>
```

### Cancel Workflow

```bash
ergors akash cancel --session <session-id>
```

## Configuration

### Default Settings

Configure defaults in `~/.ergors/config.toml`:

```toml
[akash]
chain_id = "akashnet-2"
node_endpoint = "https://rpc-akash.ecostake.com:443"
rest_endpoint = "https://rest-akash.ecostake.com"
default_key = "deployer"

[akash.defaults]
cpu = 4
memory = "16Gi"
storage = "50Gi"
gpu = 1
max_price_uakt = 50000

[akash.provider_selection]
min_reputation = 70
trusted_only = false
reputation_weight = 0.5
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ERGORS_AKASH_KEY` | Default key name |
| `ERGORS_AKASH_CHAIN_ID` | Chain ID |
| `ERGORS_AKASH_NODE` | RPC endpoint |

## API Reference

For programmatic access, the workflow manager exposes:

```rust
use ergors::deploy::workflow::AkashWorkflowManager;
use ergors::deploy::sdl::create_inference_sdl_template;

// Create workflow
let workflow = manager.create_workflow("deployer", 0).await?;

// Configure SDL
let template = create_inference_sdl_template("ollama");
let mut values = HashMap::new();
values.insert("CPU".to_string(), "8".to_string());
values.insert("MEMORY".to_string(), "32Gi".to_string());
manager.configure_workflow_sdl(&session_id, "ollama", &template, &values).await?;

// Run to completion
let result = manager.run_to_completion(&session_id).await?;
println!("Endpoints: {:?}", result.endpoints);
```

## Security Considerations

1. **Key Storage**: All keys encrypted at rest with user-provided password
2. **Network**: All Akash transactions signed locally, never exposing private keys
3. **Secrets**: SDL secrets (API keys) encrypted in transit and at rest
4. **Permissions**: Authz grants scoped to deployment operations only
5. **Feegrants**: Limited spend allowances with automatic expiration

## Cost Estimation

Typical costs for inference deployments (mainnet):

| Configuration | Approx. Cost/Day |
|---------------|------------------|
| 4 CPU, 16Gi, 1 GPU | ~2-5 AKT |
| 8 CPU, 32Gi, 1 GPU | ~4-8 AKT |
| 8 CPU, 64Gi, 2 GPU | ~8-15 AKT |

Costs vary by provider and market conditions. Use `--max-price` to cap spending.

## Testing

### Integration Testing Environment

ERGORS provides a comprehensive testing suite for validating Akash deployments without production costs.

**Components:**

- **Kind Cluster**: Local Kubernetes with Akash node/provider
- **Mock Inference Provider**: Simulates Ollama/OpenAI/TGI APIs without GPU
- **Test Wallet Manager**: Pre-funded accounts for testing
- **Network Topology**: Multi-node grant request simulation

### Quick Start

```bash
# 1. Setup Akash development environment
./packages/cw-ho/tests/scripts/setup-akash-dev.sh

# 2. Build mock inference provider
cd docker/mock-inference-provider
docker build -t ergors/mock-inference-provider .

# 3. Run mock provider
docker run -p 11434:11434 ergors/mock-inference-provider

# 4. Run integration tests
cargo test -p ergors --features testing -- --nocapture
```

### Mock Inference Provider

Test against a mock inference provider that simulates real API responses:

```bash
# Ollama API
curl http://localhost:11434/api/generate \
  -d '{"model":"llama2","prompt":"Hello","stream":false}'

# OpenAI API
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama2","messages":[{"role":"user","content":"Hello"}]}'

# TGI API
curl http://localhost:11434/generate \
  -d '{"inputs":"Hello","parameters":{"max_new_tokens":100}}'
```

### Deploy Mock Provider to Akash

Test the full deployment workflow using the mock provider:

```bash
# Deploy mock inference provider (no GPU required)
ergors akash deploy custom \
  --key deployer \
  --template docker/mock-inference-provider/deploy.sdl.yaml \
  --var MODEL_NAME=llama2 \
  --var MIN_LATENCY_MS=100
```

### Testing Grant Requests

Test the authz/feegrant workflow without real funds:

```rust
use cw_ho::deploy::testing::prelude::*;

#[tokio::test]
async fn test_deployment_with_grants() {
    // Setup network simulation
    let network = NetworkTopology::new();
    let requester = network.create_node("requester").await?;
    network.create_node("granter").await?;

    // Configure whitelist
    network.set_grant_mode("granter", GrantAcceptanceMode::Whitelist).await?;
    network.whitelist_add("granter", &requester.pubkey).await?;

    // Submit grant request
    let request = network.submit_grant_request(
        "requester", "granter",
        GrantTypeRequest::AuthzAndFeegrant,
        86400, 5_000_000, "Test deployment"
    ).await?;

    assert_eq!(request.status, GrantRequestStatus::Approved);
}
```

See [Akash Deployment Testing Plan](../akash-deployment-testing-plan.md) for complete documentation.

---

## Architecture: Automated Deployment Engine

This section documents the internal implementation of the automated deployment system.

### Overview

The ERGORS engine provides fully automated Akash deployments through the `AutomatedDeployer` component. When `--auto` flag is used (or `run_akash_deployment` gRPC is called), the entire deployment flow executes without user intervention.

```
┌─────────────────────────────────────────────────────────────────┐
│                      ErgorsAppState                             │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │            AkashDeploymentContext (optional)            │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────────┐  │   │
│  │  │ CosmosClient│  │  TxSigner   │  │ TxLifecycle    │  │   │
│  │  │  (queries)  │  │  (signing)  │  │ (broadcast)    │  │   │
│  │  └─────────────┘  └─────────────┘  └────────────────┘  │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────────┐  │   │
│  │  │CertManager  │  │ KeyManager  │  │   KeyStore     │  │   │
│  │  │ (certs)     │  │ (unlocked)  │  │  (encrypted)   │  │   │
│  │  └─────────────┘  └─────────────┘  └────────────────┘  │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                   ┌─────────────────────┐
                   │ AutomatedDeployer   │
                   │  .deploy(workflow)  │
                   └─────────────────────┘
                              │
        ┌─────────────────────┴─────────────────────┐
        ▼                                           ▼
  ┌──────────────┐                          ┌──────────────┐
  │ Workflow     │◄────────────────────────►│  Cnidarium   │
  │ State Machine│    persist/load state    │   Storage    │
  └──────────────┘                          └──────────────┘
```

### Components

#### AkashDeploymentContext

Holds all components required for automated deployments. Initialized at server startup when:

- Akash config exists in `config.toml`
- Cosmos key store present in storage
- Custody password available (for key manager unlock)

**Location:** `packages/cw-ho/src/lib.rs`

```rust
pub struct AkashDeploymentContext {
    pub cosmos: Arc<CosmosClient>,        // Chain queries (balances, bids, leases)
    pub signer: Arc<TxSigner>,            // Transaction signing with KeyStore
    pub tx_lifecycle: Arc<TxLifecycle>,   // Sign → broadcast → finality polling
    pub cert_manager: Arc<CertificateManager>, // Certificate get/create
    pub key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    pub key_store: Arc<RwLock<CosmosKeyStore>>,
}
```

#### TxSigner

Signs Cosmos SDK transactions using keys from the encrypted KeyStore.

**Location:** `packages/cw-ho/src/deploy/signer.rs`

```rust
impl TxSigner {
    /// Sign a message and return base64-encoded tx bytes
    pub async fn sign_msg(
        &self,
        key_name: &str,
        account_index: u32,
        msg: Any,
        gas_limit: u64,
        gas_price_uakt: u64,
        memo: Option<&str>,
    ) -> Result<String>
}
```

**Features:**

- HD derivation path: `m/44'/118'/0'/0/{account_index}`
- Automatic account info query (number, sequence)
- Support for multi-message transactions
- Uses cosmrs for transaction building

#### TxLifecycle

Manages the full transaction lifecycle: sign → broadcast → poll → finality.

**Location:** `packages/cw-ho/src/deploy/tx_lifecycle.rs`

```rust
impl TxLifecycle {
    /// Sign, broadcast, and wait for finality (~6s blocks on Akash)
    pub async fn sign_broadcast_wait(
        &self,
        key_name: &str,
        account_index: u32,
        msg: Any,
        gas_limit: u64,
        gas_price: u64,
        memo: Option<&str>,
    ) -> Result<TxResult>
}

pub struct TxResult {
    pub hash: String,
    pub height: u64,
    pub code: u32,        // 0 = success
    pub gas_used: u64,
    pub raw_log: String,
    pub events: Vec<TxEvent>,
}
```

**Transaction Flow:**

1. Sign transaction with TxSigner
2. POST to `/cosmos/tx/v1beta1/txs` (BROADCAST_MODE_SYNC)
3. Check immediate response (code 0 = accepted to mempool)
4. Poll `/cosmos/tx/v1beta1/txs/{hash}` every 2s (max 60s)
5. Parse events to extract dseq, lease_id, etc.

#### CertificateManager

Handles Akash mTLS certificate lifecycle.

**Location:** `packages/cw-ho/src/deploy/certificate.rs`

```rust
impl CertificateManager {
    /// Get existing valid certificate or create new one
    pub async fn get_or_create(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
    ) -> Result<AkashCertificateInfo>
}
```

**Workflow:**

1. Query chain for existing valid certificate
2. If none, generate secp256k1 keypair
3. Build certificate data structure
4. Broadcast MsgCreateCertificate
5. Return certificate info

#### AutomatedDeployer

Orchestrates the full deployment workflow.

**Location:** `packages/cw-ho/src/deploy/automated.rs`

```rust
impl AutomatedDeployer {
    /// Run full automated deployment
    pub async fn deploy(
        &self,
        workflow: &mut AkashDeploymentWorkflow,
        opts: &AkashWorkflowOptions,
    ) -> Result<DeploymentResult>

    /// Close an active deployment
    pub async fn close_deployment(
        &self,
        workflow: &AkashDeploymentWorkflow,
    ) -> Result<()>
}
```

### Automated Deployment Steps

When `deploy()` is called, it executes these steps sequentially:

| Step | Method | Description |
|------|--------|-------------|
| 1 | `step_check_balance` | Query account balance, verify >= min_balance_uakt |
| 2 | `step_setup_certificate` | Get or create Akash mTLS certificate |
| 3 | `step_create_deployment` | Build SDL → MsgCreateDeployment → broadcast |
| 4 | `step_wait_for_bids` | Poll for provider bids (~12-30s) |
| 5 | `step_select_provider` | Select cheapest from trusted_providers (or all) |
| 6 | `step_create_lease` | MsgCreateLease → broadcast |
| 7 | `step_send_manifest` | POST manifest JSON to provider REST API |
| 8 | `step_retrieve_endpoints` | Query provider for service endpoints |
| 9 | `step_save_endpoints` | Persist endpoints to Cnidarium storage |

**Step Details:**

```
step_check_balance
    └── CosmosClient.query_balance() → verify >= 5 AKT

step_setup_certificate
    └── CertificateManager.get_or_create()
        ├── query_valid_certificate() → found? return
        └── create_certificate() → MsgCreateCertificate tx

step_create_deployment
    ├── get_next_dseq() → query current + 1
    ├── DeploymentBuilder.build_from_sdl()
    └── TxLifecycle.sign_broadcast_wait(MsgCreateDeployment)

step_wait_for_bids
    ├── sleep(bid_wait_blocks * 6s)
    └── poll CosmosClient.query_open_bids() (max 10 attempts)

step_select_provider
    ├── filter by trusted_providers (if non-empty)
    └── select min(price_amount)

step_create_lease
    └── TxLifecycle.sign_broadcast_wait(MsgCreateLease)

step_send_manifest
    └── ManifestSender.send_manifest_from_sdl()
        └── POST https://{provider}:8443/deployment/{dseq}/manifest

step_retrieve_endpoints
    └── query_service_endpoints() with retries
        └── GET https://{provider}:8443/lease/{dseq}/{gseq}/{oseq}/status
```

### gRPC Integration

The automated workflow is triggered via gRPC:

**`run_akash_deployment` handler:**

```rust
async fn run_akash_deployment(&self, request: Request<RunAkashDeploymentRequest>)
    -> Result<Response<RunAkashDeploymentResponse>, Status>
{
    // 1. Get AkashDeploymentContext from app state
    let akash_ctx = self.state.akash.as_ref()
        .ok_or_else(|| Status::failed_precondition("..."))?;

    // 2. Load workflow from storage
    let mut workflow = self.state.s.get_akash_workflow(&session_id).await?;

    // 3. Create deployer from context
    let deployer = akash_ctx.create_deployer(self.state.s.clone());

    // 4. Run automated deployment
    let result = deployer.deploy(&mut workflow, &options).await?;

    // 5. Return completed workflow with endpoints
    Ok(Response::new(RunAkashDeploymentResponse {
        workflow: Some(workflow),
        completed: true,
        ..
    }))
}
```

### Workflow Options

```protobuf
message AkashWorkflowOptions {
    bool skip_grants = 1;           // Skip authz/feegrant steps
    bool auto_select_bid = 2;       // Auto-select cheapest provider
    uint64 min_balance_uakt = 3;    // Minimum balance required (default: 5M)
    uint32 bid_wait_blocks = 4;     // Blocks to wait for bids (default: 2)
    repeated string trusted_providers = 5;  // Filter bids to these providers
    uint32 max_retries = 6;         // Max retry attempts per step
}
```

### Storage Keys

Workflow state persisted to Cnidarium:

| Key Pattern | Content |
|-------------|---------|
| `akash_workflows/{session_id}` | Full AkashDeploymentWorkflow proto |
| `deployment_endpoints/{owner}/{dseq}/{provider}` | Service endpoints JSON |
| `custody/cosmos_key_store` | Encrypted CosmosKeyStore proto |

### CLI Usage

```bash
# Fully automated deployment
ergors deploy create \
  --sdl sdls/embeddings/qwen.yml \
  --key-name default \
  --auto \
  --auto-select-bid \
  --min-balance 10000000

# Check status
ergors deploy status <session-id>

# Close deployment
ergors deploy close-lease <session-id>
```

### Error Handling

Each step can fail independently. On failure:

1. Workflow status set to `Failed`
2. `last_error` field populated with error message
3. Current step preserved for retry/debugging
4. Partial state (dseq, certificate, etc.) retained

**Common Errors:**

| Error | Cause | Resolution |
|-------|-------|------------|
| `Insufficient balance` | < min_balance_uakt | Fund account with AKT |
| `No bids received` | No providers available | Increase max price or wait |
| `Certificate creation failed` | Invalid key or network | Check key permissions |
| `Deployment tx failed` | Invalid SDL or escrow | Verify SDL syntax |
| `Lease creation failed` | Bid expired or invalid | Retry with fresh bids |
| `Manifest send failed` | Provider unreachable | Check provider status |

### Monitoring

```bash
# View logs
RUST_LOG=ergors::deploy=debug ergors start

# Workflow state inspection
ergors deploy get <session-id>

# Active deployments
ergors deploy list --status running
```

### Files Reference

| File | Purpose |
|------|---------|
| `lib.rs` | `AkashDeploymentContext` struct and `ErgorsAppState` |
| `server.rs` | `init_akash_context()` initialization |
| `deploy/signer.rs` | Transaction signing with KeyStore |
| `deploy/tx_lifecycle.rs` | Sign → broadcast → finality polling |
| `deploy/certificate.rs` | Certificate management |
| `deploy/automated.rs` | Full automation orchestration |
| `deploy/deployment_builder.rs` | SDL → MsgCreateDeployment |
| `deploy/manifest.rs` | Manifest building and provider REST |
| `deploy/cosmos_client.rs` | Chain queries (balances, bids, leases) |
| `grpc/management.rs` | gRPC handlers for deployment operations |
