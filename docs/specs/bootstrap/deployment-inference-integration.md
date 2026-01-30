# Deployment → Inference Integration Specification

Automated integration of Akash deployments into the LLM inference routing system.

## Overview

ERGORS provides seamless integration between Akash deployments and inference routing, enabling deployed services to be used as model endpoints with zero additional configuration. Once a deployment completes, it automatically becomes available as a model for inference requests.

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────┐
│                    ERGORS Engine                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────┐         ┌──────────────────┐         │
│  │  Deployment      │         │  Deployment      │         │
│  │  Workflow        │────────>│  Cache           │         │
│  │                  │  Add    │  (In-Memory)     │         │
│  └──────────────────┘         └──────────────────┘         │
│         │                              │                    │
│         │ Complete                     │ O(1) Lookup       │
│         ↓                              ↓                    │
│  ┌──────────────────┐         ┌──────────────────┐         │
│  │  gRPC Handler    │         │  LLM Router      │         │
│  │  (Lifecycle)     │         │  (Route First)   │         │
│  └──────────────────┘         └──────────────────┘         │
│         │                              ↑                    │
│         │                              │                    │
│         └──────────────────────────────┘                    │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         Cnidarium Storage (Persistence)               │  │
│  │  - akash_labels/{label} → session_id                 │  │
│  │  - akash_active_labels/{label} → session_id          │  │
│  │  - akash_workflows/{session_id} → workflow           │  │
│  └──────────────────────────────────────────────────────┘  │
│                              ↑                               │
│                              │ 30s Refresh                   │
│                    ┌─────────┴──────────┐                   │
│                    │  Background Task   │                   │
│                    └────────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Deployment Creation**

   ```
   User → CLI → gRPC → Workflow Engine → Akash Chain
   ```

2. **Completion & Registration**

   ```
   Workflow Complete → gRPC Handler → Cache.add_deployment()
   ```

3. **Inference Request**

   ```
   HTTP Request → LLM Router → Cache Lookup → Forward to Deployment
   ```

4. **Cache Sync**

   ```
   Background Task (30s) → Storage Query → Cache Refresh
   ```

## Storage Schema

### Label Indices

**Active Labels (collision prevention):**

```
Key:   akash_active_labels/{label}
Value: session_id (UTF-8 string)
Usage: Uniqueness check during creation
```

**Historical Labels (all deployments):**

```
Key:   akash_labels/{label}
Value: session_id (UTF-8 string)
Usage: Historical tracking and debugging
```

**Workflow Data:**

```
Key:   akash_workflows/{session_id}
Value: AkashDeploymentWorkflow (protobuf)
Usage: Full deployment state and metadata
```

### Proto Definitions

**AkashDeploymentWorkflow:**

```protobuf
message AkashDeploymentWorkflow {
  string session_id = 1;
  string label = 35;  // User-defined label
  int32 status = 6;   // Workflow status enum
  repeated AkashServiceEndpoint service_endpoints = 29;
  // ... other fields
}

message AkashServiceEndpoint {
  string service_name = 1;
  string external_uri = 2;      // https://provider.akash:8443
  uint32 internal_port = 3;
  uint32 external_port = 4;
  string protocol = 5;           // "tcp", "http", "https"
}
```

## Implementation Details

### 1. Deployment Cache

**File:** `packages/ho-std/src/llm/deployment_cache.rs`

```rust
pub struct DeploymentProviderCache {
    cache: Arc<RwLock<HashMap<String, DeploymentEndpoint>>>,
}

impl DeploymentProviderCache {
    /// Add deployment to cache when workflow completes
    pub async fn add_deployment(&self, workflow: &AkashDeploymentWorkflow) -> HoResult<()>;

    /// Remove deployment from cache when closed
    pub async fn remove_deployment(&self, label: &str) -> HoResult<()>;

    /// O(1) lookup by model name (label)
    pub async fn get(&self, model_name: &str) -> Option<DeploymentEndpoint>;

    /// List all active deployment model names
    pub async fn list_models(&self) -> Vec<String>;

    /// Refresh cache from storage (30s background task)
    pub async fn refresh<S: StateRead>(&self, storage: &S) -> HoResult<usize>;
}
```

**Key Invariants:**

- Only `Completed` deployments with labels and endpoints are cached
- Cache is always a subset of storage (never stale additions)
- Removal is immediate (no delay)
- Refresh syncs from storage (handles restarts)

### 2. LLM Router Integration

**File:** `packages/ho-std/src/llm/router.rs`

```rust
pub async fn handle_request(&self, req: &PromptRequest, m: &str) -> HoResult<PromptResponse> {
    // PRIORITY 1: Check active Akash deployments by label (O(1))
    if let Some(deployment) = self.deployment_cache.get(m).await {
        return self.route_to_deployment(req, &deployment).await;
    }

    // PRIORITY 2: Check configured providers (OpenAI, Anthropic, etc.)
    let provider = self.find_provider_for_model(m)?;
    provider.call(&self.c, req).await
}
```

