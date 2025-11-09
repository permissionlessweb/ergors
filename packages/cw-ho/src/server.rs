use ho_std::{
    prelude::*,
    traits::{HoConfigTrait, NodeIdentityTrait},
    transports::ssh::SSHConnectionManager,
};

use crate::{error::*, AppState, CwHoConfig, CwHoNetworkManifold, CwHoStorage, LlmRouter};
use axum::{
    extract::{Query, State},
    response::Json,
};
use commonware_runtime::tokio::Context;
use std::{ops::Deref, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

pub struct Server {
    state: AppState,
}

impl Server {
    pub async fn new(config: CwHoConfig, context: Context) -> Result<Self> {
        config.validate()?;
        let config_clone = config.clone();
        // STORAGE_INIT
        let storage = Arc::new(CwHoStorage::new(&config.storage().data_dir).await?);
        // LLM_ROUTER_INIT
        let llm_config = config.llm();
        let llm_router = Arc::new(LlmRouter::new(llm_config.deref()).await?);
        // NETWORK MANIFOLD
        let mut network_manifold =
            CwHoNetworkManifold::new(config.identity().clone(), context).await;

        // Start the network
        network_manifold
            .start_network(config.network().clone())
            .await?;
        info!("🌐 Network manager initialized and started");

        let state = AppState {
            storage,
            llm_router,
            network_manifold: Arc::new(tokio::sync::Mutex::new(network_manifold)),
            start_time: Instant::now(),
            config: config_clone,
        };

        Ok(Self { state })
    }

    pub async fn run(self, port: u16) -> Result<()> {
        // Use the new route structure from ho-std
        let app = ho_std::create_routes! {
            public: {
                health => handle_health,
            },
            protected: {
                api_prompts => handle_query,
                orchestrate_bootstrap => handle_bootstrap,
                api_prompt => handle_prompt,
                orchestrate_fractal => handle_fractal_hoe_creation,
                orchestrate_prune => handle_prune,
                network_topology => handle_network_topology,
            }
        }
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(self.state);

        let addr = format!("{}:{}", "0.0.0.0", port);
        info!("🌐 Server listening on {}", addr);

        let listener = TcpListener::bind(&addr).await?;
        axum::serve(listener, app)
            .await
            .map_err(|e| CwHoError::Config(format!("Server error: {}", e)))?;

        Ok(())
    }
}

async fn handle_fractal_hoe_creation(// State(_state): State<AppState>,
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
    State(..): State<AppState>,
    Json(request): Json<BootstrapRequest>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();
    let id = uuid::Uuid::new_v4();
    let target_node = request.target_node.clone();

    // TODO: handle bootstrap via method:
    // /hoe.network.v1.bootstrap.types: (transport connections for nodes,bootstrapping types used in functions for traits )

    // Create persistent SSH connection manager
    info!("🚀 Starting bootstrap process for node: {}", target_node);
    let mut ssh_manager = SSHConnectionManager::new(target_node.clone());

    match ssh_manager.bootstrap_node().await {
        Ok(bootstrap_summary) => {
            info!(
                "✅ Bootstrap completed successfully for node: {}",
                target_node
            );

            // Close SSH connection before returning
            let _ = ssh_manager.close().await;

            let response = BootstrapResponse {
                id: id.to_string(),
                target_node: target_node.clone(),
                status: "success".to_string(),
                summary: bootstrap_summary,
                timestamp: Some(chrono::Utc::now().into()),
                duration_ms: start_time.elapsed().as_millis() as u64,
            };

            Json(serde_json::to_value(response).unwrap())
        }
        Err(e) => {
            error!("Bootstrap failed for node {}: {}", target_node, e);

            // Close SSH connection before returning error
            let _ = ssh_manager.close().await;

            Json(error_json(
                &format!("Bootstrap failed: {}", e),
                "BOOTSTRAP_ERROR",
            ))
        }
    }
}

async fn handle_prune(// State(state): State<AppState>,
    // Json(_request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    //TODO: prune all non-coordinator nodes storage state by bradcasting its cnardium state to up to the coordinator node.
    info!("🔌 Step 1: snapshot, prepend metadata & broadcast to coordinator node");
    info!("🔌 Step 2: Dump snapshot of state and broadcast to coordinator node");
    // match state.storage.create_snapshot().await {
    //     Ok(_) => {}
    //     Err(_e) => return Json(error_json("CwHoStorage snapshot failed", "STORAGE_ERROR")),
    // };

    // info!("🔌 Step 3: Prune node state");
    // match state.storage.prune_storage().await {
    //     Ok(_) => {}
    //     Err(_e) => return Json(error_json("CwHoStorage prune failed", "STORAGE_ERROR")),
    // };
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}

async fn handle_prompt(// State(state): State<AppState>,
    // Json(request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    // let start_time = Instant::now();
    // let id = Uuid::new_v4();

    // let prompt = serde_json::to_string(&request.messages).unwrap();

    // // Validate request
    // if request.messages.is_empty() {
    //     return Json(error_json(
    //         "Prompt messages cannot be empty",
    //         "INVALID_PROMPT",
    //     ));
    // }

    // // Route to LLM
    // let model = &request.model;

    // match state.llm_router.process_request(&request, model).await {
    //     Ok(llm_response) => {
    //         let duration = start_time.elapsed();

    //         let response = PromptResponse {
    //             id: id.into(),
    //             prompt,
    //             response: llm_response.response,
    //             model: model.to_string(),
    //             timestamp: None, // TODO: Fix timestamp conversion
    //             tokens_used: llm_response.tokens_used,
    //             provider: "default".to_string(),
    //             cost: Some(0.0),
    //             latency_ms: Some(duration.as_millis() as u64),
    //             // context: request.context.clone(),
    //         };

    //         // Store to Cnidarium with original request context
    //         if let Err(e) = state
    //             .storage
    //             .store_prompt_with_context(&response, Some(&request))
    //             .await
    //         {
    //             error!("Failed to store prompt to storage: {}", e);
    //             // Continue anyway - we don't want to fail the request due to storage issues
    //         }

    //         Json(serde_json::to_value(response).unwrap())
    //     }
    //     Err(e) => {
    //         error!("LLM processing failed: {}", e);
    //         Json(error_json(
    //             &format!("LLM processing failed: {}", e),
    //             "LLM_ERROR",
    //         ))
    //     }
    // }
    Json(serde_json::to_value("{}").unwrap())
}

async fn handle_query(
    State(state): State<AppState>,
    Query(query): Query<QueryRequest>,
) -> Json<serde_json::Value> {
    match state.storage.query_prompts(&query).await {
        Ok(prompts) => {
            Json(serde_json::to_value(prompts).unwrap_or_else(|_| serde_json::json!([])))
        }
        Err(e) => {
            error!("Query failed: {}", e);
            Json(error_json(&format!("Query failed: {}", e), "QUERY_ERROR"))
        }
    }
}

async fn handle_health(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();

    let storage_status = match state.storage.health_check().await {
        Ok(()) => "healthy".to_string(),
        Err(e) => format!("unhealthy: {}", e),
    };

    // Check network status
    let network_status = {
        let network_manifold = state.network_manifold.lock().await;
        let topology = network_manifold.get_topology().await;
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

async fn handle_network_topology(State(state): State<AppState>) -> Json<serde_json::Value> {
    let network_manifold = state.network_manifold.lock().await;
    let topology = network_manifold.get_topology().await;
    let identity = state.config.identity();
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
