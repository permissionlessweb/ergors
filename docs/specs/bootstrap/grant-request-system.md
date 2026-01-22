# Grant Request System Specification

Request authz permissions and feegrant allowances from other nodes in the network via a CosmWasm-mediated workflow.

## Overview

Nodes without sufficient funds can request deployment permissions from funded nodes. A CosmWasm contract manages the whitelist and approval workflow, triggering the granting node to broadcast `MsgGrant` and `MsgGrantAllowance` transactions when requests are approved.

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  Requester Node │     │  Grant Manager       │     │  Granter Node   │
│  (wants deploy) │     │  Contract (CosmWasm) │     │  (has funds)    │
└────────┬────────┘     └──────────┬───────────┘     └────────┬────────┘
         │                         │                          │
         │  1. Request Grant       │                          │
         │────────────────────────>│                          │
         │                         │                          │
         │                         │  2. Check Whitelist      │
         │                         │  & Emit Event            │
         │                         │─────────────────────────>│
         │                         │                          │
         │                         │                          │ 3. Node sees event,
         │                         │                          │    auto-approves or
         │                         │                          │    queues for review
         │                         │                          │
         │                         │  4. Approve/Reject       │
         │                         │<─────────────────────────│
         │                         │                          │
         │  5. Query Status        │                          │
         │<────────────────────────│                          │
         │                         │                          │
         │                         │                          │ 6. Broadcast
         │                         │                          │    MsgGrant &
         │                         │                          │    MsgGrantAllowance
         │                         │                          │
         │  7. Permissions Active  │                          │
         │<───────────────────────────────────────────────────│
```

## CLI Usage

### Requester Side

```bash
# Request authz + feegrant from a specific node
ergors akash deploy ollama \
  --key deployer \
  --request-grant-from <granter-node-id> \
  --grant-duration 24h \
  --spend-limit 5000000uakt

# Request from any available granter (contract finds one)
ergors akash deploy ollama \
  --key deployer \
  --request-grant \
  --grant-duration 24h

# Check grant request status
ergors akash grant-status --request-id <id>
```

### Granter Side

```bash
# Configure grant acceptance mode
ergors akash grant-config set-mode <accept-all|reject-all|whitelist>

# Manage whitelist
ergors akash grant-config whitelist add <node-pubkey>
ergors akash grant-config whitelist remove <node-pubkey>
ergors akash grant-config whitelist list

# Set default grant parameters
ergors akash grant-config defaults \
  --max-duration 48h \
  --max-spend 10000000uakt \
  --allowed-messages "MsgCreateDeployment,MsgCreateLease"

# View pending requests (when in whitelist mode)
ergors akash grant-requests list --pending

# Manually approve/reject
ergors akash grant-requests approve <request-id>
ergors akash grant-requests reject <request-id> --reason "insufficient trust"
```

## Contract Design

### Contract: `grant-manager`

**Instantiate:**
```rust
pub struct InstantiateMsg {
    /// Contract admin (can update config)
    pub admin: String,
    /// Default acceptance mode for new granters
    pub default_mode: AcceptanceMode,
}

pub enum AcceptanceMode {
    /// Automatically approve all requests
    AcceptAll,
    /// Reject all requests
    RejectAll,
    /// Only approve whitelisted requesters
    Whitelist,
    /// Require manual approval for each request
    Manual,
}
```

**Execute Messages:**
```rust
pub enum ExecuteMsg {
    // ========== Granter Configuration ==========

    /// Register as a granter node
    RegisterGranter {
        /// Node's public key (ed25519)
        node_pubkey: Binary,
        /// Cosmos address that will sign grant transactions
        granter_address: String,
        /// Acceptance mode
        mode: AcceptanceMode,
        /// Default grant parameters
        defaults: GrantDefaults,
    },

    /// Update granter configuration
    UpdateGranterConfig {
        mode: Option<AcceptanceMode>,
        defaults: Option<GrantDefaults>,
    },

    /// Add address to whitelist
    WhitelistAdd {
        /// Requester's node public key
        node_pubkey: Binary,
        /// Optional custom limits for this requester
        custom_limits: Option<GrantLimits>,
    },

