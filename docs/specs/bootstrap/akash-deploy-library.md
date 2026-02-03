# Akash Deploy Library

Standalone deployment workflow engine for Akash Network - trait-based, generic, and reusable.

## Overview

The ERGORS Akash deployment library provides a complete automated deployment workflow for Akash Network. It handles the entire lifecycle from SDL configuration to endpoint retrieval, using JWT authentication for secure provider communication.

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                    AutomatedDeployer                               │
├────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │ CosmosClient │  │ KeyManager   │  │  Storage     │              │
│  │ (REST/gRPC)  │  │ (Encrypted)  │  │ (Cnidarium)  │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
│                                                                     │
│  Workflow Steps:                                                    │
│  1. Connectivity Check (REST API)                                   │
│  2. Balance Check (CosmosClient)                                    │
│  3. Create Deployment (layer-climb tx)                              │
│  4. Wait for Bids (polling REST API)                                │
│  5. Select Provider (auto or interactive)                           │
│  6. Create Lease (layer-climb tx)                                   │
│  7. Send Manifest (JWT auth -> Provider REST)                       │
│  8. Retrieve Endpoints (JWT auth -> Provider REST)                  │
└────────────────────────────────────────────────────────────────────┘
```

## JWT Authentication

The library uses JWT (JSON Web Token) authentication for all provider communication. This replaced the previous mTLS certificate-based authentication.

### Self-Attested JWT Flow

JWTs are **self-attested** by the client and validated independently on each request. There is no registration step or challenge-response flow.

```
┌──────────┐                      ┌──────────────┐                    ┌──────────┐
│  Client  │                      │   Provider   │                    │  Chain   │
└────┬─────┘                      └──────┬───────┘                    └────┬─────┘
     │                                   │                                 │
     │  1. Create JWT locally:           │                                 │
     │     - Header: {"alg":"ES256K"}    │                                 │
     │     - Claims: {iss, iat, exp}     │                                 │
     │     - Sign with secp256k1 key     │                                 │
     │                                   │                                 │
     │  2. Request with Bearer token     │                                 │
     │     Authorization: Bearer <jwt>   │                                 │
     │ ──────────────────────────────────>                                 │
     │                                   │                                 │
     │                                   │  3. Parse JWT, extract issuer   │
     │                                   │                                 │
     │                                   │  4. Fetch account pubkey        │
     │                                   │     accountQuerier.GetPubKey()  │
     │                                   │ ────────────────────────────────>
     │                                   │                                 │
     │                                   │  5. Public key from on-chain    │
     │                                   │ <────────────────────────────────
     │                                   │                                 │
     │                                   │  6. Verify JWT signature        │
     │                                   │                                 │
     │  7. Response                      │                                 │
     │ <──────────────────────────────────                                 │
     │                                   │                                 │
