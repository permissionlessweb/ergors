//! Mock Inference Provider
//!
//! A standalone service that simulates inference provider APIs for testing
//! Akash deployments without requiring GPU resources.
//!
//! Supported APIs:
//! - Ollama (`/api/generate`, `/api/chat`, `/api/tags`, `/api/pull`, `/api/show`)
//! - OpenAI (`/v1/completions`, `/v1/chat/completions`, `/v1/models`)
//! - Anthropic (`/v1/messages`)
//! - TGI (`/generate`, `/generate_stream`, `/info`)

mod handlers;
mod state;
#[allow(dead_code)]
mod types;

use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{info, Level};

use state::{AppConfig, AppState};
use types::ModelInfo;

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "mock-inference-provider")]
#[command(about = "Mock inference provider for Akash deployment testing")]
struct Args {
    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value = "11434")]
    port: u16,

    /// Host to bind to
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,

    /// Minimum simulated latency in milliseconds
    #[arg(long, env = "MIN_LATENCY_MS", default_value = "50")]
    min_latency_ms: u64,

    /// Maximum simulated latency in milliseconds
    #[arg(long, env = "MAX_LATENCY_MS", default_value = "200")]
    max_latency_ms: u64,

    /// Error rate (0.0 - 1.0)
    #[arg(long, env = "ERROR_RATE", default_value = "0.0")]
    error_rate: f32,

    /// Model name to report
    #[arg(long, env = "MODEL_NAME", default_value = "llama2")]
    model_name: String,

    /// Enable verbose logging
    #[arg(short, long, env = "VERBOSE")]
    verbose: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let level = if args.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    let models = vec![
        ModelInfo {
            name: args.model_name.clone(),
            model_id: format!("{}:latest", args.model_name),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "llama2:7b".to_string(),
            model_id: "llama2:7b-chat".to_string(),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "mistral".to_string(),
            model_id: "mistral:latest".to_string(),
            size_bytes: 4_500_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
        ModelInfo {
            name: "codellama".to_string(),
            model_id: "codellama:latest".to_string(),
            size_bytes: 4_000_000_000,
            parameter_count: "7B".to_string(),
            quantization: "Q4_0".to_string(),
        },
    ];

    let config = AppConfig {
        min_latency_ms: args.min_latency_ms,
        max_latency_ms: args.max_latency_ms,
        error_rate: args.error_rate,
        models,
    };

    let state = AppState {
        config: Arc::new(config),
        request_count: Arc::new(AtomicU64::new(0)),
        api_keys: handlers::api_keys::new_store(),
        last_request_headers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        // System
        .route("/health", get(handlers::system::health_handler))
        .route("/", get(handlers::system::root_handler))
        .route("/metrics", get(handlers::system::metrics_handler))
        // Ollama
        .route("/api/generate", post(handlers::ollama::generate_handler))
        .route("/api/chat", post(handlers::ollama::chat_handler))
        .route("/api/tags", get(handlers::ollama::tags_handler))
        .route("/api/pull", post(handlers::ollama::pull_handler))
        .route("/api/show", post(handlers::ollama::show_handler))
        .route(
            "/api/embeddings",
            post(handlers::ollama::embeddings_handler),
        )
        // OpenAI
        .route(
            "/v1/completions",
            post(handlers::openai::completions_handler),
        )
        .route(
            "/v1/chat/completions",
            post(handlers::openai::chat_handler),
        )
        .route("/v1/models", get(handlers::openai::models_handler))
        .route(
            "/v1/embeddings",
            post(handlers::openai::embeddings_handler),
        )
        // Anthropic
        .route("/v1/messages", post(handlers::anthropic::messages_handler))
        // API Key Management
        .route("/api/keys/generate", post(handlers::api_keys::generate_handler))
        .route("/api/keys/validate", post(handlers::api_keys::validate_handler))
        .route("/api/keys/list", get(handlers::api_keys::list_handler))
        .route("/api/keys/revoke", post(handlers::api_keys::revoke_handler))
        // TGI
        .route("/generate", post(handlers::tgi::generate_handler))
        .route("/generate_stream", post(handlers::tgi::stream_handler))
        .route("/info", get(handlers::tgi::info_handler))
        // Debug (test introspection)
        .route("/debug/last-headers", get(handlers::debug::last_headers_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse().unwrap();
    info!("Starting mock inference provider on {}", addr);
    info!(
        "Latency range: {}ms - {}ms",
        args.min_latency_ms, args.max_latency_ms
    );
    info!("Error rate: {:.1}%", args.error_rate * 100.0);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