    /// Remove address from whitelist
    WhitelistRemove {
        node_pubkey: Binary,
    },

    // ========== Request Flow ==========

    /// Submit a grant request
    RequestGrant {
        /// Requester's node public key
        requester_pubkey: Binary,
        /// Requester's cosmos address (grantee)
        grantee_address: String,
        /// Specific granter to request from (None = any available)
        granter_pubkey: Option<Binary>,
        /// Requested grant type
        grant_type: GrantType,
        /// Requested parameters
        params: GrantRequestParams,
    },

    /// Approve a pending request (granter only)
    ApproveRequest {
        request_id: u64,
        /// Override requested params if desired
        approved_params: Option<GrantRequestParams>,
    },

    /// Reject a pending request (granter only)
    RejectRequest {
        request_id: u64,
        reason: String,
    },

    /// Mark grant as broadcasted (granter confirms tx sent)
    ConfirmGrantBroadcast {
        request_id: u64,
        tx_hash: String,
    },

    /// Cancel a pending request (requester only)
    CancelRequest {
        request_id: u64,
    },

    // ========== Admin ==========

    /// Update contract admin
    UpdateAdmin {
        new_admin: String,
    },
}
```

**Query Messages:**
```rust
pub enum QueryMsg {
    /// Get granter configuration
    Granter {
        node_pubkey: Binary,
    },

    /// List all registered granters
    ListGranters {
        start_after: Option<Binary>,
        limit: Option<u32>,
    },

    /// Get whitelist for a granter
    Whitelist {
        granter_pubkey: Binary,
        start_after: Option<Binary>,
        limit: Option<u32>,
    },

    /// Check if a requester is whitelisted
    IsWhitelisted {
        granter_pubkey: Binary,
        requester_pubkey: Binary,
    },

    /// Get a specific grant request
    Request {
        request_id: u64,
    },

    /// List requests by status
    ListRequests {
        granter_pubkey: Option<Binary>,
        requester_pubkey: Option<Binary>,
        status: Option<RequestStatus>,
        start_after: Option<u64>,
        limit: Option<u32>,
    },

    /// Find available granters for a request
    FindGranters {
        requester_pubkey: Binary,
        grant_type: GrantType,
        params: GrantRequestParams,
    },
}
```

**Types:**
```rust
pub struct GrantDefaults {
    /// Maximum grant duration
    pub max_duration_seconds: u64,
    /// Maximum spend limit for feegrants
    pub max_spend_limit_uakt: u64,
    /// Allowed message types for authz
    pub allowed_msg_types: Vec<String>,
    /// Auto-approve delay (0 = immediate)
    pub auto_approve_delay_seconds: u64,
}

pub struct GrantLimits {
    pub max_duration_seconds: Option<u64>,
    pub max_spend_limit_uakt: Option<u64>,
    pub max_concurrent_grants: Option<u32>,
}

pub enum GrantType {
    /// Only authz permissions
    AuthzOnly,
    /// Only feegrant allowance
    FeegrantOnly,
    /// Both authz and feegrant
    AuthzAndFeegrant,
}

pub struct GrantRequestParams {
    /// Requested duration in seconds
    pub duration_seconds: u64,
    /// Requested spend limit (for feegrant)
    pub spend_limit_uakt: u64,
    /// Specific message types to authorize
    pub msg_types: Vec<String>,
    /// Purpose/reason for the grant
    pub purpose: String,
}

pub enum RequestStatus {
    /// Waiting for granter approval
    Pending,
    /// Approved, waiting for broadcast
    Approved,
    /// Grant transaction broadcasted
    Broadcasted,
    /// Confirmed on-chain
    Confirmed,
    /// Rejected by granter
    Rejected,
    /// Cancelled by requester
    Cancelled,
    /// Expired before approval
    Expired,
}

pub struct GrantRequest {
    pub id: u64,
    pub requester_pubkey: Binary,
    pub grantee_address: String,
    pub granter_pubkey: Binary,
    pub granter_address: String,
    pub grant_type: GrantType,
    pub params: GrantRequestParams,
    pub status: RequestStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub tx_hash: Option<String>,
    pub rejection_reason: Option<String>,
}

