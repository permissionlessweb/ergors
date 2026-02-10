use crate::{
    deploy::akash::client::AkashClient, storage::ErgorsStorage, AkashDeploymentContext,
    ErgorsAppState, ErgorsConfig, ErgorsNetworkManifold, LlmRouter,
};
use axum::{
    extract::{Query, State},
    middleware, Json, Router,
};

use camino::Utf8PathBuf;
use commonware_runtime::tokio::Context;
use ho_std::constants::ENCRYPTED_API_KEYS_FILE;
use ho_std::custody::{PasswordEncryptedCustody, PlaintextCustody};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::llm::{EncryptedApiKeyManager, HoError};
use ho_std::network::AuthLayer;
use ho_std::storage::identity::EncryptedIdentityBuilder;
use ho_std::traits::{ApiKeyMethod, NodeIdentityCustody};
use ho_std::{
    error::{error_json_detailed, HoResult},
    traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityCustodyBackend, NodeIdentityTrait},
    types::ergors::{orch::v1::*, storage::v1::*},
};
use std::io::{IsTerminal as _, Read};
use std::{ops::Deref, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, warn};

#[cfg(unix)]
use termios;

pub struct Server {
    state: ErgorsAppState,
}

impl Server {
    /// Get a clone of the app state (for sharing with gRPC service)
    pub fn state(&self) -> ErgorsAppState {
        self.state.clone()
    }
}

