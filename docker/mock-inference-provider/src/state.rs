use crate::handlers::api_keys::ApiKeyStore;
use crate::types::ModelInfo;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub request_count: Arc<AtomicU64>,
    pub api_keys: ApiKeyStore,
    /// Headers from the last OpenAI chat request (for test verification)
    pub last_request_headers: Arc<RwLock<HashMap<String, String>>>,
}

/// Application configuration parsed from CLI args.
pub struct AppConfig {
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub error_rate: f32,
    pub models: Vec<ModelInfo>,
}

/// Simulate network/inference latency.
pub async fn simulate_latency(config: &AppConfig) {
    let (min, max) = (config.min_latency_ms, config.max_latency_ms);
    let latency = if min == max {
        min
    } else {
        min + (rand::random::<u64>() % (max - min))
    };
    tokio::time::sleep(Duration::from_millis(latency)).await;
}

/// Probabilistically return true based on configured error rate.
pub fn should_error(config: &AppConfig) -> bool {
    config.error_rate > 0.0 && rand::random::<f32>() < config.error_rate
}

/// Increment the global request counter.
pub fn bump(state: &AppState) {
    state.request_count.fetch_add(1, Ordering::Relaxed);
}
