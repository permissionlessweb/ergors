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