impl Server {
    pub async fn run(
        self,
        shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> HoResult<()> {
        // Start gateway manager event loop if configured
        let gateway_manager_for_shutdown = self.state.gm.clone();
        let gateway_handle = if let Some(ref gm) = self.state.gm {
            let gm = Arc::clone(gm);

            // Start all enabled gateways
            if let Err(e) = gm.start_all().await {
                error!("Failed to start gateways: {}", e);
            } else {
                info!("🌐 Gateway manager started");
            }

            // Spawn event loop
            Some(tokio::spawn(async move {
                if let Err(e) = gm.run().await {
                    error!("Gateway manager error: {}", e);
                }
            }))
        } else {
            None
        };

        // Spawn deployment cache refresh background task
        let cache_refresh_handle = {
            let storage = self.state.s.clone();
            let cache = self.state.r.deployment_cache();
            let akash_ctx = self.state.akash.clone();

            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

                    // Use chain-verified refresh if Akash context available
                    if let Some(ref ctx) = akash_ctx {
                        use crate::deploy::akash::cache_refresher::DeploymentCacheRefresher;

                        let refresher = DeploymentCacheRefresher::new(
                            storage.clone(),
                            ctx.cosmos.clone(),
                            cache.clone(),
                        );

                        let result = refresher.refresh().await;

                        if result.active_count > 0 || result.inactive_count > 0 {
                            tracing::info!(
                                "Deployment cache: {} active, {} inactive, {} low balance",
                                result.active_count,
                                result.inactive_count,
                                result.low_balance_count
                            );
                        }

                        for error in &result.errors {
                            tracing::warn!("Cache refresh error: {}", error);
                        }
                    } else {
                        // Fallback: simple storage-based refresh without chain verification
                        match storage.list_akash_workflows().await {
                            Ok(workflows) => {
                                let mut count = 0;
                                for workflow in workflows {
                                    if cache.add_deployment(&workflow).await.is_ok() {
                                        count += 1;
                                    }
                                }
                                if count > 0 {
                                    tracing::debug!(
                                        "Refreshed deployment cache (no chain verify): {} deployments",
                                        count
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to list workflows for cache refresh: {}", e);
                            }
                        }
                    }
                }
            })
        };

        // Use the new generic route structure from ho-std
        let (public_router, protected_router) = ho_std::define_routes! {
            public_routes: [
                { path: "/api/prompt", method: post, handler: crate::proxy::router::handle_prompt },
                { path: "/api/operations", method: get, handler: handle_query_operations },
                { path: "/cosmos/extend-vote", method: get, handler: crate::headstash::vote_ext::handle_vote_extension },
                { path: "/headstash/claim", method: post, handler: crate::headstash::claim::handle_headstash_claim },
                { path: "/headstash/upload", method: get, handler: crate::headstash::ipfs::handle_headstash_metadata_storage },
                { path: "/headstash/watch", method: get, handler: crate::headstash::indexer::handle_indexer_instructions },
                { path: "/network/topology", method: get, handler: handle_network_topology },
                // Open Responses API endpoint
                { path: "/v1/responses", method: post, handler: crate::proxy::router::handle_open_responses },
                // Proxy endpoints for CLI tools (Claude Code, opencode)
                { path: "/v1/messages", method: post, handler: crate::proxy::handle_anthropic_proxy },
                { path: "/v1/chat/completions", method: post, handler: crate::proxy::handle_openai_proxy },
                { path: "/v1/models", method: get, handler: crate::proxy::handle_list_models },
                // Ollama-compatible proxy endpoint
                { path: "/api/chat", method: post, handler: crate::proxy::handle_ollama_proxy },
                { path: "/api/generate", method: post, handler: crate::proxy::handle_ollama_proxy },
                // CosmWasm contract endpoints (single entry points)
                { path: "/api/cosmwasm/store", method: post, handler: crate::cosmwasm::handle_cosmwasm_store },
                { path: "/api/cosmwasm/instantiate", method: post, handler: crate::cosmwasm::handle_cosmwasm_instantiate },
                { path: "/api/cosmwasm/instantiate2", method: post, handler: crate::cosmwasm::handle_cosmwasm_instantiate2 },
                { path: "/api/cosmwasm/execute", method: post, handler: crate::cosmwasm::handle_cosmwasm_execute },
                { path: "/api/cosmwasm/query", method: post, handler: crate::cosmwasm::handle_cosmwasm_query },
                // Bootstrap endpoints
                { path: "/orchestrate/bootstrap", method: post, handler: crate::deploy::bootstrap::handle_bootstrap },
                { path: "/orchestrate/bootstrap/sessions", method: get, handler: crate::deploy::bootstrap::handle_list_bootstrap_sessions },
                { path: "/orchestrate/bootstrap/sessions/{session_id}", method: get, handler: crate::deploy::bootstrap::handle_bootstrap_status },
                { path: "/orchestrate/bootstrap/sessions/{session_id}", method: delete, handler: crate::deploy::bootstrap::handle_delete_bootstrap_session },
                { path: "/health", method: get, handler: handle_health },
                // Inbox public routes (any node can submit/query)
                { path: "/api/inbox/submit", method: post, handler: crate::deploy::grant_inbox::handle_submit },
                { path: "/api/inbox/grant", method: post, handler: crate::deploy::grant_inbox::handle_submit_grant },
                { path: "/api/inbox/{id}", method: get, handler: crate::deploy::grant_inbox::handle_get_message },
            ],
            protected_routes: [
                { path: "/api/prompts", method: get, handler: handle_query },
                { path: "/api/proxy/sessions", method: get, handler: crate::proxy::handle_query_sessions },
                { path: "/api/proxy/sessions/{id}", method: get, handler: crate::proxy::handle_get_session },
                { path: "/api/proxy/config", method: post, handler: crate::proxy::handle_update_proxy_config },
                { path: "/api/proxy/config", method: get, handler: crate::proxy::handle_get_proxy_config },
                { path: "/orchestrate/fractal", method: post, handler: crate::proxy::router::handle_fractal_hoe_creation },
                { path: "/orchestrate/prune", method: post, handler: crate::storage::handle_prune },
                // Authenticator management endpoints
                { path: "/auth/register", method: post, handler: crate::auth::handle_register_authenticator },
                { path: "/auth/list", method: get, handler: crate::auth::handle_list_authenticators },
                { path: "/auth/check", method: get, handler: crate::auth::handle_check_authorization },
                { path: "/auth/{endpoint_label}", method: delete, handler: crate::auth::handle_delete_authenticator },
                // Inbox protected routes (operator manages inbox)
                { path: "/api/inbox", method: get, handler: crate::deploy::grant_inbox::handle_list_inbox },
                { path: "/api/inbox/{id}/accept", method: post, handler: crate::deploy::grant_inbox::handle_accept },
                { path: "/api/inbox/{id}/reject", method: post, handler: crate::deploy::grant_inbox::handle_reject },
                { path: "/api/inbox/config", method: get, handler: crate::deploy::grant_inbox::handle_get_granter_config },
                { path: "/api/inbox/config", method: post, handler: crate::deploy::grant_inbox::handle_update_granter_config },
                ]
        };
        let server_addr = format!(
            "{}:{}",
            self.state.c.network().listen_address,
            self.state.c.identity().api_port,
        );

        let app = Router::new()
            .merge(public_router)
            .merge(protected_router.route_layer(AuthLayer))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .layer(middleware::from_fn_with_state(
                self.state.clone(),
                crate::auth::operation_recorder::record_operation,
            ))
            .with_state(self.state);

        info!("HTTP API server listening on {}", server_addr);

        axum::serve(TcpListener::bind(&server_addr).await?, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| HoError::Cfg(format!("Server error: {}", e)))?;

        // Stop cache refresh task
        cache_refresh_handle.abort();

        // Stop gateway manager
        if let Some(handle) = gateway_handle {
            if let Some(gm) = gateway_manager_for_shutdown {
                if let Err(e) = gm.stop_all().await {
                    error!("Error stopping gateways: {}", e);
                }
            }
            handle.abort();
            info!("🌐 Gateway manager stopped");
        }

        info!("HTTP server shut down gracefully");
        Ok(())
    }

