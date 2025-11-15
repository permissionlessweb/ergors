# CW-HOE

A minimal helper orchestration engine (HOE), written in Rust.

## LLM Provider System

The LLM system uses a macro-based approach for zero-hardcoding provider definitions.

### Adding a New Provider

```rust
use crate::llm_entity;

llm_entity! {
    MyProvider {
        name: "my_provider",
        env_key: "MY_PROVIDER_API_KEY",
        base_url: "https://api.myprovider.com/v1",
        models: ["model-1", "model-2"],
        api_type: OpenAICompatible,  // or AnthropicJoint
    }
}
```

### API Key Access

Keys are loaded from:

1. API keys JSON file (`~/api-keys.json`)
2. Environment variables
3. Custody client (future)

Format for `api-keys.json`:

```json
{
  "providers": {
    "openai": {
      "api_key": "${OPENAI_API_KEY}"
    }
  }
}
```

## Development

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

## Trustlessness

- deployment verification and reusable deployment libary

## License

MIT
