# Ergors SDL Template Contracts

This workspace contains CosmWasm smart contracts for managing Akash SDL (Stack Definition Language) templates with variable substitution and default value management.

## Contracts

### SDL Template Registrar

A CosmWasm contract that stores SDL templates as JSON with configurable variable defaults. Features include:

- **Template Storage**: Store SDL templates as JSON with validation
- **Variable Validation**: Ensures all template variables have corresponding defaults
- **Variable Management**: Define and update default values for template variables
- **Variable Substitution**: Render SDL templates with custom or default variable values
- **Factory Pattern**: Instantiate new contract instances from the same code ID
- **Access Control**: Admin-based permissions for template updates

## Quick Start with Just

The easiest way to build and test contracts is using the `just` task runner from the project root:

```bash
# Build optimized contracts (production-ready)
just contracts-optimize
# Or use the short alias
just cw

# Test all contracts
just contracts-test
# Or use the short alias
just ct

# Check contracts (faster than build)
just contracts-check

# Generate JSON schemas
just contracts-schema

# Clean contract artifacts
just contracts-clean
```

## Building Manually

Build all contracts in debug mode:

```bash
cd contracts
cargo build --workspace
```

Build a specific contract:

```bash
cd contracts/cw-sdl
cargo build --release --target wasm32-unknown-wasm32
```

## Testing

Run tests for all contracts:

```bash
just contracts-test
# Or manually:
cd contracts && cargo test --workspace
```

Run tests for a specific contract:

```bash
cd contracts/cw-sdl
cargo test
```

## Optimizing for Production

To produce optimized WASM artifacts ready for deployment, use the just command:

```bash
just contracts-optimize
```

This automatically detects your platform (ARM64 or x86_64) and runs the appropriate CosmWasm optimizer Docker image. Optimized `.wasm` files will be in `contracts/artifacts/`.

Manual optimization:

```bash
cd contracts
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.0
```

## Schema Generation

Generate JSON schemas for contract messages:

```bash
cd contracts/cw-sdl
cargo schema
```

Schemas will be output to `schema/` directory.

## Usage

### Instantiation

Instantiate a new SDL template contract:

```json
{
  "sdl_template": "{\"version\": \"2.0\", ...}",
  "variable_defaults": {
    "CPU": "1.0",
    "MEMORY": "512Mi",
    "STORAGE": "1Gi"
  },
  "label": "nginx-template",
  "admin": "akash1..."
}
```

### Execute Messages

#### Update Template (Admin Only)

```json
{
  "update_template": {
    "sdl_template": "{\"version\": \"2.0\", ...}"
  }
}
```

#### Update Variable Defaults (Admin Only)

```json
{
  "update_defaults": {
    "variable_defaults": {
      "CPU": "2.0",
      "MEMORY": "1Gi"
    }
  }
}
```

#### Update Single Default (Admin Only)

```json
{
  "update_single_default": {
    "key": "CPU",
    "value": "4.0"
  }
}
```

#### Instantiate New Contract (Factory)

```json
{
  "instantiate_new": {
    "instantiate_msg": {
      "sdl_template": "{...}",
      "variable_defaults": {...},
      "label": "new-template",
      "admin": "akash1..."
    },
    "label": "factory-instantiated-template"
  }
}
```

### Query Messages

#### Get Template

```json
{
  "get_template": {}
}
```

Returns:
```json
{
  "sdl_template": "{...}",
  "template_json": {...}
}
```

#### Get All Defaults

```json
{
  "get_defaults": {}
}
```

Returns:
```json
{
  "defaults": {
    "CPU": "1.0",
    "MEMORY": "512Mi"
  }
}
```

#### Get Single Default

```json
{
  "get_default": {
    "key": "CPU"
  }
}
```

Returns:
```json
{
  "key": "CPU",
  "value": "1.0"
}
```

#### Render SDL with Variables

```json
{
  "render_sdl": {
    "variables": {
      "CPU": "2.0"
    }
  }
}
```

Returns:
```json
{
  "rendered_sdl": "{...}",
  "used_variables": {
    "CPU": "2.0",
    "MEMORY": "512Mi"
  }
}
```

If `variables` is omitted, all defaults will be used.

#### Get Contract Info

```json
{
  "get_info": {}
}
```

Returns:
```json
{
  "label": "nginx-template",
  "admin": "akash1...",
  "code_id": 123
}
```

## Integration with Ergors

The SDL template contracts are designed to integrate with the Ergors orchestration system:

1. **Contract Discovery**: Ergors nodes query Cnidarium storage for contract addresses using label mapping
2. **Template Loading**: Load SDL templates from contracts with `GetTemplate` query
3. **Variable Configuration**: Retrieve defaults with `GetDefaults` and optionally prompt users
4. **SDL Rendering**: Use `RenderSdl` to generate final SDL with custom or default values
5. **Deployment**: Deploy rendered SDL to Akash Network

## Variable Substitution

Variables in SDL templates use the format `${VARIABLE_NAME}`. All variables must have corresponding defaults defined at instantiation. Example:

```json
{
  "profiles": {
    "compute": {
      "web": {
        "resources": {
          "cpu": {
            "units": "${CPU}"
          },
          "memory": {
            "size": "${MEMORY}"
          }
        }
      }
    }
  }
}
```

When rendered with defaults `{"CPU": "1.0", "MEMORY": "512Mi"}`, produces:

```json
{
  "profiles": {
    "compute": {
      "web": {
        "resources": {
          "cpu": {
            "units": "1.0"
          },
          "memory": {
            "size": "512Mi"
          }
        }
      }
    }
  }
}
```

## License

This project is part of the Ergors ecosystem.
