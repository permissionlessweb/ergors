use ho_std::bootstrap::BootstrapConfigGenerator;
use ho_std::error::error_json;
use ho_std::traits::NodeIdentityTrait;
use ho_std::types::ergors::network::v1::NodeType;
use ho_std::types::ergors::orch::v1::*;
use ho_std::types::ergors::orch::v1::bootstrap_method::Method;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

use crate::deploy::{BootstrapOrchestrator, NodeBootstrapParams};
use crate::ErgorsAppState;

/// HTTP handler for bootstrap requests
///
/// Initiates node bootstrap via Akash or SSH method.
/// Returns immediately with session ID - client can poll for status.
pub async fn handle_bootstrap(
    State(state): State<ErgorsAppState>,
    Json(request): Json<BootstrapRequest>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();

    info!("🚀 Received bootstrap request");

    // Verify Akash context exists
    let _akash_ctx = match &state.akash {
        Some(ctx) => ctx,
        None => {
            error!("Akash deployment not configured");
            return Json(error_json(
                "Akash deployment not configured on this node",
                "AKASH_NOT_CONFIGURED",
            ));
        }
    };

    // Extract bootstrap method
    let bootstrap_method = match &request.bootstrap_method {
        Some(ref method) => method,
        None => {
            return Json(error_json(
                "bootstrap_method is required",
                "MISSING_METHOD",
            ));
        }
    };

    // Extract node identity/type
    let identity = match &request.identity {
        Some(ref id) => id,
        None => {
            return Json(error_json("identity is required", "MISSING_IDENTITY"));
        }
    };

    let node_type_str = identity.node_type.as_str();
    let node_type = NodeType::from_str_name(node_type_str).unwrap_or(NodeType::Unspecified);

    if matches!(node_type, NodeType::Unspecified) {
        return Json(error_json("Invalid node_type", "INVALID_NODE_TYPE"));
    }

    // Handle based on method
    match &bootstrap_method.method {
        Some(Method::Cloud(_cloud)) => {
            // Check if it's Akash provider
            // Provider is an Option<i32> enum value
            handle_akash_bootstrap(state, node_type, request, start_time).await
        }
        Some(Method::Ssh(_ssh)) => {
            // SSH bootstrap not yet implemented
            Json(error_json(
                "SSH bootstrap not yet implemented - use Akash method",
                "UNIMPLEMENTED",
            ))
        }
        _ => Json(error_json("Unsupported bootstrap method", "INVALID_METHOD")),
    }
}

/// Handle Akash bootstrap method
async fn handle_akash_bootstrap(
    state: ErgorsAppState,
    node_type: NodeType,
    request: BootstrapRequest,
    start_time: Instant,
) -> Json<serde_json::Value> {
    // Use image from env or default
    let image_tag = std::env::var("BOOTSTRAP_IMAGE_TAG")
        .unwrap_or_else(|_| "ghcr.io/permissionlessweb/ergors:latest".to_string());

    // Get our node's P2P address as bootstrap peer and create transport
    let nm = state.nm.lock().await;
    let our_identity = &nm.identity;
    let bootstrap_peer = our_identity.p2p_identity();

    // Create BootstrapTransport from Channel 4 sender - this is safe to do
    // before any peer connects; the transport just needs the sender to address
    // messages later when the bootstrapped node connects via P2P.
    let transport = match nm.create_bootstrap_transport() {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create bootstrap transport: {}", e);
            return Json(error_json(
                &format!("Bootstrap transport unavailable: {}", e),
                "TRANSPORT_ERROR",
            ));
        }
    };
    drop(nm);

    // Get cosmos key info for Akash deployment
    let akash_ctx = state.akash.as_ref().unwrap();
    let key_store = akash_ctx.key_store.read().await;
    let (key_name, account_address) = match key_store.derived_accounts.first() {
        Some(account) => (account.key_name.clone(), account.address.clone()),
        None => {
            return Json(error_json(
                "No cosmos keys configured for Akash deployment",
                "NO_KEYS",
            ));
        }
    };
    drop(key_store);

    // Extract bootstrap peers from request identity, fall back to coordinator's own address
    let bootstrap_peers = if let Some(ref identity) = request.identity {
        if !identity.host.is_empty() {
            vec![format!("{}:{}", identity.host, identity.p2p_port)]
        } else {
            vec![bootstrap_peer]
        }
    } else {
        vec![bootstrap_peer]
    };

    let params = NodeBootstrapParams {
        node_type,
        image_tag,
        bootstrap_peers,
        env_vars: Vec::new(),
        cosmos_key_name: key_name,
        cosmos_account_address: account_address,
        deploy_label: format!("bootstrap-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("node")),
    };

    // Create orchestrator with real transport
    let deployer = Arc::new(akash_ctx.create_deployer(state.s.clone()));
    let transport = Arc::new(tokio::sync::Mutex::new(transport));

    let orchestrator = BootstrapOrchestrator::new(
        deployer,
        BootstrapConfigGenerator::new(),
        Some(transport),
        state.nm.clone(),
        state.s.clone(),
    );

    // Start bootstrap workflow
    match orchestrator.bootstrap_node_akash(params).await {
        Ok(session_id) => {
            info!("✅ Bootstrap session started: {}", session_id);

            // Build simple JSON response (proto mismatch - fix proto definitions later)
            let response = serde_json::json!({
                "id": session_id,
                "status": "in_progress",
                "message": "Bootstrap workflow initiated",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "duration_ms": start_time.elapsed().as_millis() as u64,
            });

            Json(response)
        }
        Err(e) => {
            error!("Bootstrap initiation failed: {}", e);
            Json(error_json(
                &format!("Failed to start bootstrap: {}", e),
                "BOOTSTRAP_ERROR",
            ))
        }
    }
}

