use crate::llm::HoResult;
use crate::llm_entity;
use crate::orchestrate::{PromptRequest, PromptResponse};
use async_trait::async_trait;

use crate::traits::llm::ApiJoint;
use reqwest::Client;
// OpenAI Provider
llm_entity! {
    OpenAiProvider {
        name: "openai",
        env_key: "OPENAI_API_KEY",
        base_url: "https://api.openai.com/v1",
        models: [
            "gpt-5-nano",
            "gpt-5",
            "gpt-5-mini",
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4-turbo",
            "gpt-4",
            "gpt-3.5-turbo",
        ],
        api_type: OpenAiJoint,
    }
}

// Anthropic Provider
llm_entity! {
    AnthropicProvider {
        name: "anthropic",
        env_key: "ANTHROPIC_API_KEY",
        base_url: "https://api.anthropic.com/v1",
        models: [
            "claude-3-5-sonnet-20240620",
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
            "claude-2.1",
        ],
        api_type: AnthropticJoint,
    }
}

// Grok Provider
llm_entity! {
    GrokProvider {
        name: "grok",
        env_key: "GROK_API_KEY",
        base_url: "https://api.x.ai/v1",
        models: [
            "grok-beta",
            "grok-vision-beta",
        ],
        api_type: OpenAiJoint,
    }
}

// Akash Chat Provider
llm_entity! {
    AkashProvider {
        name: "akashml",
        env_key: "AKASH_API_KEY",
        base_url: "https://api.akash.network/chat/v1",
        models: [
            "DeepSeek-R1-0528",
            "DeepSeek-R1-Distill-Llama-70B",
            "DeepSeek-R1-Distill-Qwen-14B",
            "DeepSeek-R1-Distill-Qwen-32B",
            "Meta-Llama-3-1-8B-Instruct-FP8",
            "Meta-Llama-3-2-3B-Instruct",
            "Meta-Llama-3-3-70B-Instruct",
            "Meta-Llama-4-Maverick-17B-128E-Instruct-FP8",
            "Qwen3-235B-A22B-Instruct-2507-FP8",
        ],
        api_type: OpenAiJoint,
    }
}

// Kimi Research Provider
llm_entity! {
    KimiProvider {
        name: "kimi",
        env_key: "KIMI_API_KEY",
        base_url: "https://api.moonshot.cn/v1",
        models: [
            "moonshot-v1-8k",
            "moonshot-v1-32k",
            "moonshot-v1-128k",
        ],
        api_type: OpenAiJoint,
    }
}

// Qwen Provider
llm_entity! {
    QwenProvider {
        name: "qwen",
        env_key: "QWEN_API_KEY",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        models: [
            "qwen-turbo",
            "qwen-plus",
            "qwen-max",
        ],
        api_type: OpenAiJoint,
    }
}

// Venice Provider
llm_entity! {
    VeniceProvider {
        name: "venice",
        env_key: "VENICE_API_KEY",
        base_url: "https://api.venice.ai/api/v1",
        models: [
            "llama-3.3-70b",
            "llama-3.1-405b",
        ],
        api_type: OpenAiJoint,
    }
}