    pub async fn new(c: ErgorsConfig, ctx: Context) -> HoResult<Self> {
        Self::validate_llm_api_keys(&c)?;
        c.validate()?;
        let mut nm = ErgorsNetworkManifold::new(c.identity(), ctx).await;
        let s = ErgorsStorage::new(
            &c.storage().data_dir,
            vec![
                "network_config".to_string(),
                "akashic_record".to_string(),
                "models_tools".to_string(),
            ],
        )
        .await?;

        // Start network using custody-backed identity (returns password for API key decryption)
        let custody_password = Self::start_network_with_custody(&mut nm, &c).await?;

        // Load encrypted API keys and build custody-backed accessor
        let key_accessor: Option<Arc<tokio::sync::RwLock<dyn ApiKeyMethod>>> =
            if let Some(password) = &custody_password {
                Self::load_and_store_encrypted_api_keys(&c, &s, password).await?
                    .map(|mgr| Arc::new(tokio::sync::RwLock::new(mgr)) as Arc<tokio::sync::RwLock<dyn ApiKeyMethod>>)
            } else {
                None
            };

        // Load LLM entities from config into Cnidarium storage
        Self::load_and_store_llm_entities(&c, &s).await?;

        // Initialize CosmWasm VM runtime
        #[cfg(feature = "cw")]
        let wasm_runtime = {
            use ho_std::wasm::WasmRuntime;
            use std::path::PathBuf;

            // Use cache_dir from cosmwasm config
            let cache_dir = PathBuf::from(c.wasm_cache_dir().as_str());
            std::fs::create_dir_all(&cache_dir)?;
            Arc::new(WasmRuntime::new(cache_dir)?)
        };

        let storage_arc = Arc::new(s);

        // Seed default trusted Akash providers on first startup
        storage_arc.seed_default_trusted_providers().await?;

        // Deploy required contracts on startup (only if CosmWasm enabled)
        #[cfg(feature = "cw")]
        {
            use crate::contracts::ContractManager;

            let contract_manager = ContractManager::new(
                storage_arc.clone(),
                wasm_runtime.clone(),
                c.identity().node_type.clone(),
            );

            contract_manager.deploy_required_contracts(&c).await?;
        }

        // Load proxy router config from storage and populate with LLM entities
        let mut proxy_router_config = match storage_arc.get_proxy_router_config().await {
            Ok(Some(stored_config)) => {
                tracing::info!(
                    "📍 Loaded proxy router config from storage (version {})",
                    stored_config.version
                );
                stored_config
            }
            Ok(None) => {
                tracing::info!(
                    "📍 No stored proxy router config found, creating from LLM entities"
                );
                ho_std::types::ergors::orch::v1::ProxyRouterConfig::default()
            }
            Err(e) => {
                tracing::warn!(
                    "⚠️  Failed to load proxy router config from storage: {}, using defaults",
                    e
                );
                crate::proxy::ProxyRouterConfig::default()
            }
        };

        // Populate proxy router config with LLM entities
        Self::populate_proxy_config_from_llm_entities(&storage_arc, &mut proxy_router_config)
            .await?;

        // Initialize Akash deployment context if config present and keys available
        let akash_context =
            Self::init_akash_context(&c, &storage_arc, custody_password.as_deref()).await;

        // Create LLM router
        let llm_router =
            Arc::new(LlmRouter::new(&storage_arc.cs.latest_snapshot(), c.llm().deref()).await?);

        // Initialize gateway manager (for Discord, Nostr, etc.)
        let gateway_manager = Self::init_gateway_manager(&llm_router, &storage_arc, &c).await;

        // Initialize RLM service (for agentic document queries)
        // Pool size: number of Python REPL workers for concurrent RLM queries
        #[cfg(feature = "rlm")]
        const RLM_WORKER_POOL_SIZE: usize = 2;
        #[cfg(feature = "rlm")]
        let rlm_service = match ergors_rlm::RlmService::new(RLM_WORKER_POOL_SIZE, llm_router.clone()).await {
            Ok(svc) => {
                tracing::info!("RLM service initialized");
                Some(Arc::new(svc))
            }
            Err(e) => {
                tracing::warn!("RLM init failed: {}", e);
                None
            }
        };

        // Build proxy router with engine role config
        let proxy_router = {
            let mut pr = crate::proxy::ProxyRouter::new(
                proxy_router_config,
                key_accessor,
            );

            // Load engine role config and warn about unassigned roles
            match storage_arc.get_engine_role_config().await {
                Ok(Some(role_config)) => {
                    use ho_std::types::ergors::orch::v1::EngineRole;
                    let roles = [
                        (EngineRole::Orchestration, "orchestration"),
                        (EngineRole::SubAgent, "sub-agent"),
                        (EngineRole::Embeddings, "embeddings"),
                        (EngineRole::ToolCalling, "tool-calling"),
                    ];
                    for (role, name) in &roles {
                        let has_provider = role_config.mappings.iter().any(|m| {
                            m.role == *role as i32 && !m.provider_ids.is_empty()
                        });
                        if !has_provider {
                            tracing::warn!(
                                "Engine role '{}' has no assigned provider — will fall back to model-pattern routing",
                                name
                            );
                        }
                    }
                    pr.set_engine_role_config(Some(role_config));
                }
                Ok(None) => {
                    tracing::info!("No engine role config found — using model-pattern routing for all requests");
                }
                Err(e) => {
                    tracing::warn!("Failed to load engine role config: {} — continuing without role assignments", e);
                }
            }

            Arc::new(tokio::sync::RwLock::new(pr))
        };

        Ok(Self {
            state: ErgorsAppState::new(
                // r == llm router (app-layer)
                llm_router,
                // s == storage layer
                storage_arc,
                // nm == network manifold
                Arc::new(tokio::sync::Mutex::new(nm)),
                // t == time
                Instant::now(),
                // c == config
                c.clone(),
                // pr == proxy router (loaded from storage or default, with custody key accessor)
                proxy_router,
                // akash == Akash deployment context (optional)
                akash_context,
                // gm == gateway manager (optional)
                gateway_manager,
                // rlm == RLM service (optional)
                #[cfg(feature = "rlm")]
                rlm_service,
                // wasm == WASM runtime
                #[cfg(feature = "cw")]
                wasm_runtime,
            ),
        })
    }

