# SDL Template Registrar Contract

A CosmWasm smart contract for storing and managing Akash SDL (Stack Definition Language) templates with variable substitution and default value management.

## Features

- **Template Storage**: Store SDL templates as JSON with automatic validation
- **Variable Validation**: Ensures all template variables have corresponding defaults
- **Variable Management**: Define, update, and remove default values for template variables
- **Variable Substitution**: Render SDL templates by replacing `${VAR}` placeholders with values
- **Access Control**: Admin-based permissions for template modifications
- **Factory Pattern**: Instantiate new contract instances from the same code ID
- **Query Interface**: Rich query API for template inspection and rendering

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
    "label": "factory-instance"
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

## Integration

This contract is designed for integration with the Ergors orchestration system for managing Akash deployments. See the main contracts README for integration details.
