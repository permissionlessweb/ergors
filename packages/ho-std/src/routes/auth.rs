//! Authentication middleware for route protection

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use commonware_codec::DecodeExt;
use commonware_cryptography::{blake3, Hasher, Verifier};
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use std::task::{Context, Poll};
use tower::{Layer, Service};
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

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        use axum::http::StatusCode;
        use serde::Serialize;

        #[derive(Serialize)]
        struct ErrorResponse {
            error: String,
        }

        let (status, message) = match self {
            AuthError::MissingSignature | AuthError::MissingTimestamp => {
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            AuthError::InvalidSignature | AuthError::VerificationFailed => {
                (StatusCode::FORBIDDEN, self.to_string())
            }
            AuthError::RequestExpired => (StatusCode::REQUEST_TIMEOUT, self.to_string()),
        };

        (status, axum::Json(ErrorResponse { error: message })).into_response()
    }
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

/// Custom Tower layer for authentication
#[derive(Clone)]
pub struct AuthLayer;

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware { inner }
    }
}

/// Auth middleware service
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
}

impl<S> Service<Request> for AuthMiddleware<S>
where
    S: Service<Request, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Move the inner service into the future
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract headers
            let headers = request.headers().clone();

            let signature = match extract_header(&headers, "x-signature") {
                Ok(sig) => sig,
                Err(_) => return Ok(AuthError::MissingSignature.into_response()),
            };

            let timestamp = match extract_header(&headers, "x-timestamp") {
                Ok(ts) => ts,
                Err(_) => return Ok(AuthError::MissingTimestamp.into_response()),
            };

            let public_key = match extract_header(&headers, "x-public-key") {
                Ok(pk) => pk,
                Err(_) => return Ok(AuthError::MissingSignature.into_response()),
            };

            // Validate timestamp
            debug!("Validating request signature for timestamp: {}", timestamp);
            if let Err(e) = validate_timestamp(&timestamp) {
                return Ok(e.into_response());
            }

            // Collect body to include in signature validation
            let (parts, body) = request.into_parts();
            let body_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return Ok(AuthError::InvalidSignature.into_response()),
            };

            // Validate signature with body contents
            if let Err(e) = validate_crypto_signature_with_body(
                &signature,
                &timestamp,
                &public_key,
                &body_bytes,
            ) {
                return Ok(e.into_response());
            }

            debug!("Request signature validated successfully");

            // Reconstruct request with body for inner service
            let request = Request::from_parts(parts, Body::from(body_bytes));

            // Call inner service with validated request
            inner.call(request).await
        })
    }
}

/// Validate crypto signature with body contents included
fn validate_crypto_signature_with_body(
    signature_hex: &str,
    timestamp: &str,
    public_key_hex: &str,
    body_bytes: &[u8],
) -> Result<(), AuthError> {
    use commonware_cryptography::ed25519::{PublicKey, Signature};

    // Parse signature from hex
    let signature = Signature::decode(
        hex::decode(signature_hex)
            .map_err(|_| AuthError::InvalidSignature)?
            .as_slice(),
    )
    .map_err(|_| AuthError::InvalidSignature)?;

    // Parse public key from hex
    let public_key = PublicKey::decode(
        hex::decode(public_key_hex)
            .map_err(|_| AuthError::InvalidSignature)?
            .as_slice(),
    )
    .map_err(|_| AuthError::InvalidSignature)?;

    // Create message to verify: H(body||timestamp)
    let mut contents = Vec::new();
    contents.extend_from_slice(body_bytes);
    contents.extend_from_slice(timestamp.as_bytes());
    let message = blake3::Blake3::hash(&contents);

    // Verify signature
    if public_key.verify(None, &message, &signature) {
        Ok(())
    } else {
        warn!("Signature verification failed");
        Err(AuthError::VerificationFailed)
    }
}
