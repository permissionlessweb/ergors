//! ManagementService gRPC implementation
//!
//! Provides the server-side implementation of the management gRPC service.

use crate::session_manager::{SessionManager, SessionManagerConfig};
use crate::ErgorsAppState;
use async_stream::try_stream;
use ho_std::traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait};
use ho_std::types::ergors::management::v1::{
    management_service_server::ManagementService,
    // Workspace types
    AddWorkspaceRequest,
    AddWorkspaceResponse,
    // Akash deployment types (advance_akash_deployment is deprecated but still in proto)
    AdvanceAkashDeploymentRequest,
    AdvanceAkashDeploymentResponse,
    CancelAkashDeploymentRequest,
    ConfigureProxyRoutesRequest,
    CreateAkashDeploymentRequest,
    CreateAkashDeploymentResponse,
    GetAkashDeploymentRequest,
    GetAkashDeploymentResponse,
    ListAkashDeploymentsRequest,
    ListAkashDeploymentsResponse,
    QueryAkashBidsRequest,
    QueryAkashBidsResponse,
    SelectAkashProviderRequest,
    SelectAkashProviderResponse,
    SetWorkflowEndpointsRequest,
    SetWorkflowEndpointsResponse,
    // Network routing types (for OpenCode tools)
    AnnounceNodeRequest,
    AnnounceNodeResponse,
    CompleteSessionRequest,
    CompleteSessionResponse,
    CompleteTaskWorktreeRequest,
    CompleteTaskWorktreeResponse,
    ConfigData,
    ConfigUpdate,
    // Session types
    CreateSessionRequest,
    CreateSessionResponse,
    CreateTaskWorktreeRequest,
    CreateTaskWorktreeResponse,
    DeleteSessionRequest,
    Empty,
    EngineState,
    EngineStatus,
    FailSessionRequest,
    FailTaskWorktreeRequest,
    GetHierarchyRequest,
    GetHierarchyResponse,
    GetSessionRequest,
    GetSessionResponse,
    GetWorkspaceRequest,
    GetWorkspaceResponse,
    HealthUpdate,
    IdentityResponse,
    ImportIdentityRequest,
    ListByNodeRequest,
    ListByNodeResponse,
    ListByRootRequest,
    ListByRootResponse,
    ListTaskWorktreesRequest,
    ListTaskWorktreesResponse,
    ListWorkspacesRequest,
    ListWorkspacesResponse,
    LogEntry,
    LogStreamRequest,
    MigrateSessionRequest,
    MigrateSessionResponse,
    NodeIdRequest,
    NodeTypeRequest,
    OperationResult,
    PauseSessionRequest,
    PauseSessionResponse,
    PeerAddress,
    ProviderConfig,
    ProviderInfo,
    ProviderList,
    ProviderName,
    ProviderTestResult,
    QuerySessionsRequest,
    QuerySessionsResponse,
    RemoveWorkspaceRequest,
    ResolveConflictRequest,
    ResolveConflictResponse,
    ResumeSessionRequest,
    ResumeSessionResponse,
    RollupRequest,
    RollupResponse,
    RouteMessageRequest,
    RouteMessageResponse,
    SessionHierarchyStats,
    SessionStatus,
    SessionUpdate,
    ShutdownRequest,
    SpawnChildRequest,
    SpawnChildResponse,
    StreamRequest,
    StreamSessionRequest,
    SyncSessionRequest,
    SyncSessionResponse,
    SyncWorkspaceRequest,
    SyncWorkspaceResponse,
    TokenIdRequest,
    TokenLabel,
    TokenList,
    TokenResponse,
    UpdateSessionRequest,
    UpdateSessionResponse,
    // Grant management types
    ApproveGrantRequest,
    CreateFeeGrantRequest,
    ListGrantRequestsRequest,
    ListGrantRequestsResponse,
    QueryBalanceRequest,
    QueryBalanceResponse,
    RequestGrantRequest,
    RequestGrantResponse,
    RevokeGrantRequest,
    RevokeFeeGrantRequest,
    // SDL template types
    ListSdlTemplatesRequest,
    ListSdlTemplatesResponse,
    RegisterSdlTemplateRequest,
    RegisterSdlTemplateResponse,
    GetSdlTemplateRequest,
    GetSdlTemplateResponse,
    GetSdlDefaultsRequest,
    GetSdlDefaultsResponse,
    RenderSdlTemplateRequest,
    RenderSdlTemplateResponse,
    // Chain config types
    SetChainConfigRequest,
    SetChainConfigResponse,
    GetChainConfigRequest,
    GetChainConfigResponse,
    ListChainConfigsRequest,
    ListChainConfigsResponse,
    DeleteChainConfigRequest,
    DeleteChainConfigResponse,
    // Key address query types
    GetKeyAddressRequest,
    GetKeyAddressResponse,
    // Cosmos key management types
    ListCosmosKeysResponse,
    ImportCosmosKeyRequest,
    ImportCosmosKeyResponse,
    DeleteCosmosKeyRequest,
    SetDefaultCosmosKeyRequest,
    CosmosKeyInfo,
    // Gateway management types
    ListGatewaysRequest,
    ListGatewaysResponse,
    GatewayInfo,
    GetGatewayStatusRequest,
    GatewayStatusResponse,
    EnableGatewayRequest,
    DisableGatewayRequest,
    ConfigureDiscordGatewayRequest,
    AddDiscordAllowedGuildRequest,
    RemoveDiscordAllowedGuildRequest,
    GetDiscordConfigRequest,
    GetDiscordConfigResponse,
};
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashWorkflowStatus, AkashWorkflowStep, ConfiguredSdl,
    // Automated workflow types
    RunAkashDeploymentRequest, RunAkashDeploymentResponse,
    CloseAkashLeaseRequest, CloseAkashDeploymentRequest, UpdateAkashDeploymentRequest, TopupAkashEscrowRequest,
    GetLeaseStatusRequest, LeaseStatusResponse,
    AddTrustedProviderRequest, RemoveTrustedProviderRequest,
    ListTrustedProvidersRequest, ListTrustedProvidersResponse, AkashWorkflowOptions, AkashLeaseInfo, AkashLeaseState,
    // Certificate management types (deprecated - JWT auth used instead, stubs for trait compliance)
    CreateAkashCertificateRequest, CreateAkashCertificateResponse,
    RevokeAkashCertificateRequest, ListAkashCertificatesRequest, ListAkashCertificatesResponse,
    // RAG types
    RagIngestRequest, RagIngestResponse, RagQueryRequest, RagQueryResponse,
    RagStatusRequest, RagStatusResponse, RagDeleteRequest, RagOperationResult,
    RagListSourcesRequest, RagListSourcesResponse, RagConfigureRequest, RagSearchResult, RagSourceInfo,
};
use ho_std::types::ergors::network::v1::{NetworkTopology, NodeIdentity, NodeType};
use ho_std::types::ergors::storage::v1::EncryptedSecret;
use ho_std::keys::cosmos::cosmos_address_from_pubkey;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use crate::deploy::cosmos_client::{CosmosClient, CosmosEndpoints};
use crate::gateway::crypto::{encrypt_gateway_secret, GATEWAY_SECRET_ENCRYPTION_METHOD};
use pbjson_types;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

/// Implementation of the ManagementService gRPC server
pub struct ManagementServiceImpl {
    state: ErgorsAppState,
    started_at: Instant,
    shutdown_tx: broadcast::Sender<()>,
    session_manager: Arc<SessionManager>,
}

impl ManagementServiceImpl {
    /// Create a new management service implementation
    pub fn new(state: ErgorsAppState, shutdown_tx: broadcast::Sender<()>) -> Self {
        // Extract node identity for SessionManager
        let identity = state.c.identity();
        let node_type = match identity.node_type.as_str() {
            "coordinator" => NodeType::Coordinator,
            "executor" => NodeType::Executor,
            "referee" => NodeType::Referee,
            "development" => NodeType::Development,
            _ => NodeType::Unspecified,
        };
        let node_id = identity
            .public_key
            .as_ref()
            .map(hex::encode)
            .unwrap_or_else(|| "local".to_string());

        // Create SessionManager with default config
        let session_manager = Arc::new(SessionManager::new(
            state.s.clone(),
            state.nm.clone(),
            node_id,
            node_type,
            SessionManagerConfig::default(),
        ));

        Self {
            state,
            started_at: Instant::now(),
            shutdown_tx,
            session_manager,
        }
    }

    /// Create a new management service with custom session config
    pub fn with_session_config(
        state: ErgorsAppState,
        shutdown_tx: broadcast::Sender<()>,
        session_config: SessionManagerConfig,
    ) -> Self {
        let identity = state.c.identity();
        let node_type = match identity.node_type.as_str() {
            "coordinator" => NodeType::Coordinator,
            "executor" => NodeType::Executor,
            "referee" => NodeType::Referee,
            "development" => NodeType::Development,
            _ => NodeType::Unspecified,
        };
        let node_id = identity
            .public_key
            .as_ref()
            .map(hex::encode)
            .unwrap_or_else(|| "local".to_string());

        let session_manager = Arc::new(SessionManager::new(
            state.s.clone(),
            state.nm.clone(),
            node_id,
            node_type,
            session_config,
        ));

        Self {
            state,
            started_at: Instant::now(),
            shutdown_tx,
            session_manager,
        }
    }

    /// Get a shutdown receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Get the session manager for external use
    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }
}

#[tonic::async_trait]
impl ManagementService for ManagementServiceImpl {
    // ============ Lifecycle ============