    /// Initialize gateway manager for communication interfaces (Discord, Nostr, etc.)
    ///
    /// The gateway manager handles:
    /// - Registering gateway modules (Discord bot, Nostr relays, etc.)
    /// - Routing incoming messages to the LLM router
    /// - Sending responses back through the appropriate gateway
    async fn init_gateway_manager(
        router: &Arc<LlmRouter>,
        storage: &Arc<ErgorsStorage>,
        config: &ErgorsConfig,
    ) -> Option<Arc<crate::gateway::GatewayManager>> {
        use crate::gateway::GatewayManager;
        use ho_std::traits::HoConfigTrait;

        let manager = Arc::new(GatewayManager::new(router.clone(), storage.clone()));

        // Get node pubkey for decrypting gateway secrets
        let node_pubkey = config.identity().public_key.clone();

        // Register Discord gateway if feature is enabled
        #[cfg(feature = "discord")]
        {
            use crate::gateway::discord::DiscordGateway;

            match DiscordGateway::from_storage(storage, node_pubkey.as_deref()).await {
                Ok(discord) => {
                    manager.register(Arc::new(discord)).await;
                    info!("📱 Discord gateway registered");
                }
                Err(e) => {
                    warn!("⚠️  Failed to initialize Discord gateway: {}", e);
                }
            }
        }

        // Future: Register Nostr gateway
        // #[cfg(feature = "nostr")]
        // { ... }

        Some(manager)
    }

