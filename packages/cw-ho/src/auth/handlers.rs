//! HTTP handlers for authenticator management API
//!
//! These handlers provide CRUD operations for managing custom authenticator
//! contracts registered for API endpoints.

use crate::ErgorsAppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use ho_std::types::ergors::orch::v1::*;
use serde::Deserialize;
use tracing::{info, warn};

/// Query parameters for listing authenticators
#[derive(Deserialize)]
pub struct ListAuthenticatorsParams {
    pub endpoint_prefix: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Register a new authenticator for an endpoint
///
/// POST /auth/register
pub async fn handle_register_authenticator(
    State(state): State<ErgorsAppState>,
    Json(request): Json<RegisterAuthenticatorRequest>,
) -> Json<RegisterAuthenticatorResponse> {
    info!(
        "Registering authenticator for endpoint '{}': {}",
        request.endpoint_label, request.contract_address
    );

    // Validate the contract address format
    if request.contract_address.is_empty() {
        return Json(RegisterAuthenticatorResponse {
            success: false,
            message: "Contract address cannot be empty".to_string(),
            entry: None,
        });
    }

    // Validate the endpoint label format
    if request.endpoint_label.is_empty() {
        return Json(RegisterAuthenticatorResponse {
            success: false,
            message: "Endpoint label cannot be empty".to_string(),
            entry: None,
        });
    }

    // Store metadata if description provided
    if !request.description.is_empty() {
        let metadata = serde_json::json!({
            "description": &request.description,
            "created_at": chrono::Utc::now().to_rfc3339(),
        })
        .to_string();

        if let Err(e) = state
            .s
            .put_authenticator_metadata(&request.endpoint_label, &metadata)
            .await
        {
            warn!(
                "Failed to store authenticator metadata: {} (continuing anyway)",
                e
            );
        }
    }

    // Register the authenticator
    match state
        .s
        .put_authenticator(&request.endpoint_label, &request.contract_address)
        .await
    {
        Ok(_) => {
            let entry = AuthenticatorEntry {
                endpoint_label: request.endpoint_label.clone(),
                contract_address: request.contract_address.clone(),
                description: request.description.clone(),
                created_at: Some(pbjson_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
                active: true,
            };

            Json(RegisterAuthenticatorResponse {
                success: true,
                message: format!(
                    "Authenticator registered for endpoint '{}'",
                    request.endpoint_label
                ),
                entry: Some(entry),
            })
        }
        Err(e) => Json(RegisterAuthenticatorResponse {
            success: false,
            message: format!("Failed to register authenticator: {}", e),
            entry: None,
        }),
    }
}

/// List all registered authenticators
///
/// GET /auth/list
pub async fn handle_list_authenticators(
    State(state): State<ErgorsAppState>,
    Query(params): Query<ListAuthenticatorsParams>,
) -> Json<ListAuthenticatorsResponse> {
    match state.s.list_authenticators().await {
        Ok(authenticators) => {
            let mut entries: Vec<AuthenticatorEntry> = Vec::new();

            for (endpoint_label, contract_address) in authenticators {
                // Filter by prefix if specified
                if let Some(ref prefix) = params.endpoint_prefix {
                    if !endpoint_label.starts_with(prefix) {
                        continue;
                    }
                }

                // Load metadata if available
                let (description, created_at) =
                    match state.s.get_authenticator_metadata(&endpoint_label).await {
                        Ok(Some(metadata_str)) => {
                            if let Ok(metadata) =
                                serde_json::from_str::<serde_json::Value>(&metadata_str)
                            {
                                let desc = metadata
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let created = metadata
                                    .get("created_at")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                    .map(|dt| pbjson_types::Timestamp {
                                        seconds: dt.timestamp(),
                                        nanos: 0,
                                    });
                                (desc, created)
                            } else {
                                (String::new(), None)
                            }
                        }
                        _ => (String::new(), None),
                    };

                entries.push(AuthenticatorEntry {
                    endpoint_label,
                    contract_address,
                    description,
                    created_at,
                    active: true,
                });
            }

            // Apply pagination
            let total_count = entries.len() as u32;
            let offset = params.offset.unwrap_or(0) as usize;
            let limit = params.limit.unwrap_or(100) as usize;

            let paginated: Vec<AuthenticatorEntry> =
                entries.into_iter().skip(offset).take(limit).collect();

            Json(ListAuthenticatorsResponse {
                authenticators: paginated,
                total_count,
            })
        }
        Err(e) => {
            warn!("Failed to list authenticators: {}", e);
            Json(ListAuthenticatorsResponse {
                authenticators: vec![],
                total_count: 0,
            })
        }
    }
}

/// Delete an authenticator
///
/// DELETE /auth/{endpoint_label}
pub async fn handle_delete_authenticator(
    State(state): State<ErgorsAppState>,
    Path(endpoint_label): Path<String>,
) -> Json<DeleteAuthenticatorResponse> {
    info!("Deleting authenticator for endpoint '{}'", endpoint_label);

    // Check if authenticator exists
    match state.s.get_authenticator(&endpoint_label).await {
        Ok(Some(_)) => {
            // Delete the authenticator
            match state.s.delete_authenticator(&endpoint_label).await {
                Ok(_) => Json(DeleteAuthenticatorResponse {
                    success: true,
                    message: format!("Authenticator deleted for endpoint '{}'", endpoint_label),
                }),
                Err(e) => Json(DeleteAuthenticatorResponse {
                    success: false,
                    message: format!("Failed to delete authenticator: {}", e),
                }),
            }
        }
        Ok(None) => Json(DeleteAuthenticatorResponse {
            success: false,
            message: format!("No authenticator found for endpoint '{}'", endpoint_label),
        }),
        Err(e) => Json(DeleteAuthenticatorResponse {
            success: false,
            message: format!("Failed to lookup authenticator: {}", e),
        }),
    }
}

/// Check authorization for an address at an endpoint
///
/// GET /auth/check
pub async fn handle_check_authorization(
    State(state): State<ErgorsAppState>,
    Query(request): Query<AuthorizationCheckRequest>,
) -> Json<AuthorizationCheckResponse> {
    // Get the authenticator contract for this endpoint
    match state.s.get_authenticator(&request.endpoint_label).await {
        Ok(Some(contract_address)) => {
            // Query the contract
            #[cfg(feature = "cw")]
            {
                use super::{IsAllowedQuery, IsAllowedResponse};

                let query_msg = IsAllowedQuery::new(&request.address);
                let query_bytes = match serde_json::to_vec(&query_msg) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        return Json(AuthorizationCheckResponse {
                            authorized: false,
                            reason: format!("Failed to serialize query: {}", e),
                            authenticator_contract: contract_address,
                        });
                    }
                };

                match state
                    .wasm
                    .query_contract(&state.s.cs, contract_address.clone(), query_bytes)
                    .await
                {
                    Ok(result) => match result {
                        cosmwasm_std::ContractResult::Ok(binary) => {
                            match serde_json::from_slice::<IsAllowedResponse>(&binary) {
                                Ok(response) => Json(AuthorizationCheckResponse {
                                    authorized: response.allowed,
                                    reason: if response.allowed {
                                        "Authorized by contract".to_string()
                                    } else {
                                        "Denied by contract".to_string()
                                    },
                                    authenticator_contract: contract_address,
                                }),
                                Err(e) => Json(AuthorizationCheckResponse {
                                    authorized: false,
                                    reason: format!("Invalid contract response: {}", e),
                                    authenticator_contract: contract_address,
                                }),
                            }
                        }
                        cosmwasm_std::ContractResult::Err(err) => Json(AuthorizationCheckResponse {
                            authorized: false,
                            reason: format!("Contract error: {}", err),
                            authenticator_contract: contract_address,
                        }),
                    },
                    Err(e) => Json(AuthorizationCheckResponse {
                        authorized: false,
                        reason: format!("Contract query failed: {}", e),
                        authenticator_contract: contract_address,
                    }),
                }
            }

            #[cfg(not(feature = "cw"))]
            {
                Json(AuthorizationCheckResponse {
                    authorized: false,
                    reason: "CosmWasm support not enabled".to_string(),
                    authenticator_contract: contract_address,
                })
            }
        }
        Ok(None) => Json(AuthorizationCheckResponse {
            authorized: true,
            reason: "No authenticator registered - default allow".to_string(),
            authenticator_contract: String::new(),
        }),
        Err(e) => Json(AuthorizationCheckResponse {
            authorized: false,
            reason: format!("Failed to lookup authenticator: {}", e),
            authenticator_contract: String::new(),
        }),
    }
}
