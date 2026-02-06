//! Sentinel Mode: Zero-secret deployment for Ergors
//!
//! Two-phase startup for headless environments (e.g., Akash). A lightweight
//! HTTP server runs first with only a public admin key, exposing endpoints
//! that require Ed25519-signed requests. After initialization completes,
//! the sentinel hands off to the full Ergors server.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use camino::Utf8PathBuf;
use ho_std::{
    constants::{CONFIG_FILE_NAME, ENCRYPTED_API_KEYS_FILE},
    custody::PasswordEncryptedCustody,
    llm::EncryptedApiKeyManager,
    network::auth::validate_admin_signature,
    storage::identity::EncryptedIdentityBuilder,
    traits::{HoConfigTrait, NodeIdentityTrait},
};
use http_body_util::BodyExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::{oneshot, RwLock};
use tracing::{error, info};

use crate::config::ErgorsConfig;

// =============================================================================
// Types
// =============================================================================

/// Sentinel startup phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SentinelPhase {
    AwaitingInit,
    AwaitingApiKeys,
    AwaitingActivation,
    Activating,
}

/// Internal mutable state for the sentinel server
struct SentinelState {
    phase: SentinelPhase,
    custody_password: Option<String>,
}

/// Shared state passed to axum handlers
struct AppState {
    admin_pubkey_hex: String,
    home_dir: Utf8PathBuf,
    state: RwLock<SentinelState>,
    shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
    password_out: Arc<RwLock<Option<String>>>,
}

// =============================================================================
// Request/Response types
// =============================================================================

#[derive(Deserialize)]
struct InitRequest {
    custody_password: String,
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    api_port: Option<u32>,
    #[serde(default)]
    p2p_port: Option<u32>,
    #[serde(default)]
    host: Option<String>,
}

#[derive(Deserialize)]
struct ApiKeysRequest {
    api_keys: HashMap<String, String>,
}

#[derive(Serialize)]
struct HealthResponse {
    phase: SentinelPhase,
    version: &'static str,
}

#[derive(Serialize)]
struct StatusResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl StatusResponse {
    fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
        }
    }
}

// =============================================================================
// Auth helper
// =============================================================================

/// Extract and validate a signed request body.
///
/// Clones headers, collects body bytes, validates the admin Ed25519 signature,
/// and deserializes the JSON body into `T`.
async fn extract_signed_body<T: DeserializeOwned>(
    app: &AppState,
    request: axum::extract::Request,
) -> Result<T, Response> {
    let headers = request.headers().clone();
    let body_bytes = request
        .into_body()
        .collect()
        .await
        .map(|collected| collected.to_bytes())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(StatusResponse::err("failed to read body")),
            )
                .into_response()
        })?;

    validate_admin_signature(&headers, &body_bytes, &app.admin_pubkey_hex)
        .map_err(|e| e.into_response())?;

    serde_json::from_slice(&body_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err(format!("invalid json: {}", e))),
        )
            .into_response()
    })
}

// =============================================================================
// SentinelServer
// =============================================================================

pub struct SentinelServer {
    admin_pubkey_hex: String,
    home_dir: Utf8PathBuf,
}

impl SentinelServer {
    pub fn new(admin_pubkey_hex: &str, home_dir: Utf8PathBuf) -> Self {
        Self {
            admin_pubkey_hex: admin_pubkey_hex.to_string(),
            home_dir,
        }
    }

    /// Run the sentinel server until activation completes.
    ///
    /// Returns the custody password collected during init, so the caller
    /// can thread it to the full server without using environment variables
    /// (which are unsound in multi-threaded Rust).
    pub async fn run(self) -> anyhow::Result<Option<String>> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let password_out = Arc::new(RwLock::new(None));

        let shared = Arc::new(AppState {
            admin_pubkey_hex: self.admin_pubkey_hex.clone(),
            home_dir: self.home_dir.clone(),
            state: RwLock::new(SentinelState {
                phase: SentinelPhase::AwaitingInit,
                custody_password: None,
            }),
            shutdown_tx: RwLock::new(Some(shutdown_tx)),
            password_out: password_out.clone(),
        });

        let app = Router::new()
            .route("/sentinel/health", get(health_handler))
            .route("/sentinel/init", post(init_handler))
            .route("/sentinel/api-keys", post(api_keys_handler))
            .route("/sentinel/activate", post(activate_handler))
            .with_state(shared);

        // Read API_PORT from env or default to 8080
        let port: u16 = std::env::var("API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        let addr = format!("0.0.0.0:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        info!(
            "Sentinel server listening on {} (admin pubkey: {}...)",
            addr,
            &self.admin_pubkey_hex[..std::cmp::min(16, self.admin_pubkey_hex.len())]
        );

        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                info!("Sentinel shutdown signal received");
            })
            .await?;

        info!("Sentinel server stopped, handing off to full server");
        let pw = password_out.read().await.clone();
        Ok(pw)
    }
}

// =============================================================================
// Handlers
// =============================================================================

