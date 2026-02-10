use crate::constants::*;
use crate::llm_entity;
use crate::traits::*;

// OpenAI Provider
llm_entity! {
    OpenAiProvider {
        name: OPEN_AI,
        env_key:"OPENAI_API_KEY",
        base_url: OPENAI_BASE_URL,
        models:  OPENAI_MODELS,
        api_type: OpenAiJoint,
    }
}

// Anthropic Provider
llm_entity! {
    AnthropicProvider {
        name:ANTHROPIC,
        env_key:"ANTHROPIC_API_KEY",
        base_url: ANTHROPIC_BASE_URL,
        models: ANTHROPIC_MODELS,
        api_type: AnthropticJoint,
    }
}

// Grok Provider
llm_entity! {
    GrokProvider {
        name: GROK,
        env_key: "GROK_API_KEY",
        base_url: GROK_BASE_URL,
        models: GROK_MODELS,
        api_type: OpenAiJoint,
    }
}

// Akash Chat Provider
llm_entity! {
    AkashProvider {
        name: AKASH_CHAT,
        env_key: "AKASHML_KEY",
        base_url: AKASH_CHAT_BASE_URL,
        models:  AKASHML_MODELS,
        api_type: OpenAiJoint,
    }
}

// Kimi Research Provider
llm_entity! {
    KimiProvider {
        name: KIMI,
        env_key: "KIMI_API_KEY",
        base_url: KIMI_RESEARCH_BASE_URL,
        models:  KIMI_RESEARCH_MODELS,
        api_type: OpenAiJoint,
    }
}

// Qwen Provider
llm_entity! {
    QwenProvider {
        name: QUEN,
        env_key: "QWEN_API_KEY",
        base_url: QUEN_BASE_URL,
        models: QWEN_MODELS,
        api_type: OpenAiJoint,
    }
}

// Venice Provider
llm_entity! {
    VeniceProvider {
        name: "venice",
        env_key: "VENICE_API_KEY",
        base_url: "https://api.venice.ai/api/v1",
        models: VENICE_MODELS,
        api_type: OpenAiJoint,
    }
}

// Ollama Provider (OpenAI-compatible local inference)
llm_entity! {
    OllamaProvider {
        name: "ollama",
        env_key: "OLLAMA_API_KEY",
        base_url: "http://127.0.0.1:11434",
        models: ["llama2", "llama3", "mistral", "codellama"],
        api_type: OpenAiJoint,
    }
}