```

### Implementation Details

#### JWT Structure

```
Header: {"alg": "ES256K", "typ": "JWT"}
Claims: {
  "iss": "akash1...",    // Issuer = account address (44 chars: akash1 + 38)
  "iat": 1706000000,     // Issued at (Unix timestamp)
  "exp": 1706000900,     // Expiration (15 min)
  "nbf": 1706000000,     // Not before
  "jti": "akash1abc...-uuid",  // JWT ID for replay protection
  "version": "v1",       // Must be exactly "v1"
  "leases": {
    "access": "full"     // AccessTypeFull: "full" | "scoped" | "granular"
  }
}
Signature: ES256K(SHA256(header.claims))  // Single-SHA256 per RFC 8812
```

**Important:** ES256K (RFC 8812) uses **single-SHA256** hashing before ECDSA signing with secp256k1 curve. This follows the standard ECDSA specification, NOT Bitcoin's double-SHA256.

#### JWT Creation (Client-Side)

```rust
fn create_jwt(&mut self, address: &str, keypair: &CosmosKeyPair) -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("System time error: {}", e))?
        .as_secs() as i64;

    let exp = now + 15 * 60;  // 15 minute expiry

    let header = JwtHeader {
        alg: "ES256K".to_string(),
        typ: "JWT".to_string()
    };

    // Generate unique JWT ID for replay protection
    let jti = format!("{}-{}", &address[..12], uuid::Uuid::new_v4());

    let claims = JwtClaims {
        iss: address.to_string(),
        iat: now,
        exp,
        nbf: now,
        jti: Some(jti),  // Replay protection
        version: "v1".to_string(),
        leases: JwtLeases { access: "full".to_string() },
    };

    // VALIDATE CLAIMS BEFORE SIGNING
    validate_claims(&claims)?;  // Validates format, time, version, access type

    // Base64url encode header and claims
    let header_b64 = base64url_encode(&serde_json::to_string(&header)?);
    let claims_b64 = base64url_encode(&serde_json::to_string(&claims)?);

    // Create signing input: header.claims
    let signing_input = format!("{}.{}", header_b64, claims_b64);

    // ES256K: Single-SHA256 (RFC 8812 standard), then ECDSA sign
    let hash = Sha256::digest(signing_input.as_bytes());
    let signature = keypair.sign_prehash(&hash)?;  // 64-byte compact signature (r || s)
    let signature_b64 = base64url_encode_bytes(&signature);

    // JWT: header.claims.signature
    Ok(format!("{}.{}", signing_input, signature_b64))
}
```

**Key Implementation Details:**

1. **ES256K Signing:** Uses `sign_jwt_es256k()` which performs single-SHA256 hashing (RFC 8812) before ECDSA signing with secp256k1 curve
2. **Claims Validation:** All claims are validated before signing:
   - Issuer format: `akash1` + 38 chars (44 total)
   - Time relationships: `nbf <= iat <= exp`
   - Token not expired or future-dated
   - Version exactly `"v1"`
   - Access type one of: `"full"`, `"scoped"`, `"granular"`
3. **Replay Protection:** Unique `jti` (JWT ID) prevents token reuse
4. **Signature Format:** 64-byte compact format (r || s), base64url-encoded
5. **Error Handling:** Proper Result propagation, no panics

#### Provider Validation

The provider validates JWTs in its gateway middleware:

1. **Parse JWT** - Extract header and claims from `Authorization: Bearer <token>`
2. **Validate Claims Schema:**
   - Issuer (`iss`) is valid bech32 format (44 chars)
   - Timestamps are logical: `nbf <= iat <= exp`
   - Current time within validity window: `nbf <= now <= exp`
   - Version is exactly `"v1"`
   - Access type is valid: `"full"`, `"scoped"`, or `"granular"`
3. **Fetch Public Key** - Query blockchain for issuer's account public key via `accountQuerier.GetAccountPublicKey()`
4. **Verify ES256K Signature:**
   - Reconstruct signing input: `base64url(header).base64url(claims)`
   - Single-SHA256 hash the signing input (RFC 8812 standard)
   - Verify ECDSA signature using on-chain public key
5. **Check JWT ID (optional)** - If `jti` present, verify not reused (provider-specific)

If validation fails, returns `ErrJWTInvalid`, `ErrJWTExpired`, or `ErrJWTMissing`.

#### Token Caching

Tokens are cached client-side with expiration tracking:

```rust
pub struct JwtToken {
    pub token: String,
    pub expires_at: std::time::Instant,
}

pub struct JwtAuthClient {
    http: HttpClient,
    provider_uri: String,
    cached_token: Option<JwtToken>,
}

impl JwtAuthClient {
    pub async fn get_token(&mut self, address: &str, keypair: &CosmosKeyPair) -> Result<String> {
        // Return cached token if still valid (with 60s buffer)
        if let Some(ref token) = self.cached_token {
            if !token.is_expired() {
                return Ok(token.token.clone());
            }
        }

        // Create fresh self-signed JWT
        let token = self.create_jwt(address, keypair)?;

        // Cache with 14-minute expiration (tokens valid 15 min)
        self.cached_token = Some(JwtToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(14 * 60),
        });

