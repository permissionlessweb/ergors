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
use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use ho_std::{
    constants::{CONFIG_FILE_NAME, ENCRYPTED_API_KEYS_FILE},
    custody::PasswordEncryptedCustody,
    keys::commonware::NodePrivKey,
    llm::EncryptedApiKeyManager,
    network::auth::validate_admin_signature,
    storage::identity::EncryptedIdentityBuilder,
    traits::{HoConfigTrait, NodeIdentityTrait},
};
use http_body_util::BodyExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::{oneshot, RwLock};
use tracing::{error, info};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};

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
    /// Ephemeral X25519 private key for encrypted transport (lives only in memory)
    session_privkey: X25519Secret,
    /// Corresponding X25519 public key (returned in /sentinel/health)
    session_pubkey: X25519PublicKey,
}

// =============================================================================
// Request/Response types
// =============================================================================

#[derive(Deserialize)]
struct InitRequest {
    custody_password: String,
    #[serde(default)]
    mnemonic: Option<String>,
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
    session_pubkey: String,
}

#[derive(Serialize)]
struct StatusResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Encrypted envelope for sentinel requests.
///
/// Client generates an ephemeral X25519 keypair, performs DH with the
/// server's session pubkey, derives a ChaCha20Poly1305 key via blake3,
/// and encrypts the JSON body. The provider sees only ciphertext.
#[derive(Deserialize)]
struct EncryptedEnvelope {
    /// Client's ephemeral X25519 public key (hex-encoded, 32 bytes)
    ephemeral_pubkey: String,
    /// ChaCha20Poly1305 nonce (hex-encoded, 12 bytes)
    nonce: String,
    /// Encrypted JSON body (hex-encoded)
    ciphertext: String,
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
// Encryption helpers
// =============================================================================

/// Key derivation context for the sentinel encrypted transport.
pub const SENTINEL_KDF_CONTEXT: &str = "ergors sentinel v1";

/// Decrypt an `EncryptedEnvelope` using the server's X25519 private key.
///
/// 1. Parse the client's ephemeral X25519 public key from hex.
/// 2. Compute shared secret via Diffie-Hellman.
/// 3. Derive a 256-bit ChaCha20Poly1305 key using blake3 keyed derivation.
/// 4. Decrypt ciphertext using the derived key + nonce.
fn decrypt_envelope(
    envelope: &EncryptedEnvelope,
    server_privkey: &X25519Secret,
) -> Result<Vec<u8>, String> {
    // Parse client ephemeral pubkey
    let client_pub_bytes: [u8; 32] = hex::decode(&envelope.ephemeral_pubkey)
        .map_err(|_| "invalid ephemeral_pubkey hex")?
        .try_into()
        .map_err(|_| "ephemeral_pubkey must be 32 bytes")?;
    let client_pubkey = X25519PublicKey::from(client_pub_bytes);

    // X25519 Diffie-Hellman
    let shared_secret = server_privkey.diffie_hellman(&client_pubkey);

    // Derive encryption key via blake3
    let derived = blake3::derive_key(SENTINEL_KDF_CONTEXT, shared_secret.as_bytes());
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived));

    // Parse nonce
    let nonce_bytes: [u8; 12] = hex::decode(&envelope.nonce)
        .map_err(|_| "invalid nonce hex")?
        .try_into()
        .map_err(|_| "nonce must be 12 bytes")?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Parse ciphertext
    let ct = hex::decode(&envelope.ciphertext).map_err(|_| "invalid ciphertext hex")?;

    // Decrypt
    cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| "decryption failed — wrong key or tampered ciphertext".to_string())
}

// =============================================================================
// Auth helper
// =============================================================================