**Routing Logic:**

1. **Deployment lookup** - O(1) HashMap by label
2. **Provider lookup** - Linear scan of registered providers
3. **Error** - No match found

**Why deployments first?**

- User-deployed services are more specific than generic providers
- Enables model override (deploy "gpt-4" → uses Akash instead of OpenAI)
- Explicit intent (user created deployment with specific label)

### 3. Lifecycle Hooks

**File:** `packages/cw-ho/src/grpc/management.rs`

**On Completion:**

```rust
// In handle workflow advancement when reaching Complete status
if !workflow.label.is_empty() {
    self.state.r.deployment_cache().add_deployment(&workflow).await?;
    tracing::info!("Added deployment '{}' to inference cache", workflow.label);
}
```

**On Close:**

```rust
// In close_lease and close_deployment handlers
if !workflow.label.is_empty() {
    self.state.r.deployment_cache().remove_deployment(&workflow.label).await?;
    tracing::info!("Removed deployment '{}' from inference cache", workflow.label);
}
```

### 4. Background Refresh

**File:** `packages/cw-ho/src/server.rs`

```rust
let cache_refresh_handle = {
    let storage = self.state.s.clone();
    let cache = self.state.r.deployment_cache();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            match storage.cs.latest_snapshot() {
                snapshot => match cache.refresh(&snapshot).await {
                    Ok(count) if count > 0 => {
                        tracing::debug!("Refreshed deployment cache: {} active", count);
                    }
                    Err(e) => tracing::warn!("Cache refresh failed: {}", e),
                    _ => {}
                }
            }
        }
    })
};
```

**Purpose:**

- Sync cache with storage after restarts
- Handle external updates (e.g., via different node)
- Defensive consistency (detect storage corruption)

**Frequency:** 30 seconds (hardcoded, no configuration)

### 5. Label Collision Prevention

**File:** `packages/cw-ho/src/storage.rs`

```rust
pub async fn check_label_collision(&self, label: &str) -> HoResult<Option<String>> {
    let key = format!("{}/{}", AKASH_ACTIVE_LABELS_PREFIX, label);
    match self.cs.latest_snapshot().get_raw(&key).await? {
        Some(session_id_bytes) => {
            let session_id = String::from_utf8_lossy(&session_id_bytes).to_string();
            Ok(Some(session_id))
        }
        None => Ok(None),
    }
}
```

**Enforcement Point:**

```rust
// In CreateAkashDeployment gRPC handler
if !req.label.is_empty() {
    match self.state.s.check_label_collision(&req.label).await {
        Ok(Some(existing_session_id)) => {
            return Err(Status::already_exists(
                format!("Label '{}' is already in use by active deployment: {}",
                    req.label, existing_session_id)
            ));
        }
        // ... continue if no collision
    }
}
```

## OpenAI Compatibility Layer

### Request Translation

**Input (ERGORS PromptRequest):**

```protobuf
message PromptRequest {
  repeated Message messages = 1;
  optional LlmConfig llm_config = 2;
}
```

**Output (OpenAI ChatCompletion):**

```json
{
  "model": "qwen-inference",
  "messages": [
    {"role": "user", "content": "Hello!"}
  ],
  "temperature": 0.7,
  "max_tokens": 1024,
  "stream": false
}
```

### Response Translation

**Input (OpenAI ChatCompletion):**

```json
{
  "choices": [{
    "message": {
      "content": "Hello! How can I help?"
    }
  }],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 45,
    "total_tokens": 57
  }
}
```

**Output (ERGORS PromptResponse):**

```protobuf
message PromptResponse {
  string provider = 2;           // "akash-deployment:{session_id}"
  string model = 3;              // "qwen-inference"
  repeated string response = 5;  // ["Hello! How can I help?"]
  optional TokenUsage tokens_used = 9;  // {prompt: 12, completion: 45, total: 57}
}
```

### Token Usage Extraction

```rust
let tokens_used = openai_response.get("usage").and_then(|usage| {
    let prompt = usage.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
    let completion = usage.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
    let total = usage.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;

    Some(TokenUsage { prompt, completion, total })
});
```

## HTTP API Extensions

### GET /v1/models

**File:** `packages/cw-ho/src/proxy/endpoints.rs`

**Response Format:**

```json
{
  "object": "list",
  "data": [
    {
      "id": "gpt-4",
      "object": "model",
      "created": 1738276800,
      "owned_by": "openai"
    },
    {
      "id": "claude-3-5-sonnet-20241022",
      "object": "model",
      "created": 1738276800,
      "owned_by": "anthropic"
    },
    {
      "id": "qwen-inference",
      "object": "model",
      "created": 1738276800,
      "owned_by": "akash-deployment"
    }
  ]
}
```

**Implementation:**

