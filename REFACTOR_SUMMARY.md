# LLM Router Refactoring Summary

## Completed: Macro-Based LLM Provider System

### What We Built

Refactored the LLM router from hardcoded provider implementations to a zero-hardcoding, macro-based system that enables declarative provider definitions.

### Architecture

```
llm/
├── mod.rs              - Module exports
├── macros.rs           - llm_entity! macro definition
├── providers.rs        - Provider definitions using macro
├── api_handlers.rs     - Reusable API logic (OpenAICompatible, AnthropicJoint)
├── key_accessor.rs     - API key access abstraction
└── router.rs           - Unified routing entrypoint
```

### Key Components

#### 1. `llm_entity!` Macro

Declarative provider definition with automatic trait implementations:

```rust
llm_entity! {
    OpenAiProvider {
        name: "openai",
        env_key: "OPENAI_API_KEY",
        base_url: "https://api.openai.com/v1",
        models: ["gpt-4", "gpt-3.5-turbo"],
        api_type: OpenAICompatible,
    }
}
```

**Generates**:
- Provider struct
- `LlmProvider` trait implementation
- `ApiKeyProvider` trait implementation
- Registry entry for dynamic discovery

#### 2. API Key Accessor Trait

Abstraction for key management supporting multiple backends:

```rust
pub trait ApiKeyMethod: Send + Sync {
    async fn get_key(&self, provider: &str) -> Result<Option<String>>;
    async fn set_key(&mut self, provider: &str, key: String) -> Result<()>;
    async fn available_providers(&self) -> Vec<String>;
}
```

**Implementations**:
- `EnvKeyAccessor` - Environment variables and JSON config
- `CustodyKeyAccessor` - Placeholder for custody client
- `HybridKeyAccessor` - Fallback chain

#### 3. API Handlers

Reusable request/response logic per API type:

```rust
pub trait ApiHandler {
    async fn handle_request<T: ApiKeyProvider>(
        provider: &T,
        client: &Client,
        request: &PromptRequest,
        base_url: &str,
        provider_name: &str,
    ) -> Result<PromptResponse>;
}
```

**Implementations**:
- `OpenAICompatible` - OpenAI, Grok, Akash, Kimi, Qwen, Venice
- `AnthropicJoint` - Anthropic Claude models

#### 4. Unified Router

Single entrypoint for all LLM inference:

```rust
impl LlmRouter {
    pub async fn handle_request(&self, request: &PromptRequest, model: &str) -> Result<PromptResponse>;
    pub async fn route_to_provider(&self, provider_name: &str, request: &PromptRequest) -> Result<PromptResponse>;
    pub fn get_available_models(&self) -> Vec<String>;
    pub fn get_providers(&self) -> Vec<&str>;
}
```

### Current Providers

1. **OpenAI** - GPT-4, GPT-3.5, etc.
2. **Anthropic** - Claude 3.5, Claude 3, Claude 2.1
3. **Grok** - Grok Beta
4. **Akash Chat** - DeepSeek, Llama, Qwen models
5. **Kimi** - Moonshot models
6. **Qwen** - Qwen Turbo/Plus/Max
7. **Venice** - Llama 3.3/3.1

### Benefits

- ✅ Zero hardcoding - all provider data in macro definitions
- ✅ Automatic registration via `inventory` crate
- ✅ Trait-based polymorphism enables uniform handling
- ✅ Easy to add new providers (just invoke macro)
- ✅ Separation of concerns (handlers vs providers)
- ✅ Future-proof for custody client integration
- ✅ Compile-time provider discovery

### Documentation Updated

- `packages/cw-ho/README.md` - Library usage examples
- `docs/specs/orchestration.md` - Conceptual architecture

### Dependencies Added

```toml
async-trait = { workspace = true }
inventory = "0.3"
```

## Next Step: Encrypted Storage (PROJECT_02)

See `PROJECT_02.md` for the next phase:
- Encrypt API keys with node identity keys
- Store in Cnidarium database
- Network-wide encrypted propagation
- Migration from JSON to encrypted storage

### Why This Matters

Current system loads keys from plaintext JSON/env vars. Next phase encrypts keys at rest using node's Ed25519 identity, stores in database, and enables secure network-wide key distribution.