pub struct GranterInfo {
    pub node_pubkey: Binary,
    pub granter_address: String,
    pub mode: AcceptanceMode,
    pub defaults: GrantDefaults,
    pub active_grants: u32,
    pub total_granted_uakt: u64,
    pub registered_at: Timestamp,
}
```

**Events:**
```rust
// Emitted when a new request is created
#[cw_serde]
pub struct RequestCreatedEvent {
    pub request_id: u64,
    pub requester_pubkey: String,  // hex encoded
    pub granter_pubkey: String,    // hex encoded
    pub grant_type: String,
    pub spend_limit_uakt: u64,
    pub duration_seconds: u64,
}

// Emitted when request status changes
#[cw_serde]
pub struct RequestStatusChangedEvent {
    pub request_id: u64,
    pub old_status: String,
    pub new_status: String,
    pub granter_pubkey: String,
}

// Emitted when grant is confirmed
#[cw_serde]
pub struct GrantConfirmedEvent {
    pub request_id: u64,
    pub granter_address: String,
    pub grantee_address: String,
    pub tx_hash: String,
}
```

## Node Integration

### Granter Node Behavior

The granter node subscribes to contract events and processes requests:

```rust
pub struct GranterService {
    contract_address: String,
    config: GranterConfig,
    custody: Arc<dyn NodeIdentityCustody>,
    tx_broadcaster: TxBroadcaster,
}

impl GranterService {
    /// Handle incoming grant request event
    pub async fn handle_request(&self, event: RequestCreatedEvent) -> Result<()> {
        let request = self.query_request(event.request_id).await?;

        match self.config.mode {
            AcceptanceMode::AcceptAll => {
                // Auto-approve and broadcast
                self.approve_and_broadcast(request).await?;
            }
            AcceptanceMode::RejectAll => {
                self.reject_request(request.id, "Auto-reject mode").await?;
            }
            AcceptanceMode::Whitelist => {
                if self.is_whitelisted(&request.requester_pubkey).await? {
                    self.approve_and_broadcast(request).await?;
                } else {
                    self.reject_request(request.id, "Not whitelisted").await?;
                }
            }
            AcceptanceMode::Manual => {
                // Queue for manual review
                self.queue_for_review(request).await?;
            }
        }

        Ok(())
    }

    /// Approve request and broadcast grant transactions
    async fn approve_and_broadcast(&self, request: GrantRequest) -> Result<()> {
        // 1. Approve in contract
        self.execute_approve(request.id).await?;

        // 2. Build and sign MsgGrant transactions
        let authz_msgs = self.build_authz_grants(&request)?;
        let feegrant_msg = self.build_feegrant(&request)?;

        // 3. Broadcast to chain
        let tx_hash = self.tx_broadcaster
            .broadcast_msgs(vec![authz_msgs, feegrant_msg].concat())
            .await?;

        // 4. Confirm in contract
        self.execute_confirm(request.id, &tx_hash).await?;

        Ok(())
    }
}
```

### Requester Node Behavior

```rust
impl AkashWorkflowManager {
    /// Request grant from another node
    pub async fn request_grant(
        &self,
        granter_pubkey: Option<&[u8]>,
        params: GrantRequestParams,
    ) -> Result<u64> {
        // Find granter if not specified
        let granter = match granter_pubkey {
            Some(pk) => pk.to_vec(),
            None => self.find_available_granter(&params).await?,
        };

        // Submit request to contract
        let request_id = self.execute_request_grant(
            &self.node_pubkey,
            &self.account_address,
            &granter,
            GrantType::AuthzAndFeegrant,
            params,
        ).await?;

        Ok(request_id)
    }