```rust
pub async fn handle_list_models(State(state): State<ErgorsAppState>) -> Json<serde_json::Value> {
    let mut models = Vec::new();

    // Get models from configured providers
    for provider in state.r.get_providers() {
        for model in provider.supported_models() {
            models.push(serde_json::json!({
                "id": model,
                "object": "model",
                "created": now,
                "owned_by": provider.name(),
            }));
        }
    }

    // Get models from active deployments
    for deployment_model in state.r.deployment_cache().list_models().await {
        models.push(serde_json::json!({
            "id": deployment_model,
            "object": "model",
            "created": now,
            "owned_by": "akash-deployment",
        }));
    }

    Json(serde_json::json!({ "object": "list", "data": models }))
}
```

## Testing

### Unit Tests

**File:** `packages/ho-std/src/llm/deployment_cache.rs`

1. **test_cache_add_remove** - Basic add/remove operations
2. **test_deployment_inference_integration** - Full lifecycle test
3. **test_label_collision_handling** - Cache replacement behavior

### Integration Test Checklist

- [ ] Create deployment with label → completes successfully
- [ ] Check `/v1/models` → deployment appears in list
- [ ] Send inference request with label → routes to deployment
- [ ] Verify token usage extraction from response
- [ ] Close deployment → removed from models list
- [ ] Send inference request after close → falls back to provider

## Performance Characteristics

| Operation | Complexity | Latency |
|-----------|------------|---------|
| Cache lookup (by label) | O(1) | ~10ns (HashMap) |
| Add to cache | O(1) | ~1μs (write lock) |
| Remove from cache | O(1) | ~1μs (write lock) |
| Refresh from storage | O(n) workflows | ~10-100ms (depends on n) |
| Storage lookup (by label) | O(log n) | ~1-10ms (JMT traversal) |

**Memory Usage:**

- ~200 bytes per cached deployment endpoint
- 1000 deployments = ~200KB memory
- Negligible overhead

## Security Considerations

### Authentication Stripping

**Design:** Deployment-specific authentication (Option C from design phase)

**Implementation:**

```rust
// In route_to_deployment()
let response = self
    .c
    .post(&full_url)
    .json(&openai_request)  // No auth headers
    .send()
    .await?;
```

**Rationale:**

- Each deployment has its own authentication mechanism
- ERGORS doesn't forward user auth headers to deployments
- Future: Deployment-specific auth configuration

### Label Validation

**Current:** No validation (accepts any non-empty string)

**Future Improvements:**

- Restrict to alphanumeric + hyphens (DNS-safe)
- Max length enforcement (e.g., 64 chars)
- Profanity/reserved word filtering

## Future Enhancements

1. **Streaming Support** - Handle SSE responses from deployments
2. **Cost Tracking** - Calculate deployment cost based on lease pricing
3. **Health Checks** - Periodic endpoint health probes
4. **Failover** - Multiple endpoints per deployment for redundancy
5. **Load Balancing** - Round-robin across multiple instances
6. **Metrics** - Request count, latency, error rate per deployment

## Troubleshooting

### Deployment Not Appearing in /v1/models

**Symptoms:** Deployment completed but not listed

**Debug Steps:**

1. Check deployment status: `ergors deploy info <label>`
2. Verify label is set: `label` field not empty
3. Check endpoints exist: `service_endpoints` not empty
4. Query cache directly: `deployment_cache().get(<label>)`
5. Check logs: "Added deployment '...' to inference cache"

**Common Causes:**

- Deployment not in `Completed` status
- No label specified at creation time
- No service endpoints (manifest send failed)
- Cache refresh hasn't run yet (wait 30s)

### Inference Request Routes to Wrong Provider

**Symptoms:** Expected deployment but got OpenAI/Anthropic

**Debug Steps:**

1. Verify exact model name matches label
2. Check deployment is active: `ergors deploy info <label>`
3. Confirm cache contains deployment: Check logs
4. Test with `/v1/models` endpoint

**Common Causes:**

- Typo in model name (case-sensitive)
- Deployment closed/failed
- Label collision (check `akash_active_labels` storage)

### Token Usage Not Tracked

**Symptoms:** `tokens_used` field is None

**Debug Steps:**

1. Check deployment response format
2. Verify OpenAI compatibility
3. Inspect raw response in logs

**Common Causes:**

- Deployment doesn't return `usage` field
- Response format not OpenAI-compatible
- Embedding endpoint (different token field names)

## References

- **CLI Reference:** `/packages/cw-ho/CLI_REFERENCE.md`
- **Akash Deployment Spec:** `/docs/specs/bootstrap/akash-deployment.md`
- **Proto Definitions:** `/proto/ergors/orch/v1/orch.proto`
- **LLM Router:** `/packages/ho-std/src/llm/router.rs`
- **Deployment Cache:** `/packages/ho-std/src/llm/deployment_cache.rs`