    async fn get_status(&self, _request: Request<Empty>) -> Result<Response<EngineStatus>, Status> {
        let uptime = self.started_at.elapsed().as_secs();

        // Check storage health
        let storage_status = match self.state.s.health_check().await {
            Ok(()) => "healthy".to_string(),
            Err(e) => format!("unhealthy: {}", e),
        };

        // Check network status
        let (network_status, connected_peers) = {
            let nm = self.state.nm.lock().await;
            let topology = nm.get_topology().await;
            let online_count = topology.online_nodes().len() as u32;
            let status = if online_count == 0 {
                "no peers".to_string()
            } else {
                format!("{} peers connected", online_count)
            };
            (status, online_count)
        };

        Ok(Response::new(EngineStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    - uptime as i64,
                nanos: 0,
            }),
            uptime_seconds: uptime,
            state: EngineState::Running.into(),
            storage_status,
            network_status,
            connected_peers,
            total_requests_handled: 0,
        }))
    }

    async fn shutdown(
        &self,
        request: Request<ShutdownRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Shutdown requested (force: {})", req.force);

        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        Ok(Response::new(OperationResult {
            success: true,
            message: "Shutdown initiated".to_string(),
        }))
    }

    // ============ Node Identity ============

    async fn get_node_identity(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NodeIdentity>, Status> {
        // Get identity from NetworkManifold (has updated public_key and bech32_address)
        let nm = self.state.nm.lock().await;
        let identity = nm.identity();

        Ok(Response::new(NodeIdentity {
            host: identity.host.clone(),
            p2p_port: identity.p2p_port,
            api_port: identity.api_port,
            user: identity.user.clone(),
            os: identity.os,
            ssh_port: identity.ssh_port,
            node_type: identity.node_type.clone(),
            public_key: identity.public_key.clone(),
            bech32_address: identity.bech32_address.clone(),
        }))
    }

    async fn generate_node_identity(
        &self,
        _request: Request<NodeTypeRequest>,
    ) -> Result<Response<IdentityResponse>, Status> {
        Err(Status::unimplemented(
            "Key generation not yet implemented via gRPC",
        ))
    }

    async fn import_node_identity(
        &self,
        _request: Request<ImportIdentityRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        Err(Status::unimplemented(
            "Key import not yet implemented via gRPC",
        ))
    }

    /// Get a cosmos bech32 address for a stored key with custom prefix and coin type
    async fn get_key_address(
        &self,
        request: Request<GetKeyAddressRequest>,
    ) -> Result<Response<GetKeyAddressResponse>, Status> {
        let req = request.into_inner();

        // Get the key store from storage
        let key_store = self
            .state
            .s
            .get_cosmos_key_store()
            .await
            .map_err(|e| Status::internal(format!("Failed to access key store: {}", e)))?
            .ok_or_else(|| {
                Status::not_found("No key store found. Import a key with `ergors keys import-mnemonic`")
            })?;

        // Determine which key to use
        let key_name = if req.key_name.is_empty() {
            EncryptedCosmosKeyManager::get_default_key_name(&key_store)
                .ok_or_else(|| Status::not_found("No default key configured"))?
                .to_string()
        } else {
            req.key_name.clone()
        };

        // Get the encrypted key
        let encrypted_key = EncryptedCosmosKeyManager::get_key_by_name(&key_store, &key_name)
            .ok_or_else(|| Status::not_found(format!("Key '{}' not found", key_name)))?;

        // Get custody password from environment
        let password = std::env::var("ERGORS_CUSTODY_PASSWORD").map_err(|_| {
            Status::failed_precondition(
                "ERGORS_CUSTODY_PASSWORD environment variable not set",
            )
        })?;

        // Create key manager from store and unlock
        let mut manager = EncryptedCosmosKeyManager::from_store(&key_store);
        manager.unlock(&password).map_err(|e| {
            Status::internal(format!("Failed to unlock key manager: {}", e))
        })?;

        // Determine coin type (default to 118 for cosmos)
        let coin_type = if req.coin_type == 0 { 118 } else { req.coin_type };

        // Determine address prefix (default to original key's prefix)
        let address_prefix = if req.address_prefix.is_empty() {
            encrypted_key.address_prefix.clone()
        } else {
            req.address_prefix.clone()
        };

        // Derive keypair with custom coin type
        let keypair = manager
            .get_keypair_with_coin_type(encrypted_key, req.account_index, coin_type)
            .map_err(|e| Status::internal(format!("Failed to derive keypair: {}", e)))?;

        // Generate address with custom prefix
        let address = cosmos_address_from_pubkey(keypair.public_key(), &address_prefix)
            .map_err(|e| Status::internal(format!("Failed to generate address: {}", e)))?;

        Ok(Response::new(GetKeyAddressResponse {
            address,
            public_key: keypair.public_key().to_vec(),
            hd_path: keypair.hd_path().to_string(),
            key_name,
            address_prefix,
            coin_type,
        }))
    }

    // ============ Cosmos Key Management ============

    async fn list_cosmos_keys(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListCosmosKeysResponse>, Status> {
        let key_store = match self.state.s.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => {
                return Ok(Response::new(ListCosmosKeysResponse { keys: vec![] }));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to access key store: {}", e)));
            }
        };

        let default_name = EncryptedCosmosKeyManager::get_default_key_name(&key_store);

        let keys: Vec<CosmosKeyInfo> = key_store
            .keys
            .iter()
            .map(|k| {
                let address = key_store
                    .derived_accounts
                    .iter()
                    .find(|a| a.key_name == k.key_name)
                    .map(|a| a.address.clone())
                    .unwrap_or_default();

                CosmosKeyInfo {
                    key_name: k.key_name.clone(),
                    label: k.label.clone(),
                    address,
                    chain_id: k.chain_id.clone(),
                    is_default: default_name == Some(k.key_name.as_str()),
                }
            })
            .collect();

        Ok(Response::new(ListCosmosKeysResponse { keys }))
    }

    async fn import_cosmos_key(
        &self,
        request: Request<ImportCosmosKeyRequest>,
    ) -> Result<Response<ImportCosmosKeyResponse>, Status> {
        let req = request.into_inner();

        if req.mnemonic.is_empty() {
            return Err(Status::invalid_argument("Mnemonic cannot be empty"));
        }

        // Use custody password from akash context if available (ensures consistency).
        // Fall back to provided password for backward compatibility.
        let encryption_password = if let Some(ref akash_ctx) = self.state.akash {
            if akash_ctx.custody_password.is_empty() {
                if req.password.is_empty() {
                    return Err(Status::invalid_argument(
                        "Password required (custody password not available)"
                    ));
                }
                req.password.clone()
            } else {
                // Use custody password for consistency with cert operations
                akash_ctx.custody_password.clone()
            }
        } else {
            if req.password.is_empty() {
                return Err(Status::invalid_argument("Password cannot be empty"));
            }
            req.password.clone()
        };

        let key_name = if req.key_name.is_empty() {
            "default".to_string()
        } else {
            req.key_name
        };
        let chain_id = if req.chain_id.is_empty() {
            "akashnet-2".to_string()
        } else {
            req.chain_id
        };
        let address_prefix = if req.address_prefix.is_empty() {
            "akash".to_string()
        } else {
            req.address_prefix
        };

        // Load or create key store
        let mut store = match self.state.s.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => EncryptedCosmosKeyManager::create_empty_store(),
            Err(e) => return Err(Status::internal(format!("Failed to load key store: {}", e))),
        };

        // Check for duplicate key name
        if store.keys.iter().any(|k| k.key_name == key_name) {
            return Ok(Response::new(ImportCosmosKeyResponse {
                success: false,
                key: None,
                error_message: format!("Key '{}' already exists", key_name),
            }));
        }

        // Create key manager
        let mut manager = if store.keys.is_empty() {
            EncryptedCosmosKeyManager::new()
        } else {
            EncryptedCosmosKeyManager::from_store(&store)
        };

        // Unlock with password
        if let Err(e) = manager.unlock(&encryption_password) {
            return Ok(Response::new(ImportCosmosKeyResponse {
                success: false,
                key: None,
                error_message: format!("Failed to unlock key manager: {}", e),
            }));
        }

        // Import the mnemonic
        let (encrypted, account_info) = match manager.import_mnemonic_with_label(
            &key_name,
            &req.mnemonic,
            &chain_id,
            &address_prefix,
            &req.label,
            req.make_default,
        ) {
            Ok(result) => result,
            Err(e) => {
                return Ok(Response::new(ImportCosmosKeyResponse {
                    success: false,
                    key: None,
                    error_message: format!("Import failed: {}", e),
                }));
            }
        };

        // Check for duplicate address
        if EncryptedCosmosKeyManager::address_exists(&store, &account_info.address) {
            return Ok(Response::new(ImportCosmosKeyResponse {
                success: false,
                key: None,
                error_message: format!("Address {} already exists", account_info.address),
            }));
        }

        // Add to store and persist
        manager.add_key_to_store(&mut store, encrypted, account_info.clone());
        tracing::info!("💾 Saving cosmos key store with {} keys...", store.keys.len());
        if let Err(e) = self.state.s.put_cosmos_key_store(&store).await {
            tracing::error!("❌ Failed to save key store: {}", e);
            return Err(Status::internal(format!("Failed to save key store: {}", e)));
        }
        tracing::info!("✅ Key store saved successfully");

        // Verify the save by reading back
        match self.state.s.get_cosmos_key_store().await {
            Ok(Some(verified)) => {
                tracing::info!("✅ Verified: key store has {} keys after save", verified.keys.len());
            }
            Ok(None) => {
                tracing::error!("❌ Verification failed: key store is empty after save!");
            }
            Err(e) => {
                tracing::error!("❌ Verification failed: {}", e);
            }
        }

        // Update in-memory akash context key store if available
        if let Some(ref akash_ctx) = self.state.akash {
            let mut key_store = akash_ctx.key_store.write().await;
            *key_store = store;
            tracing::info!("✅ Updated in-memory key store");
        }

        Ok(Response::new(ImportCosmosKeyResponse {
            success: true,
            key: Some(CosmosKeyInfo {
                key_name,
                label: req.label,
                address: account_info.address,
                chain_id,
                is_default: req.make_default,
            }),
            error_message: String::new(),
        }))
    }

    async fn delete_cosmos_key(
        &self,
        request: Request<DeleteCosmosKeyRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Require valid akash context (daemon must be initialized with custody password)
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "Daemon not initialized with Akash context. Key deletion requires authentication.",
            )
        })?;

        // Verify custody password is valid by attempting to unlock key manager
        let mut manager = akash_ctx.key_manager.write().await;
        if manager.unlock(&akash_ctx.custody_password).is_err() {
            return Err(Status::unauthenticated(
                "Invalid custody password. Key deletion requires valid authentication.",
            ));
        }

        let mut store = match self.state.s.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(Status::not_found("No key store found")),
            Err(e) => return Err(Status::internal(format!("Failed to load key store: {}", e))),
        };

        if let Err(e) = EncryptedCosmosKeyManager::delete_key(&mut store, &req.key_name) {
            return Ok(Response::new(OperationResult {
                success: false,
                message: format!("Failed to delete key: {}", e),
            }));
        }

        if let Err(e) = self.state.s.put_cosmos_key_store(&store).await {
            return Err(Status::internal(format!("Failed to save key store: {}", e)));
        }

        // Update in-memory akash context key store
        let mut key_store = akash_ctx.key_store.write().await;
        *key_store = store;

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Key '{}' deleted", req.key_name),
        }))
    }

    async fn set_default_cosmos_key(
        &self,
        request: Request<SetDefaultCosmosKeyRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        let mut store = match self.state.s.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(Status::not_found("No key store found")),
            Err(e) => return Err(Status::internal(format!("Failed to load key store: {}", e))),
        };

        if let Err(e) = EncryptedCosmosKeyManager::set_default_key(&mut store, &req.key_name) {
            return Ok(Response::new(OperationResult {
                success: false,
                message: format!("Failed to set default: {}", e),
            }));
        }

        if let Err(e) = self.state.s.put_cosmos_key_store(&store).await {
            return Err(Status::internal(format!("Failed to save key store: {}", e)));
        }

        // Update in-memory akash context key store if available
        if let Some(ref akash_ctx) = self.state.akash {
            let mut key_store = akash_ctx.key_store.write().await;
            *key_store = store;
        }

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Key '{}' set as default", req.key_name),
        }))
    }

    // ============ Configuration ============

    async fn get_config(&self, _request: Request<Empty>) -> Result<Response<ConfigData>, Status> {
        let config_toml = toml::to_string_pretty(&self.state.c)
            .map_err(|e| Status::internal(format!("Failed to serialize config: {}", e)))?;

        Ok(Response::new(ConfigData {
            data: config_toml.into_bytes(),
            format: "toml".to_string(),
        }))
    }

    async fn update_config(
        &self,
        request: Request<ConfigUpdate>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Config update requested: {} = {}", req.key, req.value);

        Ok(Response::new(OperationResult {
            success: false,
            message: "Config updates via gRPC not yet implemented".to_string(),
        }))
    }

    // ============ Network ============

    async fn get_network_topology(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetworkTopology>, Status> {
        let nm = self.state.nm.lock().await;
        let topology = nm.get_topology().await;
        Ok(Response::new(topology))
    }

    async fn add_peer(
        &self,
        request: Request<PeerAddress>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Add peer requested: {}", req.address);

        Ok(Response::new(OperationResult {
            success: false,
            message: "Peer management via gRPC not yet implemented".to_string(),
        }))
    }

    async fn remove_peer(
        &self,
        request: Request<NodeIdRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Remove peer requested: {}", req.node_id);

        Ok(Response::new(OperationResult {
            success: false,
            message: "Peer management via gRPC not yet implemented".to_string(),
        }))
    }

    // ============ Network Routing (OpenCode Tools) ============

    async fn announce_node(
        &self,
        request: Request<AnnounceNodeRequest>,
    ) -> Result<Response<AnnounceNodeResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Announce node requested with {} capabilities, load_factor: {}",
            req.capabilities.len(),
            req.load_factor
        );

        // Get network manifold and announce to peers
        let mut nm = self.state.nm.lock().await;

        // Get peer count for response
        let topology = nm.get_topology().await;
        let peer_count = topology.online_nodes().len() as u32;

        // Announce to network if running
        if nm.is_running().await {
            if let Err(e) = nm.announce_node(req.capabilities.clone(), req.load_factor).await {
                tracing::warn!("Failed to announce node: {}", e);
                return Ok(Response::new(AnnounceNodeResponse {
                    acknowledged: false,
                    peers_notified: 0,
                }));
            }
        }

        Ok(Response::new(AnnounceNodeResponse {
            acknowledged: true,
            peers_notified: peer_count,
        }))
    }

    async fn route_message(
        &self,
        request: Request<RouteMessageRequest>,
    ) -> Result<Response<RouteMessageResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Route message requested: action={}, message_type={}",
            req.action,
            req.message_type
        );

        let nm = self.state.nm.lock().await;

        // Check if network is running
        if !nm.is_running().await {
            return Ok(Response::new(RouteMessageResponse {
                success: false,
                nodes_reached: 0,
                response_payload: None,
                error_message: "Network not running".to_string(),
            }));
        }

        match req.action.as_str() {
            "broadcast" => {
                // Broadcast to all peers
                match nm.broadcast_raw(&req.payload).await {
                    Ok(count) => Ok(Response::new(RouteMessageResponse {
                        success: true,
                        nodes_reached: count as u32,
                        response_payload: None,
                        error_message: String::new(),
                    })),
                    Err(e) => Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: format!("Broadcast failed: {}", e),
                    })),
                }
            }
            "send_to_role" => {
                // Send to nodes of a specific role
                let target_role = req.target_role.map(|r| NodeType::try_from(r).unwrap_or(NodeType::Unspecified));

                if target_role.is_none() {
                    return Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: "target_role is required for send_to_role action".to_string(),
                    }));
                }

                match nm.send_to_role_raw(target_role.unwrap(), &req.payload).await {
                    Ok(count) => Ok(Response::new(RouteMessageResponse {
                        success: true,
                        nodes_reached: count as u32,
                        response_payload: None,
                        error_message: String::new(),
                    })),
                    Err(e) => Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: format!("Send to role failed: {}", e),
                    })),
                }
            }
            "request" => {
                // Send request and wait for response
                let target_node_id = req.target_node_id.clone();

                if target_node_id.is_none() {
                    return Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: "target_node_id is required for request action".to_string(),
                    }));
                }

                let timeout = std::time::Duration::from_millis(req.timeout_ms as u64);
                match nm.request_raw(&target_node_id.unwrap(), &req.payload, timeout).await {
                    Ok(response) => Ok(Response::new(RouteMessageResponse {
                        success: true,
                        nodes_reached: 1,
                        response_payload: Some(response),
                        error_message: String::new(),
                    })),
                    Err(e) => Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: format!("Request failed: {}", e),
                    })),
                }
            }
            _ => Ok(Response::new(RouteMessageResponse {
                success: false,
                nodes_reached: 0,
                response_payload: None,
                error_message: format!("Unknown action: {}. Use 'broadcast', 'send_to_role', or 'request'", req.action),
            })),
        }
    }

    // ============ Providers ============

    async fn list_providers(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ProviderList>, Status> {
        // Return configured providers from LLM config
        let llm_config = self.state.c.llm();

        let providers: Vec<ProviderInfo> = llm_config
            .entities
            .iter()
            .map(|entity| ProviderInfo {
                name: entity.name.clone(),
                configured: true,
                enabled: entity.enabled,
            })
            .collect();

        // Get default provider name from index
        let default_provider = llm_config
            .entities
            .get(llm_config.default_entity as usize)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        Ok(Response::new(ProviderList {
            providers,
            default_provider,
        }))
    }

    async fn configure_provider(
        &self,
        request: Request<ProviderConfig>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Configure provider requested: {} (default: {})",
            req.name,
            req.set_as_default
        );

        Ok(Response::new(OperationResult {
            success: false,
            message: "Provider configuration via gRPC not yet implemented".to_string(),
        }))
    }

    async fn test_provider(
        &self,
        request: Request<ProviderName>,
    ) -> Result<Response<ProviderTestResult>, Status> {
        let req = request.into_inner();

        // Check if provider exists in config
        let llm_config = self.state.c.llm();
        let provider_exists = llm_config.entities.iter().any(|e| e.name == req.name);

        if !provider_exists {
            return Ok(Response::new(ProviderTestResult {
                success: false,
                latency_ms: 0,
                error_message: format!("Provider '{}' not found", req.name),
            }));
        }

        // TODO: Implement actual provider test
        Ok(Response::new(ProviderTestResult {
            success: true,
            latency_ms: 100,
            error_message: String::new(),
        }))
    }

    // ============ Auth Tokens ============

    async fn register_auth_token(
        &self,
        request: Request<TokenLabel>,
    ) -> Result<Response<TokenResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Register auth token requested: {}", req.label);

        Err(Status::unimplemented(
            "Auth token management not yet implemented",
        ))
    }

    async fn revoke_auth_token(
        &self,
        request: Request<TokenIdRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Revoke auth token requested: {}", req.token_id);

        Err(Status::unimplemented(
            "Auth token management not yet implemented",
        ))
    }

    async fn list_auth_tokens(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<TokenList>, Status> {
        Ok(Response::new(TokenList { tokens: vec![] }))
    }

    // ============ Streaming ============

    type StreamHealthStream =
        Pin<Box<dyn Stream<Item = Result<HealthUpdate, Status>> + Send + 'static>>;

    async fn stream_health(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamHealthStream>, Status> {
        let req = request.into_inner();
        let interval_ms = if req.interval_ms == 0 {
            1000
        } else {
            req.interval_ms
        };

        let state = self.state.clone();

        let stream = try_stream! {
            loop {
                let nm = state.nm.lock().await;
                let topology = nm.get_topology().await;
                let active_connections = topology.online_nodes().len() as u32;
                drop(nm);

                let update = HealthUpdate {
                    timestamp: Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    cpu_usage: 0.0,
                    memory_bytes: 0,
                    active_connections,
                    requests_per_second: 0,
                    component_status: Default::default(),
                };

                yield update;

                tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms as u64)).await;
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamLogsStream = Pin<Box<dyn Stream<Item = Result<LogEntry, Status>> + Send + 'static>>;

    async fn stream_logs(
        &self,
        request: Request<LogStreamRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let _req = request.into_inner();

        let stream = try_stream! {
            yield LogEntry {
                timestamp: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                level: "info".to_string(),
                target: "ergors".to_string(),
                message: "Log streaming started".to_string(),
                fields: Default::default(),
            };

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    // ============ Session Management ============

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Create session requested (type: {:?})", req.session_type);

        match self.session_manager.create_session(req).await {
            Ok(session) => Ok(Response::new(CreateSessionResponse {
                session: Some(session),
                success: true,
                error_message: String::new(),
            })),
            Err(e) => Err(Status::internal(format!("Failed to create session: {}", e))),
        }
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Get session requested: {}", req.session_id);

        match self.session_manager.get_session(&req.session_id).await {
            Ok(Some(session)) => Ok(Response::new(GetSessionResponse {
                session: Some(session),
                children: vec![], // Populated if include_children requested
            })),
            Ok(None) => Err(Status::not_found(format!(
                "Session {} not found",
                req.session_id
            ))),
            Err(e) => Err(Status::internal(format!("Failed to get session: {}", e))),
        }
    }

    async fn update_session(
        &self,
        request: Request<UpdateSessionRequest>,
    ) -> Result<Response<UpdateSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Update session requested: {}", req.session_id);

        let labels = if req.labels.is_empty() {
            None
        } else {
            Some(req.labels)
        };
        let metadata = if req.metadata.is_empty() {
            None
        } else {
            Some(req.metadata)
        };
        let tags = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags)
        };

        match self
            .session_manager
            .update_session(&req.session_id, labels, metadata, tags)
            .await
        {
            Ok(session) => Ok(Response::new(UpdateSessionResponse {
                session: Some(session),
            })),
            Err(e) => Err(Status::internal(format!("Failed to update session: {}", e))),
        }
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Delete session requested: {} (cascade: {})",
            req.session_id,
            req.cascade
        );

        match self
            .session_manager
            .delete_session(&req.session_id, req.cascade)
            .await
        {
            Ok(()) => Ok(Response::new(OperationResult {
                success: true,
                message: format!("Session {} deleted", req.session_id),
            })),
            Err(e) => Err(Status::internal(format!("Failed to delete session: {}", e))),
        }
    }

    async fn pause_session(
        &self,
        request: Request<PauseSessionRequest>,
    ) -> Result<Response<PauseSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Pause session requested: {} (cascade: {})",
            req.session_id,
            req.cascade
        );

        // Get the session first to include in response
        let session = self
            .session_manager
            .get_session(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get session: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session {} not found", req.session_id)))?;

        match self
            .session_manager
            .pause_session(&req.session_id, req.cascade)
            .await
        {
            Ok(snapshot) => Ok(Response::new(PauseSessionResponse {
                session: Some(session),
                paused_child_ids: vec![], // TODO: populate from cascade
                snapshot: Some(snapshot),
            })),
            Err(e) => Err(Status::internal(format!("Failed to pause session: {}", e))),
        }
    }

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Resume session requested: {} (cascade: {})",
            req.session_id,
            req.cascade
        );

        match self
            .session_manager
            .resume_session(&req.session_id, req.cascade)
            .await
        {
            Ok(session) => Ok(Response::new(ResumeSessionResponse {
                session: Some(session),
                resumed_child_ids: vec![], // TODO: populate from cascade
            })),
            Err(e) => Err(Status::internal(format!("Failed to resume session: {}", e))),
        }
    }

    async fn complete_session(
        &self,
        request: Request<CompleteSessionRequest>,
    ) -> Result<Response<CompleteSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Complete session requested: {}", req.session_id);

        match self
            .session_manager
            .complete_session(&req.session_id, req.result)
            .await
        {
            Ok(session) => {
                let metrics = session.metrics;
                Ok(Response::new(CompleteSessionResponse {
                    session: Some(session),
                    final_metrics: metrics,
                }))
            }
            Err(e) => Err(Status::internal(format!(
                "Failed to complete session: {}",
                e
            ))),
        }
    }

    async fn fail_session(
        &self,
        request: Request<FailSessionRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Fail session requested: {} ({})",
            req.session_id,
            req.error_message
        );

        let error_code = if req.error_code.is_empty() {
            None
        } else {
            Some(req.error_code.as_str())
        };

        match self
            .session_manager
            .fail_session(&req.session_id, &req.error_message, error_code)
            .await
        {
            Ok(_) => Ok(Response::new(OperationResult {
                success: true,
                message: format!("Session {} marked as failed", req.session_id),
            })),
            Err(e) => Err(Status::internal(format!("Failed to fail session: {}", e))),
        }
    }

    // ============ Fractal Hierarchy Operations ============

    async fn spawn_child_session(
        &self,
        request: Request<SpawnChildRequest>,
    ) -> Result<Response<SpawnChildResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Spawn child session requested for parent: {}",
            req.parent_session_id
        );

        match self.session_manager.spawn_child(req).await {
            Ok(session) => Ok(Response::new(SpawnChildResponse {
                child_session: Some(session),
                success: true,
                error_message: String::new(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to spawn child session: {}",
                e
            ))),
        }
    }

    async fn get_session_hierarchy(
        &self,
        request: Request<GetHierarchyRequest>,
    ) -> Result<Response<GetHierarchyResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Get session hierarchy requested: {}", req.session_id);

        let max_depth = if req.max_depth == 0 {
            None
        } else {
            Some(req.max_depth)
        };

        match self
            .session_manager
            .get_hierarchy(
                &req.session_id,
                req.include_ancestors,
                req.include_descendants,
                max_depth,
            )
            .await
        {
            Ok((session, ancestors, descendants)) => {
                let total = 1 + ancestors.len() + descendants.len();
                let max_d = descendants
                    .iter()
                    .map(|s| s.fractal_depth)
                    .max()
                    .unwrap_or(0);
                let active = descendants
                    .iter()
                    .filter(|s| s.status == i32::from(SessionStatus::Active))
                    .count();
                Ok(Response::new(GetHierarchyResponse {
                    session: Some(session),
                    ancestors,
                    descendants,
                    stats: Some(SessionHierarchyStats {
                        total_sessions: total as u32,
                        max_depth: max_d,
                        active_sessions: active as u32,
                        completed_sessions: 0,
                        aggregated_metrics: None,
                    }),
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to get hierarchy: {}", e))),
        }
    }

    async fn rollup_child_sessions(
        &self,
        request: Request<RollupRequest>,
    ) -> Result<Response<RollupResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Rollup child sessions requested: {}", req.parent_session_id);

        // Get the parent session with rolled up metrics
        let parent = self
            .session_manager
            .get_session(&req.parent_session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get session: {}", e)))?
            .ok_or_else(|| {
                Status::not_found(format!("Session {} not found", req.parent_session_id))
            })?;

        match self
            .session_manager
            .rollup_metrics(&req.parent_session_id)
            .await
        {
            Ok(metrics) => Ok(Response::new(RollupResponse {
                updated_parent: Some(parent),
                rolled_up_metrics: Some(metrics),
            })),
            Err(e) => Err(Status::internal(format!("Failed to rollup metrics: {}", e))),
        }
    }

    // ============ Query Operations ============

    async fn query_sessions(
        &self,
        request: Request<QuerySessionsRequest>,
    ) -> Result<Response<QuerySessionsResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Query sessions requested");

        match self.state.s.query_fractal_sessions(&req).await {
            Ok(sessions) => {
                let count = sessions.len() as u32;
                Ok(Response::new(QuerySessionsResponse {
                    sessions,
                    total_count: count,
                    has_more: false,
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to query sessions: {}", e))),
        }
    }

    async fn list_sessions_by_node(
        &self,
        request: Request<ListByNodeRequest>,
    ) -> Result<Response<ListByNodeResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("List sessions by node requested: {}", req.node_id);

        match self.state.s.get_sessions_by_owner(&req.node_id).await {
            Ok(sessions) => Ok(Response::new(ListByNodeResponse { sessions })),
            Err(e) => Err(Status::internal(format!("Failed to list sessions: {}", e))),
        }
    }

    async fn list_sessions_by_root(
        &self,
        request: Request<ListByRootRequest>,
    ) -> Result<Response<ListByRootResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("List sessions by root requested: {}", req.root_session_id);

        match self
            .state
            .s
            .get_sessions_by_root(&req.root_session_id)
            .await
        {
            Ok(sessions) => {
                let total = sessions.len() as u32;
                let active = sessions
                    .iter()
                    .filter(|s| s.status == i32::from(SessionStatus::Active))
                    .count() as u32;
                Ok(Response::new(ListByRootResponse {
                    sessions,
                    stats: Some(SessionHierarchyStats {
                        total_sessions: total,
                        max_depth: 0,
                        active_sessions: active,
                        completed_sessions: 0,
                        aggregated_metrics: None,
                    }),
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to list sessions: {}", e))),
        }
    }

    // ============ Cross-Node Operations ============

    async fn sync_session(
        &self,
        request: Request<SyncSessionRequest>,
    ) -> Result<Response<SyncSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Sync session requested: {} -> {}",
            req.session_id,
            req.target_node_id
        );

        // Cross-node sync requires network protocol - return success if session exists
        match self.session_manager.get_session(&req.session_id).await {
            Ok(Some(_)) => Ok(Response::new(SyncSessionResponse {
                success: true,
                sync_hash: String::new(),
                sessions_synced: 1,
            })),
            Ok(None) => Err(Status::not_found(format!(
                "Session {} not found",
                req.session_id
            ))),
            Err(e) => Err(Status::internal(format!("Failed to sync session: {}", e))),
        }
    }

    async fn migrate_session(
        &self,
        request: Request<MigrateSessionRequest>,
    ) -> Result<Response<MigrateSessionResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Migrate session requested: {} -> {}",
            req.session_id,
            req.target_node_id
        );

        // Migration requires network protocol - return session if exists
        match self.session_manager.get_session(&req.session_id).await {
            Ok(Some(session)) => Ok(Response::new(MigrateSessionResponse {
                migrated_session: Some(session),
                new_owner_node_id: req.target_node_id,
                migrated_child_ids: vec![],
            })),
            Ok(None) => Err(Status::not_found(format!(
                "Session {} not found",
                req.session_id
            ))),
            Err(e) => Err(Status::internal(format!(
                "Failed to migrate session: {}",
                e
            ))),
        }
    }

    // ============ Session Streaming ============

    type StreamSessionUpdatesStream =
        Pin<Box<dyn Stream<Item = Result<SessionUpdate, Status>> + Send + 'static>>;

    async fn stream_session_updates(
        &self,
        request: Request<StreamSessionRequest>,
    ) -> Result<Response<Self::StreamSessionUpdatesStream>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.clone();
        tracing::info!("Stream session updates requested: {}", session_id);

        // Verify session exists
        let session = self
            .session_manager
            .get_session(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get session: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session {} not found", session_id)))?;

        let session_manager = self.session_manager.clone();

        let initial_status = session.status;
        let stream = try_stream! {
            // Yield initial state
            yield SessionUpdate {
                session_id: session_id.clone(),
                old_status: initial_status,
                new_status: initial_status,
                timestamp: Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                update_type: "initial".to_string(),
                snapshot: Some(session),
            };

            let mut last_status = initial_status;

            // Poll for updates
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                if let Ok(Some(updated_session)) = session_manager.get_session(&session_id).await {
                    let new_status = updated_session.status;
                    yield SessionUpdate {
                        session_id: session_id.clone(),
                        old_status: last_status,
                        new_status,
                        timestamp: Some(pbjson_types::Timestamp {
                            seconds: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as i64,
                            nanos: 0,
                        }),
                        update_type: if new_status != last_status { "status".to_string() } else { "poll".to_string() },
                        snapshot: Some(updated_session),
                    };
                    last_status = new_status;
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    // ============ Workspace Management ============

    async fn add_workspace(
        &self,
        request: Request<AddWorkspaceRequest>,
    ) -> Result<Response<AddWorkspaceResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Add workspace requested: {}", req.name);

        // Generate workspace ID
        let workspace_id = uuid::Uuid::new_v4().to_string();

        // Create workspace metadata
        let workspace = ho_std::types::ergors::git::v1::WorkspaceMetadata {
            workspace_id: workspace_id.clone(),
            name: req.name.clone(),
            remote_url: req.remote_url.clone(),
            local_path: format!("~/.ergors/workspaces/{}", req.name),
            head_commit: vec![],
            default_branch: "main".to_string(),
            created_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                nanos: 0,
            }),
            last_synced: None,
        };

        // Store workspace
        self.state
            .s
            .put_workspace(&workspace)
            .await
            .map_err(|e| Status::internal(format!("Failed to store workspace: {}", e)))?;

        Ok(Response::new(AddWorkspaceResponse {
            success: true,
            workspace: Some(workspace),
            error_message: String::new(),
        }))
    }

    async fn get_workspace(
        &self,
        request: Request<GetWorkspaceRequest>,
    ) -> Result<Response<GetWorkspaceResponse>, Status> {
        let req = request.into_inner();

        let workspace = self
            .state
            .s
            .get_workspace(&req.workspace_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workspace: {}", e)))?;

        match workspace {
            Some(ws) => {
                // Get active worktrees for this workspace
                let worktrees = self
                    .state
                    .s
                    .list_task_worktrees_by_workspace(&req.workspace_id)
                    .await
                    .unwrap_or_default();
                Ok(Response::new(GetWorkspaceResponse {
                    workspace: Some(ws),
                    active_worktrees: worktrees,
                }))
            }
            None => Err(Status::not_found(format!(
                "Workspace '{}' not found",
                req.workspace_id
            ))),
        }
    }

    async fn list_workspaces(
        &self,
        _request: Request<ListWorkspacesRequest>,
    ) -> Result<Response<ListWorkspacesResponse>, Status> {
        let workspaces = self
            .state
            .s
            .list_workspaces()
            .await
            .map_err(|e| Status::internal(format!("Failed to list workspaces: {}", e)))?;

        let count = workspaces.len() as u32;
        Ok(Response::new(ListWorkspacesResponse {
            workspaces,
            total_count: count,
        }))
    }

    async fn remove_workspace(
        &self,
        request: Request<RemoveWorkspaceRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Remove workspace requested: {} (force: {})",
            req.workspace_id,
            req.force
        );

        // Check if workspace exists
        let workspace = self
            .state
            .s
            .get_workspace(&req.workspace_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workspace: {}", e)))?;

        if workspace.is_none() {
            return Err(Status::not_found(format!(
                "Workspace '{}' not found",
                req.workspace_id
            )));
        }

        // Check for active worktrees if not force
        if !req.force {
            let active_count = self
                .state
                .s
                .count_active_worktrees(&req.workspace_id)
                .await
                .map_err(|e| Status::internal(format!("Failed to count worktrees: {}", e)))?;

            if active_count > 0 {
                return Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Cannot remove workspace with {} active worktrees. Use --force to override.", active_count),
                }));
            }
        }

        // Delete workspace
        self.state
            .s
            .delete_workspace(&req.workspace_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete workspace: {}", e)))?;

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Workspace '{}' removed successfully", req.workspace_id),
        }))
    }

    async fn sync_workspace(
        &self,
        request: Request<SyncWorkspaceRequest>,
    ) -> Result<Response<SyncWorkspaceResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Sync workspace requested: {} (push: {}, fetch: {})",
            req.workspace_id,
            req.push,
            req.fetch
        );

        // TODO: Implement actual git sync operations
        Ok(Response::new(SyncWorkspaceResponse {
            success: true,
            message: "Sync operation not yet implemented".to_string(),
            new_head_commit: vec![],
        }))
    }

    async fn create_task_worktree(
        &self,
        request: Request<CreateTaskWorktreeRequest>,
    ) -> Result<Response<CreateTaskWorktreeResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Create task worktree requested: workspace={}, task={}",
            req.workspace_id,
            req.task_id
        );

        // Check if workspace exists
        let workspace = self
            .state
            .s
            .get_workspace(&req.workspace_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workspace: {}", e)))?;

        if workspace.is_none() {
            return Err(Status::not_found(format!(
                "Workspace '{}' not found",
                req.workspace_id
            )));
        }

        let workspace = workspace.unwrap();

        // Create task worktree metadata
        let worktree = ho_std::types::ergors::git::v1::TaskWorktree {
            task_id: req.task_id.clone(),
            workspace_id: req.workspace_id.clone(),
            branch: format!("task/{}", req.task_id),
            worktree_path: format!("{}/tasks/task-{}", workspace.local_path, req.task_id),
            base_commit: workspace.head_commit.clone(),
            status: ho_std::types::ergors::git::v1::TaskWorktreeStatus::Active as i32,
            assigned_node_id: req.assigned_node_id.clone(),
            created_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                nanos: 0,
            }),
        };

        // Store worktree
        self.state
            .s
            .put_task_worktree(&worktree)
            .await
            .map_err(|e| Status::internal(format!("Failed to store task worktree: {}", e)))?;

        Ok(Response::new(CreateTaskWorktreeResponse {
            success: true,
            worktree: Some(worktree),
            error_message: String::new(),
        }))
    }

    async fn list_task_worktrees(
        &self,
        request: Request<ListTaskWorktreesRequest>,
    ) -> Result<Response<ListTaskWorktreesResponse>, Status> {
        let req = request.into_inner();

        let worktrees = if !req.workspace_id.is_empty() {
            self.state
                .s
                .list_task_worktrees_by_workspace(&req.workspace_id)
                .await
                .map_err(|e| Status::internal(format!("Failed to list worktrees: {}", e)))?
        } else if !req.assigned_node_id.is_empty() {
            self.state
                .s
                .list_task_worktrees_by_node(&req.assigned_node_id)
                .await
                .map_err(|e| Status::internal(format!("Failed to list worktrees: {}", e)))?
        } else {
            vec![]
        };

        Ok(Response::new(ListTaskWorktreesResponse { worktrees }))
    }

    async fn complete_task_worktree(
        &self,
        request: Request<CompleteTaskWorktreeRequest>,
    ) -> Result<Response<CompleteTaskWorktreeResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Complete task worktree requested: task={} (merge: {})",
            req.task_id,
            req.merge_to_main
        );

        // Get the worktree
        let worktree = self
            .state
            .s
            .get_task_worktree(&req.task_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get worktree: {}", e)))?;

        match worktree {
            Some(mut wt) => {
                // Update status to committing
                wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Committing as i32;
                self.state
                    .s
                    .put_task_worktree(&wt)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to update worktree: {}", e)))?;

                // Get the workspace to get paths
                let workspace = self
                    .state
                    .s
                    .get_workspace(&wt.workspace_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to get workspace: {}", e)))?
                    .ok_or_else(|| Status::not_found(format!("Workspace '{}' not found", wt.workspace_id)))?;

                // Perform git operations
                let worktree_path = std::path::PathBuf::from(&wt.worktree_path);
                let workspace_path = std::path::PathBuf::from(&workspace.local_path);

                // Check if paths exist
                if !worktree_path.exists() {
                    return Err(Status::failed_precondition(format!(
                        "Worktree path does not exist: {:?}",
                        worktree_path
                    )));
                }

                // Open the worktree repository and commit changes
                let mut worktree_repo = ho_std::git::GitRepository::open(&worktree_path)
                    .map_err(|e| Status::internal(format!("Failed to open worktree: {}", e)))?;

                // Get git identity from config
                let identity = self.state.c.identity();
                let node_id = identity
                    .public_key
                    .as_ref()
                    .map(hex::encode)
                    .unwrap_or_else(|| "local".to_string());
                let git_identity = ho_std::git::GitIdentity::minimal(&node_id, &identity.node_type);
                worktree_repo.set_identity(git_identity.clone());

                // Stage and commit changes
                worktree_repo
                    .stage_all()
                    .map_err(|e| Status::internal(format!("Failed to stage changes: {}", e)))?;

                let commit_message = if req.commit_message.is_empty() {
                    format!("Complete task {}", req.task_id)
                } else {
                    req.commit_message.clone()
                };
                let task_commit_hash = worktree_repo
                    .commit(&commit_message)
                    .map_err(|e| Status::internal(format!("Failed to commit: {}", e)))?;

                tracing::info!(
                    "Committed changes for task {} with hash {}",
                    req.task_id,
                    task_commit_hash
                );

                // If merge_to_main is requested, perform the merge
                let (merged, final_hash) = if req.merge_to_main && workspace_path.exists() {
                    wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Merging as i32;
                    self.state
                        .s
                        .put_task_worktree(&wt)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to update worktree: {}", e)))?;

                    // Open the main workspace repository
                    let mut main_repo = ho_std::git::GitRepository::open(&workspace_path)
                        .map_err(|e| Status::internal(format!("Failed to open workspace: {}", e)))?;
                    main_repo.set_identity(git_identity);

                    // Checkout main branch
                    main_repo.checkout_branch("main").or_else(|_| {
                        main_repo.checkout_branch("master")
                    }).map_err(|e| Status::internal(format!("Failed to checkout main: {}", e)))?;

                    // Merge the task branch
                    match main_repo.merge_branch(&wt.branch) {
                        Ok(ho_std::git::MergeResult::FastForward(hash)) => {
                            tracing::info!("Fast-forward merged task {} to main: {}", req.task_id, hash);
                            (true, hash)
                        }
                        Ok(ho_std::git::MergeResult::Merged(hash)) => {
                            tracing::info!("Merged task {} to main with commit: {}", req.task_id, hash);
                            (true, hash)
                        }
                        Ok(ho_std::git::MergeResult::UpToDate) => {
                            tracing::info!("Task {} already up-to-date with main", req.task_id);
                            (true, task_commit_hash.clone())
                        }
                        Ok(ho_std::git::MergeResult::Conflict(conflicts)) => {
                            tracing::warn!(
                                "Merge conflict for task {}: {} files",
                                req.task_id,
                                conflicts.len()
                            );
                            // Mark as conflict status
                            const TASK_WORKTREE_STATUS_CONFLICT: i32 = 7;
                            wt.status = TASK_WORKTREE_STATUS_CONFLICT;
                            self.state.s.put_task_worktree(&wt).await.ok();

                            return Ok(Response::new(CompleteTaskWorktreeResponse {
                                success: false,
                                merged: false,
                                commit_hash: task_commit_hash,
                                error_message: format!(
                                    "Merge conflict in {} files: {}",
                                    conflicts.len(),
                                    conflicts.join(", ")
                                ),
                            }));
                        }
                        Err(e) => {
                            tracing::error!("Merge failed for task {}: {}", req.task_id, e);
                            wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Failed as i32;
                            self.state.s.put_task_worktree(&wt).await.ok();

                            return Ok(Response::new(CompleteTaskWorktreeResponse {
                                success: false,
                                merged: false,
                                commit_hash: task_commit_hash,
                                error_message: format!("Merge failed: {}", e),
                            }));
                        }
                    }
                } else {
                    (false, task_commit_hash.clone())
                };

                // Update worktree status to completed
                wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Completed as i32;
                self.state
                    .s
                    .put_task_worktree(&wt)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to update worktree: {}", e)))?;

                // Update workspace head commit
                let mut updated_workspace = workspace;
                updated_workspace.head_commit = final_hash.as_bytes().to_vec();
                updated_workspace.last_synced = Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    nanos: 0,
                });
                self.state.s.put_workspace(&updated_workspace).await.ok();

                Ok(Response::new(CompleteTaskWorktreeResponse {
                    success: true,
                    merged,
                    commit_hash: final_hash,
                    error_message: String::new(),
                }))
            }
            None => Err(Status::not_found(format!(
                "Task worktree '{}' not found",
                req.task_id
            ))),
        }
    }

    async fn fail_task_worktree(
        &self,
        request: Request<FailTaskWorktreeRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Fail task worktree requested: task={} (cleanup: {})",
            req.task_id,
            req.cleanup
        );

        // Get the worktree
        let worktree = self
            .state
            .s
            .get_task_worktree(&req.task_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get worktree: {}", e)))?;

        match worktree {
            Some(mut wt) => {
                if req.cleanup {
                    // Delete the worktree record
                    self.state
                        .s
                        .delete_task_worktree(&req.task_id)
                        .await
                        .map_err(|e| {
                            Status::internal(format!("Failed to delete worktree: {}", e))
                        })?;
                } else {
                    // Just mark as failed
                    wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Failed as i32;
                    self.state.s.put_task_worktree(&wt).await.map_err(|e| {
                        Status::internal(format!("Failed to update worktree: {}", e))
                    })?;
                }

                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Task '{}' marked as failed: {}", req.task_id, req.reason),
                }))
            }
            None => Err(Status::not_found(format!(
                "Task worktree '{}' not found",
                req.task_id
            ))),
        }
    }

    async fn resolve_conflict(
        &self,
        request: Request<ResolveConflictRequest>,
    ) -> Result<Response<ResolveConflictResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Resolve conflict requested: task={}, strategy={:?}",
            req.task_id,
            req.strategy
        );

        // Get the worktree
        let worktree = self
            .state
            .s
            .get_task_worktree(&req.task_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get worktree: {}", e)))?;

        match worktree {
            Some(mut wt) => {
                // Check if worktree is in conflict state
                const TASK_WORKTREE_STATUS_CONFLICT: i32 = 7;
                if wt.status != TASK_WORKTREE_STATUS_CONFLICT {
                    return Err(Status::failed_precondition(format!(
                        "Task '{}' is not in conflict state (status: {})",
                        req.task_id, wt.status
                    )));
                }

                // Get the workspace
                let workspace = self
                    .state
                    .s
                    .get_workspace(&wt.workspace_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to get workspace: {}", e)))?
                    .ok_or_else(|| Status::not_found(format!("Workspace '{}' not found", wt.workspace_id)))?;

                let workspace_path = std::path::PathBuf::from(&workspace.local_path);

                // Open the workspace repository
                let mut repo = ho_std::git::GitRepository::open(&workspace_path)
                    .map_err(|e| Status::internal(format!("Failed to open workspace: {}", e)))?;

                // Get git identity
                let identity = self.state.c.identity();
                let node_id = identity
                    .public_key
                    .as_ref()
                    .map(hex::encode)
                    .unwrap_or_else(|| "local".to_string());
                let git_identity = ho_std::git::GitIdentity::minimal(&node_id, &identity.node_type);
                repo.set_identity(git_identity);

                // Convert proto strategy to internal strategy
                let strategy = match req.strategy {
                    1 => ho_std::git::ConflictStrategy::Ours,   // OURS
                    2 => ho_std::git::ConflictStrategy::Theirs, // THEIRS
                    4 => ho_std::git::ConflictStrategy::Abort,  // ABORT
                    3 => {
                        // MANUAL - not yet implemented
                        return Err(Status::unimplemented(
                            "Manual conflict resolution not yet implemented",
                        ));
                    }
                    _ => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid conflict resolution strategy: {}",
                            req.strategy
                        )));
                    }
                };

                // Check if repo has conflicts
                if !repo.has_conflicts() {
                    return Err(Status::failed_precondition(
                        "Repository is not in a conflicted merge state",
                    ));
                }

                // Resolve conflicts
                match repo.resolve_conflicts_with_strategy(strategy) {
                    Ok(Some(commit_hash)) => {
                        tracing::info!(
                            "Resolved conflicts for task {} with commit {}",
                            req.task_id,
                            commit_hash
                        );

                        // Update worktree status to completed
                        wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Completed as i32;
                        self.state.s.put_task_worktree(&wt).await.ok();

                        // Update workspace head commit
                        let mut updated_workspace = workspace;
                        updated_workspace.head_commit = commit_hash.as_bytes().to_vec();
                        self.state.s.put_workspace(&updated_workspace).await.ok();

                        Ok(Response::new(ResolveConflictResponse {
                            success: true,
                            commit_hash,
                            remaining_conflicts: vec![],
                            error_message: String::new(),
                        }))
                    }
                    Ok(None) => {
                        // Abort was requested
                        tracing::info!("Aborted merge for task {}", req.task_id);

                        // Update worktree status back to active
                        wt.status = ho_std::types::ergors::git::v1::TaskWorktreeStatus::Active as i32;
                        self.state.s.put_task_worktree(&wt).await.ok();

                        Ok(Response::new(ResolveConflictResponse {
                            success: true,
                            commit_hash: String::new(),
                            remaining_conflicts: vec![],
                            error_message: "Merge aborted".to_string(),
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Failed to resolve conflicts for task {}: {}", req.task_id, e);

                        Ok(Response::new(ResolveConflictResponse {
                            success: false,
                            commit_hash: String::new(),
                            remaining_conflicts: repo.get_conflicting_files().unwrap_or_default(),
                            error_message: format!("Failed to resolve conflicts: {}", e),
                        }))
                    }
                }
            }
            None => Err(Status::not_found(format!(
                "Task worktree '{}' not found",
                req.task_id
            ))),
        }
    }

    // ============ Akash Deployment Management ============

    async fn create_akash_deployment(
        &self,
        request: Request<CreateAkashDeploymentRequest>,
    ) -> Result<Response<CreateAkashDeploymentResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "Create Akash deployment: key={}, chain={}, node={}",
            req.key_name,
            req.chain_id,
            req.node_endpoint
        );

        let session_id = uuid::Uuid::new_v4().to_string();

        // Use engine config defaults, override with request params if provided
        let akash_config = self.state.c.akash(); // Uses default_akash_config() if not set
        let chain_id = if !req.chain_id.is_empty() {
            req.chain_id.clone()
        } else {
            akash_config.chain_id.clone()
        };
        let node_endpoint = if !req.node_endpoint.is_empty() {
            req.node_endpoint.clone()
        } else if !akash_config.rpc_endpoints.is_empty() {
            akash_config.rpc_endpoints[0].clone()
        } else {
            "https://rpc-akash.ecostake.com:443".to_string()
        };

        // Get key store and resolve account address
        let key_store = match self.state.s.get_cosmos_key_store().await {
            Ok(Some(ks)) => ks,
            Ok(None) => {
                return Ok(Response::new(CreateAkashDeploymentResponse {
                    success: false,
                    workflow: None,
                    error_message: "No key store found. Import a key with `ergors keys import-mnemonic`".to_string(),
                }));
            }
            Err(e) => {
                return Ok(Response::new(CreateAkashDeploymentResponse {
                    success: false,
                    workflow: None,
                    error_message: format!("Failed to access key store: {}", e),
                }));
            }
        };

        // Determine key name: use request param or default
        let key_name = if !req.key_name.is_empty() {
            req.key_name.clone()
        } else {
            // Use default key
            match ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager::get_default_key_name(&key_store) {
                Some(name) => name.to_string(),
                None => {
                    return Ok(Response::new(CreateAkashDeploymentResponse {
                        success: false,
                        workflow: None,
                        error_message: "No key specified and no default key set. Use `ergors keys set-default --key-name <name>`".to_string(),
                    }));
                }
            }
        };

        // Look up account address from key store
        let account = match key_store.derived_accounts.iter().find(|a| {
            a.key_name == key_name && a.account_index == req.hd_account_index
        }) {
            Some(acc) => acc,
            None => {
                return Ok(Response::new(CreateAkashDeploymentResponse {
                    success: false,
                    workflow: None,
                    error_message: format!(
                        "Key '{}' with account index {} not found. Use `ergors keys list` to see available keys.",
                        key_name, req.hd_account_index
                    ),
                }));
            }
        };

        let account_address = account.address.clone();
        tracing::info!("Resolved account address: {}", account_address);

        // Check for label collision if label is provided
        if !req.label.is_empty() {
            match self.state.s.check_label_collision(&req.label).await {
                Ok(Some(existing_session_id)) => {
                    return Ok(Response::new(CreateAkashDeploymentResponse {
                        success: false,
                        workflow: None,
                        error_message: format!(
                            "Label '{}' is already in use by active deployment: {}. Please choose a different label or close the existing deployment first.",
                            req.label, existing_session_id
                        ),
                    }));
                }
                Ok(None) => {
                    tracing::info!("Label '{}' is available", req.label);
                }
                Err(e) => {
                    tracing::error!("Failed to check label collision: {}", e);
                    return Ok(Response::new(CreateAkashDeploymentResponse {
                        success: false,
                        workflow: None,
                        error_message: format!("Failed to validate label: {}", e),
                    }));
                }
            }
        }

        let now = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        };

        // Configure SDL if provided
        let configured_sdl = if !req.sdl_content.is_empty() {
            Some(ConfiguredSdl {
                template_name: req.template_name,
                resolved_content: req.sdl_content,
                variable_values: req.sdl_variables,
                content_hash: vec![],
                configured_at: Some(now),
            })
        } else {
            None
        };

        let workflow = AkashDeploymentWorkflow {
            session_id: session_id.clone(),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            selected_key_name: key_name,
            account_address,
            hd_account_index: req.hd_account_index,
            authz_grants: vec![],
            feegrants: vec![],
            configured_sdl,
            deployment: None,
            provider: None,
            endpoints: std::collections::HashMap::new(),
            test_results: vec![],
            last_error: String::new(),
            retry_count: 0,
            created_at: Some(now),
            updated_at: Some(now),
            completed_at: None,
            chain_id,
            node_endpoint,
            max_retries: 3,
            timeout_seconds: 3600,
            grant_request: None,
            request_grant_from: vec![],
            grant_duration_seconds: 0,
            grant_spend_limit_uakt: 0,
            grant_purpose: String::new(),
            // Automated workflow fields
            available_bids: vec![],
            certificate: None,
            encrypted_cert_private_key: vec![],
            lease_id_info: None,
            options: None,
            service_endpoints: vec![],
            // User-defined label for easy access (empty if not provided)
            label: req.label.clone(),
            // Actual model name for inference routing (stamped onto endpoints)
            model_name: req.model_name.clone(),
        };

        // Persist to storage
        if let Err(e) = self.state.s.put_akash_workflow(&workflow).await {
            return Ok(Response::new(CreateAkashDeploymentResponse {
                success: false,
                workflow: None,
                error_message: format!("Failed to persist workflow: {}", e),
            }));
        }

        Ok(Response::new(CreateAkashDeploymentResponse {
            success: true,
            workflow: Some(workflow),
            error_message: String::new(),
        }))
    }

    async fn list_akash_deployments(
        &self,
        request: Request<ListAkashDeploymentsRequest>,
    ) -> Result<Response<ListAkashDeploymentsResponse>, Status> {
        let req = request.into_inner();

        let workflows = self
            .state
            .s
            .list_akash_workflows()
            .await
            .map_err(|e| Status::internal(format!("Failed to list workflows: {}", e)))?;

        // Apply status filter if set
        let filtered: Vec<_> = if req.status != 0 {
            workflows
                .into_iter()
                .filter(|wf| wf.status == req.status)
                .collect()
        } else {
            workflows
        };

        let total_count = filtered.len() as u32;
        let limited = filtered
            .into_iter()
            .skip(req.offset as usize)
            .take(if req.limit > 0 { req.limit as usize } else { 50 })
            .collect();

        Ok(Response::new(ListAkashDeploymentsResponse {
            workflows: limited,
            total_count,
        }))
    }

    async fn get_akash_deployment(
        &self,
        request: Request<GetAkashDeploymentRequest>,
    ) -> Result<Response<GetAkashDeploymentResponse>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let workflow = match self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
        {
            Ok(wf) => Some(wf),
            Err(ho_std::error::HoError::Storage(ref msg)) if msg.contains("No deployment found") => {
                None
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get workflow: {}", e)));
            }
        };

        Ok(Response::new(GetAkashDeploymentResponse { workflow }))
    }

    /// DEPRECATED: Manual step advancement is no longer supported.
    /// Use run_akash_deployment for automated workflows.
    async fn advance_akash_deployment(
        &self,
        _request: Request<AdvanceAkashDeploymentRequest>,
    ) -> Result<Response<AdvanceAkashDeploymentResponse>, Status> {
        Err(Status::unimplemented(
            "Manual workflow advancement is deprecated. Use automated deployment with `deploy create` instead."
        ))
    }

    async fn query_akash_bids(
        &self,
        request: Request<QueryAkashBidsRequest>,
    ) -> Result<Response<QueryAkashBidsResponse>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // TODO: Query actual bids from Akash node using workflow.node_endpoint
        // For now, return empty bids list
        tracing::info!(
            "Querying bids for deployment on node: {}",
            workflow.node_endpoint
        );

        Ok(Response::new(QueryAkashBidsResponse {
            bids: vec![],
            total_bids: 0,
        }))
    }

    async fn select_akash_provider(
        &self,
        request: Request<SelectAkashProviderRequest>,
    ) -> Result<Response<SelectAkashProviderResponse>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let mut workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Update provider selection
        workflow.provider = Some(ho_std::types::ergors::orch::v1::AkashProviderSelection {
            provider_address: req.provider_address,
            reputation_score: 0,
            bid_price_uakt: req.bid_price_uakt,
            total_bids_received: 0,
            selected_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                nanos: 0,
            }),
            is_trusted_provider: false,
        });

        workflow.current_step = AkashWorkflowStep::LeaseCreate as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Persist
        self.state.s.put_akash_workflow(&workflow).await.ok();

        Ok(Response::new(SelectAkashProviderResponse {
            success: true,
            workflow: Some(workflow),
            error_message: String::new(),
        }))
    }

    async fn cancel_akash_deployment(
        &self,
        request: Request<CancelAkashDeploymentRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let mut workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        workflow.status = AkashWorkflowStatus::Cancelled as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Deactivate label index so label can be reused
        if !workflow.label.is_empty() {
            if let Err(e) = self.state.s.deactivate_deployment_label(&workflow.label).await {
                tracing::warn!("Failed to deactivate label '{}': {}", workflow.label, e);
            }
        }

        // Persist
        self.state.s.put_akash_workflow(&workflow).await.ok();

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Deployment {} cancelled", req.session_id),
        }))
    }

    async fn set_workflow_endpoints(
        &self,
        request: Request<SetWorkflowEndpointsRequest>,
    ) -> Result<Response<SetWorkflowEndpointsResponse>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let mut workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Store discovered endpoints in the workflow
        workflow.endpoints = req.endpoints;
        workflow.current_step = AkashWorkflowStep::EndpointRetrieval as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Persist
        self.state.s.put_akash_workflow(&workflow).await.ok();

        tracing::info!(
            "Set {} endpoints for workflow {}",
            workflow.endpoints.len(),
            req.session_id
        );

        Ok(Response::new(SetWorkflowEndpointsResponse {
            success: true,
            workflow: Some(workflow),
            error_message: String::new(),
        }))
    }

    async fn configure_proxy_routes(
        &self,
        request: Request<ConfigureProxyRoutesRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Load current config from storage to get version
        let current_version = self
            .state
            .s
            .get_proxy_router_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to get current config: {}", e)))?
            .map(|c| c.version)
            .unwrap_or(0);

        // Create new proto config with incremented version
        let proto_config = ho_std::types::ergors::orch::v1::ProxyRouterConfig {
            ollama_base_url: req.ollama_base_url.clone(),
            model_routes: req.model_routes.clone(),
            providers: std::collections::HashMap::new(), // TODO: populate from request
            updated_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                nanos: 0,
            }),
            version: current_version + 1,
            change_reason: "Updated via gRPC configure_proxy_routes".to_string(),
        };

        // Persist to cnidarium for immutable audit log
        self.state
            .s
            .put_proxy_router_config(&proto_config)
            .await
            .map_err(|e| Status::internal(format!("Failed to persist config: {}", e)))?;

        // Update in-memory proxy router with the proto config
        let mut router = self.state.pr.write().await;
        router.update_config(proto_config.clone());

        tracing::info!(
            "Proxy routes configured v{}: {} providers, {} model routes",
            proto_config.version,
            proto_config.providers.len(),
            proto_config.model_routes.len(),
        );

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Proxy routes configured (version {})", proto_config.version),
        }))
    }

    /// Run automated deployment workflow using AutomatedDeployer.
    ///
    /// This is the main entry point for fully automated Akash deployments.
    /// It executes all steps without user intervention:
    /// 1. Check balance
    /// 2. Setup/verify certificate
    /// 3. Create deployment transaction
    /// 4. Wait for and collect bids
    /// 5. Select provider (cheapest from trusted list)
    /// 6. Create lease
    /// 7. Send manifest
    /// 8. Retrieve and save endpoints
    async fn run_akash_deployment(
        &self,
        request: Request<RunAkashDeploymentRequest>,
    ) -> Result<Response<RunAkashDeploymentResponse>, Status> {
        let req = request.into_inner();

        // Check if Akash context is available
        let akash_ctx = self
            .state
            .akash
            .as_ref()
            .ok_or_else(|| Status::failed_precondition(
                "Akash deployment context not initialized. \
                 Ensure Akash config is present and keys are imported."
            ))?;

        // Get the workflow - support both session-id and label lookups
        let mut workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Apply options if provided
        let options = req.options.unwrap_or_else(|| AkashWorkflowOptions {
            min_balance_uakt: 5_000_000,
            bid_wait_blocks: 2,
            trusted_providers: vec![],
            max_retries: 3,
            interactive_bid: false,          // Auto-select by default
            request_grant_from: String::new(), // No grant request by default
            grant_duration_seconds: 86400,   // 24 hours
            grant_spend_limit_uakt: 5_000_000, // 5 AKT
        });

        tracing::info!(
            "Running automated deployment for session {}",
            req.session_id
        );

        // Unlock key manager with provided password
        if !req.key_password.is_empty() {
            let mut key_manager = akash_ctx.key_manager.write().await;
            if !key_manager.is_unlocked() {
                tracing::info!("🔐 Unlocking Cosmos key manager for deployment signing...");
                key_manager.unlock(&req.key_password).map_err(|e| {
                    Status::unauthenticated(format!("Failed to unlock key manager: {}", e))
                })?;
                tracing::info!("🔓 Cosmos key manager unlocked");
            } else {
                tracing::debug!("Key manager already unlocked");
            }
        } else if !akash_ctx.key_manager.read().await.is_unlocked() {
            return Err(Status::unauthenticated(
                "Key manager is locked. Provide key_password to unlock for signing."
            ));
        }

        // Create deployer from context
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        // Capture state for background task
        let storage = self.state.s.clone();
        let router = self.state.r.clone();
        let label = workflow.label.clone();
        let session_id = workflow.session_id.clone();

        // Update workflow to running status before spawning
        workflow.status = AkashWorkflowStatus::Running as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Clone workflow for response (before moving into background task)
        let response_workflow = workflow.clone();

        tracing::info!(
            "🚀 Spawning automated deployment workflow in background for session {}",
            session_id
        );

        // Spawn the automated deployment in a background task
        // This allows the CLI to exit immediately after password validation
        tokio::spawn(async move {
            match deployer.deploy(&mut workflow, &options).await {
                Ok(result) => {
                    tracing::info!(
                        "✅ Deployment completed successfully: session={}, dseq={}, provider={}, endpoints={}",
                        result.session_id,
                        result.dseq,
                        result.provider,
                        result.endpoints.len()
                    );

                    // Add deployment to inference cache if it has a label
                    if !label.is_empty() {
                        if let Err(e) = router.deployment_cache().add_deployment(&workflow).await {
                            tracing::warn!(
                                "Failed to add deployment '{}' to inference cache: {}",
                                label,
                                e
                            );
                        } else {
                            tracing::info!(
                                "Added deployment '{}' to inference cache - now available as model",
                                label
                            );
                        }
                    }
                }
                Err(e) => {
                    // Determine which step failed based on current_step
                    let failed_step = AkashWorkflowStep::try_from(workflow.current_step)
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_else(|_| format!("step_{}", workflow.current_step));

                    tracing::error!(
                        "❌ Automated deployment FAILED at step '{}': {}",
                        failed_step,
                        e
                    );

                    // Update workflow status to failed with detailed error
                    workflow.status = AkashWorkflowStatus::Failed as i32;
                    workflow.last_error = format!("[{}] {}", failed_step, e);
                    workflow.updated_at = Some(pbjson_types::Timestamp {
                        seconds: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    });

                    // Persist failed state - log any storage errors
                    if let Err(storage_err) = storage.put_akash_workflow(&workflow).await {
                        tracing::error!(
                            "Failed to persist workflow failure state: {} (session: {})",
                            storage_err,
                            workflow.session_id
                        );
                    } else {
                        tracing::info!(
                            "💾 Persisted failed workflow state: session={}, step={}, error={}",
                            workflow.session_id,
                            failed_step,
                            e
                        );
                    }

                    // Deactivate label from active deployments set on failure
                    if !label.is_empty() {
                        if let Err(e) = storage.deactivate_deployment_label(&label).await {
                            tracing::warn!("Failed to deactivate label '{}': {}", label, e);
                        } else {
                            tracing::info!("Deactivated label '{}' from active deployments (workflow failed)", label);
                        }
                    }
                }
            }
        });

        // Return immediately with running status
        // The CLI will exit and the workflow continues in the engine
        tracing::info!(
            "✅ Deployment workflow started successfully for session {}",
            session_id
        );
        tracing::info!(
            "   Use 'ergors deploy get {}' to check status",
            session_id
        );

        Ok(Response::new(RunAkashDeploymentResponse {
            workflow: Some(response_workflow),
            completed: false,
            input_required: None,
        }))
    }

    /// Close an active lease by submitting MsgCloseDeployment transaction.
    async fn close_akash_lease(
        &self,
        request: Request<CloseAkashLeaseRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Check if Akash context is available
        let akash_ctx = self
            .state
            .akash
            .as_ref()
            .ok_or_else(|| Status::failed_precondition(
                "Akash deployment context not initialized"
            ))?;

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Verify there's a lease to close
        if workflow.lease_id_info.is_none() && workflow.deployment.is_none() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "No active lease found for this workflow".to_string(),
            }));
        }

        tracing::info!("Closing lease for session {}", req.session_id);

        // Create deployer and close the deployment
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        match deployer.close_deployment(&workflow).await {
            Ok(()) => {
                // Update workflow status
                let mut updated_workflow = workflow.clone();
                updated_workflow.status = AkashWorkflowStatus::Cancelled as i32;
                updated_workflow.updated_at = Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                });
                self.state.s.put_akash_workflow(&updated_workflow).await.ok();

                // Remove from inference cache
                if !workflow.label.is_empty() {
                    if let Err(e) = self.state.r.deployment_cache().remove_deployment(&workflow.label).await {
                        tracing::warn!(
                            "Failed to remove deployment '{}' from inference cache: {}",
                            workflow.label,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Removed deployment '{}' from inference cache",
                            workflow.label
                        );
                    }
                }

                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Lease closed successfully for session {}", req.session_id),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to close lease: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to close lease: {}", e),
                }))
            }
        }
    }

    /// Close a deployment (also closes any active leases)
    async fn close_akash_deployment(
        &self,
        request: Request<CloseAkashDeploymentRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Check if Akash context is available
        let akash_ctx = self
            .state
            .akash
            .as_ref()
            .ok_or_else(|| Status::failed_precondition(
                "Akash deployment context not initialized"
            ))?;

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Verify there's a deployment to close
        if workflow.deployment.is_none() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "No deployment found for this workflow".to_string(),
            }));
        }

        tracing::info!("Closing deployment for session {}", req.session_id);

        // Create deployer and close the deployment
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        match deployer.close_deployment(&workflow).await {
            Ok(()) => {
                // Update workflow status
                let mut updated_workflow = workflow.clone();
                updated_workflow.status = AkashWorkflowStatus::Cancelled as i32;
                updated_workflow.updated_at = Some(pbjson_types::Timestamp {
                    seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    nanos: 0,
                });
                self.state.s.put_akash_workflow(&updated_workflow).await.ok();

                // Remove from inference cache
                if !workflow.label.is_empty() {
                    if let Err(e) = self.state.r.deployment_cache().remove_deployment(&workflow.label).await {
                        tracing::warn!(
                            "Failed to remove deployment '{}' from inference cache: {}",
                            workflow.label,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Removed deployment '{}' from inference cache",
                            workflow.label
                        );
                    }
                }

                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Deployment closed successfully for session {}", req.session_id),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to close deployment: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to close deployment: {}", e),
                }))
            }
        }
    }

    /// Update a deployment with new SDL
    async fn update_akash_deployment(
        &self,
        request: Request<UpdateAkashDeploymentRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Check if Akash context is available
        let akash_ctx = self
            .state
            .akash
            .as_ref()
            .ok_or_else(|| Status::failed_precondition(
                "Akash deployment context not initialized"
            ))?;

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Verify there's a deployment to update
        if workflow.deployment.is_none() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "No deployment found for this workflow".to_string(),
            }));
        }

        if req.sdl_content.is_empty() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "SDL content is required".to_string(),
            }));
        }

        tracing::info!("Updating deployment for session {}", req.session_id);

        // Create deployer and update the deployment
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        match deployer.update_deployment(&workflow, &req.sdl_content).await {
            Ok(()) => {
                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Deployment updated successfully for session {}", req.session_id),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to update deployment: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to update deployment: {}", e),
                }))
            }
        }
    }

    /// Top up escrow account for a deployment
    async fn topup_akash_escrow(
        &self,
        request: Request<TopupAkashEscrowRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // Check if Akash context is available
        let akash_ctx = self
            .state
            .akash
            .as_ref()
            .ok_or_else(|| Status::failed_precondition(
                "Akash deployment context not initialized"
            ))?;

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Verify there's a deployment to top up
        if workflow.deployment.is_none() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "No deployment found for this workflow".to_string(),
            }));
        }

        if req.amount_uakt == 0 {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Amount must be greater than 0".to_string(),
            }));
        }

        tracing::info!("Topping up escrow for session {} with {} uakt", req.session_id, req.amount_uakt);

        // Create deployer and top up escrow
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        match deployer.topup_escrow(&workflow, req.amount_uakt).await {
            Ok(()) => {
                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Escrow topped up with {} uakt for session {}", req.amount_uakt, req.session_id),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to top up escrow: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to top up escrow: {}", e),
                }))
            }
        }
    }

    /// Get lease status
    async fn get_lease_status(
        &self,
        request: Request<GetLeaseStatusRequest>,
    ) -> Result<Response<LeaseStatusResponse>, Status> {
        let req = request.into_inner();

        // Support both session-id and label lookups
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        // Determine deployment status from workflow state
        let deployment_status = match AkashWorkflowStatus::try_from(workflow.status).unwrap_or(AkashWorkflowStatus::Unspecified) {
            AkashWorkflowStatus::Pending => "pending",
            AkashWorkflowStatus::Running => "running",
            AkashWorkflowStatus::Completed => "active",
            AkashWorkflowStatus::Failed => "failed",
            AkashWorkflowStatus::Cancelled => "closed",
            _ => "unknown",
        };

        // Convert lease_id_info to full AkashLeaseInfo
        let lease = workflow.lease_id_info.map(|id| AkashLeaseInfo {
            owner: id.owner,
            dseq: id.dseq,
            gseq: id.gseq,
            oseq: id.oseq,
            provider: id.provider,
            state: if workflow.status == AkashWorkflowStatus::Completed as i32 {
                AkashLeaseState::Active as i32
            } else if workflow.status == AkashWorkflowStatus::Cancelled as i32 {
                AkashLeaseState::Closed as i32
            } else {
                AkashLeaseState::Invalid as i32
            },
            price_denom: "uakt".to_string(),
            price_amount: workflow.provider.as_ref().map(|p| p.bid_price_uakt.to_string()).unwrap_or_default(),
            created_at: workflow.created_at.as_ref().map(|t| t.seconds).unwrap_or(0),
            closed_on: workflow.completed_at.as_ref().map(|t| t.seconds).unwrap_or(0),
        });

        Ok(Response::new(LeaseStatusResponse {
            lease,
            endpoints: workflow.service_endpoints,
            balance_remaining_uakt: 0, // TODO: Query from chain
            deployment_status: deployment_status.to_string(),
        }))
    }

    /// Add trusted provider
    async fn add_trusted_provider(
        &self,
        request: Request<AddTrustedProviderRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        if req.address.is_empty() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Provider address is required".to_string(),
            }));
        }

        // Validate address format (basic check)
        if !req.address.starts_with("akash1") {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Invalid Akash address format (must start with 'akash1')".to_string(),
            }));
        }

        match self.state.s.add_trusted_provider(&req.address, &req.label).await {
            Ok(()) => {
                tracing::info!("Added trusted provider: {} ({})", req.address, req.label);
                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("Trusted provider {} added", req.address),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to add trusted provider: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to add trusted provider: {}", e),
                }))
            }
        }
    }

    /// Remove trusted provider
    async fn remove_trusted_provider(
        &self,
        request: Request<RemoveTrustedProviderRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        if req.address.is_empty() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Provider address is required".to_string(),
            }));
        }

        match self.state.s.remove_trusted_provider(&req.address).await {
            Ok(removed) => {
                if removed {
                    tracing::info!("Removed trusted provider: {}", req.address);
                    Ok(Response::new(OperationResult {
                        success: true,
                        message: format!("Trusted provider {} removed", req.address),
                    }))
                } else {
                    Ok(Response::new(OperationResult {
                        success: false,
                        message: format!("Provider {} not found in trusted list", req.address),
                    }))
                }
            }
            Err(e) => {
                tracing::error!("Failed to remove trusted provider: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to remove trusted provider: {}", e),
                }))
            }
        }
    }

    /// List trusted providers
    async fn list_trusted_providers(
        &self,
        _request: Request<ListTrustedProvidersRequest>,
    ) -> Result<Response<ListTrustedProvidersResponse>, Status> {
        match self.state.s.get_trusted_providers().await {
            Ok(list) => {
                Ok(Response::new(ListTrustedProvidersResponse {
                    providers: list.providers,
                }))
            }
            Err(e) => {
                tracing::error!("Failed to list trusted providers: {}", e);
                Err(Status::internal(format!("Failed to list trusted providers: {}", e)))
            }
        }
    }

    // ============ Certificate Management (DEPRECATED) ============
    // NOTE: Certificate-based mTLS authentication has been replaced with JWT authentication.
    // These stub implementations exist only for gRPC trait compliance.

    /// DEPRECATED: Certificate creation no longer needed - JWT auth is used instead.
    async fn create_akash_certificate(
        &self,
        _request: Request<CreateAkashCertificateRequest>,
    ) -> Result<Response<CreateAkashCertificateResponse>, Status> {
        Ok(Response::new(CreateAkashCertificateResponse {
            success: false,
            tx_hash: String::new(),
            serial: String::new(),
            error_message: "Certificate management deprecated. JWT authentication is used for provider communication.".to_string(),
        }))
    }

    /// DEPRECATED: Certificate revocation no longer needed - JWT auth is used instead.
    async fn revoke_akash_certificate(
        &self,
        _request: Request<RevokeAkashCertificateRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        Ok(Response::new(OperationResult {
            success: false,
            message: "Certificate management deprecated. JWT authentication is used for provider communication.".to_string(),
        }))
    }

    /// DEPRECATED: Certificate listing no longer needed - JWT auth is used instead.
    async fn list_akash_certificates(
        &self,
        _request: Request<ListAkashCertificatesRequest>,
    ) -> Result<Response<ListAkashCertificatesResponse>, Status> {
        Ok(Response::new(ListAkashCertificatesResponse {
            certificates: vec![],
            address: String::new(),
        }))
    }

    // ============ RAG Vector Database Handlers ============

    /// Ingest document into vector database
    async fn rag_ingest(
        &self,
        request: Request<RagIngestRequest>,
    ) -> Result<Response<RagIngestResponse>, Status> {
        let req = request.into_inner();

        // Check if embedder is configured
        let rag_config = match self.state.s.get_rag_config().await {
            Ok(Some(config)) => config,
            Ok(None) => {
                return Ok(Response::new(RagIngestResponse {
                    success: false,
                    chunk_count: 0,
                    chunk_ids: vec![],
                    message: "Embedder not configured. Use 'ergors rag configure' first.".to_string(),
                }));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get RAG config: {}", e)));
            }
        };

        // Create document
        let doc = ergors_rag::Document {
            content: req.content,
            uri: req.uri,
            doc_type: req.doc_type,
            tags: req.tags,
        };

        // Get storage and create RAG instance
        match crate::proxy::rag::new_remote(
            &self.state.s,
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        ) {
            Ok(rag) => {
                match rag.ingest(doc, None).await {
                    Ok(chunk_ids) => {
                        let ids: Vec<String> = chunk_ids.iter().map(|id| id.to_string()).collect();
                        Ok(Response::new(RagIngestResponse {
                            success: true,
                            chunk_count: ids.len() as u32,
                            chunk_ids: ids,
                            message: "Document ingested successfully".to_string(),
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Failed to ingest document: {}", e);
                        Ok(Response::new(RagIngestResponse {
                            success: false,
                            chunk_count: 0,
                            chunk_ids: vec![],
                            message: format!("Failed to ingest: {}", e),
                        }))
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create RAG instance: {}", e);
                Ok(Response::new(RagIngestResponse {
                    success: false,
                    chunk_count: 0,
                    chunk_ids: vec![],
                    message: format!("Failed to initialize RAG: {}", e),
                }))
            }
        }
    }

    /// Query vector database
    async fn rag_query(
        &self,
        request: Request<RagQueryRequest>,
    ) -> Result<Response<RagQueryResponse>, Status> {
        let req = request.into_inner();

        // Check if embedder is configured
        let rag_config = match self.state.s.get_rag_config().await {
            Ok(Some(config)) => config,
            Ok(None) => {
                return Ok(Response::new(RagQueryResponse {
                    results: vec![],
                    verified: false,
                }));
            }
            Err(e) => {
                return Err(Status::internal(format!("Failed to get RAG config: {}", e)));
            }
        };

        match crate::proxy::rag::new_remote(
            &self.state.s,
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        ) {
            Ok(rag) => {
                let options = ergors_rag::QueryOptions {
                    verify: req.verify,
                    ..Default::default()
                };

                match rag.query(&req.query, req.top_k as usize, options).await {
                    Ok(result) => {
                        let (results, verified) = match result {
                            ergors_rag::QueryResult::Standard(results) => {
                                let mapped: Vec<RagSearchResult> = results
                                    .iter()
                                    .map(|r| RagSearchResult {
                                        chunk_id: r.chunk_id.to_string(),
                                        similarity: r.similarity,
                                        content_preview: r.metadata.preview.clone(),
                                        source_uri: r.metadata.source_type.clone(), // Use source_type for standard results
                                    })
                                    .collect();
                                (mapped, false)
                            }
                            ergors_rag::QueryResult::Verified(results) => {
                                let mapped: Vec<RagSearchResult> = results
                                    .iter()
                                    .map(|r| RagSearchResult {
                                        chunk_id: r.chunk_id.to_string(),
                                        similarity: r.similarity,
                                        content_preview: r.content[..r.content.len().min(200)].to_string(),
                                        source_uri: r.provenance.source_uri.clone(),
                                    })
                                    .collect();
                                (mapped, true)
                            }
                        };
                        Ok(Response::new(RagQueryResponse { results, verified }))
                    }
                    Err(e) => {
                        tracing::error!("Failed to query RAG: {}", e);
                        Err(Status::internal(format!("Query failed: {}", e)))
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create RAG instance: {}", e);
                Err(Status::internal(format!("Failed to initialize RAG: {}", e)))
            }
        }
    }

    /// Get RAG status
    async fn rag_status(
        &self,
        _request: Request<RagStatusRequest>,
    ) -> Result<Response<RagStatusResponse>, Status> {
        let (embedder_configured, endpoint, model, dimension) = match self.state.s.get_rag_config().await {
            Ok(Some(config)) => (true, config.endpoint, config.model, config.dimension),
            Ok(None) => (false, String::new(), String::new(), 0),
            Err(e) => {
                tracing::error!("Failed to get RAG config: {}", e);
                (false, String::new(), String::new(), 0)
            }
        };

        // Get chunk count from storage
        let (total_chunks, total_sources) = match self.state.s.get_rag_stats().await {
            Ok((chunks, sources)) => (chunks, sources),
            Err(_) => (0, 0),
        };

        Ok(Response::new(RagStatusResponse {
            total_chunks,
            total_sources,
            embedder_configured,
            embedder_endpoint: endpoint,
            embedder_model: model,
            embedding_dimension: dimension,
        }))
    }

    /// Delete chunks by source URI
    async fn rag_delete(
        &self,
        request: Request<RagDeleteRequest>,
    ) -> Result<Response<RagOperationResult>, Status> {
        let req = request.into_inner();

        match self.state.s.delete_rag_source(&req.source_uri).await {
            Ok(count) => {
                Ok(Response::new(RagOperationResult {
                    success: true,
                    message: format!("Deleted {} chunks from source '{}'", count, req.source_uri),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to delete RAG source: {}", e);
                Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!("Failed to delete: {}", e),
                }))
            }
        }
    }

    /// List ingested sources
    async fn rag_list_sources(
        &self,
        request: Request<RagListSourcesRequest>,
    ) -> Result<Response<RagListSourcesResponse>, Status> {
        let req = request.into_inner();

        match self.state.s.list_rag_sources(req.limit as usize).await {
            Ok((sources, total)) => {
                let mapped: Vec<RagSourceInfo> = sources
                    .into_iter()
                    .map(|s| RagSourceInfo {
                        uri: s.uri,
                        chunk_count: s.chunk_count,
                        doc_type: s.doc_type,
                        ingested_at: s.ingested_at,
                    })
                    .collect();
                Ok(Response::new(RagListSourcesResponse {
                    sources: mapped,
                    total_count: total as u32,
                }))
            }
            Err(e) => {
                tracing::error!("Failed to list RAG sources: {}", e);
                Err(Status::internal(format!("Failed to list sources: {}", e)))
            }
        }
    }

    /// Configure embedder endpoint
    async fn rag_configure(
        &self,
        request: Request<RagConfigureRequest>,
    ) -> Result<Response<RagOperationResult>, Status> {
        let req = request.into_inner();

        // Validate endpoint URL
        if !req.endpoint.starts_with("http://") && !req.endpoint.starts_with("https://") {
            return Ok(Response::new(RagOperationResult {
                success: false,
                message: "Endpoint must start with http:// or https://".to_string(),
            }));
        }

        // Store configuration
        match self.state.s.set_rag_config(&req.endpoint, &req.model, req.dimension).await {
            Ok(()) => {
                Ok(Response::new(RagOperationResult {
                    success: true,
                    message: format!("Embedder configured: {} ({}, {} dims)", req.endpoint, req.model, req.dimension),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to configure RAG: {}", e);
                Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!("Failed to configure: {}", e),
                }))
            }
        }
    }

    /// Request authz grant from coordinator
    async fn request_grant(
        &self,
        request: Request<RequestGrantRequest>,
    ) -> Result<Response<RequestGrantResponse>, Status> {
        let req = request.into_inner();

        // Generate unique request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // Store grant request in storage
        let _grant_request = ho_std::types::ergors::management::v1::GrantRequest {
            request_id: request_id.clone(),
            granter_address: req.granter_address.clone(),
            grantee_address: req.grantee_address.clone(),
            msg_types: req.msg_types.clone(),
            allowance_amount: req.allowance_amount,
            expiration: req.expiration,
            reason: req.reason.clone(),
            status: "pending".to_string(),
            created_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                nanos: 0,
            }),
        };

        // TODO: Store in cnidarium with key: grant_request/{request_id}
        // For now, just log the request
        tracing::info!(
            "Grant request submitted: {} -> {} (request_id: {})",
            req.granter_address,
            req.grantee_address,
            request_id
        );

        Ok(Response::new(RequestGrantResponse {
            success: true,
            message: "Grant request submitted for approval".to_string(),
            request_id,
        }))
    }

    /// Approve or reject pending grant request
    async fn approve_grant(
        &self,
        request: Request<ApproveGrantRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // TODO: Load grant request from storage, verify it exists and is pending
        // TODO: If approved, submit actual authz grant and feegrant transactions to blockchain
        // TODO: Update grant request status in storage

        let action = if req.approve { "approved" } else { "rejected" };
        tracing::info!("Grant request {} {}", req.request_id, action);

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Grant request {}", action),
        }))
    }

    /// Revoke an existing grant
    async fn revoke_grant(
        &self,
        request: Request<RevokeGrantRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // TODO: Submit revoke grant transaction to blockchain
        // TODO: If revoke_feegrant, also submit revoke feegrant transaction

        tracing::info!(
            "Revoking grant: {} -> {} (msg_type: {}, revoke_feegrant: {})",
            req.granter_address,
            req.grantee_address,
            req.msg_type,
            req.revoke_feegrant
        );

        Ok(Response::new(OperationResult {
            success: true,
            message: "Grant revoked".to_string(),
        }))
    }

    /// List pending grant requests
    async fn list_grant_requests(
        &self,
        request: Request<ListGrantRequestsRequest>,
    ) -> Result<Response<ListGrantRequestsResponse>, Status> {
        let req = request.into_inner();

        // TODO: Query grant requests from storage with filters
        // For now, return empty list

        tracing::debug!(
            "Listing grant requests (filters: granter={}, grantee={}, status={})",
            req.granter_address,
            req.grantee_address,
            req.status
        );

        Ok(Response::new(ListGrantRequestsResponse {
            requests: vec![],
        }))
    }

    /// Create feegrant allowance
    async fn create_fee_grant(
        &self,
        request: Request<CreateFeeGrantRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // TODO: Submit feegrant allowance transaction to blockchain
        tracing::info!(
            "Creating feegrant: {} -> {} (amount: {} uakt)",
            req.granter_address,
            req.grantee_address,
            req.allowance_amount
        );

        Ok(Response::new(OperationResult {
            success: true,
            message: "Feegrant created".to_string(),
        }))
    }

    /// Revoke feegrant allowance
    async fn revoke_fee_grant(
        &self,
        request: Request<RevokeFeeGrantRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        // TODO: Submit revoke feegrant transaction to blockchain
        tracing::info!(
            "Revoking feegrant: {} -> {}",
            req.granter_address,
            req.grantee_address
        );

        Ok(Response::new(OperationResult {
            success: true,
            message: "Feegrant revoked".to_string(),
        }))
    }

    /// Query account balance from blockchain
    async fn query_balance(
        &self,
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        let req = request.into_inner();

        // Get endpoints from config (uses mainnet defaults if not configured)
        let akash_config = self.state.c.akash();
        let endpoints = CosmosEndpoints::from_akash_config(&akash_config);

        tracing::debug!(
            "Querying balance for {} (denom: {}) via {}",
            req.address,
            req.denom,
            endpoints.rest
        );

        let client = CosmosClient::new(endpoints)
            .map_err(|e| Status::internal(format!("Failed to create client: {}", e)))?;

        let coin = client
            .query_balance(&req.address, &req.denom)
            .await
            .map_err(|e| Status::internal(format!("Balance query failed: {}", e)))?;

        Ok(Response::new(QueryBalanceResponse {
            address: req.address,
            denom: coin.denom,
            amount: coin.amount,
        }))
    }

    // ============================================
    // SDL Template Management
    // ============================================

    /// List deployed SDL template contracts
    async fn list_sdl_templates(
        &self,
        _request: Request<ListSdlTemplatesRequest>,
    ) -> Result<Response<ListSdlTemplatesResponse>, Status> {
        tracing::info!("Listing SDL template contracts");

        match self.state.s.list_sdl_template_contracts().await {
            Ok(contracts) => {
                let templates = contracts
                    .into_iter()
                    .map(|(contract_address, label, code_id)| {
                        ho_std::types::ergors::management::v1::SdlTemplateInfo {
                            contract_address,
                            label,
                            code_id,
                        }
                    })
                    .collect();

                Ok(Response::new(ListSdlTemplatesResponse { templates }))
            }
            Err(e) => {
                tracing::error!("Failed to list SDL template contracts: {}", e);
                Err(Status::internal(format!(
                    "Failed to list SDL template contracts: {}",
                    e
                )))
            }
        }
    }

    /// Register an SDL template contract
    async fn register_sdl_template(
        &self,
        request: Request<RegisterSdlTemplateRequest>,
    ) -> Result<Response<RegisterSdlTemplateResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Registering SDL template contract: {}", req.contract_address);

        match self.state.s.register_sdl_template_contract(
            &req.contract_address,
            req.label,
            req.code_id,
        ).await {
            Ok(()) => {
                Ok(Response::new(RegisterSdlTemplateResponse {
                    success: true,
                    message: format!("SDL template contract registered: {}", req.contract_address),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to register SDL template contract: {}", e);
                Err(Status::internal(format!(
                    "Failed to register SDL template contract: {}",
                    e
                )))
            }
        }
    }

    /// Get SDL template from contract
    async fn get_sdl_template(
        &self,
        request: Request<GetSdlTemplateRequest>,
    ) -> Result<Response<GetSdlTemplateResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Getting SDL template from contract {}", req.contract_address);

        #[cfg(feature = "cw")]
        {
            use crate::deploy::sdl::SdlTemplateManager;
            let sdl_manager = SdlTemplateManager::new();

            // Query template from contract
            match sdl_manager
                .query_template_from_contract(
                    &self.state.wasm,
                    &self.state.s.cs,
                    &req.contract_address,
                )
                .await
            {
                Ok((sdl_template, template_json)) => {
                    // Convert serde_json::Value to prost_types::Struct
                    let template_json_bytes = serde_json::to_vec(&template_json)
                        .map_err(|e| Status::internal(format!("Failed to serialize template JSON: {}", e)))?;
                    let template_json_struct: pbjson_types::Struct = serde_json::from_slice(&template_json_bytes)
                        .map_err(|e| Status::internal(format!("Failed to convert template JSON: {}", e)))?;

                    Ok(Response::new(GetSdlTemplateResponse {
                        sdl_template,
                        template_json: Some(template_json_struct),
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to query SDL template: {}", e);
                    Err(Status::internal(format!("Failed to query SDL template: {}", e)))
                }
            }
        }

        #[cfg(not(feature = "cw"))]
        {
            Err(Status::unimplemented("CosmWasm support not enabled"))
        }
    }

    /// Get variable defaults from contract
    async fn get_sdl_defaults(
        &self,
        request: Request<GetSdlDefaultsRequest>,
    ) -> Result<Response<GetSdlDefaultsResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Getting SDL defaults from contract {}", req.contract_address);

        #[cfg(feature = "cw")]
        {
            use crate::deploy::sdl::SdlTemplateManager;
            let sdl_manager = SdlTemplateManager::new();

            // Query defaults from contract
            match sdl_manager
                .query_defaults_from_contract(
                    &self.state.wasm,
                    &self.state.s.cs,
                    &req.contract_address,
                )
                .await
            {
                Ok(defaults) => Ok(Response::new(GetSdlDefaultsResponse { defaults })),
                Err(e) => {
                    tracing::error!("Failed to query SDL defaults: {}", e);
                    Err(Status::internal(format!("Failed to query SDL defaults: {}", e)))
                }
            }
        }

        #[cfg(not(feature = "cw"))]
        {
            Err(Status::unimplemented("CosmWasm support not enabled"))
        }
    }

    /// Render SDL template with variables
    async fn render_sdl_template(
        &self,
        request: Request<RenderSdlTemplateRequest>,
    ) -> Result<Response<RenderSdlTemplateResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Rendering SDL template from contract {}", req.contract_address);

        #[cfg(feature = "cw")]
        {
            use crate::deploy::sdl::SdlTemplateManager;
            let sdl_manager = SdlTemplateManager::new();

            let variables = if req.variables.is_empty() {
                None
            } else {
                Some(req.variables)
            };

            // Query rendered SDL from contract
            match sdl_manager
                .query_rendered_sdl_from_contract(
                    &self.state.wasm,
                    &self.state.s.cs,
                    &req.contract_address,
                    variables,
                )
                .await
            {
                Ok((rendered_sdl, used_variables)) => Ok(Response::new(RenderSdlTemplateResponse {
                    rendered_sdl,
                    used_variables,
                })),
                Err(e) => {
                    tracing::error!("Failed to render SDL template: {}", e);
                    Err(Status::internal(format!("Failed to render SDL template: {}", e)))
                }
            }
        }

        #[cfg(not(feature = "cw"))]
        {
            Err(Status::unimplemented("CosmWasm support not enabled"))
        }
    }

    // ============================================
    // Cosmos Chain Configuration Management
    // ============================================

    /// Set or update a Cosmos chain configuration (stored in cnidarium)
    async fn set_chain_config(
        &self,
        request: Request<SetChainConfigRequest>,
    ) -> Result<Response<SetChainConfigResponse>, Status> {
        let req = request.into_inner();

        let config = req.config.ok_or_else(|| {
            Status::invalid_argument("Chain config is required")
        })?;

        if config.chain_id.is_empty() {
            return Err(Status::invalid_argument("chain_id cannot be empty"));
        }

        tracing::info!("Setting chain config for: {} ({})", config.chain_name, config.chain_id);

        match self.state.s.put_chain_config(&config).await {
            Ok(()) => Ok(Response::new(SetChainConfigResponse {
                success: true,
                message: format!("Chain config stored for: {}", config.chain_id),
            })),
            Err(e) => {
                tracing::error!("Failed to store chain config: {}", e);
                Err(Status::internal(format!("Failed to store chain config: {}", e)))
            }
        }
    }

    /// Get a Cosmos chain configuration by chain ID (from cnidarium)
    async fn get_chain_config(
        &self,
        request: Request<GetChainConfigRequest>,
    ) -> Result<Response<GetChainConfigResponse>, Status> {
        let req = request.into_inner();

        if req.chain_id.is_empty() {
            return Err(Status::invalid_argument("chain_id cannot be empty"));
        }

        tracing::info!("Getting chain config for: {}", req.chain_id);

        match self.state.s.get_chain_config(&req.chain_id).await {
            Ok(Some(config)) => Ok(Response::new(GetChainConfigResponse {
                config: Some(config),
                found: true,
            })),
            Ok(None) => Ok(Response::new(GetChainConfigResponse {
                config: None,
                found: false,
            })),
            Err(e) => {
                tracing::error!("Failed to get chain config: {}", e);
                Err(Status::internal(format!("Failed to get chain config: {}", e)))
            }
        }
    }

    /// List all registered Cosmos chain configurations (from cnidarium)
    async fn list_chain_configs(
        &self,
        _request: Request<ListChainConfigsRequest>,
    ) -> Result<Response<ListChainConfigsResponse>, Status> {
        tracing::info!("Listing all chain configs");

        match self.state.s.list_chain_configs().await {
            Ok(chains) => Ok(Response::new(ListChainConfigsResponse { chains })),
            Err(e) => {
                tracing::error!("Failed to list chain configs: {}", e);
                Err(Status::internal(format!("Failed to list chain configs: {}", e)))
            }
        }
    }

    /// Delete a Cosmos chain configuration (from cnidarium)
    async fn delete_chain_config(
        &self,
        request: Request<DeleteChainConfigRequest>,
    ) -> Result<Response<DeleteChainConfigResponse>, Status> {
        let req = request.into_inner();

        if req.chain_id.is_empty() {
            return Err(Status::invalid_argument("chain_id cannot be empty"));
        }

        tracing::info!("Deleting chain config for: {}", req.chain_id);

        match self.state.s.delete_chain_config(&req.chain_id).await {
            Ok(()) => Ok(Response::new(DeleteChainConfigResponse {
                success: true,
                message: format!("Chain config deleted for: {}", req.chain_id),
            })),
            Err(e) => {
                tracing::error!("Failed to delete chain config: {}", e);
                Err(Status::internal(format!("Failed to delete chain config: {}", e)))
            }
        }
    }

    // ============ Gateway Management ============

    /// List all registered gateways
    async fn list_gateways(
        &self,
        _request: Request<ListGatewaysRequest>,
    ) -> Result<Response<ListGatewaysResponse>, Status> {
        tracing::info!("Listing registered gateways");

        let mut gateways = vec![];

        // Query actual runtime state from GatewayManager
        if let Some(ref gm) = self.state.gm {
            let runtime_gateways = gm.list_gateways().await;
            for gw in runtime_gateways {
                // Merge with config state
                let enabled = self.state.s
                    .get_gateway_config(&gw.gateway_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| c.enabled)
                    .unwrap_or(false);

                gateways.push(GatewayInfo {
                    gateway_id: gw.gateway_id,
                    name: gw.name,
                    enabled,
                    connected: gw.connected, // Real runtime state
                });
            }
        } else {
            // Fallback: check config only (no runtime manager)
            if let Ok(Some(config)) = self.state.s.get_gateway_config("discord").await {
                gateways.push(GatewayInfo {
                    gateway_id: "discord".to_string(),
                    name: "Discord Bot".to_string(),
                    enabled: config.enabled,
                    connected: false, // No runtime manager available
                });
            }
        }

        Ok(Response::new(ListGatewaysResponse { gateways }))
    }

    /// Get gateway status
    async fn get_gateway_status(
        &self,
        request: Request<GetGatewayStatusRequest>,
    ) -> Result<Response<GatewayStatusResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Getting status for gateway: {}", req.gateway_id);

        // Query actual runtime state from GatewayManager
        let connected = if let Some(ref gm) = self.state.gm {
            gm.list_gateways()
                .await
                .into_iter()
                .find(|g| g.gateway_id == req.gateway_id)
                .map(|g| g.connected)
                .unwrap_or(false)
        } else {
            false
        };

        // Get metrics from GatewayManager
        let (messages_processed, last_message_timestamp) = if let Some(ref gm) = self.state.gm {
            gm.get_gateway_metrics(&req.gateway_id)
                .await
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };

        Ok(Response::new(GatewayStatusResponse {
            gateway_id: req.gateway_id,
            connected,
            messages_processed,
            last_message_timestamp: last_message_timestamp as i64,
        }))
    }

    /// Enable a gateway
    async fn enable_gateway(
        &self,
        request: Request<EnableGatewayRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Enabling gateway: {}", req.gateway_id);

        // Get current config or create default
        let mut config = self
            .state
            .s
            .get_gateway_config(&req.gateway_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get gateway config: {}", e)))?
            .unwrap_or_else(|| ho_std::types::ergors::gateway::v1::GatewayConfig {
                gateway_id: req.gateway_id.clone(),
                gateway_type: req.gateway_id.clone(),
                enabled: false,
                settings: std::collections::HashMap::new(),
            });

        config.enabled = true;

        self.state
            .s
            .put_gateway_config(&config)
            .await
            .map_err(|e| Status::internal(format!("Failed to save gateway config: {}", e)))?;

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Gateway {} enabled", req.gateway_id),
        }))
    }

    /// Disable a gateway
    async fn disable_gateway(
        &self,
        request: Request<DisableGatewayRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Disabling gateway: {}", req.gateway_id);

        if let Ok(Some(mut config)) = self.state.s.get_gateway_config(&req.gateway_id).await {
            config.enabled = false;
            self.state
                .s
                .put_gateway_config(&config)
                .await
                .map_err(|e| Status::internal(format!("Failed to save gateway config: {}", e)))?;
        }

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Gateway {} disabled", req.gateway_id),
        }))
    }

    /// Configure Discord gateway
    async fn configure_discord_gateway(
        &self,
        request: Request<ConfigureDiscordGatewayRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        use ho_std::traits::HoConfigTrait;

        let req = request.into_inner();
        tracing::info!("Configuring Discord gateway");

        // Get node pubkey for encryption
        let node_pubkey = self.state.c.identity().public_key.clone()
            .ok_or_else(|| Status::internal("Node public key not available for encryption"))?;

        // Get or create config
        let mut config = self
            .state
            .s
            .get_gateway_config("discord")
            .await
            .map_err(|e| Status::internal(format!("Failed to get gateway config: {}", e)))?
            .unwrap_or_else(|| ho_std::types::ergors::gateway::v1::GatewayConfig {
                gateway_id: "discord".to_string(),
                gateway_type: "discord".to_string(),
                enabled: false,
                settings: std::collections::HashMap::new(),
            });

        // Encrypt and store bot token securely
        if !req.bot_token.is_empty() {
            let (encrypted_value, nonce) = encrypt_gateway_secret(&req.bot_token, &node_pubkey)
                .map_err(Status::internal)?;

            let secret = EncryptedSecret {
                secret_id: "discord_bot_token".to_string(),
                secret_type: "gateway_token".to_string(),
                label: "Discord Bot Token".to_string(),
                encrypted_value,
                nonce,
                encryption_method: GATEWAY_SECRET_ENCRYPTION_METHOD.to_string(),
                created_at: Some(pbjson_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
                last_accessed_at: None,
                access_count: 0,
                metadata: [("gateway_id".to_string(), "discord".to_string())]
                    .into_iter()
                    .collect(),
            };

            self.state
                .s
                .store_encrypted_secret(&secret, "grpc_handler", "configure_discord_gateway")
                .await
                .map_err(|e| Status::internal(format!("Failed to store encrypted token: {}", e)))?;

            // Store marker in config that token is encrypted (not the actual token)
            config.settings.insert("bot_token_encrypted".to_string(), "true".to_string());
            config.settings.remove("bot_token"); // Remove any plaintext token
        }

        if let Some(prefix) = req.command_prefix {
            config.settings.insert("command_prefix".to_string(), prefix);
        }

        if let Some(respond_mentions) = req.respond_to_mentions {
            config
                .settings
                .insert("respond_to_mentions".to_string(), respond_mentions.to_string());
        }

        self.state
            .s
            .put_gateway_config(&config)
            .await
            .map_err(|e| Status::internal(format!("Failed to save gateway config: {}", e)))?;

        Ok(Response::new(OperationResult {
            success: true,
            message: "Discord gateway configured".to_string(),
        }))
    }

    /// Add Discord allowed guild
    async fn add_discord_allowed_guild(
        &self,
        request: Request<AddDiscordAllowedGuildRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Adding Discord allowed guild: {}", req.guild_id);

        if let Ok(Some(mut config)) = self.state.s.get_gateway_config("discord").await {
            let mut guilds: Vec<String> = config
                .settings
                .get("allowed_guild_ids")
                .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();

            if !guilds.contains(&req.guild_id) {
                guilds.push(req.guild_id.clone());
                config.settings.insert("allowed_guild_ids".to_string(), guilds.join(","));

                self.state
                    .s
                    .put_gateway_config(&config)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to save gateway config: {}", e)))?;
            }

            Ok(Response::new(OperationResult {
                success: true,
                message: format!("Guild {} added to allowlist", req.guild_id),
            }))
        } else {
            Err(Status::not_found("Discord gateway not configured"))
        }
    }

    /// Remove Discord allowed guild
    async fn remove_discord_allowed_guild(
        &self,
        request: Request<RemoveDiscordAllowedGuildRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        tracing::info!("Removing Discord allowed guild: {}", req.guild_id);

        if let Ok(Some(mut config)) = self.state.s.get_gateway_config("discord").await {
            let guilds: Vec<String> = config
                .settings
                .get("allowed_guild_ids")
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty() && x != &req.guild_id)
                        .collect()
                })
                .unwrap_or_default();

            config.settings.insert("allowed_guild_ids".to_string(), guilds.join(","));

            self.state
                .s
                .put_gateway_config(&config)
                .await
                .map_err(|e| Status::internal(format!("Failed to save gateway config: {}", e)))?;

            Ok(Response::new(OperationResult {
                success: true,
                message: format!("Guild {} removed from allowlist", req.guild_id),
            }))
        } else {
            Err(Status::not_found("Discord gateway not configured"))
        }
    }

    /// Get Discord configuration (token redacted)
    async fn get_discord_config(
        &self,
        _request: Request<GetDiscordConfigRequest>,
    ) -> Result<Response<GetDiscordConfigResponse>, Status> {
        tracing::info!("Getting Discord configuration");

        if let Ok(Some(config)) = self.state.s.get_gateway_config("discord").await {
            let allowed_guild_ids: Vec<String> = config
                .settings
                .get("allowed_guild_ids")
                .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();

            let allowed_channel_ids: Vec<String> = config
                .settings
                .get("allowed_channel_ids")
                .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();

            Ok(Response::new(GetDiscordConfigResponse {
                token_configured: config.settings.contains_key("bot_token"),
                allowed_guild_ids,
                allowed_channel_ids,
                command_prefix: config.settings.get("command_prefix").cloned().unwrap_or_else(|| "!".to_string()),
                respond_to_mentions: config.settings.get("respond_to_mentions").map(|s| s == "true").unwrap_or(true),
                respond_to_dms: config.settings.get("respond_to_dms").map(|s| s == "true").unwrap_or(false),
            }))
        } else {
            Ok(Response::new(GetDiscordConfigResponse {
                token_configured: false,
                allowed_guild_ids: vec![],
                allowed_channel_ids: vec![],
                command_prefix: "!".to_string(),
                respond_to_mentions: true,
                respond_to_dms: false,
            }))
        }
    }
}

/// Start the gRPC management server
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    service: ManagementServiceImpl,
    rlm_service: Option<crate::client::RlmDocService>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ho_std::types::ergors::management::v1::management_service_server::ManagementServiceServer;

    tracing::info!("Starting gRPC management server on {}", addr);

    let mut server = tonic::transport::Server::builder()
        .add_service(ManagementServiceServer::new(service));

    // Add RLM document service if provided
    if let Some(rlm_svc) = rlm_service {
        tracing::info!("Registering RLM document service");
        server = server.add_service(rlm_svc.into_server());
    }

    server.serve(addr).await?;

    Ok(())
}