    /// Wait for grant to be confirmed
    pub async fn wait_for_grant(&self, request_id: u64) -> Result<GrantRequest> {
        loop {
            let request = self.query_request(request_id).await?;

            match request.status {
                RequestStatus::Confirmed => return Ok(request),
                RequestStatus::Rejected => {
                    return Err(anyhow!(
                        "Grant request rejected: {}",
                        request.rejection_reason.unwrap_or_default()
                    ));
                }
                RequestStatus::Cancelled | RequestStatus::Expired => {
                    return Err(anyhow!("Grant request cancelled or expired"));
                }
                _ => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}
```

## Workflow Integration

### Extended Workflow Steps

```
[1/16] KeySelection        - Validating selected key
[2/16] BalanceCheck        - Checking AKT balance
[3/16] GrantRequest        - Requesting authz/feegrant (NEW)
[4/16] GrantWait           - Waiting for grant approval (NEW)
[5/16] AuthzSetup          - Verifying deployment permissions
[6/16] FeegrantSetup       - Checking fee allowances
...
```

### Deployment with Grant Request

```bash
# Full deployment with grant request
ergors akash deploy ollama \
  --key deployer \
  --request-grant-from akash1granter... \
  --grant-duration 24h \
  --spend-limit 5akt \
  --grant-purpose "Deploy Ollama for inference testing"
```

The workflow will:
1. Submit grant request to contract
2. Wait for granter node to approve
3. Wait for on-chain confirmation
4. Proceed with normal deployment

## Configuration

### Granter Node Config

```toml
[akash.granter]
# Enable grant service
enabled = true

# Acceptance mode: accept_all, reject_all, whitelist, manual
mode = "whitelist"

# Contract address
contract = "akash1contractaddress..."

# Default limits
[akash.granter.defaults]
max_duration_hours = 48
max_spend_limit_akt = 10
auto_approve_delay_seconds = 0

# Allowed message types
allowed_messages = [
  "/akash.deployment.v1beta3.MsgCreateDeployment",
  "/akash.deployment.v1beta3.MsgUpdateDeployment",
  "/akash.deployment.v1beta3.MsgCloseDeployment",
  "/akash.market.v1beta4.MsgCreateLease",
  "/akash.market.v1beta4.MsgCloseBid",
]

# Whitelisted requesters
[[akash.granter.whitelist]]
pubkey = "abc123..."  # hex-encoded node pubkey
max_spend_akt = 20
max_duration_hours = 72
note = "Trusted team member"

[[akash.granter.whitelist]]
pubkey = "def456..."
# Uses defaults
```

### Requester Node Config

```toml
[akash.grants]
# Preferred granters (tried in order)
preferred_granters = [
  "akash1granter1...",
  "akash1granter2...",
]

# Fallback to any available granter
allow_any_granter = true

# Default request parameters
default_duration_hours = 24
default_spend_limit_akt = 5
```

## Security Considerations

1. **Pubkey Verification**: All requests are signed by the requester's node identity key
2. **Spend Limits**: Granters can cap spending per request and per requester
3. **Message Filtering**: Granters specify exactly which message types to authorize
4. **Expiration**: All grants have mandatory expiration times
5. **Revocation**: Granters can revoke grants at any time via MsgRevoke
6. **Audit Trail**: All requests and approvals logged in contract state

## Contract Deployment

The grant-manager contract should be deployed during network bootstrap:

```bash
# Upload contract code
ergors contract upload grant-manager.wasm --from admin

# Instantiate
ergors contract instantiate <code-id> \
  --label "grant-manager" \
  --admin akash1admin... \
  --msg '{"admin":"akash1admin...","default_mode":"whitelist"}'
```

Contract address is stored in node config and shared across the network.

## Error Handling

| Error | Cause | Resolution |
|-------|-------|------------|
| `GranterNotFound` | Specified granter not registered | Use different granter or let contract find one |
| `NotWhitelisted` | Requester not on granter's whitelist | Request whitelist addition or use different granter |
| `ExceedsLimits` | Request exceeds granter's limits | Reduce duration or spend limit |
| `RequestExpired` | Request timed out | Submit new request |
| `InsufficientFunds` | Granter has insufficient balance | Use different granter |
| `GranterBusy` | Too many pending requests | Wait and retry |

## Metrics

Track grant system health:

- `grant_requests_total{status}` - Total requests by status
- `grant_approval_latency_seconds` - Time from request to approval
- `grant_broadcast_latency_seconds` - Time from approval to on-chain
- `active_grants_count{granter}` - Active grants per granter
- `grant_spend_total_uakt{granter}` - Total spend authorized
