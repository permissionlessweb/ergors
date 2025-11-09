use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// TODO: model-cost rate metric constants map. Store, update, & export versioned cost mappings in very optimized manner (bitwise mapping w vecotrized & encoded format)
//
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const DATA_FOLDER_NAME: &str = "memories";
pub const OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
pub const GROK_API_KEY: &str = "GROK_API_KEY";
pub const AKASH_API_KEY: &str = "AKASH_API_KEY";
pub const KIMI_API_KEY: &str = "KIMI_API_KEY";
pub const QWEN_API_KEY: &str = "QWEN_API_KEY";
pub const VENICE_API_KEY: &str = "VENICE_API_KEY";

pub const OPEN_AI: &str = "openai";
pub const ANTHROPIC: &str = "anthropic";
pub const GROK: &str = "grok";
pub const AKASH_CHAT: &str = "akashchat";
pub const KIMI: &str = "kimi";
pub const QUEN: &str = "qwen";
pub const VENICE: &str = "venice";

pub const KIMI_RESEARCH_MODELS: &[&str] = &["kimi_research"];
pub const GROK_MODELS: &[&str] = &["grok"];
pub const OLLAMA_LOCAL_MODELS: &[&str] = &["ollama_local"];

pub const ENV_KEYS: &[&(&str, &str)] = &[
    &(OPEN_AI, OPENAI_API_KEY),
    &(ANTHROPIC, ANTHROPIC_API_KEY),
    &(VENICE, VENICE_API_KEY),
    &(QUEN, QWEN_API_KEY),
    &(VENICE, VENICE_API_KEY),
    &(KIMI, KIMI_API_KEY),
];

// ORCHESTRATION RELATED
pub const AKASH_CHAT_BASE_URL: &str = "https://api.akash.network/chat/v1";
pub const KIMI_RESEARCH_BASE_URL: &str = "https://api.moonshot.cn/v1";
pub const GROK_BASE_URL: &str = "https://api.x.ai/v1";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";

pub const OLLAMA_LOCAL_HOST: &str = "localhost";
pub const OLLAMA_LOCAL_PORT: u16 = 11_434;
// the default recursion depth is deliberately modest – deep recursion
// can explode memory usage if the rest of the pipeline isn’t tuned.
pub const DEFAULT_RECURSION_DEPTH: u32 = 2;
pub const GOLDEN_RATIO: f32 = 1.618033988749894;
pub const TETRAHEDRAL_VERTICES: usize = 4;
pub const FRACTAL_MAX_DEPTH: u32 = 10;
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MiB;

// WORKSPACE RELATED
pub const CNARDIUM_STORAGE: &str = "./data/cnardium";
pub const WORKSPACE: &str = "../../src";
pub const WORKSPACE_HOME: &str = "~/CW-AGENT";
pub const WORKSPACE_ARCHIVE_PATH: &str = "./workspace.tar.gz";

// TOOLS RELATED
pub const TOOLS_LINUX_CONFIGURE: &str = "tools/linux/configure.sh";
pub const TOOLS_SSH_TRANSPORT: &str = "tools/ssh/transport.py";
pub const TOOLS_METAPROMPT_GENERATOR: &str = "/tools/python/prompt_generator.py";

// SSH RELATED
pub const SSH_JSON_PATH: &str = "priv/ssh-config.json";
pub const SSH_TEMPLATE_PATH: &str = "templates/ssh-config.json";
pub const SSH_TEMPLATE_FLAG: &str = "--config templates/ssh-config.json";
pub const DEFAULT_CONFIG_FILE_PATH: &str = "priv/config.toml";

// COMMANDS
pub const CMD_BASH: &str = "bash";
pub const CMD_PYTHON3: &str = "python3";
pub const CMD_WSL: &str = "wsl bash -c";

