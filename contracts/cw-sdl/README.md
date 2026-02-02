# CW-SDL: SDL Template Registrar Contract

A CosmWasm smart contract for storing and managing Akash SDL (Stack Definition Language) templates with variable substitution, deployment result tracking, and workflow chaining.

## Features

- **Template Storage**: Store SDL templates as JSON with automatic validation
- **Variable Validation**: Ensures all template variables have corresponding defaults
- **Variable Management**: Define, update, and remove default values for template variables
- **Variable Substitution**: Render SDL templates by replacing `${VAR}` placeholders with values
- **Access Control**: Admin-based permissions for template modifications
- **Factory Pattern**: Instantiate new contract instances from the same code ID
- **Deployment Result Tracking**: Store key-value results from Akash deployments (peer IDs, endpoints, etc.)
- **Workflow Chaining**: Pass deployment results from parent to child contracts via factory instantiation
- **Child Contract Registry**: Track all factory-spawned contracts by label
- **Query Interface**: Rich query API for template inspection, rendering, and deployment data

## Contract State

The contract maintains:

- SDL template as a JSON string
- Variable defaults as key-value pairs
- Configuration (label, admin address)

## Variable Substitution

Variables in SDL templates follow the format `${VARIABLE_NAME}`. The contract supports:

- **Default Values**: Stored in contract state
- **Custom Values**: Provided at render time (override defaults)
- **Automatic Detection**: Any `${VAR}` pattern in the template
- **Variable Validation**: All variables in template must have defaults (enforced at instantiation and update)

### Variable Naming Rules

Variable names must:

- Be enclosed in `${...}` format
- Contain only alphanumeric characters and underscores
- Be non-empty

### Validation Behavior

**On Instantiation**: The contract validates that every variable in the SDL template has a corresponding default value. If any variables are missing defaults, instantiation fails with `MissingVariableDefaults` error.

**On Template Update**: When updating the template, the contract validates against the current defaults. You must add any new required defaults before updating the template to use new variables.

**Extra Defaults**: Having default values for variables not used in the template is allowed. This supports flexible template updates.

Example template:

```json
{
  "resources": {
    "cpu": {"units": "${CPU}"},
    "memory": {"size": "${MEMORY}"}
  }
}
```

With defaults `{"CPU": "1.0", "MEMORY": "512Mi"}`, renders to:

```json
{
  "resources": {
    "cpu": {"units": "1.0"},
    "memory": {"size": "512Mi"}
  }
}
```

## Usage

### Instantiate

```json
{
  "sdl_template": "{\"version\": \"2.0\", \"cpu\": \"${CPU}\", \"memory\": \"${MEMORY}\"}",
  "variable_defaults": {
    "CPU": "1.0",
    "MEMORY": "512Mi"
  },
  "label": "nginx-template",
  "admin": "akash1..."
}
```

**Important**: All variables in `sdl_template` must have corresponding entries in `variable_defaults`. For example, if your template contains `${CPU}` and `${MEMORY}`, you must provide defaults for both `CPU` and `MEMORY`.

**Example of rejected instantiation**:

```json
{
  "sdl_template": "{\"cpu\": \"${CPU}\", \"memory\": \"${MEMORY}\", \"storage\": \"${STORAGE}\"}",
  "variable_defaults": {
    "CPU": "1.0"
  }
}
```

This will fail with: `MissingVariableDefaults { variables: ["MEMORY", "STORAGE"] }`

### Execute

#### Update Template (Admin)

```json
{"update_template": {"sdl_template": "{...}"}}
```

#### Update Defaults (Admin)

```json
{"update_defaults": {"variable_defaults": {"CPU": "2.0"}}}
```

#### Update Single Default (Admin)

```json
{"update_single_default": {"key": "CPU", "value": "4.0"}}
```

#### Remove Default (Admin)

```json
{"remove_default": {"key": "CPU"}}
```

#### Transfer Admin (Admin)

```json
{"transfer_admin": {"new_admin": "akash1..."}}
```

#### Factory Instantiation

```json
{
  "instantiate_new": {
    "instantiate_msg": {
      "sdl_template": "{...}",
      "variable_defaults": {...},
      "label": "new-template",
      "admin": "akash1..."
    },
    "label": "factory-instance",
    "parent_results": {
      "NODE_A_PEER_ID": "abc123@1.2.3.4:26656",
      "NODE_A_ENDPOINT": "https://node-a.example.com"
    }
  }
}
```

