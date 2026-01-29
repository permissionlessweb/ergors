use crate::{
    deploy::{certificate::CertificateManager, cosmos_client::CosmosClient},
    storage::ErgorsStorage,
    AkashDeploymentContext, ErgorsAppState, ErgorsConfig, ErgorsNetworkManifold, LlmRouter,
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
use ho_std::traits::NodeIdentityCustody;
use ho_std::{
    error::{error_json_detailed, HoResult},
    traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityCustodyBackend, NodeIdentityTrait},
    types::ergors::{orch::v1::*, storage::v1::*},
};
use std::collections::HashMap;
use std::io::{IsTerminal as _, Read};
use std::{ops::Deref, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, warn};

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
        // Use the new generic route structure from ho-std
        let (public_router, protected_router) = ho_std::define_routes! {
            public_routes: [
                { path: "/api/prompt", method: post, handler: crate::orchestrator::handle_prompt },
                { path: "/api/operations", method: get, handler: handle_query_operations },
                { path: "/cosmos/extend-vote", method: get, handler: crate::headstash::vote_ext::handle_vote_extension },
                { path: "/headstash/claim", method: post, handler: crate::headstash::claim::handle_headstash_claim },
                { path: "/headstash/upload", method: get, handler: crate::headstash::ipfs::handle_headstash_metadata_storage },
                { path: "/headstash/watch", method: get, handler: crate::headstash::indexer::handle_indexer_instructions },
                { path: "/network/topology", method: get, handler: handle_network_topology },
                // Open Responses API endpoint
                { path: "/v1/responses", method: post, handler: crate::orchestrator::handle_open_responses },
                // Proxy endpoints for CLI tools (Claude Code, opencode)
                { path: "/v1/messages", method: post, handler: crate::proxy::handle_anthropic_proxy },
                { path: "/v1/chat/completions", method: post, handler: crate::proxy::handle_openai_proxy },
                // Ollama-compatible proxy endpoint
                { path: "/api/chat", method: post, handler: crate::proxy::handle_ollama_proxy },
                { path: "/api/generate", method: post, handler: crate::proxy::handle_ollama_proxy },
                // CosmWasm contract endpoints (single entry points)
                { path: "/api/cosmwasm/store", method: post, handler: crate::cosmwasm::handle_cosmwasm_store },
                { path: "/api/cosmwasm/instantiate", method: post, handler: crate::cosmwasm::handle_cosmwasm_instantiate },
                { path: "/api/cosmwasm/instantiate2", method: post, handler: crate::cosmwasm::handle_cosmwasm_instantiate2 },
                { path: "/api/cosmwasm/execute", method: post, handler: crate::cosmwasm::handle_cosmwasm_execute },
                { path: "/api/cosmwasm/query", method: post, handler: crate::cosmwasm::handle_cosmwasm_query },
                // { path: "/orchestrate/bootstrap", method: post, handler: crate::deploy::handle_bootstrap },
                { path: "/health", method: get, handler: handle_health },
            ],
            protected_routes: [
                { path: "/api/prompts", method: get, handler: handle_query },
                { path: "/api/proxy/sessions", method: get, handler: crate::proxy::handle_query_sessions },
                { path: "/api/proxy/sessions/{id}", method: get, handler: crate::proxy::handle_get_session },
                { path: "/api/proxy/config", method: post, handler: crate::proxy::handle_update_proxy_config },
                { path: "/api/proxy/config", method: get, handler: crate::proxy::handle_get_proxy_config },
                { path: "/orchestrate/fractal", method: post, handler: crate::orchestrator::handle_fractal_hoe_creation },
                { path: "/orchestrate/prune", method: post, handler: crate::storage::handle_prune },
                // Authenticator management endpoints
                { path: "/auth/register", method: post, handler: crate::auth::handle_register_authenticator },
                { path: "/auth/list", method: get, handler: crate::auth::handle_list_authenticators },
                { path: "/auth/check", method: get, handler: crate::auth::handle_check_authorization },
                { path: "/auth/{endpoint_label}", method: delete, handler: crate::auth::handle_delete_authenticator },
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
                crate::middleware::record_operation,
            ))
            .with_state(self.state);

        info!("HTTP API server listening on {}", server_addr);

        axum::serve(TcpListener::bind(&server_addr).await?, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| HoError::Cfg(format!("Server error: {}", e)))?;

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

        // Load encrypted API keys and import to Cnidarium storage
        if let Some(password) = &custody_password {
            Self::load_and_store_encrypted_api_keys(&c, &s, password).await?;
        }

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

        // Load proxy router config from storage (or use default if none stored)
        let proxy_router_config = match storage_arc.get_proxy_router_config().await {
            Ok(Some(stored_config)) => {
                tracing::info!(
                    "📍 Loaded proxy router config from storage (version {})",
                    stored_config.version
                );
                // Convert proto config to in-memory config
                let mut config = crate::proxy::ProxyRouterConfig::default();
                if !stored_config.anthropic_base_url.is_empty() {
                    config.anthropic_base_url = Some(stored_config.anthropic_base_url);
                }
                if !stored_config.openai_base_url.is_empty() {
                    config.openai_base_url = Some(stored_config.openai_base_url);
                }
                if !stored_config.ollama_base_url.is_empty() {
                    config.ollama_base_url = Some(stored_config.ollama_base_url);
                }
                config.model_routes = stored_config.model_routes;
                config.api_keys = stored_config.api_keys;
                config.provider_api_keys = stored_config.provider_api_keys;
                config
            }
            Ok(None) => {
                tracing::info!("📍 No stored proxy router config found, using defaults");
                crate::proxy::ProxyRouterConfig::default()
            }
            Err(e) => {
                tracing::warn!(
                    "⚠️  Failed to load proxy router config from storage: {}, using defaults",
                    e
                );
                crate::proxy::ProxyRouterConfig::default()
            }
        };

        // Initialize Akash deployment context if config present and keys available
        let akash_context =
            Self::init_akash_context(&c, &storage_arc, custody_password.as_deref()).await;

        Ok(Self {
            state: ErgorsAppState::new(
                // r == llm router (app-layer)
                Arc::new(LlmRouter::new(&storage_arc.cs.latest_snapshot(), c.llm().deref()).await?),
                // s == storage layer
                storage_arc,
                // nm == network manifold
                Arc::new(tokio::sync::Mutex::new(nm)),
                // t == time
                Instant::now(),
                // c == config
                c.clone(),
                // pr == proxy router (loaded from storage or default)
                Arc::new(tokio::sync::RwLock::new(crate::proxy::ProxyRouter::new(
                    proxy_router_config,
                ))),
                // akash == Akash deployment context (optional)
                akash_context,
                // wasm == WASM runtime
                #[cfg(feature = "cw")]
                wasm_runtime,
            ),
        })
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
        _custody_password: Option<&str>,
    ) -> Option<AkashDeploymentContext> {
        use crate::deploy::cosmos_client::CosmosEndpoints;
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
        tracing::info!("   RPC:      {} endpoint(s)", akash_config.rpc_endpoints.len());
        tracing::info!("   gRPC:     {} endpoint(s)", akash_config.grpc_endpoints.len());
        tracing::info!("   REST:     {} endpoint(s)", akash_config.rest_endpoints.len());

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
        tracing::info!("🔐 Cosmos key manager initialized (locked - will unlock during deployment)");

        // Get endpoints from config
        let endpoints = CosmosEndpoints::from_akash_config(&akash_config);
        let _rest_endpoint = endpoints.rest.clone();
        let _chain_id = akash_config.chain_id.clone();

        // Create CosmosClient
        let cosmos = match CosmosClient::new(endpoints) {
            Ok(client) => Arc::new(client),
            Err(e) => {
                tracing::warn!("⚠️  Failed to create CosmosClient: {}", e);
                return None;
            }
        };

        // Create key manager and store as Arc<RwLock>
        let key_manager_arc = Arc::new(RwLock::new(key_manager));
        let key_store_arc = Arc::new(RwLock::new(key_store));

        // Create CertificateManager with layer-climb integration
        let cert_manager = Arc::new(CertificateManager::new(
            cosmos.clone(),
            key_manager_arc.clone(),
            key_store_arc.clone(),
            akash_config.clone(),
        ));

        tracing::info!("✅ Akash deployment context initialized (using layer-climb)");

        Some(AkashDeploymentContext {
            cosmos,
            cert_manager,
            key_manager: key_manager_arc,
            key_store: key_store_arc,
            akash_config,
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
                    eprintln!(); // Newline after prompt
                    Err(HoError::Cfg("Password entry cancelled (Ctrl+C)".to_string()))
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
    /// 4. Decrypts keys and sets them as environment variables for LLM router
    async fn load_and_store_encrypted_api_keys(
        c: &ErgorsConfig,
        s: &ErgorsStorage,
        password: &str,
    ) -> HoResult<()> {
        use cnidarium::StateWrite as _;
        use ho_std::llm::state_ext::{state_key, StateReadExt};
        use ho_std::Message as _;

        let data_dir = Utf8PathBuf::from(&c.home);
        let encrypted_file = data_dir.join(ENCRYPTED_API_KEYS_FILE);

        // Check if we already have encrypted keys in Cnidarium storage
        let snapshot = s.cs.latest_snapshot();
        let existing_store = snapshot.get_encrypted_api_key_store().await?;

        if existing_store.is_some() {
            info!("🔐 Encrypted API keys found in Cnidarium storage");

            // Decrypt and set environment variables for LLM router
            let decrypted_keys = snapshot.load_and_decrypt_api_keys(password).await?;
            Self::set_api_keys_env(&decrypted_keys);

            return Ok(());
        }

        // No keys in Cnidarium - check for file
        if !encrypted_file.exists() {
            info!(
                "📋 No encrypted API keys file at {} - skipping",
                encrypted_file
            );
            return Ok(());
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

        // Decrypt and set environment variables for LLM router
        let mut manager = EncryptedApiKeyManager::from_store(&store);
        manager
            .unlock(password)
            .map_err(|e| HoError::Crypto(format!("Failed to unlock API key manager: {}", e)))?;

        let decrypted_keys = manager
            .load_store(&store)
            .map_err(|e| HoError::Crypto(format!("Failed to decrypt API keys: {}", e)))?;

        Self::set_api_keys_env(&decrypted_keys);

        Ok(())
    }

    /// Set decrypted API keys as environment variables for LLM router
    fn set_api_keys_env(keys: &HashMap<String, String>) {
        use ho_std::constants::*;

        for (provider, api_key) in keys {
            let env_var = match provider.as_str() {
                "anthropic" => ANTHROPIC_API_KEY,
                "openai" => OPENAI_API_KEY,
                "grok" => GROK_API_KEY,
                "akashml" => AKASHML_KEY,
                "kimi" => KIMI_API_KEY,
                _ => {
                    // Use provider name uppercased with _API_KEY suffix
                    let env_name = format!("{}_API_KEY", provider.to_uppercase());
                    std::env::set_var(&env_name, api_key);
                    info!("🔑 Set {} from encrypted storage", env_name);
                    continue;
                }
            };

            std::env::set_var(env_var, api_key);
            info!("🔑 Set {} from encrypted storage", env_var);
        }
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
