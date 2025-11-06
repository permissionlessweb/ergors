//! Authentication middleware for route protection

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use commonware_codec::{DecodeExt, Encode};
use commonware_cryptography::Verifier;
use hex::ToHex;
use tracing::{debug, warn};

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing signature header")]
    MissingSignature,
    #[error("Missing timestamp header")]
    MissingTimestamp,
    #[error("Invalid signature format")]
    InvalidSignature,
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Request expired")]
    RequestExpired,
}

impl From<AuthError> for StatusCode {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::MissingSignature | AuthError::MissingTimestamp => StatusCode::UNAUTHORIZED,
            AuthError::InvalidSignature | AuthError::VerificationFailed => StatusCode::FORBIDDEN,
            AuthError::RequestExpired => StatusCode::REQUEST_TIMEOUT,
        }
    }
}

/// Authentication middleware that validates crypto signatures
///
/// Expected headers:
/// - `x-signature`: Ed25519 signature of the request
/// - `x-timestamp`: Unix timestamp to prevent replay attacks
/// - `x-public-key`: Public key for signature verification
///
/// # Example
/// ```rust
/// use axum::{middleware, Router};
/// use ho_std::routes::auth::auth_middleware;
///
/// let protected_router = Router::new()
///     .route("/protected", get(handler))
///     .layer(middleware::from_fn(auth_middleware));
/// ```
pub async fn auth_middleware(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let headers = request.headers();

    // Extract required headers
    let signature =
        extract_header(headers, "x-signature").map_err(|_| AuthError::MissingSignature)?;

    let timestamp =
        extract_header(headers, "x-timestamp").map_err(|_| AuthError::MissingTimestamp)?;

    let public_key =
        extract_header(headers, "x-public-key").map_err(|_| AuthError::MissingSignature)?;

    debug!("Validating request signature for timestamp: {}", timestamp);

    // Validate timestamp (prevent replay attacks)
    validate_timestamp(&timestamp)?;

    // Validate signature
    validate_crypto_signature(&signature, &timestamp, &public_key, &request).await?;

    debug!("Request signature validated successfully");
    Ok(next.run(request).await)
}

/// Extract header value as string
fn extract_header(headers: &HeaderMap, name: &str) -> Result<String, AuthError> {
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .ok_or(AuthError::MissingSignature)
}

/// Validate timestamp to prevent replay attacks
fn validate_timestamp(timestamp_str: &str) -> Result<(), AuthError> {
    let timestamp: u64 = timestamp_str
        .parse()
        .map_err(|_| AuthError::InvalidSignature)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Allow 5 minute window for clock skew
    const MAX_AGE_SECONDS: u64 = 300;

    if now.saturating_sub(timestamp) > MAX_AGE_SECONDS {
        warn!("Request timestamp too old: {} vs {}", timestamp, now);
        return Err(AuthError::RequestExpired);
    }

    if timestamp > now + MAX_AGE_SECONDS {
        warn!(
            "Request timestamp too far in future: {} vs {}",
            timestamp, now
        );
        return Err(AuthError::RequestExpired);
    }

    Ok(())
}

/// Validate crypto signature using Ed25519
async fn validate_crypto_signature(
    signature_hex: &str,
    timestamp: &str,
    public_key_hex: &str,
    request: &Request,
) -> Result<(), AuthError> {
    use commonware_cryptography::ed25519::{PublicKey, Signature};

    // Parse signature from hex
    let signature_bytes = hex::decode(signature_hex).map_err(|_| AuthError::InvalidSignature)?;

    let signature =
        Signature::decode(signature_bytes.as_slice()).map_err(|_| AuthError::InvalidSignature)?;

    // Parse public key from hex
    let public_key_bytes = hex::decode(public_key_hex).map_err(|_| AuthError::InvalidSignature)?;

    let public_key =
        PublicKey::decode(public_key_bytes.as_slice()).map_err(|_| AuthError::InvalidSignature)?;

    // Create message to verify (method + path + timestamp)
    let method = request.method().as_str();
    let path = request.uri().path();
    let message = format!("{}:{}:{}", method, path, timestamp);

    // Verify signature
    if public_key.verify(None, message.as_bytes(), &signature) {
        Ok(())
    } else {
        warn!("Signature verification failed for message: {}", message);
        Err(AuthError::VerificationFailed)
    }
}

/// Configuration for authentication behavior
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Maximum age of requests in seconds
    pub max_request_age: u64,
    /// Whether to require authentication (can be disabled for development)
    pub require_auth: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            max_request_age: 300, // 5 minutes
            require_auth: true,
        }
    }
}

/// Configurable auth middleware that accepts custom settings
pub async fn auth_middleware_with_config(
    config: AuthConfig,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !config.require_auth {
        return Ok(next.run(request).await);
    }

    auth_middleware(request, next).await
}

/// Simple auth middleware that can be disabled for development
pub async fn optional_auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check if auth is disabled via environment variable
    if std::env::var("DISABLE_AUTH").is_ok() {
        return Ok(next.run(request).await);
    }

    auth_middleware(request, next).await
}