Parent results are merged into the child's `variable_defaults`, allowing deployment chaining where results from one deployment feed variables into the next.

#### Record Deployment Result (Admin)

```json
{"record_deployment_result": {"key": "NODE_A_PEER_ID", "value": "abc123@1.2.3.4:26656"}}
```

#### Record Multiple Results (Admin)

```json
{
  "record_deployment_results": {
    "results": {
      "NODE_A_PEER_ID": "abc123@1.2.3.4:26656",
      "NODE_A_ENDPOINT": "https://node-a.example.com",
      "NODE_A_RPC": "26657"
    }
  }
}
```

### Query

#### Get Template

```json
{"get_template": {}}
```

Returns the SDL template as both string and parsed JSON.

#### Get All Defaults

```json
{"get_defaults": {}}
```

Returns all variable defaults as a HashMap.

#### Get Single Default

```json
{"get_default": {"key": "CPU"}}
```

Returns a specific variable default.

#### List Keys

```json
{"list_keys": {}}
```

Returns all variable keys.

#### Render SDL

```json
{
  "render_sdl": {
    "variables": {"CPU": "2.0"}
  }
}
```

Returns rendered SDL with variables substituted. Provided variables override defaults.

#### Get Info

```json
{"get_info": {}}
```

Returns contract label, admin, and code ID.

#### Get Deployment Result

```json
{"get_deployment_result": {"key": "NODE_A_PEER_ID"}}
```

Returns a specific deployment result value.

#### List Deployment Results

```json
{"list_deployment_results": {}}
```

Returns all stored deployment results as a HashMap.

#### List Child Contracts

```json
{"list_child_contracts": {}}
```

Returns all factory-spawned child contracts as label → address mappings.

## Building

```bash
cargo build --release --target wasm32-unknown-wasm32
```

## Testing

```bash
cargo test
```

## Schema Generation

```bash
cargo schema
```

## Deployment Workflow Chaining

The contract supports sequential deployment workflows where each deployment feeds variables into the next. This is critical for multi-node deployments like the Terp Network O-Line setup:

### Example: O-Line Deployment Flow

```
1. Instantiate cw-sdl-snapshot with NODE_A SDL template
   └─> variable_defaults: {"CPU": "4", "MEMORY": "8Gi", ...}

2. CLI deploys NODE_A to Akash, retrieves peer ID and endpoint

3. CLI calls RecordDeploymentResults on cw-sdl-snapshot:
   └─> {"NODE_A_PEER_ID": "abc123@1.2.3.4:26656", "NODE_A_ENDPOINT": "https://..."}

4. CLI calls InstantiateNew on cw-sdl-snapshot to create cw-sdl-tackle:
   └─> parent_results: cw-sdl-snapshot's deployment results
   └─> Creates cw-sdl-tackle with NODE_A_PEER_ID already in variable_defaults

5. CLI queries cw-sdl-tackle, gets rendered SDL with NODE_A peer ID injected

6. CLI deploys NODE_B to Akash

7. Repeat for subsequent nodes (tackle → forward, etc.)
```

This eliminates manual variable passing between deployments and enables fully automated sequential workflows.

### Key Workflow Patterns

**Parent-to-Child Variable Injection:**
```rust
// Parent contract stores deployment results
RecordDeploymentResults {
  results: {"PEER_ID": "abc@ip:port", "ENDPOINT": "https://..."}
}

// Create child with parent results automatically injected
InstantiateNew {
  instantiate_msg: {...},
  parent_results: Some(parent_deployment_results), // Merged into child's defaults
}
```

**Child Contract Registry:**
All factory-created children are tracked by label, allowing programmatic queries and message routing to the entire contract family.

## Integration

This contract is designed for integration with the Ergors orchestration system for managing Akash deployments. The `cw-ho` CLI tool handles:
- Instantiating SDL contracts
- Deploying rendered SDLs to Akash
- Retrieving deployment results (peer IDs, endpoints)
- Recording results back to contracts via `RecordDeploymentResults`
- Chaining deployments via `InstantiateNew` with parent results

See the main contracts README and `packages/cw-ho` for CLI integration details.
