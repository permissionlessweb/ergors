//! Authentication middleware for route protection

use axum::{
    body::Body,
    extract::Request,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use commonware_codec::DecodeExt;
use commonware_cryptography::{blake3, Hasher, Verifier};
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::{debug, warn};

/// Custom Tower layer for authentication: used to route verification checks for this server.
/// TODO: We need to make this have access to storage client for retrieval of encrypted api keys
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
                Err(_) => return Ok(Auth::MissingSignature.into_response()),
            };

            let timestamp = match extract_header(&headers, "x-timestamp") {
                Ok(ts) => ts,
                Err(_) => return Ok(Auth::MissingTimestamp.into_response()),
            };

            let public_key = match extract_header(&headers, "x-public-key") {
                Ok(pk) => pk,
                Err(_) => return Ok(Auth::MissingSignature.into_response()),
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
                Err(_) => return Ok(Auth::InvalidSignature.into_response()),
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
) -> Result<(), Auth> {
    use commonware_cryptography::ed25519::{PublicKey, Signature};

    // Parse signature from hex
    let signature = Signature::decode(
        hex::decode(signature_hex)
            .map_err(|_| Auth::InvalidSignature)?
            .as_slice(),
    )
    .map_err(|_| Auth::InvalidSignature)?;

    // Parse public key from hex
    let public_key = PublicKey::decode(
        hex::decode(public_key_hex)
            .map_err(|_| Auth::InvalidSignature)?
            .as_slice(),
    )
    .map_err(|_| Auth::InvalidSignature)?;

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
        Err(Auth::VerificationFailed)
    }
}

/// Extract header value as string
fn extract_header(headers: &HeaderMap, name: &str) -> Result<String, Auth> {
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .ok_or(Auth::MissingSignature)
}

/// Validate timestamp to prevent replay attacks
fn validate_timestamp(timestamp_str: &str) -> Result<(), Auth> {
    let timestamp: u64 = timestamp_str.parse().map_err(|_| Auth::InvalidSignature)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Allow 5 minute window for clock skew
    const MAX_AGE_SECONDS: u64 = 300;

    if now.saturating_sub(timestamp) > MAX_AGE_SECONDS {
        warn!("Request timestamp too old: {} vs {}", timestamp, now);
        return Err(Auth::RequestExpired);
    }

    if timestamp > now + MAX_AGE_SECONDS {
        warn!(
            "Request timestamp too far in future: {} vs {}",
            timestamp, now
        );
        return Err(Auth::RequestExpired);
    }

    Ok(())
}

/// Validate an admin-signed request against an expected Ed25519 public key.
///
/// Extracts `x-signature`, `x-timestamp`, `x-public-key` headers, validates
/// the timestamp window, verifies the Ed25519 signature over `H(body||timestamp)`,
/// and confirms the public key matches `expected_pubkey_hex`.
pub fn validate_admin_signature(
    headers: &HeaderMap,
    body: &[u8],
    expected_pubkey_hex: &str,
) -> Result<(), Auth> {
    let signature = extract_header(headers, "x-signature")?;
    let timestamp = extract_header(headers, "x-timestamp")?;
    let public_key = extract_header(headers, "x-public-key")?;

    // Verify the public key matches expected admin key
    if public_key != expected_pubkey_hex {
        warn!("Public key mismatch: expected {}, got {}", expected_pubkey_hex, public_key);
        return Err(Auth::VerificationFailed);
    }

    validate_timestamp(&timestamp)?;
    validate_crypto_signature_with_body(&signature, &timestamp, &public_key, body)?;

    Ok(())
}

use anyhow::Result;
use rand::{CryptoRng, RngCore};

use crate::error::Auth;
use crate::types::{actions::v1::TransactionPlan, ergors::keys::v1::*};

impl TransactionPlan {
    /// Authorize this [`TransactionPlan`] with the provided [`SpendKey`].
    ///
    /// The returned [`AuthorizationData`] can be used to build a [`Transaction`](crate::Transaction).
    pub fn authorize<R: RngCore + CryptoRng>(&self, _rng: R, _sk: &SpendKey) -> Result<()> {
        // ) -> Result<AuthorizationData> {
        // let effect_hash = self.effect_hash(sk.full_viewing_key())?;
        // let mut spend_auths = Vec::new();
        // let mut delegator_vote_auths = Vec::new();
        // let mut lqt_vote_auths = Vec::new();

        // for spend_plan in self.spend_plans() {
        //     let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
        //     let auth_sig = rsk.sign(&mut rng, effect_hash.as_ref());
        //     spend_auths.push(auth_sig);
        // }
        // for delegator_vote_plan in self.delegator_vote_plans() {
        //     let rsk = sk
        //         .spend_auth_key()
        //         .randomize(&delegator_vote_plan.randomizer);
        //     let auth_sig = rsk.sign(&mut rng, effect_hash.as_ref());
        //     delegator_vote_auths.push(auth_sig);
        // }
        // for lqt_vote_plan in self.lqt_vote_plans() {
        //     let rsk = sk.spend_auth_key().randomize(&lqt_vote_plan.randomizer);
        //     let auth_sig = rsk.sign(&mut rng, effect_hash.as_ref());
        //     lqt_vote_auths.push(auth_sig);
        // }
        // Ok(AuthorizationData {
        //     effect_hash: Some(effect_hash),
        //     spend_auths,
        //     delegator_vote_auths,
        //     lqt_vote_auths,
        // })
        Ok(())
    }
}
