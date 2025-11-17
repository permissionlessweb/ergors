use crate::{
    middleware::record_operation, ErgorsAppState, ErgorsConfig, ErgorsNetworkManifold,
    ErgorsStorage, LlmRouter,
};
use axum::{
    extract::{Query, State},
    middleware, Json, Router,
};
use commonware_runtime::tokio::Context;
use ho_std::llm::HoError;
use ho_std::{error::error_json, network::AuthLayer};
use ho_std::{
    error::{error_json_detailed, HoResult},
    traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait},
    types::ergors::{orch::v1::*, storage::v1::*},
};
use std::{ops::Deref, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info};
use uuid::Uuid;

pub struct Server {
    state: ErgorsAppState,
}

impl Server {
    pub async fn new(config: ErgorsConfig, context: Context) -> HoResult<Self> {
        config.validate()?;

        let mut nm = ErgorsNetworkManifold::new(config.identity(), context).await;
        nm.start_network(config.network()).await?;

        Ok(Self {
            state: ErgorsAppState::new(
                // r == llm router (app-layer)
                Arc::new(LlmRouter::new(config.llm().deref()).await?),
                // s == storage layer
                Arc::new(ErgorsStorage::new(&config.storage().data_dir).await?),
                // nm == network manifold
                Arc::new(tokio::sync::Mutex::new(nm)),
                // t == time
                Instant::now(),
                // c == config
                config.clone(),
            ),
        })
    }

    pub async fn run(self) -> HoResult<()> {
        // Use the new generic route structure from ho-std
        let (public_router, protected_router) = ho_std::define_routes! {
            public_routes: [
                { path: "/health", method: get, handler: handle_health },
                { path: "/api/prompt", method: post, handler: handle_prompt },
                { path: "/network/topology", method: get, handler: handle_network_topology },
                { path: "/api/operations", method: get, handler: handle_query_operations },
            ],
            protected_routes: [
                { path: "/api/prompts", method: get, handler: handle_query },
                { path: "/orchestrate/bootstrap", method: post, handler: handle_bootstrap },
                { path: "/orchestrate/fractal", method: post, handler: handle_fractal_hoe_creation },
                { path: "/orchestrate/prune", method: post, handler: handle_prune },

                ]
        };
        let server_addr = format!(
            "{}:{}",
            self.state.c.network().listen_address,
            self.state.c.identity().api_port,
        );

        // Build router with operation recording middleware
        let app = Router::new()
            .merge(public_router)
            .merge(protected_router.route_layer(AuthLayer))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .layer(middleware::from_fn_with_state(
                self.state.clone(),
                record_operation,
            ))
            .with_state(self.state);

        axum::serve(TcpListener::bind(&server_addr).await?, app)
            .await
            .map_err(|e| HoError::Cfg(format!("Dayum yo: {}", e)))?;
        Ok(())
    }
}

fn parse_prompt_request(value: serde_json::Value) -> HoResult<PromptRequest> {
    let testing = PromptRequest::default();
    debug!("deafult prompt_request: {:#?}", testing);
    debug!(
        "deafult prompt_request: {:#?}",
        serde_json::to_string(&testing)
    );
    debug!("your provision: {:#?}", serde_json::to_string(&value));
    // Try to deserialize as canonical PromptRequest first
    if let Ok(request) = serde_json::from_value::<PromptRequest>(value.clone()) {
        return Ok(request);
    }

    // If we get here, format is not recognized
    Err(HoError::InvalidRequest(
        "Request must be in the format serializable for `PromptRequest`".to_string(),
    ))
}

async fn handle_fractal_hoe_creation(// State(_state): State<ErgorsAppState>,
    // Json(request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    info!("🌀 Creating fractal hoe");
    //TODO: boostrap new node via desired method
    // Create persistent SSH connection manager
    // let mut ssh_manager = SSHConnectionManager::new(target_node.to_string());
    info!("🔌 Step 1: Establishing persistent SSH connection");
    // match ssh_manager.connect().await {}
    info!("🛠️  Step 2: Installing development environment on target node");
    // ssh_manager.install_dev_environment_via_ssh(&mut ssh_manager).await
    info!("📊  Step 3: Closing SSH connection before returning");
    // Close SSH connection before returning
    // let _ = ssh_manager.close().await;
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}

async fn handle_bootstrap(
    State(..): State<ErgorsAppState>,
    Json(..): Json<BootstrapRequest>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();
    let id = uuid::Uuid::new_v4();

    // TODO: handle bootstrap via method:
    // /ergors.network.v1.bootstrap.types: (transport connections for nodes,bootstrapping types used in functions for traits )
    // Create persistent SSH connection manager
    // info!("🚀 Starting bootstrap process for node: {}", target_node);
    // let mut ssh_manager = SSHConnectionManager::new(target_node.clone());

    // match ssh_manager.bootstrap_node().await {
    //     Ok(bootstrap_summary) => {
    //         info!(
    //             "✅ Bootstrap completed successfully for node: {}",
    //             target_node
    //         );

    //         // Close SSH connection before returning
    //         let _ = ssh_manager.close().await;

    //         let response = BootstrapResponse {
    //             id: id.to_string(),
    //             target_node: target_node.clone(),
    //             status: "success".to_string(),
    //             summary: bootstrap_summary,
    //             timestamp: Some(chrono::Utc::now().into()),
    //             duration_ms: start_time.elapsed().as_millis() as u64,
    //         };

    //         Json(serde_json::to_value(response).unwrap())
    //     }
    //     Err(e) => {
    //         error!("Bootstrap failed for node {}: {}", target_node, e);

    //         // Close SSH connection before returning error
    //         let _ = ssh_manager.close().await;

    //         Json(error_json(
    //             &format!("Bootstrap failed: {}", e),
    //             "BOOTSTRAP_ERROR",
    //         ))
    //     }
    // }
    unimplemented!()
}