/// GET /sentinel/health — no auth required
async fn health_handler(State(app): State<Arc<AppState>>) -> Json<HealthResponse> {
    let state = app.state.read().await;
    Json(HealthResponse {
        phase: state.phase,
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// POST /sentinel/init — creates custody, identity, and config
async fn init_handler(
    State(app): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    // Single write lock: check phase, do work, advance phase — all under one lock.
    let mut state = app.state.write().await;
    if state.phase != SentinelPhase::AwaitingInit {
        return (
            StatusCode::CONFLICT,
            Json(StatusResponse::err(format!(
                "invalid phase: expected awaiting_init, got {:?}",
                state.phase
            ))),
        )
            .into_response();
    }

    let req: InitRequest = match extract_signed_body(&app, request).await {
        Ok(r) => r,
        Err(e) => return e,
    };

    if req.custody_password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err("password must be at least 8 characters")),
        )
            .into_response();
    }

    // Create config
    let mut config = ErgorsConfig::new(app.home_dir.as_path());

    // Apply overrides from request
    {
        let identity = config.identity().clone();
        let mut new_identity = identity;
        if let Some(ref host) = req.host {
            new_identity.host = host.clone();
        }
        if let Some(api_port) = req.api_port {
            new_identity.api_port = api_port;
        }
        if let Some(p2p_port) = req.p2p_port {
            new_identity.p2p_port = p2p_port;
        }
        if let Some(ref node_type) = req.node_type {
            new_identity.node_type = node_type.clone();
        }
        config.set_identity(new_identity);
    }

    // Create encrypted custody + identity
    let identity_path = config.identity_path();
    let custody = PasswordEncryptedCustody::new(&identity_path);

    if !custody.exists() {
        let metadata = EncryptedIdentityBuilder::new()
            .user(config.identity().user.clone())
            .host(config.identity().host.clone())
            .p2p_port(config.identity().p2p_port)
            .api_port(config.identity().api_port)
            .node_type(config.identity().node_type.clone())
            .build();

        if let Err(e) = custody.create_identity(&req.custody_password, Some(metadata)) {
            error!("Failed to create identity: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusResponse::err(format!("failed to create identity: {}", e))),
            )
                .into_response();
        }
    }

    // Save config
    let config_path = app.home_dir.join(CONFIG_FILE_NAME);
    if let Err(e) = config.save(&config_path) {
        error!("Failed to save config: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(StatusResponse::err(format!("failed to save config: {}", e))),
        )
            .into_response();
    }

    info!("Sentinel: identity and config created");

    // Advance phase and store password — still under the same write lock
    state.phase = SentinelPhase::AwaitingApiKeys;
    state.custody_password = Some(req.custody_password);

    (StatusCode::OK, Json(StatusResponse::ok())).into_response()
}

/// POST /sentinel/api-keys — encrypts API keys to api-keys.enc
async fn api_keys_handler(
    State(app): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    // Single write lock for the entire handler
    let mut state = app.state.write().await;
    if state.phase != SentinelPhase::AwaitingApiKeys {
        return (
            StatusCode::CONFLICT,
            Json(StatusResponse::err(format!(
                "invalid phase: expected awaiting_api_keys, got {:?}",
                state.phase
            ))),
        )
            .into_response();
    }

    let req: ApiKeysRequest = match extract_signed_body(&app, request).await {
        Ok(r) => r,
        Err(e) => return e,
    };

    if req.api_keys.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err("api_keys cannot be empty")),
        )
            .into_response();
    }

    // Get custody password from state (still holding write lock)
    let password = match state.custody_password.as_ref() {
        Some(p) => p.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusResponse::err("custody password not set")),
            )
                .into_response()
        }
    };

    // Encrypt and save API keys
    let encrypted_path = app.home_dir.join(ENCRYPTED_API_KEYS_FILE);
    let mut manager = EncryptedApiKeyManager::new();
    if let Err(e) = manager.unlock(&password) {
        error!("Failed to unlock API key manager: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(StatusResponse::err(format!("encryption setup failed: {}", e))),
        )
            .into_response();
    }

    let store = match manager.create_store(&req.api_keys) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to create API key store: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusResponse::err(format!("failed to encrypt keys: {}", e))),
            )
                .into_response();
        }
    };

    let encrypted_bytes = EncryptedApiKeyManager::serialize_store(&store);
    if let Err(e) = std::fs::write(&encrypted_path, &encrypted_bytes) {
        error!("Failed to write encrypted keys: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(StatusResponse::err(format!("failed to write keys: {}", e))),
        )
            .into_response();
    }

    // Set restrictive permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&encrypted_path, std::fs::Permissions::from_mode(0o600));
    }

    info!(
        "Sentinel: {} API key(s) encrypted and saved",
        req.api_keys.len()
    );

    // Advance phase — still under the same write lock
    state.phase = SentinelPhase::AwaitingActivation;

    (StatusCode::OK, Json(StatusResponse::ok())).into_response()
}

/// POST /sentinel/activate — stores password for caller and signals shutdown
async fn activate_handler(
    State(app): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    // Single write lock for the entire handler
    let mut state = app.state.write().await;
    if state.phase != SentinelPhase::AwaitingActivation {
        return (
            StatusCode::CONFLICT,
            Json(StatusResponse::err(format!(
                "invalid phase: expected awaiting_activation, got {:?}",
                state.phase
            ))),
        )
            .into_response();
    }

    // Validate signature (activate body is typically empty `{}`)
    if let Err(e) = extract_signed_body::<serde_json::Value>(&app, request).await {
        return e;
    }

    state.phase = SentinelPhase::Activating;
    let password = state.custody_password.take();

    match password {
        Some(pw) => {
            // Write password to shared output so run() can return it.
            // SAFETY: password_out is only read after server shutdown, so this
            // nested lock (state.write held above) has no deadlock path.
            *app.password_out.write().await = Some(pw);
        }
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusResponse::err("custody password was already consumed")),
            )
                .into_response()
        }
    }

    info!("Sentinel: activation complete, signaling handoff to full server");

    // Signal shutdown (non-blocking)
    let tx = app.shutdown_tx.write().await.take();
    if let Some(tx) = tx {
        let _ = tx.send(());
    }

    (StatusCode::OK, Json(StatusResponse::ok())).into_response()
}