/// Extract and validate a signed, encrypted request body.
///
/// 1. Collects body bytes and validates the admin Ed25519 signature (over the
///    outer encrypted envelope).
/// 2. Deserializes the envelope, decrypts the inner JSON via X25519 + ChaCha20.
/// 3. Deserializes the decrypted plaintext into `T`.
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

    // Ed25519 signature covers the outer body (the encrypted envelope)
    validate_admin_signature(&headers, &body_bytes, &app.admin_pubkey_hex)
        .map_err(|e| e.into_response())?;

    // Parse as EncryptedEnvelope — plaintext requests are rejected
    let envelope: EncryptedEnvelope = serde_json::from_slice(&body_bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err(
                "request body must be an encrypted envelope \
                 (fields: ephemeral_pubkey, nonce, ciphertext)",
            )),
        )
            .into_response()
    })?;

    // Decrypt the inner JSON
    let plaintext = decrypt_envelope(&envelope, &app.session_privkey).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err(format!("envelope decrypt failed: {}", e))),
        )
            .into_response()
    })?;

    // Deserialize decrypted plaintext as T
    serde_json::from_slice(&plaintext).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(StatusResponse::err(format!("invalid inner json: {}", e))),
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

        // Generate ephemeral X25519 keypair for encrypted transport
        let session_privkey = X25519Secret::random_from_rng(rand::thread_rng());
        let session_pubkey = X25519PublicKey::from(&session_privkey);

        let shared = Arc::new(AppState {
            admin_pubkey_hex: self.admin_pubkey_hex.clone(),
            home_dir: self.home_dir.clone(),
            state: RwLock::new(SentinelState {
                phase: SentinelPhase::AwaitingInit,
                custody_password: None,
            }),
            shutdown_tx: RwLock::new(Some(shutdown_tx)),
            password_out: password_out.clone(),
            session_privkey,
            session_pubkey,
        });

        let app = Router::new()
            .route("/sentinel/health", get(health_handler))
            .route("/sentinel/init", post(init_handler))
            .route("/sentinel/api-keys", post(api_keys_handler))
            .route("/sentinel/activate", post(activate_handler))
            .with_state(shared);

        // Read ERGORS_API_PORT from env or default to 8080
        let port: u16 = std::env::var("ERGORS_API_PORT")
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
        session_pubkey: hex::encode(app.session_pubkey.as_bytes()),
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

        let result = if let Some(ref phrase) = req.mnemonic {
            // Derive deterministic key from mnemonic via SLIP-0010
            match NodePrivKey::from_mnemonic(phrase) {
                Some(key) => custody.import_identity(&key, &req.custody_password, Some(metadata)),
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(StatusResponse::err("invalid mnemonic phrase")),
                    )
                        .into_response();
                }
            }
        } else {
            // Generate a random keypair (existing behavior)
            custody.create_identity(&req.custody_password, Some(metadata))
        };

        if let Err(e) = result {
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    /// Build an encrypted envelope from the client side.
    ///
    /// Mirrors the client-side protocol: generate ephemeral X25519 keypair,
    /// DH with server pubkey, blake3 KDF, ChaCha20Poly1305 encrypt.
    fn encrypt_for_server(
        plaintext: &[u8],
        server_pubkey: &X25519PublicKey,
    ) -> (String, String, String, X25519Secret) {
        let client_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let client_pubkey = X25519PublicKey::from(&client_secret);

        let shared = client_secret.diffie_hellman(server_pubkey);
        let derived = blake3::derive_key(SENTINEL_KDF_CONTEXT, shared.as_bytes());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived));

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ct = cipher.encrypt(nonce, plaintext).expect("encrypt");

        (
            hex::encode(client_pubkey.as_bytes()),
            hex::encode(nonce_bytes),
            hex::encode(ct),
            client_secret,
        )
    }

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let server_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let server_pubkey = X25519PublicKey::from(&server_secret);

        let plaintext = br#"{"custody_password":"test12345678","host":"0.0.0.0"}"#;
        let (epk, nonce, ct, _client_secret) = encrypt_for_server(plaintext, &server_pubkey);

        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: epk,
            nonce,
            ciphertext: ct,
        };

        let decrypted = decrypt_envelope(&envelope, &server_secret).expect("decrypt should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_server_key_fails() {
        let server_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let server_pubkey = X25519PublicKey::from(&server_secret);

        let plaintext = b"secret data";
        let (epk, nonce, ct, _) = encrypt_for_server(plaintext, &server_pubkey);

        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: epk,
            nonce,
            ciphertext: ct,
        };

        // Decrypt with a different server key — must fail
        let wrong_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let result = decrypt_envelope(&envelope, &wrong_secret);
        assert!(result.is_err(), "decryption with wrong key should fail");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let server_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let server_pubkey = X25519PublicKey::from(&server_secret);

        let plaintext = b"secret data";
        let (epk, nonce, mut ct, _) = encrypt_for_server(plaintext, &server_pubkey);

        // Flip a byte in the ciphertext
        let mut ct_bytes = hex::decode(&ct).unwrap();
        ct_bytes[0] ^= 0xff;
        ct = hex::encode(ct_bytes);

        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: epk,
            nonce,
            ciphertext: ct,
        };

        let result = decrypt_envelope(&envelope, &server_secret);
        assert!(result.is_err(), "tampered ciphertext should fail auth tag check");
    }

    #[test]
    fn invalid_hex_fields_rejected() {
        let server_secret = X25519Secret::random_from_rng(rand::thread_rng());

        // Bad ephemeral pubkey hex
        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: "not-hex".to_string(),
            nonce: hex::encode([0u8; 12]),
            ciphertext: hex::encode(b"data"),
        };
        assert!(decrypt_envelope(&envelope, &server_secret).is_err());

        // Wrong-length pubkey (16 bytes instead of 32)
        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: hex::encode([0u8; 16]),
            nonce: hex::encode([0u8; 12]),
            ciphertext: hex::encode(b"data"),
        };
        assert!(decrypt_envelope(&envelope, &server_secret).is_err());

        // Wrong-length nonce (8 bytes instead of 12)
        let server_pubkey = X25519PublicKey::from(&server_secret);
        let (epk, _, ct, _) = encrypt_for_server(b"data", &server_pubkey);
        let envelope = EncryptedEnvelope {
            ephemeral_pubkey: epk,
            nonce: hex::encode([0u8; 8]),
            ciphertext: ct,
        };
        assert!(decrypt_envelope(&envelope, &server_secret).is_err());
    }

    #[test]
    fn plaintext_body_not_parseable_as_envelope() {
        // Verify that a raw JSON init request does NOT parse as EncryptedEnvelope
        let plaintext_body = r#"{"custody_password":"test12345678","host":"0.0.0.0"}"#;
        let result: Result<EncryptedEnvelope, _> = serde_json::from_str(plaintext_body);
        assert!(
            result.is_err(),
            "plaintext init body must not parse as EncryptedEnvelope"
        );
    }
}