        Ok(token)
    }
}
```

### Key Points

1. **Self-Attested**: Client creates and signs JWT entirely client-side. No challenge-response or registration.

2. **No Certificate Required**: Unlike mTLS, JWT auth doesn't require publishing certificates to the blockchain.

3. **On-Chain Key Verification**: Provider fetches account public key from on-chain state - no pre-registration needed.

4. **Per-Request Validation**: Each request is independently validated. No session state on provider.

5. **Short-Lived Tokens**: JWTs are valid for 15 minutes and must be refreshed.

6. **Same Keys**: Uses the same secp256k1 keypair used for blockchain transactions.

7. **ES256K Algorithm**: ECDSA with secp256k1 curve + **single-SHA256** hashing per RFC 8812, NOT Bitcoin's double-SHA256.

8. **Replay Protection**: JWT ID (`jti`) field prevents token reuse during cache window.

9. **Claims Validation**: All claims validated before signing to prevent malformed tokens.

10. **Cannot Mix Auth Types**: Provider returns `ErrAuthAmbiguous` if both mTLS cert and JWT are presented.

### Security Considerations

**✅ Implemented:**
- Single-SHA256 hashing for ES256K (RFC 8812) compliance
- Comprehensive claims validation (format, time, version, access)
- JWT ID (jti) for replay protection
- Token caching with 60s expiry buffer
- Sanitized logging (no full address exposure)

**⚠️ Known Issues:**
- **TLS Certificate Validation:** Currently accepts any provider certificate (`danger_accept_invalid_certs: true`). This is vulnerable to MITM attacks. Future work should implement certificate pinning or fingerprint validation during bid acceptance phase.

**Recommended:**
- Test JWT generation and validation against real Akash providers
- Verify provider logs show successful signature verification
- Monitor for unexpected 401 errors indicating signature format mismatches

## Deployment Workflow

### Step 1: Connectivity Check

Verifies connection to the Akash network via REST API:

```rust
async fn step_connectivity_check(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
    let response = endpoint_manager
        .execute_with_failover(EndpointType::Rest, |rest_endpoint| {
            // Query /cosmos/base/tendermint/v1beta1/node_info
        })
        .await?;
    // Verify chain ID matches expected
}
```

### Step 2: Balance Check

Queries wallet balance to ensure sufficient funds:

```rust
async fn step_check_balance(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
    let balance = self.cosmos.query_balance(&workflow.account_address, "uakt").await?;
    // Require minimum 5 AKT for deployment
}
```

### Step 3: Create Deployment

Broadcasts `MsgCreateDeployment` to the chain:

```rust
async fn step_create_deployment(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<u64> {
    let dseq = get_next_dseq(rest_endpoint, &workflow.account_address).await?;
    let msg = deployment_builder.build_from_sdl(&sdl.resolved_content)?;
    broadcast_akash_msg(signing_client, &MsgCreateDeployment::type_url(), &msg, memo).await?;
}
```

### Step 4: Wait for Bids

Polls for provider bids on the deployment:

```rust
async fn step_wait_for_bids(&self, workflow: &mut AkashDeploymentWorkflow, dseq: u64) -> Result<Vec<BidInfo>> {
    // Wait initial 2 blocks (~12s)
    // Poll up to 10 times with 6s intervals
    let bids = self.cosmos.query_open_bids(&owner, dseq).await?;
}
```

### Step 5: Select Provider

Auto-selects cheapest from trusted providers or prompts for interactive selection:

```rust
async fn step_select_provider(&self, workflow: &mut AkashDeploymentWorkflow, bids: &[BidInfo]) -> Result<BidInfo> {
    // Filter by trusted providers if configured
    // Sort by price
    // Auto-select or interactive prompt
}
```

### Step 6: Create Lease

Broadcasts `MsgCreateLease` to accept the bid:

```rust
async fn step_create_lease(&self, workflow: &mut AkashDeploymentWorkflow, bid: &BidInfo) -> Result<()> {
    let msg = build_create_lease_msg(&bid.owner, bid.dseq, bid.gseq, bid.oseq, &bid.provider, bid.bseq);
    broadcast_akash_msg(signing_client, &MsgCreateLease::type_url(), &msg, memo).await?;
}
```

### Step 7: Send Manifest

Sends the SDL manifest to the provider using JWT authentication:

```rust
async fn step_send_manifest(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<()> {
    let keypair = self.get_workflow_keypair(workflow).await?;
    let mut sender = ManifestSender::new(&provider_uri);
    sender.send_manifest_from_sdl(
        &lease_info.owner,
        lease_info.dseq,
        lease_info.gseq,
        lease_info.oseq,
        &sdl.resolved_content,
        &workflow.account_address,
        &keypair,
    ).await?;
}
```

### Step 8: Retrieve Endpoints

Queries the provider for service endpoints:

```rust
async fn step_retrieve_endpoints(&self, workflow: &mut AkashDeploymentWorkflow) -> Result<Vec<AkashServiceEndpoint>> {
    let keypair = self.get_workflow_keypair(workflow).await?;
    let endpoints = query_service_endpoints(
        &provider_uri,
        &lease_info.owner,
        lease_info.dseq,
        lease_info.gseq,
        lease_info.oseq,
        &workflow.account_address,
        &keypair,
    ).await?;
}
```

## Automatic Cleanup

If any step fails after `MsgCreateDeployment` succeeds, the workflow automatically broadcasts `MsgCloseDeployment` to recover the escrow deposit:

```rust
async fn cleanup_failed_deployment(&self, workflow: &AkashDeploymentWorkflow, dseq: u64) -> Result<()> {
    let msg = build_close_deployment_msg(&workflow.account_address, dseq);
    broadcast_akash_msg(signing_client, &MsgCloseDeployment::type_url(), &msg, "cleanup").await?;
}
```

## Layer-Climb Integration

The library uses [layer-climb](https://github.com/ArnaudBroworsky/layer-climb) for robust Cosmos SDK transaction signing:

- Automatic account sequence management (QueryAndIncrement strategy)
- Endpoint failover for production resilience
- Type-safe message encoding via prost

## Message Types

The following message types are used for authz grants:

```rust
pub fn all_deployment_msg_types() -> Vec<String> {
    vec![
        MsgCreateDeployment::type_url(),
        MsgUpdateDeployment::type_url(),
        MsgCloseDeployment::type_url(),
        MsgCreateLease::type_url(),
        MsgCloseBid::type_url(),
        MsgWithdrawLease::type_url(),
    ]
}
```

## Deployment as Provider

Completed deployments are automatically integrated into the ERGORS inference routing system:

1. **Label Assignment**: Deployments can be assigned labels during creation
2. **Cache Integration**: Completed deployments are added to `DeploymentProviderCache`
3. **Model Routing**: Deployment labels become model names for inference requests
4. **Auto-Sync**: Cache refreshes every 30s from storage

Example:
```bash
# Deploy with label
ergors deploy create --sdl qwen.yml --label qwen-inference --auto

# Use deployment as model
curl /v1/chat/completions -d '{"model": "qwen-inference", ...}'
```

## Files

| File | Description |
|------|-------------|
| `deploy/automated.rs` | Main `AutomatedDeployer` implementation |
| `deploy/manifest.rs` | Manifest sending and endpoint querying with JWT auth |
| `deploy/akash.rs` | Message type URLs and broadcast helpers |
| `deploy/deployment_builder.rs` | SDL parsing and message construction |
| `deploy/cosmos_client.rs` | REST API client for chain queries |
| `deploy/climb_signer.rs` | layer-climb signing client factory |
| `deploy/endpoint_manager.rs` | Endpoint failover management |