pub const DEFAULT_PROVIDERS_NODE_ACCESS: &[&str] = &["akash_chat", "anthropic", "grok"];
pub const AKASH_CHAT_MODELS: &[&str] = &[
    "DeepSeek-R1-0528",
    "DeepSeek-R1-Distill-Llama-70B",
    "DeepSeek-R1-Distill-Qwen-14B",
    "DeepSeek-R1-Distill-Qwen-32B",
    "Meta-Llama-3-1-8B-Instruct-FP8",
    "Meta-Llama-3-2-3B-Instruct",
    "Meta-Llama-3-3-70B-Instruct",
    "Meta-Llama-4-Maverick-17B-128E-Instruct-FP8",
    "Qwen3-235B-A22B-Instruct-2507-FP8",
];
pub const OPENAI_MODELS: &[&str] = &[
    "gpt-5-nano",
    "gpt-5",
    "gpt-5-mini",
    "gpt-4o-mini",
    "gpt-4o",
    "gpt-4-turbo",
    "gpt-4",
    "gpt-3.5-turbo",
];
pub const ANTHROPIC_MODELS: &[&str] = &[
    "claude-3-5-sonnet-20240620",
    "claude-3-opus-20240229",
    "claude-3-sonnet-20240229",
    "claude-2.1",
];
pub const QWEN_MODELS: &[&str] = &[
    "claude-3-5-sonnet-20240620",
    "claude-3-opus-20240229",
    "claude-3-sonnet-20240229",
    "claude-2.1",
];
pub const VENICE_MODELS: &[&str] = &[
    "claude-3-5-sonnet-20240620",
    "claude-3-opus-20240229",
    "claude-3-sonnet-20240229",
    "claude-2.1",
];
pub const EXTERNAL_MODELS: &[&str] = &["external"]; // placeholder

// CAPABILITIES: TODO: COMPLETE CAPABILITY DEFINITIONS FOR AGENTIC WORKFLOW
pub const COMMON_CAPS: &[&str] = &["state-sync", "task-coordination", "geometric-ratios"];
pub const EXECUTOR_CAPS: &[&str] = &["code-execution", "sandboxed-env", "task-processing"];
pub const REFEREE_CAPS: &[&str] = &["quality-audit", "compliance-check", "fractal-validation"];
pub const DEVELOPMENT_CAPS: &[&str] = &["development-tools", "debugging", "prototype-testing"];
pub const COORDINATOR_CAPS: &[&str] = &[
    "task-assignment",
    "network-coordination",
    "consensus-participation",
    "tetrahedral-routing",
];
pub const COSMIC_ORCHESTRATION: &str = "cosmic-orchestration";
pub const FRACTAL_RECURSION: &str = "fractal-recursion";
pub const GEOMETRIC_VALIDATION: &str = "geometric-validation";
pub const TETRAHEDRAL_CONNECTIVITY: &str = "tetrahedral-connectivity";
pub const GOLDEN_RATIO_SCALING: &str = "golden-ratio-scaling";

pub mod llm {
    use crate::traits::LlmModelTrait;

    use super::*;
    use crate::prelude::LlmModel;

    impl LlmModelTrait for LlmModel {
        /// (default_model, all_available_models)
        fn models(&self) -> (String, Vec<String>) {
            let all: Vec<String> = match self {
                LlmModel::AkashChat => AKASH_CHAT_MODELS,
                LlmModel::KimiResearch => KIMI_RESEARCH_MODELS,
                LlmModel::Grok => GROK_MODELS,
                LlmModel::OllamaLocal => OLLAMA_LOCAL_MODELS,
                LlmModel::OpenAi => OPENAI_MODELS,
                LlmModel::Anthropic => ANTHROPIC_MODELS,
                LlmModel::Custom { .. } => EXTERNAL_MODELS,
            }
            .iter()
            .map(|s| (*s).to_string())
            .collect();

            (all.first().cloned().unwrap_or_default(), all)
        }
        fn default_base_url(&self) -> String {
            match self {
                LlmModel::AkashChat => AKASH_CHAT_BASE_URL.to_string(),
                LlmModel::KimiResearch => KIMI_RESEARCH_BASE_URL.to_string(),
                LlmModel::Grok => GROK_BASE_URL.to_string(),
                LlmModel::OpenAi => OPENAI_BASE_URL.to_string(),
                LlmModel::Anthropic => ANTHROPIC_BASE_URL.to_string(),
                // Any other variant (e.g., `Custom`) gets an empty string.
                _ => String::new(),
            }
        }
    }
}