    /// Initialize Akash deployment context if config and keys are available.
    ///
    /// Returns None if:
    /// - No Akash config in config.toml
    /// - No Cosmos key store in storage
    /// - Failed to unlock key manager
    async fn init_akash_context(
        c: &ErgorsConfig,
        storage: &Arc<ErgorsStorage>,
        custody_password: Option<&str>,
    ) -> Option<AkashDeploymentContext> {
        use crate::deploy::akash::client::AkashClient;
        use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
        use tokio::sync::RwLock;

        // Get Akash config (uses mainnet defaults if not explicitly configured)
        let akash_config = c.akash();

        // Skip if endpoints are empty (meaning Akash is intentionally disabled)
        if akash_config.rpc_endpoints.is_empty() || akash_config.chain_id.is_empty() {
            tracing::info!("📋 Akash deployment disabled (empty endpoints in config)");
            return None;
        }

        tracing::info!("🚀 Initializing Akash deployment context...");
        tracing::info!("   Chain:    {}", akash_config.chain_id);
        tracing::info!(
            "   RPC:      {} endpoint(s)",
            akash_config.rpc_endpoints.len()
        );
        tracing::info!(
            "   gRPC:     {} endpoint(s)",
            akash_config.grpc_endpoints.len()
        );
        tracing::info!(
            "   REST:     {} endpoint(s)",
            akash_config.rest_endpoints.len()
        );

        // Get Cosmos key store from storage (use empty store if not found)
        let key_store = match storage.get_cosmos_key_store().await {
            Ok(Some(store)) => {
                tracing::info!(
                    "🔑 Loaded Cosmos key store with {} keys",
                    store.derived_accounts.len()
                );
                store
            }
            Ok(None) => {
                tracing::info!(
                    "📋 No Cosmos key store found - run 'ergors keys import-mnemonic' to import keys"
                );
                // Create empty key store - deployment will fail at signing with clear error
                CosmosKeyStore::default()
            }
            Err(e) => {
                tracing::warn!("⚠️  Failed to load Cosmos key store: {}", e);
                // Create empty key store - deployment will fail at signing with clear error
                CosmosKeyStore::default()
            }
        };

        // Create key manager from store (loads salt from existing keys)
        // NOTE: We keep it locked on startup - it will be unlocked with password during deployment
        let key_manager = EncryptedCosmosKeyManager::from_store(&key_store);
        tracing::info!(
            "🔐 Cosmos key manager initialized (locked - will unlock during deployment)"
        );

        // Create AkashClient from config
        let cosmos = match AkashClient::from_akash_config(&akash_config) {
            Ok(client) => Arc::new(client),
            Err(e) => {
                tracing::warn!("⚠️  Failed to create AkashClient: {}", e);
                return None;
            }
        };

        // Create key manager and store as Arc<RwLock>
        let key_manager_arc = Arc::new(RwLock::new(key_manager));
        let key_store_arc = Arc::new(RwLock::new(key_store));

        tracing::info!("✅ Akash deployment context initialized (JWT auth, layer-climb)");

        Some(AkashDeploymentContext {
            cosmos,
            key_manager: key_manager_arc,
            key_store: key_store_arc,
            akash_config,
            custody_password: custody_password.unwrap_or_default().to_string(),
        })
    }

    /// Start network using custody-backed identity
    ///
    /// This method:
    /// 1. Determines the custody backend from config
    /// 2. Handles password prompts for encrypted custody
    /// 3. Migrates plaintext keys to encrypted custody if needed
    /// 4. Starts the network with the custody-backed identity
    /// 5. Returns the custody password for reuse (API key decryption)
    async fn start_network_with_custody(
        nm: &mut ErgorsNetworkManifold,
        c: &ErgorsConfig,
    ) -> HoResult<Option<String>> {
        let custody_backend = c.custody_backend();
        let identity_path = c.identity_path();

        match custody_backend {
            NodeIdentityCustodyBackend::PasswordEncrypted => {
                let custody = c.create_password_custody();

                // Check if encrypted identity exists
                if custody.exists() {
                    // Unlock with password (async, interruptible by Ctrl+C)
                    let password = Self::get_custody_password().await?;
                    custody
                        .unlock(&password)
                        .await
                        .map_err(|e| HoError::Cfg(format!("Failed to unlock custody: {}", e)))?;

                    info!("🔓 Unlocked encrypted node identity");
                    nm.start_network_with_custody(c.network(), &custody).await?;
                    Ok(Some(password))
                } else {
                    // No identity exists - need to create one
                    info!("🆕 Creating new encrypted node identity...");
                    let password = Self::create_custody_password().await?;

                    let metadata = EncryptedIdentityBuilder::new()
                        .user(c.identity().user.clone())
                        .host(c.identity().host.clone())
                        .p2p_port(c.identity().p2p_port)
                        .api_port(c.identity().api_port)
                        .node_type(c.identity().node_type.clone())
                        .build();

                    custody
                        .create_identity(&password, Some(metadata))
                        .map_err(|e| {
                            HoError::Cfg(format!("Failed to create encrypted identity: {}", e))
                        })?;

                    custody
                        .unlock(&password)
                        .await
                        .map_err(|e| HoError::Cfg(format!("Failed to unlock custody: {}", e)))?;

                    info!("✅ Created encrypted node identity at: {}", identity_path);
                    nm.start_network_with_custody(c.network(), &custody).await?;
                    Ok(Some(password))
                }
            }
            NodeIdentityCustodyBackend::Plaintext => {
                // Legacy: use plaintext custody (for testing/development only)
                warn!("⚠️ Using plaintext custody - NOT RECOMMENDED FOR PRODUCTION");
                let custody = PlaintextCustody::generate();
                nm.start_network_with_custody(c.network(), &custody).await?;
                Ok(None)
            }
            NodeIdentityCustodyBackend::NodeKeyEncrypted => {
                // Future: node-key encrypted
                Err(HoError::Cfg(
                    "NodeKeyEncrypted custody backend not yet implemented".to_string(),
                ))
            }
            NodeIdentityCustodyBackend::Threshold => {
                // Future: threshold custody
                Err(HoError::Cfg(
                    "Threshold custody backend not yet implemented".to_string(),
                ))
            }
            NodeIdentityCustodyBackend::RemoteCustody(endpoint) => {
                // Future: remote custody
                Err(HoError::Cfg(format!(
                    "RemoteCustody ({}) not yet implemented",
                    endpoint
                )))
            }
        }
    }

