//! Contract-based authentication middleware implementation
//!
//! Uses axum's middleware function pattern for simplicity.

use super::{IsAllowedQuery, IsAllowedResponse};
use crate::ErgorsAppState;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ho_std::traits::HoConfigTrait;
use tracing::{debug, info, warn};

/// Middleware function for contract-based authentication.
///
/// For each request:
/// 1. Extract the endpoint path and caller's public key
/// 2. Check if a custom authenticator is registered for this endpoint
/// 3. If yes, query the contract for authorization
/// 4. If no contract or contract allows, proceed
/// 5. Otherwise, reject the request
pub async fn contract_auth_middleware(
    State(state): State<ErgorsAppState>,
    request: Request,
    next: Next,
) -> Response {
    // Extract endpoint path for authenticator lookup
    let path = request.uri().path().to_string();
    let endpoint_label = normalize_endpoint_path(&path);

    debug!("Contract auth check for endpoint: {}", endpoint_label);

    // Extract public key from headers
    let headers = request.headers().clone();
    let public_key = match extract_header(&headers, "x-public-key") {
        Some(pk) => pk,
        None => {
            // No public key provided - check if contract auth is required
            if let Ok(Some(_)) = state.s.get_authenticator(&endpoint_label).await {
                return unauthorized_response("Missing x-public-key header");
            }
            // No contract registered, proceed
            return next.run(request).await;
        }
    };

    // Check if a custom authenticator is registered for this endpoint
    let authenticator_contract = match state.s.get_authenticator(&endpoint_label).await {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            // No custom authenticator - proceed
            debug!(
                "No authenticator registered for {}, proceeding",
                endpoint_label
            );
            return next.run(request).await;
        }
        Err(e) => {
            warn!(
                "Failed to lookup authenticator for {}: {}",
                endpoint_label, e
            );
            // On error, fall back to proceeding (fail open for lookups)
            return next.run(request).await;
        }
    };

    info!(
        "Using contract authenticator {} for endpoint {}",
        authenticator_contract, endpoint_label
    );

    // Generate caller address from public key
    let caller_address = generate_caller_address(&public_key, &state);

    // Query the authenticator contract
    match query_authenticator_contract(&state, &authenticator_contract, &caller_address).await {
        Ok(true) => {
            debug!(
                "Contract {} authorized {} for {}",
                authenticator_contract, caller_address, endpoint_label
            );
            // Contract approved - proceed with request
            next.run(request).await
        }
        Ok(false) => {
            info!(
                "Contract {} denied {} for {}",
                authenticator_contract, caller_address, endpoint_label
            );
            forbidden_response("Access denied by authenticator contract")
        }
        Err(e) => {
            warn!(
                "Failed to query authenticator contract {}: {}",
                authenticator_contract, e
            );
            // On contract query failure, deny access for security
            internal_error_response(&format!("Authenticator contract query failed: {}", e))
        }
    }
}

/// Normalize endpoint path to a label suitable for authenticator lookup
fn normalize_endpoint_path(path: &str) -> String {
    // Remove leading/trailing slashes and convert to lowercase
    path.trim_matches('/').to_lowercase()
}

/// Extract header value as string
fn extract_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

/// Generate a deterministic address from the public key
fn generate_caller_address(public_key_hex: &str, state: &ErgorsAppState) -> String {
    use sha2::{Digest, Sha256};

    // Generate node identifier from node's public key (first 8 hex chars)
    let node_id = state
        .c
        .identity()
        .public_key
        .as_ref()
        .map(|pk| hex::encode(&pk[..4]))
        .unwrap_or_else(|| "unknown".to_string());

    // Hash the caller's public key to create a short identifier
    let pubkey_bytes = hex::decode(public_key_hex).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&pubkey_bytes);
    let hash = hasher.finalize();

    // Format: ergors{node_id}_{first 20 bytes of hash in hex}
    format!("ergors{}_{}", node_id, hex::encode(&hash[..20]))
}

/// Query an authenticator contract to check if an address is allowed
#[cfg(feature = "cw")]
async fn query_authenticator_contract(
    state: &ErgorsAppState,
    contract_address: &str,
    caller_address: &str,
) -> Result<bool, String> {
    let query_bytes = serde_json::to_vec(&IsAllowedQuery::new(caller_address))
        .map_err(|e| format!("Failed to serialize query: {}", e))?;

    let result = state
        .wasm
        .query_contract(&state.s.cs, contract_address.to_string(), query_bytes)
        .await
        .map_err(|e| format!("Contract query failed: {}", e))?;

    match result {
        cosmwasm_std::ContractResult::Ok(binary) => {
            let response: IsAllowedResponse = serde_json::from_slice(&binary)
                .map_err(|e| format!("Failed to deserialize response: {}", e))?;
            Ok(response.allowed)
        }
        cosmwasm_std::ContractResult::Err(err) => Err(format!("Contract returned error: {}", err)),
    }
}

/// Stub for when CosmWasm feature is not enabled
#[cfg(not(feature = "cw"))]
async fn query_authenticator_contract(
    _state: &ErgorsAppState,
    _contract_address: &str,
    _caller_address: &str,
) -> Result<bool, String> {
    // Without CosmWasm support, always allow (no contract auth possible)
    Ok(true)
}

fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [("content-type", "application/json")],
        format!(r#"{{"error": "Unauthorized", "message": "{}"}}"#, message),
    )
        .into_response()
}

fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        [("content-type", "application/json")],
        format!(r#"{{"error": "Forbidden", "message": "{}"}}"#, message),
    )
        .into_response()
}

fn internal_error_response(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [("content-type", "application/json")],
        format!(
            r#"{{"error": "Internal Server Error", "message": "{}"}}"#,
            message
        ),
    )
        .into_response()
}
