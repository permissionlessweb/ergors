use {
    camino::Utf8PathBuf,
    directories::ProjectDirs,
    std::{env, path::PathBuf},
};

pub const CONFIG_FILE_NAME: &str = "config.toml";
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
    "AKASH_API_KEY",
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
    let path = ProjectDirs::from("", "", "cw-ho")
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

    // XDG config dir (~/.config/cw-ho/config.toml)
    if let Some(mut dir) = dirs::config_dir() {
        dir.push("cw-ho");
        dir.push("config.toml");
        paths.push(dir);
    }

    // Current dir fallback
    paths.push(
        std::env::current_dir()
            .unwrap_or_default()
            .join("config.toml"),
    );

    // Return first existing path, or default to XDG or local
    paths.into_iter().find(|p| p.exists()).unwrap_or_else(|| {
        // Fallback: use XDG or `./config.toml`
        dirs::config_dir()
            .map(|mut p| {
                p.push("cw-ho");
                p.push("config.toml");
                p
            })
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    })
}

pub fn init_env() {
    let debug_flag = std::env::var("DEBUG_ENV").unwrap_or("0".into());
    match debug_flag.as_str() {
        "0" => {}
        "1" => {
            eprintln!("🔍 [DEBUG_ENV=1] All environment variables:");
            for (key, value) in std::env::vars() {
                eprintln!("  {}={}", key, value);
            }
        }
        _ => {
            eprintln!(
                "🔍 [DEBUG_ENV={}] Whitelisted environment variables:",
                debug_flag
            );
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
        env::set_var("RUST_LOG", "info"); // default log level
    }

    //DEBUG_ENV
}