/// Query parameters for listing bootstrap sessions
#[derive(Deserialize)]
pub struct ListBootstrapQuery {
    pub active: Option<bool>,
}

/// HTTP handler: List bootstrap sessions
pub async fn handle_list_bootstrap_sessions(
    State(state): State<ErgorsAppState>,
    Query(query): Query<ListBootstrapQuery>,
) -> Json<serde_json::Value> {
    match state.s.list_bootstrap_sessions().await {
        Ok(sessions) => {
            let sessions: Vec<_> = if query.active.unwrap_or(false) {
                sessions
                    .into_iter()
                    .filter(|s| !s.is_terminal())
                    .collect()
            } else {
                sessions
            };

            let list: Vec<serde_json::Value> = sessions
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "session_id": s.session_id,
                        "step": s.status_string(),
                        "node_type": s.target_node_type.as_str_name(),
                        "akash_dseq": s.akash_dseq,
                        "p2p_connected": s.p2p_connected,
                        "created_at": s.created_at.to_rfc3339(),
                        "updated_at": s.updated_at.to_rfc3339(),
                        "is_complete": s.is_complete(),
                        "is_failed": s.is_failed(),
                    })
                })
                .collect();

            Json(serde_json::json!({ "sessions": list, "count": list.len() }))
        }
        Err(e) => Json(error_json(
            &format!("Failed to list sessions: {}", e),
            "STORAGE_ERROR",
        )),
    }
}

/// HTTP handler: Get bootstrap session status
pub async fn handle_bootstrap_status(
    State(state): State<ErgorsAppState>,
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.s.load_bootstrap_state(&session_id).await {
        Ok(Some(session)) => Json(serde_json::json!({
            "session_id": session.session_id,
            "step": session.status_string(),
            "node_type": session.target_node_type.as_str_name(),
            "docker_image_tag": session.docker_image_tag,
            "generated_pubkey": session.generated_identity_pubkey,
            "akash_session_id": session.akash_session_id,
            "akash_dseq": session.akash_dseq,
            "akash_provider": session.akash_provider,
            "akash_endpoints": session.akash_endpoints,
            "p2p_connected": session.p2p_connected,
            "p2p_check_attempts": session.p2p_check_attempts,
            "bootstrap_peer": session.bootstrap_peer,
            "errors": session.errors,
            "created_at": session.created_at.to_rfc3339(),
            "updated_at": session.updated_at.to_rfc3339(),
            "is_complete": session.is_complete(),
            "is_failed": session.is_failed(),
        })),
        Ok(None) => Json(error_json(
            &format!("Session not found: {}", session_id),
            "NOT_FOUND",
        )),
        Err(e) => Json(error_json(
            &format!("Failed to load session: {}", e),
            "STORAGE_ERROR",
        )),
    }
}

/// HTTP handler: Delete bootstrap session
pub async fn handle_delete_bootstrap_session(
    State(state): State<ErgorsAppState>,
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    // Check session exists first
    match state.s.load_bootstrap_state(&session_id).await {
        Ok(Some(_)) => {
            match state.s.delete_bootstrap_session(&session_id).await {
                Ok(()) => Json(serde_json::json!({
                    "deleted": true,
                    "session_id": session_id,
                })),
                Err(e) => Json(error_json(
                    &format!("Failed to delete session: {}", e),
                    "DELETE_ERROR",
                )),
            }
        }
        Ok(None) => Json(error_json(
            &format!("Session not found: {}", session_id),
            "NOT_FOUND",
        )),
        Err(e) => Json(error_json(
            &format!("Failed to check session: {}", e),
            "STORAGE_ERROR",
        )),
    }
}