async fn handle_prune(
    State(state): State<ErgorsAppState>,
    Json(_request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    //TODO: prune all non-coordinator nodes storage state by bradcasting its cnardium state to up to the coordinator node.
    info!("🔌 Step 1: snapshot, prepend metadata & broadcast to coordinator node");
    info!("🔌 Step 2: Dump snapshot of state and broadcast to coordinator node");
    // match state.storage.create_snapshot().await {
    //     Ok(_) => {}
    //     Err(_e) => return Json(error_json("ErgorsStorage snapshot failed", "STORAGE_ERROR")),
    // };

    // info!("🔌 Step 3: Prune node state");
    // match state.storage.prune_storage().await {
    //     Ok(_) => {}
    //     Err(_e) => return Json(error_json("ErgorsStorage prune failed", "STORAGE_ERROR")),
    // };
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}

async fn handle_prompt(
    State(state): State<ErgorsAppState>,
    Json(raw_request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // Try to deserialize into canonical PromptRequest, accepting flexible formats
    let request = match parse_prompt_request(raw_request) {
        Ok(req) => req,
        Err(e) => {
            error!(
                error = %e,
                "❌ Failed to parse prompt request"
            );
            return Json(error_json(
                &format!("Invalid request format: {}. Expected format: {{\"messages\": [{{\"role\": \"user\", \"content\": \"...\"}}, ...], \"model\": \"gpt-4\"}}", e),
                "INVALID_REQUEST",
            ));
        }
    };

    let prompt = serde_json::to_string(&request.messages).unwrap();

    // Validate request
    if request.messages.is_empty() {
        return Json(error_json(
            "Prompt messages cannot be empty",
            "INVALID_PROMPT",
        ));
    }

    // Route to LLM
    let model = &request.model;

    match state.r.handle_request(&request, model).await {
        Ok(llm_response) => {
            let response = PromptResponse {
                id: Uuid::new_v4().into(),
                prompt,
                response: llm_response.response,
                model: model.to_string(),
                timestamp: None, // TODO: Fix timestamp conversion
                tokens_used: llm_response.tokens_used,
                provider: "default".to_string(), // TODO: get deterministic provider from storage
                cost: Some(0.0),
                latency_ms: None,
                // context: request.context.clone(),
            };

            // Store to Cnidarium with original request context
            if let Err(e) = state
                .s
                .store_prompt_with_context(&response, Some(&request))
                .await
            {
                error!("Failed to store prompt to storage: {}", e);
                // Continue anyway - we don't want to fail the request due to storage issues
            }

            Json(serde_json::to_value(response).unwrap())
        }
        Err(e) => {
            // Log error with full chain if detail enabled
            let error_chain = e.error_chain();
            error!(
                error_type = e.to_string(),
                error = %e,
                error_chain = ?error_chain,
                root_cause = ?error_chain.last(),
                "❌ LLM processing failed"
            );
            // Use detailed error response which respects RUST_LOG_DETAIL env
            Json(error_json_detailed(&e))
        }
    }
}

async fn handle_query(
    State(state): State<ErgorsAppState>,
    Query(query): Query<QueryRequest>,
) -> Json<serde_json::Value> {
    match state.s.query_prompts(&query).await {
        Ok(prompts) => {
            Json(serde_json::to_value(prompts).unwrap_or_else(|_| serde_json::json!([])))
        }
        Err(e) => {
            let error_chain = e.error_chain();
            error!(
                error = %e,
                error_chain = ?error_chain,
                root_cause = ?error_chain.last(),
                "  Query failed"
            );
            Json(error_json_detailed(&e))
        }
    }
}

async fn handle_auth(State(state): State<ErgorsAppState>) -> Json<()> {
    Json(())
}

async fn handle_health(State(state): State<ErgorsAppState>) -> Json<HealthResponse> {
    let uptime = state.t.elapsed().as_secs();

    let storage_status = match state.s.health_check().await {
        Ok(()) => "healthy".to_string(),
        Err(e) => format!("unhealthy: {}", e),
    };

    // Check network status
    let network_status = {
        let topology = state.nm.lock().await.get_topology().await;
        if topology.online_nodes().is_empty() {
            "no peers connected".to_string()
        } else {
            format!("connected ({} peers)", topology.online_nodes().len())
        }
    };

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        storage_status,
        network_status: Some(network_status),
    })
}

async fn handle_network_topology(State(state): State<ErgorsAppState>) -> Json<serde_json::Value> {
    let topology = state.nm.lock().await.get_topology().await;
    let identity = state.c.identity();
    Json(serde_json::json!({
        "topology": topology,
        "node_identity": {
            "node_id": identity.display_id(),
            "node_type": identity.node_type,
            "p2p_address": identity.p2p_address(),
            "api_address": identity.api_address(),
        }
    }))
}

async fn handle_query_operations(
    State(state): State<ErgorsAppState>,
    Query(params): Query<serde_json::Value>,
) -> Json<serde_json::Value> {
    let operation_type = params
        .get("operation_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|l| l as u32);

    match state.s.q_ops(operation_type.as_deref(), limit).await {
        Ok(operations) => Json(serde_json::json!({
            "operations": operations,
            "count": operations.len()
        })),
        Err(e) => {
            let error_chain = e.error_chain();
            error!(
                error = %e,
                error_chain = ?error_chain,
                root_cause = ?error_chain.last(),
                "❌ Failed to query operations"
            );
            Json(error_json_detailed(&e))
        }
    }
}
