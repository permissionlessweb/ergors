use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Shared API key store — plug into AppState.
pub type ApiKeyStore = Arc<RwLock<HashMap<String, ApiKeyRecord>>>;

pub fn new_store() -> ApiKeyStore {
    Arc::new(RwLock::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub key: String,
    pub provider: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub valid: bool,
    pub usage_count: u64,
}

// ==================== Request / Response types ====================

#[derive(Debug, Deserialize)]
pub struct GenerateKeyRequest {
    pub provider: String,
    #[serde(default)]
    pub expiry_seconds: Option<u64>,
    #[serde(default = "default_valid")]
    pub valid: bool,
}

fn default_valid() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct GenerateKeyResponse {
    api_key: String,
    provider: String,
    expires_at: Option<u64>,
    valid: bool,
}

#[derive(Debug, Deserialize)]
pub struct ValidateKeyRequest {
    pub api_key: String,
}

#[derive(Debug, Serialize)]
struct ValidateKeyResponse {
    valid: bool,
    provider: Option<String>,
    expired: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct RevokeKeyRequest {
    pub api_key: String,
}

// ==================== Handlers ====================

use crate::state::AppState;

pub async fn generate_handler(
    State(state): State<AppState>,
    Json(req): Json<GenerateKeyRequest>,
) -> impl IntoResponse {
    let now = now_secs();

    // Deterministic key format: sk-mock-{provider}-{counter}
    let count = {
        let keys = state.api_keys.read().await;
        keys.len()
    };
    let key = format!("sk-mock-{}-{:024}", req.provider, count);

    let expires_at = req.expiry_seconds.map(|secs| now + secs);

    let record = ApiKeyRecord {
        key: key.clone(),
        provider: req.provider.clone(),
        created_at: now,
        expires_at,
        valid: req.valid,
        usage_count: 0,
    };

    state.api_keys.write().await.insert(key.clone(), record);

    info!("Generated mock API key for provider '{}': {}", req.provider, &key);

    Json(GenerateKeyResponse {
        api_key: key,
        provider: req.provider,
        expires_at,
        valid: req.valid,
    })
}

pub async fn validate_handler(
    State(state): State<AppState>,
    Json(req): Json<ValidateKeyRequest>,
) -> impl IntoResponse {
    let now = now_secs();
    let keys = state.api_keys.read().await;

    match keys.get(&req.api_key) {
        Some(record) => {
            let expired = record.expires_at.map(|exp| now > exp).unwrap_or(false);
            let is_valid = record.valid && !expired;

            let message = if !record.valid {
                "Key marked as invalid"
            } else if expired {
                "Key has expired"
            } else {
                "Key is valid"
            };

            Json(ValidateKeyResponse {
                valid: is_valid,
                provider: Some(record.provider.clone()),
                expired,
                message: message.to_string(),
            })
        }
        None => Json(ValidateKeyResponse {
            valid: false,
            provider: None,
            expired: false,
            message: "Key not found".to_string(),
        }),
    }
}

pub async fn list_handler(State(state): State<AppState>) -> impl IntoResponse {
    let now = now_secs();
    let keys = state.api_keys.read().await;

    let key_list: Vec<serde_json::Value> = keys
        .values()
        .map(|r| {
            let expired = r.expires_at.map(|exp| now > exp).unwrap_or(false);
            serde_json::json!({
                "key": r.key,
                "provider": r.provider,
                "created_at": r.created_at,
                "expires_at": r.expires_at,
                "valid": r.valid && !expired,
                "expired": expired,
                "usage_count": r.usage_count
            })
        })
        .collect();

    Json(serde_json::json!({
        "keys": key_list,
        "total": keys.len()
    }))
}

pub async fn revoke_handler(
    State(state): State<AppState>,
    Json(req): Json<RevokeKeyRequest>,
) -> Response {
    let mut keys = state.api_keys.write().await;

    match keys.get_mut(&req.api_key) {
        Some(record) => {
            record.valid = false;
            info!("Revoked API key: {}", &req.api_key);
            Json(serde_json::json!({
                "success": true,
                "message": "Key revoked successfully"
            }))
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "message": "Key not found"
            })),
        )
            .into_response(),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
