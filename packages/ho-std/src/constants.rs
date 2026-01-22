use {
    camino::{Utf8Path, Utf8PathBuf},
    directories::ProjectDirs,
    std::{env, path::PathBuf},
};

// TODO: model-cost rate metric constants map. Store, update, & export versioned cost mappings in very optimized manner (bitwise mapping w vecotrized & encoded format)
//
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const LLM_API_KEYS_FILE: &str = "api-keys.json";
pub const ENCRYPTED_API_KEYS_FILE: &str = "api-keys.enc";
pub const ENV_VARIABLES_FILE: &str = ".env";
pub const DATA_FOLDER_NAME: &str = "memories";

pub const OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
pub const GROK_API_KEY: &str = "GROK_API_KEY";
pub const AKASHML_KEY: &str = "AKASHML_KEY";
pub const KIMI_API_KEY: &str = "KIMI_API_KEY";
pub const QWEN_API_KEY: &str = "QWEN_API_KEY";
pub const VENICE_API_KEY: &str = "VENICE_API_KEY";

pub const OPEN_AI: &str = "openai";
pub const ANTHROPIC: &str = "anthropic";
pub const GROK: &str = "grok";
pub const AKASH_CHAT: &str = "akashml";
pub const KIMI: &str = "kimi";
pub const QUEN: &str = "qwen";
pub const VENICE: &str = "venice";

pub const KIMI_RESEARCH_MODELS: &[&str] = &["kimi_research"];
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
pub const QUEN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
pub const ANTHROPIC_MESSAGE_URL: &str = "https://api.anthropic.com/v1/messages";

pub const OLLAMA_LOCAL_HOST: &str = "localhost";
pub const OLLAMA_LOCAL_PORT: u16 = 11_434;
// the default recursion depth is deliberately modest – deep recursion
// can explode memory usage if the rest of the pipeline isn’t tuned.
pub const DEFAULT_RECURSION_DEPTH: u32 = 2;
pub const GOLDEN_RATIO: f32 = 1.618_034;
pub const TETRAHEDRAL_VERTICES: usize = 4;
pub const FRACTAL_MAX_DEPTH: u32 = 10;
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MiB;

// WORKSPACE RELATED
pub const cnidarium_STORAGE: &str = "./data/cnidarium";
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

pub const DEFAULT_PROVIDERS_NODE_ACCESS: &[&str] = &["akashml", "anthropic", "grok"];
pub const AKASHML_MODELS: &[&str] = &[
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
pub const ANTHROPIC_MODELS: &[&str] = &["claude-sonnet-4-5", "claude-haiku-4-5", "claude-opus-4-1"];
pub const GROK_MODELS: &[&str] = &[
    "grok-code-fast-1",
    "grok-3-mini",
    "grok-4-1-fast",
    "grok-4-0709",
];

pub const QWEN_MODELS: &[&str] = &[];
pub const VENICE_MODELS: &[&str] = &[];
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

pub const NODE_DATA_PATH: &str = "data";

pub const ALL_ENV_VARS: &[&[&str]] = &[
    CONFIG_ENV_VARS,
    LLM_ENV_VARS,
    ORCHESTRATION_ENV_VARS,
    PLUGIN_ENV_VARS,
];

// list of env variables
pub const CONFIG_ENV_VARS: &[&str] = &[
    "APP_NAME",
    "RUST_LOG",
    "DEBUG_ENV",
    "CONFIG_PATH",
    "API_KEYS_PATH",
    "ENV_PATH",
    "SSH_CONFIG_PATH",
];

pub const LLM_ENV_VARS: &[&str] = &[
    "AKASHML_KEY",
    "KIMI_API_KEY",
    "GROK_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "OLLAMA_PRIMARY_HOST",
    "OLLAMA_SECONDARY_HOST",
];

pub const ORCHESTRATION_ENV_VARS: &[&str] = &[
    "SECRET_KEY",
    "ALLOWED_HOSTS",
    "CORS_ORIGINS",
    "MAX_CONCURRENT_TASKS",
    "DEFAULT_TASK_TIMEOUT",
];

pub const PLUGIN_ENV_VARS: &[&str] = &[
    "DOCKER_HOST",
    "DOCKER_DEFAULT_MEMORY_LIMIT",
    "DOCKER_DEFAULT_CPU_COUNT",
];

pub fn default_home() -> Utf8PathBuf {
    let path = ProjectDirs::from("", "", "ergors")
        .expect("Failed to get platform data dir")
        .data_dir()
        .to_path_buf();
    Utf8PathBuf::from_path_buf(path).expect("Platform default data dir was not UTF-8")
}

pub fn default_config_path() -> PathBuf {
    // Print all env variables
    for (key, value) in env::vars() {
        println!("{}={}", key, value);
    }
    // 1. Check env var
    if let Ok(env_path) = std::env::var("CW_HO_CONFIG") {
        return PathBuf::from(env_path);
    }

    // 2. Check well-known paths
    let mut paths = vec![];

    // Local directory (project root)
    paths.push(PathBuf::from("config.toml"));

    // XDG config dir (~/.config/ergors/config.toml)
    if let Some(mut dir) = dirs::config_dir() {
        dir.push("ergors");
        dir.push("config.toml");
        paths.push(dir);
    }

    paths.push(env::current_dir().unwrap_or_default().join("config.toml"));

    // Return first existing path, or default to XDG or local
    paths.into_iter().find(|p| p.exists()).unwrap_or_else(|| {
        // Fallback: use XDG or `./config.toml`
        dirs::config_dir()
            .map(|mut p| {
                p.push("ergors");
                p.push("config.toml");
                p
            })
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    })
}

pub fn init_env(home_dir: &Utf8Path) -> anyhow::Result<()> {
    let env_file = home_dir.join(".env");
    if env_file.exists() {
        dotenvy::from_path(&env_file).ok();
        eprintln!("📁 Loaded environment variables from: {}", env_file);
    } else {
        eprintln!("⚠️  No .env file found at: {}", env_file);
        eprintln!("   API keys must be set via environment variables");
    }

    match std::env::var("DEBUG_ENV").unwrap_or("0".into()).as_str() {
        "0" => {}
        "1" => {
            eprintln!("🔍 [DEBUG_ENV=1] All environment variables:");
            for (key, value) in std::env::vars() {
                eprintln!("  {}={}", key, value);
            }
        }
        _ => {
            for &var_list in ALL_ENV_VARS.iter() {
                for &key in var_list {
                    if let Ok(value) = env::var(key) {
                        eprintln!("  {}={}", key, value);
                    } else {
                        eprintln!("  {}=❌ (not set)", key);
                    }
                }
            }
        }
    }

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    Ok(())
}
