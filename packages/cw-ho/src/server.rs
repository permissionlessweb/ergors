use crate::{
    middleware::record_operation, storage::ErgorsStorage, ErgorsAppState, ErgorsConfig,
    ErgorsNetworkManifold, LlmRouter,
};
use axum::{
    extract::{Query, State},
    middleware, Json, Router,
};
use commonware_cryptography::{blake3, Hasher};
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
use tracing::{error, info};
use uuid::Uuid;

pub struct Server {
    state: ErgorsAppState,
}

impl Server {
    pub async fn new(c: ErgorsConfig, ctx: Context) -> HoResult<Self> {
        Self::validate_llm_api_keys(&c)?;
        c.validate()?;

        let mut nm = ErgorsNetworkManifold::new(c.identity(), ctx).await;
        let s = ErgorsStorage::new(&c.storage().data_dir).await?;
        nm.start_network(c.network()).await?;

        // Encrypt and store API keys on server startup
        // Self::encrypt_and_store_api_keys(&c, &s).await?;

        Ok(Self {
            state: ErgorsAppState::new(
                // r == llm router (app-layer)
                Arc::new(LlmRouter::new(&s.cs.latest_snapshot(), c.llm().deref()).await?),
                // s == storage layer
                Arc::new(s),
                // nm == network manifold
                Arc::new(tokio::sync::Mutex::new(nm)),
                // t == time
                Instant::now(),
                // c == config
                c.clone(),
            ),
        })
    }

    /// Validate that all enabled LLM providers have API keys configured
    ///
    /// This prevents the server from starting if required API keys are missing.
    fn validate_llm_api_keys(c: &ErgorsConfig) -> HoResult<()> {
        use std::env;
        let api_keys_path = &c.llm().api_keys_file;

        // Load api-keys.json
        let config = ApiKeysJson::load(&api_keys_path.into())
            .map_err(|e| HoError::Cfg(format!("Failed to load API keys config: {}", e)))?;

        let mut missing_keys = Vec::new();
        let mut found_keys = Vec::new();

        // Check each enabled provider
        for (provider_name, provider_config) in &config.providers {
            // Skip disabled providers
            if let Some(entity) = &provider_config.entity {
                if !entity.enabled {
                    continue;
                }
            }

            // Check if API key is configured in environment
            if let Some(key_ref) = &provider_config.api_key {
                if key_ref.starts_with("${") && key_ref.ends_with("}") {
                    let env_var_name = &key_ref[2..key_ref.len() - 1];

                    match env::var(env_var_name) {
                        Ok(value) if !value.is_empty() => {
                            found_keys.push(provider_name.clone());
                        }
                        _ => {
                            missing_keys.push(format!("{} ({})", provider_name, env_var_name));
                        }
                    }
                } else if !key_ref.is_empty() {
                    // Direct API key configured
                    found_keys.push(provider_name.clone());
                }
            } else {
                // No API key needed (e.g., ollama_local)
                tracing::debug!("Provider {} doesn't require API key", provider_name);
            }
        }

        if !missing_keys.is_empty() {
            return Err(HoError::Cfg(format!(
                "❌ Missing API keys for enabled providers: {}\n\
                 Please set these environment variables in {}/.env\n\
                 Or run 'ergors init llm-api-keys' to reconfigure",
                missing_keys.join(", "),
                api_keys_path
            )));
        }

        if !found_keys.is_empty() {
            tracing::info!(
                "✅ Validated API keys for {} provider(s): {}",
                found_keys.len(),
                found_keys.join(", ")
            );
        }

        Ok(())
    }

    // /// Encrypt and store API keys from environment into the database
    // ///
    // /// This function runs on server startup and:
    // /// 1. Reads api-keys.json config (env vars already loaded by Server::new)
    // /// 2. Encrypts each API key with the node's private key
    // /// 3. Stores encrypted keys in the database
    // async fn encrypt_and_store_api_keys(c: &ErgorsConfig, s: &ErgorsStorage) -> HoResult<()> {
    //     use ho_std::constants::LLM_API_KEYS_FILE;
    //     use ho_std::custody::encrypted::encrypt_with_node_key;
    //     use ho_std::llm::state_ext::StateWriteExt;
    //     use rand_core::OsRng;
    //     use std::env;

    //     // Get home directory from storage config
    //     let home_dir = camino::Utf8PathBuf::from_str(&c.storage().data_dir).unwrap();
    //     let api_keys_path = home_dir.join(LLM_API_KEYS_FILE);

    //     // Skip if api-keys.json doesn't exist yet
    //     if !api_keys_path.exists() {
    //         tracing::debug!("No api-keys.json found, skipping API key encryption");
    //         return Ok(());
    //     }

    //     // Load api-keys.json (env vars already loaded in Server::new)
    //     let config = ApiKeysJson::load(&api_keys_path)
    //         .map_err(|e| HoError::Cfg(format!("Failed to load API keys config: {}", e)))?;

    //     // Get node's private key bytes for encryption
    //     let node_private_key = c
    //         .identity()
    //         .private_key
    //         .as_ref()
    //         .ok_or_else(|| HoError::Cfg("Node private key not configured".to_string()))?;