    /// Get custody password from environment or interactive prompt
    async fn get_custody_password() -> HoResult<String> {
        // First check environment variable for non-interactive use
        if let Ok(password) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
            if !password.is_empty() {
                return Ok(password);
            }
        }

        // Interactive password prompt (async, interruptible by Ctrl+C)
        Self::prompt_for_password_async("Enter custody password: ").await
    }

    /// Create a new custody password with confirmation
    async fn create_custody_password() -> HoResult<String> {
        // Check environment variable first
        if let Ok(password) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
            if !password.is_empty() {
                return Ok(password);
            }
        }

        // Interactive prompts (async, interruptible by Ctrl+C)
        let password = Self::prompt_for_password_async("Create custody password: ").await?;
        let confirm = Self::prompt_for_password_async("Confirm custody password: ").await?;

        if password != confirm {
            return Err(HoError::Cfg("Passwords do not match".to_string()));
        }

        if password.len() < 8 {
            return Err(HoError::Cfg(
                "Password must be at least 8 characters".to_string(),
            ));
        }

        Ok(password)
    }

    /// Prompt for password (interactive or from stdin)
    ///
    /// This async function runs the blocking password prompt in a separate thread
    /// and can be cancelled via Ctrl+C (which will cause the process to exit).
    /// Use Ctrl+D to cancel password entry gracefully.
    async fn prompt_for_password_async(msg: &str) -> HoResult<String> {
        if std::io::stdin().is_terminal() {
            let msg = msg.to_string();

            // Run blocking password prompt in a separate thread
            let handle = tokio::task::spawn_blocking(move || rpassword::prompt_password(&msg));

            // Race against Ctrl+C signal
            tokio::select! {
                result = handle => {
                    match result {
                        Ok(Ok(password)) if password.is_empty() => {
                            Err(HoError::Cfg("Password entry cancelled (empty input)".to_string()))
                        }
                        Ok(Ok(password)) => Ok(password),
                        Ok(Err(e)) => Err(HoError::Cfg(format!("Password read error: {}", e))),
                        Err(e) => Err(HoError::Cfg(format!("Password prompt interrupted: {}", e))),
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    // Restore terminal state - rpassword disables echo
                    restore_terminal_echo();
                    // Exit immediately - the spawn_blocking thread is stuck waiting for input
                    // and cannot be cancelled, so we must force exit to avoid hanging
                    eprintln!("Password entry cancelled (Ctrl+C)");
                    std::process::exit(130) // Standard exit code for SIGINT
                }
            }
        } else {
            // Non-interactive: read from stdin (blocking is fine here)
            let mut password = String::new();
            match std::io::stdin().lock().read_to_string(&mut password) {
                Ok(0) => Err(HoError::Cfg("No password provided on stdin".to_string())),
                Ok(_) => Ok(password.trim().to_string()),
                Err(e) => Err(HoError::Cfg(format!("Failed to read password: {}", e))),
            }
        }
    }
    /// Import an existing private key to encrypted custody
    ///
    /// Used when migrating from external key sources or restoring from backup.
    #[allow(dead_code)]
    async fn import_to_encrypted_custody(
        c: &ErgorsConfig,
        custody: &PasswordEncryptedCustody,
        private_key: &NodePrivKey,
    ) -> HoResult<()> {
        // Create password for new encrypted storage
        info!("🔐 Importing key to encrypted custody...");
        let password = Self::create_custody_password().await?;

        let metadata = EncryptedIdentityBuilder::new()
            .user(c.identity().user.clone())
            .host(c.identity().host.clone())
            .p2p_port(c.identity().p2p_port)
            .api_port(c.identity().api_port)
            .node_type(c.identity().node_type.clone())
            .build();

        custody
            .import_identity(private_key, &password, Some(metadata))
            .map_err(|e| HoError::Cfg(format!("Failed to import identity to custody: {}", e)))?;

        info!("✅ Successfully imported key to encrypted custody");

        Ok(())
    }

    /// Load encrypted API keys from file and store in Cnidarium
    ///
    /// This method:
    /// 1. Checks if encrypted API keys already exist in Cnidarium storage
    /// 2. If not, loads from `api-keys.enc` file in the data directory
    /// 3. Imports the encrypted store into Cnidarium for network consensus
    /// 4. Decrypts keys and returns custody-backed ApiKeyMethod accessor
    async fn load_and_store_encrypted_api_keys(
        c: &ErgorsConfig,
        s: &ErgorsStorage,
        password: &str,
    ) -> HoResult<Option<EncryptedApiKeyManager>> {
        use cnidarium::StateWrite as _;
        use ho_std::llm::state_ext::{state_key, StateReadExt};
        use ho_std::Message as _;

        let data_dir = Utf8PathBuf::from(&c.home);
        let encrypted_file = data_dir.join(ENCRYPTED_API_KEYS_FILE);

        // Check if we already have encrypted keys in Cnidarium storage
        let snapshot = s.cs.latest_snapshot();
        let existing_store = snapshot.get_encrypted_api_key_store().await?;

        if let Some(store) = existing_store {
            info!("🔐 Encrypted API keys found in Cnidarium storage");

            let mut manager = EncryptedApiKeyManager::from_store(&store);
            manager.unlock(password).map_err(|e| {
                HoError::Crypto(format!("Failed to unlock API key manager: {}", e))
            })?;
            manager.load_store(&store).map_err(|e| {
                HoError::Crypto(format!("Failed to decrypt API keys: {}", e))
            })?;

            let count = manager.available_providers_sync();
            info!("🔑 Loaded {} API keys from custody", count);
            return Ok(Some(manager));
        }

        // No keys in Cnidarium - check for file
        if !encrypted_file.exists() {
            info!(
                "📋 No encrypted API keys file at {} - skipping",
                encrypted_file
            );
            return Ok(None);
        }

        // Load encrypted store from file
        info!("🔐 Loading encrypted API keys from {}", encrypted_file);
        let encrypted_bytes = std::fs::read(&encrypted_file).map_err(|e| {
            HoError::Storage(format!("Failed to read encrypted API keys file: {}", e))
        })?;

        let store = EncryptedApiKeyManager::deserialize_store(&encrypted_bytes).map_err(|e| {
            HoError::DeSerialization(format!("Failed to deserialize encrypted API keys: {}", e))
        })?;

        // Import to Cnidarium storage using direct put_raw
        let mut delta = cnidarium::StateDelta::new(s.cs.latest_snapshot());
        let key = state_key::encrypted_api_keys().to_string();
        let data = store.encode_to_vec();
        delta.put_raw(key, data);
        s.cs.commit(delta)
            .await
            .map_err(|e| HoError::Storage(format!("Failed to commit encrypted API keys: {}", e)))?;

        info!(
            "✅ Imported {} encrypted API keys to Cnidarium storage",
            store.keys.len()
        );

        // Decrypt and return as accessor
        let mut manager = EncryptedApiKeyManager::from_store(&store);
        manager
            .unlock(password)
            .map_err(|e| HoError::Crypto(format!("Failed to unlock API key manager: {}", e)))?;

        manager
            .load_store(&store)
            .map_err(|e| HoError::Crypto(format!("Failed to decrypt API keys: {}", e)))?;

        let count = manager.available_providers_sync();
        info!("🔑 Loaded {} API keys from custody", count);
        Ok(Some(manager))
    }

    /// Load LLM entities from config and store in Cnidarium storage
    ///
    /// This method:
    /// 1. Checks if entities are already stored in Cnidarium
    /// 2. If not, imports entities from config.toml
    /// 3. Stores them in verifiable storage for consensus
    async fn load_and_store_llm_entities(c: &ErgorsConfig, s: &ErgorsStorage) -> HoResult<()> {
        use ho_std::llm::state_ext::StateReadExt;
        use ho_std::llm::state_ext::StateWriteExt;

        let snapshot = s.cs.latest_snapshot();
        let existing_providers = snapshot.get_llm_providers().await?;

        if !existing_providers.is_empty() {
            info!(
                "📍 Found {} LLM entities in storage - skipping import",
                existing_providers.len()
            );
            return Ok(());
        }

        // Get entities from config
        let entities = &c.llm().entities;

        if entities.is_empty() {
            info!("📋 No LLM entities configured in config.toml");
            return Ok(());
        }

        // Store entities in Cnidarium
        let mut delta = cnidarium::StateDelta::new(s.cs.latest_snapshot());
        delta.put_llm_providers(entities);
        s.cs.commit(delta)
            .await
            .map_err(|e| HoError::Storage(format!("Failed to commit LLM entities: {}", e)))?;

        info!(
            "✅ Imported {} LLM entities to Cnidarium storage",
            entities.len()
        );

        // Log each entity with details
        for entity in entities {
            info!(
                "   - {} (base_url: {}, models: {})",
                entity.name,
                entity.base_url,
                entity.models.join(", ")
            );
        }

        Ok(())
    }

    /// Populate proxy router config with LLM entities from storage
    ///
    /// Converts LlmEntity configs to InferenceProviderConfig for the proxy router
    async fn populate_proxy_config_from_llm_entities(
        s: &ErgorsStorage,
        config: &mut ProxyRouterConfig,
    ) -> HoResult<()> {
        use ho_std::llm::state_ext::StateReadExt;
        use ho_std::types::ergors::orch::v1::InferenceProviderType;

        let snapshot = s.cs.latest_snapshot();
        let entities = snapshot.get_llm_providers().await?;

        if entities.is_empty() {
            info!("⚠️  No LLM entities found in storage for proxy router");
            return Ok(());
        }

        // Convert LlmEntity to InferenceProviderConfig
        for entity in &entities {
            let provider_type = match entity.name.to_lowercase().as_str() {
                "openai" => InferenceProviderType::Openai,
                "anthropic" => InferenceProviderType::Anthropic,
                "ollama" => InferenceProviderType::Ollama,
                "grok" => InferenceProviderType::Openai, // Grok uses OpenAI-compatible API
                "akashml" | "akash" => InferenceProviderType::Openai, // Akash uses OpenAI-compatible API
                _ => InferenceProviderType::Openai, // Default to OpenAI-compatible
            };

            let provider_config = InferenceProviderConfig {
                provider_id: entity.name.clone(),
                base_url: entity.base_url.clone(),
                api_key_ref: String::new(),
                enabled: entity.enabled,
                provider_type: provider_type as i32,
                ..Default::default()
            };

            config
                .providers
                .insert(entity.name.clone(), provider_config);

            // Add model routes for this provider
            for model in &entity.models {
                config
                    .model_routes
                    .insert(model.clone(), entity.name.clone());
            }
        }

        info!(
            "✅ Populated proxy router with {} providers from LLM entities",
            entities.len()
        );

        Ok(())
    }

    /// Validate that all enabled LLM providers have API keys configured
    ///
    /// This prevents the server from starting if required API keys are missing.
    /// If api-keys.json doesn't exist, validation is skipped (no LLM providers configured).
    fn validate_llm_api_keys(c: &ErgorsConfig) -> HoResult<()> {
        use std::env;
        let api_keys_path = &c.llm().api_keys_file;

        // Check if api-keys.json exists
        if !std::path::Path::new(api_keys_path).exists() {
            info!(
                "📋 No api-keys.json found at {} - LLM providers not configured",
                api_keys_path
            );
            info!("   Run 'ergors init llms' to configure LLM providers");
            return Ok(());
        }

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
            let key_ref = provider_config.api_key.clone();
            // Check if API key is configured in environment
            if !key_ref.is_empty() {
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
}

/// Restore terminal echo after password prompt interruption.
/// rpassword disables echo during input; if interrupted, we must re-enable it.
#[cfg(unix)]
fn restore_terminal_echo() {
    use std::os::unix::io::AsRawFd;
    if let Ok(mut termios) = termios::Termios::from_fd(std::io::stdin().as_raw_fd()) {
        termios.c_lflag |= termios::ECHO | termios::ICANON;
        let _ = termios::tcsetattr(std::io::stdin().as_raw_fd(), termios::TCSANOW, &termios);
    }
    eprintln!(); // Newline after prompt
}

#[cfg(not(unix))]
fn restore_terminal_echo() {
    eprintln!(); // Newline after prompt
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
        network_status,
    })
}

async fn handle_network_topology(State(state): State<ErgorsAppState>) -> Json<serde_json::Value> {
    let nm = state.nm.lock().await;
    let topology = nm.get_topology().await;
    // Get identity from NetworkManifold (has updated public_key and bech32_address)
    let identity = nm.identity();

    Json(serde_json::json!({
        "topology": topology,
        "node_identity": {
            "node_id": identity.display_id(),
            "node_type": identity.node_type,
            "p2p_address": identity.p2p_address(),
            "api_address": identity.api_address(),
            "bech32_address": identity.bech32_address.as_ref(),
        }
    }))
}

/// `handle_query_operations`: query
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