    //     if node_private_key.len() != 32 {
    //         return Err(HoError::Cfg(format!(
    //             "Invalid node private key length: expected 32 bytes, got {}",
    //             node_private_key.len()
    //         )));
    //     }

    //     let key_bytes: [u8; 32] = node_private_key[..32]
    //         .try_into()
    //         .map_err(|_| HoError::Cfg("Failed to convert node key to array".to_string()))?;

    //     let mut encrypted_count = 0;
    //     let mut skipped_count = 0;

    //     // Get a mutable state delta for writing
    //     let mut delta = s.cs.latest_snapshot();

    //     // Iterate through all configured providers
    //     for (provider_name, provider_config) in &config.providers {
    //         // Skip if provider is not enabled
    //         if let Some(entity) = &provider_config.entity {
    //             if !entity.enabled {
    //                 tracing::debug!("Skipping disabled provider: {}", provider_name);
    //                 skipped_count += 1;
    //                 continue;
    //             }
    //         }

    //         // Get the API key from environment or skip if none configured
    //         let api_key_value = match &provider_config.api_key {
    //             Some(key_ref) if key_ref.starts_with("${") && key_ref.ends_with("}") => {
    //                 // Extract environment variable name
    //                 let env_var_name = &key_ref[2..key_ref.len() - 1];

    //                 match env::var(env_var_name) {
    //                     Ok(value) if !value.is_empty() => value,
    //                     _ => {
    //                         tracing::debug!(
    //                             "No env var {} for provider {}",
    //                             env_var_name,
    //                             provider_name
    //                         );
    //                         skipped_count += 1;
    //                         continue;
    //                     }
    //                 }
    //             }
    //             Some(direct_key) if !direct_key.is_empty() => direct_key.clone(),
    //             _ => {
    //                 tracing::debug!("No API key configured for provider: {}", provider_name);
    //                 skipped_count += 1;
    //                 continue;
    //             }
    //         };

    //         // Encrypt the API key
    //         let encrypted_data =
    //             encrypt_with_node_key(&mut OsRng, &key_bytes, api_key_value.as_bytes());

    //         // Store in database
    //         delta.put_encrypted_api_key(provider_name, encrypted_data);
    //         encrypted_count += 1;

    //         tracing::info!(
    //             "🔐 Encrypted and stored API key for provider: {}",
    //             provider_name
    //         );
    //     }

    //     // Commit the transaction
    //     s.cs.commit(delta)
    //         .await
    //         .map_err(|e| HoError::Storage(format!("Failed to commit encrypted keys: {}", e)))?;

    //     if encrypted_count > 0 {
    //         tracing::info!(
    //             "✅ API Key Encryption: {} encrypted, {} skipped",
    //             encrypted_count,
    //             skipped_count
    //         );
    //     }

    //     Ok(())
    // }

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
                { path: "/orchestrate/fractal", method: post, handler: handle_fractal_hoe_creation },
                { path: "/orchestrate/prune", method: post, handler: handle_prune },
                // { path: "/orchestrate/bootstrap", method: post, handler: handle_bootstrap },

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

async fn handle_prune(
    State(state): State<ErgorsAppState>,
    Json(_request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    //TODO: prune all non-coordinator nodes storage state by bradcasting its cnardium state to up to the coordinator node.
    info!("🔌 Step 1: snapshot, prepend metadata & broadcast to coordinator node");
    info!("🔌 Step 2: Dump snapshot of state and broadcast to coordinator node");
    match state.s.create_snapshot().await {
        Ok(_) => {}
        Err(_e) => return Json(error_json("ErgorsStorage snapshot failed", "STORAGE_ERROR")),
    };

    info!("🔌 Step 3: Prune node state");
    match state.s.prune_storage().await {
        Ok(_) => {}
        Err(_e) => return Json(error_json("ErgorsStorage prune failed", "STORAGE_ERROR")),
    };
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}

async fn handle_prompt(
    State(state): State<ErgorsAppState>,
    Json(r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let pr = match serde_json::from_value::<PromptRequest>(r.clone()) {
        Ok(r) => r,
        Err(e) => {
            return Json(error_json(
                &format!(
                    "Invalid request format: {}. Expected format: {:#?}",
                    e,
                    PromptRequest::default()
                ),
                "INVALID_REQUEST",
            ));
        }
    };

    if pr.messages.is_empty() {
        return Json(error_json(
            "Prompt messages cannot be empty",
            "INVALID_PROMPT",
        ));
    }

    match state.r.handle_request(&pr, &pr.model).await {
        Ok(llm_response) => {
            let response = PromptResponse {
                id: Uuid::new_v4().into(),
                prompt: blake3::Blake3::hash(serde_json::to_string(&pr).unwrap().as_bytes())
                    .to_string(),
                response: llm_response.response,
                model: pr.model.to_string(),
                timestamp: Some(pbjson_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
                tokens_used: llm_response.tokens_used,
                provider: "default".to_string(), // TODO: get deterministic provider from storage
                cost: Some(0.0),
                latency_ms: None,
                // context: request.context.clone(),
            };

            // Store to Cnidarium with original request context
            if let Err(e) = state.s.put_prompt_w_ctx(&response, Some(&pr)).await {
                error!("Failed to store prompt to storage: {}", e);
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
    match state.s.get_prompts(&query).await {
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
