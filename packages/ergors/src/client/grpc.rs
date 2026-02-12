//! ManagementService gRPC implementation
//!
//! Provides the server-side implementation of the management gRPC service.

use crate::deploy::akash::client::AkashClient;
use crate::gateway::crypto::{encrypt_gateway_secret, GATEWAY_SECRET_ENCRYPTION_METHOD};
use crate::session_manager::{SessionManager, SessionManagerConfig};
use crate::ErgorsAppState;
use async_stream::try_stream;
use ho_std::keys::cosmos::cosmos_address_from_pubkey;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait};
use ho_std::types::ergors::management::v1::{
    management_service_server::ManagementService,
    AddDiscordAllowedGuildRequest,
    // Akash deployment types (advance_akash_deployment is deprecated but still in proto)
    AdvanceAkashDeploymentRequest,
    AdvanceAkashDeploymentResponse,
    // Network routing types (for OpenCode tools)
    AnnounceNodeRequest,
    AnnounceNodeResponse,
    // Grant management types
    ApproveGrantRequest,
    CancelAkashDeploymentRequest,
    CompleteSessionRequest,
    CompleteSessionResponse,
    ConfigData,
    ConfigUpdate,
    ConfigureDiscordGatewayRequest,
    ConfigureProxyRoutesRequest,
    CosmosKeyInfo,
    CreateAkashDeploymentRequest,
    CreateAkashDeploymentResponse,
    CreateFeeGrantRequest,
    // Session types
    CreateSessionRequest,
    CreateSessionResponse,
    DeleteChainConfigRequest,
    DeleteChainConfigResponse,
    DeleteCosmosKeyRequest,
    DeleteSessionRequest,
    DisableGatewayRequest,
    Empty,
    EnableGatewayRequest,
    EngineState,
    EngineStatus,
    FailSessionRequest,
    GatewayInfo,
    GatewayStatusResponse,
    GetAkashDeploymentRequest,
    GetAkashDeploymentResponse,
    GetChainConfigRequest,
    GetChainConfigResponse,
    GetDiscordConfigRequest,
    GetDiscordConfigResponse,
    GetGatewayStatusRequest,
    GetHierarchyRequest,
    GetHierarchyResponse,
    // Key address query types
    GetKeyAddressRequest,
    GetKeyAddressResponse,
    GetSdlDefaultsRequest,
    GetSdlDefaultsResponse,
    GetSdlTemplateRequest,
    GetSdlTemplateResponse,
    GetSessionRequest,
    GetSessionResponse,
    HealthUpdate,
    IdentityResponse,
    ImportCosmosKeyRequest,
    ImportCosmosKeyResponse,
    ImportIdentityRequest,
    ListAkashDeploymentsRequest,
    ListAkashDeploymentsResponse,
    ListByNodeRequest,
    ListByNodeResponse,
    ListByRootRequest,
    ListByRootResponse,
    ListChainConfigsRequest,
    ListChainConfigsResponse,
    // CLI key management types
    ListCliKeysRequest,
    ListCliKeysResponse,
    RegisterCliKeyRequest,
    RevokeCliKeyRequest,
    // RLM config types
    RlmGetConfigRequest,
    RlmGetConfigResponse,
    // Provider registration types
    RegisterDeploymentProvidersRequest,
    RegisterDeploymentProvidersResponse,
    // Cosmos key management types
    ListCosmosKeysResponse,
    // Gateway management types
    ListGatewaysRequest,
    ListGatewaysResponse,
    ListGrantRequestsRequest,
    ListGrantRequestsResponse,
    // SDL template types
    ListSdlTemplatesRequest,
    ListSdlTemplatesResponse,
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
    AssignProviderRoleRequest,
    UnassignProviderRoleRequest,
    ProviderConfig,
    ProviderInfo,
    ProviderList,
    ProviderName,
    ProviderTestResult,
    RemoveProviderRequest,
    QueryAkashBidsRequest,
    QueryAkashBidsResponse,
    QueryBalanceRequest,
    QueryBalanceResponse,
    QuerySessionsRequest,
    QuerySessionsResponse,
    RegisterSdlTemplateRequest,
    RegisterSdlTemplateResponse,
    RemoveDiscordAllowedGuildRequest,
    RenderSdlTemplateRequest,
    RenderSdlTemplateResponse,
    RequestGrantRequest,
    RequestGrantResponse,
    ResumeSessionRequest,
    ResumeSessionResponse,
    RevokeFeeGrantRequest,
    RevokeGrantRequest,
    RollupRequest,
    RollupResponse,
    RouteMessageRequest,
    RouteMessageResponse,
    SelectAkashProviderRequest,
    SelectAkashProviderResponse,
    SessionHierarchyStats,
    SessionStatus,
    SessionUpdate,
    // Chain config types
    SetChainConfigRequest,
    SetChainConfigResponse,
    SetDefaultCosmosKeyRequest,
    SetWorkflowEndpointsRequest,
    SetWorkflowEndpointsResponse,
    ShutdownRequest,
    SpawnChildRequest,
    SpawnChildResponse,
    StreamRequest,
    StreamSessionRequest,
    SyncSessionRequest,
    SyncSessionResponse,
    TokenIdRequest,
    TokenLabel,
    TokenList,
    TokenResponse,
    UpdateSessionRequest,
    UpdateSessionResponse,
};
use ho_std::types::ergors::network::v1::{NetworkTopology, NodeIdentity, NodeType};
use ho_std::types::ergors::orch::v1::{
    AddTrustedProviderRequest,
    AkashDeploymentWorkflow,
    AkashLeaseInfo,
    AkashLeaseState,
    AkashWorkflowOptions,
    AkashWorkflowStatus,
    AkashWorkflowStep,
    CloseAkashDeploymentRequest,
    CloseAkashLeaseRequest,
    ConfiguredSdl,
    // Certificate management types (deprecated - JWT auth used instead, stubs for trait compliance)
    CreateAkashCertificateRequest,
    CreateAkashCertificateResponse,
    GetLeaseStatusRequest,
    LeaseStatusResponse,
    ListAkashCertificatesRequest,
    ListAkashCertificatesResponse,
    ListTrustedProvidersRequest,
    ListTrustedProvidersResponse,
    RagConfigureRequest,
    RagDeleteRequest,
    // RAG types
    RagIngestRequest,
    RagIngestResponse,
    RagListSourcesRequest,
    RagListSourcesResponse,
    RagOperationResult,
    RagQueryRequest,
    RagQueryResponse,
    RagSearchResult,
    RagSourceInfo,
    RagStatusRequest,
    RagStatusResponse,
    RlmConfigureRequest,
    RlmQueryRequest,
    RlmQueryResponse,
    // Document Storage types
    DeleteDocumentRequest,
    DeleteDocumentResponse,
    IngestDocumentRequest,
    IngestDocumentResponse,
    ListDocumentsRequest,
    ListDocumentsResponse,
    RetrieveDocumentRequest,
    RetrieveDocumentResponse,
    // Engine Role types
    EngineRole,
    EngineRoleConfig,
    EngineRoleMapping,
    RemoveTrustedProviderRequest,
    RevokeAkashCertificateRequest,
    // Automated workflow types
    RunAkashDeploymentRequest,
    RunAkashDeploymentResponse,
    TopupAkashEscrowRequest,
    UpdateAkashDeploymentRequest,
};
use ho_std::types::ergors::storage::v1::EncryptedSecret;
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
    authorized_keys: crate::auth::grpc::AuthorizedCliKeys,
}

impl ManagementServiceImpl {
    /// Create a new management service implementation
    pub fn new(
        state: ErgorsAppState,
        shutdown_tx: broadcast::Sender<()>,
        authorized_keys: crate::auth::grpc::AuthorizedCliKeys,
    ) -> Self {
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
            authorized_keys,
        }
    }

    /// Create a new management service with custom session config
    pub fn with_session_config(
        state: ErgorsAppState,
        shutdown_tx: broadcast::Sender<()>,
        session_config: SessionManagerConfig,
        authorized_keys: crate::auth::grpc::AuthorizedCliKeys,
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
            authorized_keys,
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

    /// Simple document ingestion: chunk and store without embeddings.
    /// Used by `ergors ask ingest-file` for document storage without an embedder.
    async fn rag_ingest_simple(
        &self,
        content: String,
        uri: String,
        _doc_type: String,
        _tags: Vec<String>,
    ) -> Result<Response<RagIngestResponse>, Status> {
        use ergors_rag::ingest::chunk_text;
        use ergors_rag::types::VerifiableChunk;
        use ergors_rag::storage::RagStorage;
        use uuid::Uuid;

        let chunks_text = chunk_text(&content, 1000);
        if chunks_text.is_empty() {
            return Ok(Response::new(RagIngestResponse {
                success: true,
                chunk_count: 0,
                chunk_ids: vec![],
                message: "No content to ingest".to_string(),
            }));
        }

        let now = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        };

        let rag_storage = RagStorage::new(Arc::new(self.state.s.cs.clone()));

        let verifiable_chunks: Vec<VerifiableChunk> = chunks_text
            .iter()
            .map(|text| {
                let content_hash = ::blake3::hash(text.as_bytes());
                VerifiableChunk {
                    chunk_id: Uuid::new_v4(),
                    content: text.clone(),
                    content_hash: *content_hash.as_bytes(),
                    embedding_hash: [0u8; 32], // No embedding
                    version: 0,
                    ingested_at: now,
                    source_uri: uri.clone(),
                    uploader_id: None,
                    access_policy: None,
                    commit_ref: None,
                    previous_version: None,
                }
            })
            .collect();

        let ids: Vec<String> = verifiable_chunks.iter().map(|c| c.chunk_id.to_string()).collect();
        let count = verifiable_chunks.len() as u32;

        match rag_storage.put_chunks_batch(&verifiable_chunks).await {
            Ok(()) => Ok(Response::new(RagIngestResponse {
                success: true,
                chunk_count: count,
                chunk_ids: ids,
                message: format!("Ingested {} chunks (no embeddings)", count),
            })),
            Err(e) => {
                tracing::error!("Failed to store chunks: {}", e);
                Ok(Response::new(RagIngestResponse {
                    success: false,
                    chunk_count: 0,
                    chunk_ids: vec![],
                    message: format!("Failed to store: {}", e),
                }))
            }
        }
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
                Status::not_found(
                    "No key store found. Import a key with `ergors keys import-mnemonic`",
                )
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
            Status::failed_precondition("ERGORS_CUSTODY_PASSWORD environment variable not set")
        })?;

        // Create key manager from store and unlock
        let mut manager = EncryptedCosmosKeyManager::from_store(&key_store);
        manager
            .unlock(&password)
            .map_err(|e| Status::internal(format!("Failed to unlock key manager: {}", e)))?;

        // Determine coin type (default to 118 for cosmos)
        let coin_type = if req.coin_type == 0 {
            118
        } else {
            req.coin_type
        };

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
                return Err(Status::internal(format!(
                    "Failed to access key store: {}",
                    e
                )));
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
                        "Password required (custody password not available)",
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
        tracing::info!(
            "💾 Saving cosmos key store with {} keys...",
            store.keys.len()
        );
        if let Err(e) = self.state.s.put_cosmos_key_store(&store).await {
            tracing::error!("❌ Failed to save key store: {}", e);
            return Err(Status::internal(format!("Failed to save key store: {}", e)));
        }
        tracing::info!("✅ Key store saved successfully");

        // Verify the save by reading back
        match self.state.s.get_cosmos_key_store().await {
            Ok(Some(verified)) => {
                tracing::info!(
                    "✅ Verified: key store has {} keys after save",
                    verified.keys.len()
                );
            }
            Ok(None) => {
                tracing::error!("❌ Verification failed: key store is empty after save!");
            }
            Err(e) => {
                tracing::error!("❌ Verification failed: {}", e);
            }
        }

        // Update in-memory akash context key store AND key manager.
        // The key manager must be rebuilt so it picks up the correct salt
        // from the newly stored key (otherwise decrypt uses stale/zero salt).
        if let Some(ref akash_ctx) = self.state.akash {
            let mut key_store = akash_ctx.key_store.write().await;
            *key_store = store;
            let mut key_mgr = akash_ctx.key_manager.write().await;
            *key_mgr = EncryptedCosmosKeyManager::from_store(&key_store);
            tracing::info!("✅ Updated in-memory key store and key manager");
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
            if let Err(e) = nm
                .announce_node(req.capabilities.clone(), req.load_factor)
                .await
            {
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
                let target_role = req
                    .target_role
                    .map(|r| NodeType::try_from(r).unwrap_or(NodeType::Unspecified));

                if target_role.is_none() {
                    return Ok(Response::new(RouteMessageResponse {
                        success: false,
                        nodes_reached: 0,
                        response_payload: None,
                        error_message: "target_role is required for send_to_role action"
                            .to_string(),
                    }));
                }

                match nm
                    .send_to_role_raw(target_role.unwrap(), &req.payload)
                    .await
                {
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
                match nm
                    .request_raw(&target_node_id.unwrap(), &req.payload, timeout)
                    .await
                {
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
                error_message: format!(
                    "Unknown action: {}. Use 'broadcast', 'send_to_role', or 'request'",
                    req.action
                ),
            })),
        }
    }

    // ============ Providers ============

    async fn list_providers(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ProviderList>, Status> {
        // Read from ProxyRouter config (source of truth for dynamically configured providers)
        let pr = self.state.pr.read().await;
        let config = pr.config();

        let mut providers: Vec<ProviderInfo> = config
            .providers
            .iter()
            .map(|(name, provider)| ProviderInfo {
                name: name.clone(),
                configured: !provider.base_url.is_empty(),
                enabled: provider.enabled,
                base_url: provider.base_url.clone(),
                keyless: provider.api_key_ref.is_empty(),
                deployment_session_id: String::new(),
            })
            .collect();

        // Also include providers from LLM config that aren't in the proxy router
        let llm_config = self.state.c.llm();
        for entity in &llm_config.entities {
            if !providers.iter().any(|p| p.name == entity.name) {
                providers.push(ProviderInfo {
                    name: entity.name.clone(),
                    configured: true,
                    enabled: entity.enabled,
                    base_url: entity.base_url.clone(),
                    keyless: true,
                    deployment_session_id: String::new(),
                });
            }
        }

        // Match providers to active deployments
        let deploy_cache = self.state.r.deployment_cache();
        let cache = deploy_cache.cache.read().await;
        for provider in &mut providers {
            if let Some(endpoint) = cache.get(&provider.name) {
                provider.deployment_session_id = endpoint.session_id.clone();
            }
        }

        // Get default provider
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
        let mut req = request.into_inner();

        // Normalize provider name to lowercase for consistency
        req.name = req.name.to_lowercase();

        tracing::info!(
            "Configure provider requested: {} (default: {})",
            req.name,
            req.set_as_default
        );

        if req.name.is_empty() {
            return Err(Status::invalid_argument("Provider name is required"));
        }

        if !req.no_key && req.api_key_ref.is_empty() {
            return Err(Status::invalid_argument(
                "API key is required (use no_key=true for keyless providers)",
            ));
        }

        // Skip key encryption for keyless providers
        if !req.no_key {
            // Get custody password from akash context
            let custody_password = match &self.state.akash {
                Some(ctx) if !ctx.custody_password.is_empty() => ctx.custody_password.clone(),
                _ => {
                    return Err(Status::failed_precondition(
                        "Custody not initialized — run sentinel bootstrap first",
                    ));
                }
            };

            // Load existing store from Cnidarium (or create fresh)
            
            use ho_std::llm::state_ext::{StateReadExt as _, StateWriteExt as _};
            use ho_std::llm::EncryptedApiKeyManager;

            let snapshot = self.state.s.cs.latest_snapshot();
            let existing_store = snapshot
                .get_encrypted_api_key_store()
                .await
                .map_err(|e| Status::internal(format!("Failed to load key store: {}", e)))?;

            let (mut manager, mut store) = if let Some(store) = existing_store {
                let mgr = EncryptedApiKeyManager::from_store(&store);
                (mgr, store)
            } else {
                let mgr = EncryptedApiKeyManager::new();
                let now = {
                    use std::time::SystemTime;
                    let d = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap();
                    pbjson_types::Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    }
                };
                let store = ho_std::types::ergors::storage::v1::EncryptedApiKeyStore {
                    version: 1,
                    keys: vec![],
                    created_at: Some(now),
                    updated_at: None,
                    kdf_salt: mgr.salt().to_vec(),
                    kdf_params: String::new(),
                };
                (mgr, store)
            };

            // Unlock and add key
            manager
                .unlock(&custody_password)
                .map_err(|e| Status::internal(format!("Failed to unlock key manager: {}", e)))?;

            manager
                .add_key_to_store(&mut store, &req.name, &req.api_key_ref)
                .map_err(|e| Status::internal(format!("Failed to encrypt key: {}", e)))?;

            // Persist to Cnidarium
            let mut delta = cnidarium::StateDelta::new(self.state.s.cs.latest_snapshot());
            delta.put_encrypted_api_key_store(&store);
            self.state
                .s
                .cs
                .commit(delta)
                .await
                .map_err(|e| Status::internal(format!("Failed to commit key store: {}", e)))?;

            // Update live key accessor (so proxy resolves immediately without restart)
            {
                let pr = self.state.pr.read().await;
                if let Some(accessor) = pr.key_accessor() {
                    let mut guard = accessor.write().await;
                    if let Err(e) = guard.set_key(&req.name, req.api_key_ref.clone()).await {
                        tracing::warn!("Failed to update live key accessor: {}", e);
                    }
                }
            }
        }

        // If base_url is provided, update or create the LLM entity
        if !req.base_url.is_empty() {
            use ho_std::llm::state_ext::{StateReadExt, StateWriteExt};
            use ho_std::types::ergors::orch::v1::{InferenceProviderConfig, InferenceProviderType, LlmEntity};

            let snapshot = self.state.s.cs.latest_snapshot();
            let mut entities = snapshot.get_llm_providers().await
                .map_err(|e| Status::internal(format!("Failed to load LLM entities: {}", e)))?;

            // Determine provider type
            let provider_type = match req.name.to_lowercase().as_str() {
                "openai" => InferenceProviderType::Openai,
                "anthropic" => InferenceProviderType::Anthropic,
                "ollama" => InferenceProviderType::Ollama,
                "grok" => InferenceProviderType::Openai,
                "akashml" | "akash" => InferenceProviderType::Openai,
                _ => InferenceProviderType::Openai,
            };

            // Default model patterns based on provider type (using glob patterns for flexibility)
            let default_models = match provider_type {
                InferenceProviderType::Openai => vec!["gpt-*".to_string(), "chatgpt-*".to_string(), "o1-*".to_string()],
                InferenceProviderType::Anthropic => vec!["claude-*".to_string()],
                InferenceProviderType::Ollama => vec!["llama*".to_string(), "mistral*".to_string(), "*".to_string()],
                _ => vec![],
            };

            // Find existing entity or create new one
            let entity_idx = entities.iter().position(|e| e.name == req.name);

            if let Some(idx) = entity_idx {
                // Update existing entity
                entities[idx].base_url = req.base_url.clone();
                tracing::info!("Updated LLM entity '{}' with base_url: {}", req.name, req.base_url);
            } else {
                // Create new entity — default_model is empty (no model substitution for manually added providers)
                let entity = LlmEntity {
                    name: req.name.clone(),
                    base_url: req.base_url.clone(),
                    models: default_models.clone(),
                    default_model: String::new(),
                    priority: entities.len() as u32 + 1,
                    enabled: true,
                    default_strategy: 0,
                    timeout_seconds: 30,
                    max_retries: 3,
                };
                entities.push(entity);
                tracing::info!("Created LLM entity '{}' with base_url: {}", req.name, req.base_url);
            }

            // Store updated entities in Cnidarium
            let mut delta = cnidarium::StateDelta::new(self.state.s.cs.latest_snapshot());
            delta.put_llm_providers(&entities);
            self.state.s.cs.commit(delta).await
                .map_err(|e| Status::internal(format!("Failed to commit LLM entities: {}", e)))?;

            // Update proxy router config directly
            let mut pr = self.state.pr.write().await;
            let mut config = pr.config().clone();

            // Clear existing providers and routes for this provider (to avoid duplicates)
            config.providers.remove(&req.name);
            config.model_routes.retain(|_, provider| provider != &req.name);

            // Add updated provider config
            let api_key_ref = if req.no_key {
                String::new()
            } else {
                format!("custody://{}", req.name)
            };
            let provider_config = InferenceProviderConfig {
                provider_id: req.name.clone(),
                base_url: req.base_url.clone(),
                api_key_ref,
                enabled: true,
                provider_type: provider_type as i32,
                ..Default::default()
            };

            config.providers.insert(req.name.clone(), provider_config);

            // Add model routes
            for model in &default_models {
                config.model_routes.insert(model.clone(), req.name.clone());
            }

            // Update the router with the new config
            pr.update_config(config);

            tracing::info!("✅ Updated proxy router with provider '{}' → {}", req.name, req.base_url);

            // Register in LlmRouter so call_provider_by_name works immediately
            let entity = LlmEntity {
                name: req.name.clone(),
                base_url: req.base_url.clone(),
                models: default_models.clone(),
                default_model: String::new(), // No model substitution for manually added providers
                priority: 0,
                enabled: true,
                default_strategy: 0,
                timeout_seconds: 30,
                max_retries: 3,
            };
            if let Err(e) = self.state.r.register_provider(&entity).await {
                tracing::warn!("Failed to register provider in LLM router: {}", e);
            }
        }

        let ref_label = if req.no_key {
            format!("keyless ({})", req.base_url)
        } else {
            format!("custody://{}", req.name)
        };
        tracing::info!("Provider '{}' configured ({})", req.name, ref_label);

        Ok(Response::new(OperationResult {
            success: true,
            message: ref_label,
        }))
    }

    async fn test_provider(
        &self,
        request: Request<ProviderName>,
    ) -> Result<Response<ProviderTestResult>, Status> {
        let req = request.into_inner();
        let name = req.name.to_lowercase();

        // Look up provider in proxy router config for base_url
        let router_config = self
            .state
            .s
            .get_proxy_router_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load router config: {}", e)))?
            .unwrap_or_default();

        let provider_cfg = router_config.providers.get(&name);
        if provider_cfg.is_none() {
            // Also check LLM entities (built-in providers)
            let llm_config = self.state.c.llm();
            let entity = llm_config.entities.iter().find(|e| e.name == name);
            if entity.is_none() {
                return Ok(Response::new(ProviderTestResult {
                    success: false,
                    latency_ms: 0,
                    error_message: format!("Provider '{}' not found in config", name),
                    base_url: String::new(),
                    model_tested: String::new(),
                }));
            }
        }

        // Verify provider is registered in LlmRouter (catches registration gaps)
        let llm_provider = self.state.r.get_provider(&name).await;
        if llm_provider.is_none() {
            return Ok(Response::new(ProviderTestResult {
                success: false,
                latency_ms: 0,
                error_message: format!(
                    "Provider '{}' exists in config but is not registered in LLM router. \
                     Try 'deploy register-providers' or restart the engine.",
                    name
                ),
                base_url: String::new(),
                model_tested: String::new(),
            }));
        }

        // Determine base_url: proxy router config first, then LLM entity
        let base_url = provider_cfg
            .map(|c| c.base_url.clone())
            .or_else(|| {
                let llm_config = self.state.c.llm();
                llm_config
                    .entities
                    .iter()
                    .find(|e| e.name == name)
                    .map(|e| e.base_url.clone())
            })
            .unwrap_or_default();

        // Determine model: check default_models (model_map override), fallback to provider name
        let model = self
            .state
            .r
            .get_default_model(&name)
            .await
            .unwrap_or_else(|| name.clone());

        // Determine API key
        let api_key: Option<String> = if let Some(cfg) = provider_cfg {
            if cfg.api_key_ref.is_empty() {
                // Keyless provider
                None
            } else {
                // Try to resolve from encrypted key store
                use ho_std::llm::state_ext::StateReadExt as _;
                let snapshot = self.state.s.cs.latest_snapshot();
                match snapshot.get_encrypted_api_key_store().await {
                    Ok(Some(store)) => {
                        use ho_std::llm::EncryptedApiKeyManager;
                        let manager = EncryptedApiKeyManager::from_store(&store);
                        // Can't decrypt without password, skip auth header for test
                        let _ = manager;
                        None
                    }
                    _ => None,
                }
            }
        } else {
            None
        };

        // Build minimal OpenAI-compatible test request
        let test_body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        });

        let url = if base_url.ends_with('/') {
            format!("{}v1/chat/completions", base_url)
        } else {
            format!("{}/v1/chat/completions", base_url)
        };

        // Execute real HTTP test
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Status::internal(format!("Failed to create HTTP client: {}", e)))?;

        let start = std::time::Instant::now();
        let mut request_builder = client.post(&url).json(&test_body);
        if let Some(key) = &api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", key));
        }

        match request_builder.send().await {
            Ok(response) => {
                let latency = start.elapsed().as_millis() as u32;
                let status = response.status();
                if status.is_success() {
                    Ok(Response::new(ProviderTestResult {
                        success: true,
                        latency_ms: latency,
                        error_message: String::new(),
                        base_url: base_url.clone(),
                        model_tested: model,
                    }))
                } else {
                    let body = response.text().await.unwrap_or_default();
                    let error = if body.len() > 200 {
                        format!("HTTP {} — {}", status, &body[..200])
                    } else {
                        format!("HTTP {} — {}", status, body)
                    };
                    Ok(Response::new(ProviderTestResult {
                        success: false,
                        latency_ms: latency,
                        error_message: error,
                        base_url: base_url.clone(),
                        model_tested: model,
                    }))
                }
            }
            Err(e) => {
                let latency = start.elapsed().as_millis() as u32;
                Ok(Response::new(ProviderTestResult {
                    success: false,
                    latency_ms: latency,
                    error_message: format!("Connection failed: {}", e),
                    base_url: base_url.clone(),
                    model_tested: model,
                }))
            }
        }
    }

    async fn remove_provider(
        &self,
        request: Request<RemoveProviderRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        let name = req.name.to_lowercase();

        if name.is_empty() {
            return Err(Status::invalid_argument("Provider name is required"));
        }

        if req.custody_password.is_empty() {
            return Err(Status::invalid_argument("Custody password is required"));
        }

        tracing::info!("Remove provider requested: {}", name);

        // Verify custody password by attempting to unlock the key store
        {
            use ho_std::llm::state_ext::StateReadExt as _;
            use ho_std::llm::EncryptedApiKeyManager;

            let snapshot = self.state.s.cs.latest_snapshot();
            if let Some(store) = snapshot
                .get_encrypted_api_key_store()
                .await
                .map_err(|e| Status::internal(format!("Failed to load key store: {}", e)))?
            {
                let mut manager = EncryptedApiKeyManager::from_store(&store);
                manager
                    .unlock(&req.custody_password)
                    .map_err(|_| Status::unauthenticated("Invalid custody password"))?;
            } else {
                return Err(Status::failed_precondition(
                    "No key store found — run sentinel bootstrap first",
                ));
            }
        }

        let mut removed = Vec::new();

        // 1. Remove from proxy router config
        {
            let mut pr = self.state.pr.write().await;
            let mut config = pr.config().clone();

            if config.providers.remove(&name).is_some() {
                removed.push("proxy config");
            }
            config.model_routes.retain(|_, provider| provider != &name);
            pr.update_config(config);
        }

        // 2. Remove LLM entity from Cnidarium
        {
            use ho_std::llm::state_ext::{StateReadExt, StateWriteExt};

            let snapshot = self.state.s.cs.latest_snapshot();
            let entities = snapshot
                .get_llm_providers()
                .await
                .map_err(|e| Status::internal(format!("Failed to load LLM entities: {}", e)))?;

            if entities.iter().any(|e| e.name == name) {
                let mut delta = cnidarium::StateDelta::new(self.state.s.cs.latest_snapshot());
                delta.delete_llm_provider(&name);
                self.state
                    .s
                    .cs
                    .commit(delta)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to commit entity removal: {}", e)))?;
                removed.push("llm entity");
            }
        }

        // 3. Remove encrypted API key (if exists)
        {
            use ho_std::llm::state_ext::{StateReadExt as _, StateWriteExt as _};

            let snapshot = self.state.s.cs.latest_snapshot();
            if let Some(mut store) = snapshot
                .get_encrypted_api_key_store()
                .await
                .map_err(|e| Status::internal(format!("Failed to load key store: {}", e)))?
            {
                let before = store.keys.len();
                store.keys.retain(|k| k.provider_name != name);
                if store.keys.len() < before {
                    let mut delta = cnidarium::StateDelta::new(self.state.s.cs.latest_snapshot());
                    delta.put_encrypted_api_key_store(&store);
                    self.state
                        .s
                        .cs
                        .commit(delta)
                        .await
                        .map_err(|e| Status::internal(format!("Failed to commit key removal: {}", e)))?;
                    removed.push("api key");

                    // Clear live key accessor
                    let pr = self.state.pr.read().await;
                    if let Some(accessor) = pr.key_accessor() {
                        let mut guard = accessor.write().await;
                        let _ = guard.set_key(&name, String::new()).await;
                    }
                }
            }
        }

        // 4. Unassign from all engine roles
        {
            let mut config = self
                .state
                .s
                .get_engine_role_config()
                .await
                .map_err(|e| Status::internal(format!("Failed to load engine role config: {}", e)))?
                .unwrap_or_default();

            let mut role_changed = false;
            for mapping in &mut config.mappings {
                if let Some(pos) = mapping.provider_ids.iter().position(|id| id == &name) {
                    mapping.provider_ids.remove(pos);
                    role_changed = true;
                }
            }

            if role_changed {
                config.version += 1;
                config.updated_at = Some({
                    let d = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    pbjson_types::Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: 0,
                    }
                });

                self.state
                    .s
                    .put_engine_role_config(&config)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("Failed to persist role config: {}", e))
                    })?;

                let mut pr = self.state.pr.write().await;
                pr.set_engine_role_config(Some(config));
                removed.push("role assignments");
            }
        }

        if removed.is_empty() {
            Ok(Response::new(OperationResult {
                success: false,
                message: format!("Provider '{}' not found in any configuration", name),
            }))
        } else {
            let msg = format!("Provider '{}' removed ({})", name, removed.join(", "));
            tracing::info!("{}", msg);
            Ok(Response::new(OperationResult {
                success: true,
                message: msg,
            }))
        }
    }

    // ============ Provider Role Assignments ============

    async fn assign_provider_role(
        &self,
        request: Request<AssignProviderRoleRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        let provider_name = req.provider_name.to_lowercase();
        let role = EngineRole::try_from(req.role)
            .map_err(|_| Status::invalid_argument("Invalid engine role"))?;

        if role == EngineRole::Unspecified {
            return Err(Status::invalid_argument("Engine role must be specified"));
        }

        // Validate provider exists
        {
            let pr = self.state.pr.read().await;
            if pr.get_provider(&provider_name).is_none() {
                return Err(Status::not_found(format!(
                    "Provider '{}' not found",
                    provider_name
                )));
            }
        }

        // Load or create EngineRoleConfig
        let mut config = self
            .state
            .s
            .get_engine_role_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load engine role config: {}", e)))?
            .unwrap_or_default();

        // Find or create mapping for this role
        let role_i32 = role as i32;
        let mapping = config
            .mappings
            .iter_mut()
            .find(|m| m.role == role_i32);

        match mapping {
            Some(m) => {
                // Skip if already assigned (idempotent)
                if !m.provider_ids.contains(&provider_name) {
                    m.provider_ids.push(provider_name.clone());
                }
            }
            None => {
                config.mappings.push(EngineRoleMapping {
                    role: role_i32,
                    provider_ids: vec![provider_name.clone()],
                });
            }
        }

        config.version += 1;
        config.updated_at = Some({
            let d = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            pbjson_types::Timestamp {
                seconds: d.as_secs() as i64,
                nanos: 0,
            }
        });

        self.state
            .s
            .put_engine_role_config(&config)
            .await
            .map_err(|e| Status::internal(format!("Failed to persist engine role config: {}", e)))?;

        // Update in-memory proxy router
        {
            let mut pr = self.state.pr.write().await;
            pr.set_engine_role_config(Some(config));
        }

        let role_name = format!("{:?}", role);
        Ok(Response::new(OperationResult {
            success: true,
            message: format!(
                "Provider '{}' assigned to role '{}'",
                provider_name, role_name
            ),
        }))
    }

    async fn unassign_provider_role(
        &self,
        request: Request<UnassignProviderRoleRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();
        let provider_name = req.provider_name.to_lowercase();
        let role = EngineRole::try_from(req.role)
            .map_err(|_| Status::invalid_argument("Invalid engine role"))?;

        if role == EngineRole::Unspecified {
            return Err(Status::invalid_argument("Engine role must be specified"));
        }

        let mut config = self
            .state
            .s
            .get_engine_role_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load engine role config: {}", e)))?
            .unwrap_or_default();

        let role_i32 = role as i32;
        let mut found = false;
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.role == role_i32) {
            if let Some(pos) = mapping.provider_ids.iter().position(|id| id == &provider_name) {
                mapping.provider_ids.remove(pos);
                found = true;
            }
        }

        if !found {
            return Ok(Response::new(OperationResult {
                success: false,
                message: format!(
                    "Provider '{}' was not assigned to role '{:?}'",
                    provider_name, role
                ),
            }));
        }

        config.version += 1;
        config.updated_at = Some({
            let d = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            pbjson_types::Timestamp {
                seconds: d.as_secs() as i64,
                nanos: 0,
            }
        });

        self.state
            .s
            .put_engine_role_config(&config)
            .await
            .map_err(|e| Status::internal(format!("Failed to persist engine role config: {}", e)))?;

        // Update in-memory proxy router
        {
            let mut pr = self.state.pr.write().await;
            pr.set_engine_role_config(Some(config));
        }

        Ok(Response::new(OperationResult {
            success: true,
            message: format!(
                "Provider '{}' unassigned from role '{:?}'",
                provider_name, role
            ),
        }))
    }

    async fn list_provider_roles(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<EngineRoleConfig>, Status> {
        let config = self
            .state
            .s
            .get_engine_role_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load engine role config: {}", e)))?
            .unwrap_or_default();

        Ok(Response::new(config))
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

    // Workspace and task worktree management removed - will simplify later

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
                    error_message:
                        "No key store found. Import a key with `ergors keys import-mnemonic`"
                            .to_string(),
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
            match ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager::get_default_key_name(
                &key_store,
            ) {
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
        let account = match key_store
            .derived_accounts
            .iter()
            .find(|a| a.key_name == key_name && a.account_index == req.hd_account_index)
        {
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
            // Per-service model name mapping
            model_map: req.model_map.clone(),
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
            .take(if req.limit > 0 {
                req.limit as usize
            } else {
                50
            })
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
            Err(ho_std::error::HoError::Storage(ref msg))
                if msg.contains("No deployment found") =>
            {
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
            provider_uri: String::new(), // Will be populated when endpoints are queried
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
            if let Err(e) = self
                .state
                .s
                .deactivate_deployment_label(&workflow.label)
                .await
            {
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
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "Akash deployment context not initialized. \
                 Ensure Akash config is present and keys are imported.",
            )
        })?;

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
            interactive_bid: false,            // Auto-select by default
            request_grant_from: String::new(), // No grant request by default
            grant_duration_seconds: 86400,     // 24 hours
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
                "Key manager is locked. Provide key_password to unlock for signing.",
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
                            tracing::info!(
                                "Deactivated label '{}' from active deployments (workflow failed)",
                                label
                            );
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
        tracing::info!("   Use 'ergors deploy get {}' to check status", session_id);

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
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition("Akash deployment context not initialized")
        })?;

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
                self.state
                    .s
                    .put_akash_workflow(&updated_workflow)
                    .await
                    .ok();

                // Remove from inference cache
                if !workflow.label.is_empty() {
                    if let Err(e) = self
                        .state
                        .r
                        .deployment_cache()
                        .remove_deployment(&workflow.label)
                        .await
                    {
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
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition("Akash deployment context not initialized")
        })?;

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
                self.state
                    .s
                    .put_akash_workflow(&updated_workflow)
                    .await
                    .ok();

                // Remove from inference cache
                if !workflow.label.is_empty() {
                    if let Err(e) = self
                        .state
                        .r
                        .deployment_cache()
                        .remove_deployment(&workflow.label)
                        .await
                    {
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
                    message: format!(
                        "Deployment closed successfully for session {}",
                        req.session_id
                    ),
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
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition("Akash deployment context not initialized")
        })?;

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

        match deployer
            .update_deployment(&workflow, &req.sdl_content)
            .await
        {
            Ok(()) => Ok(Response::new(OperationResult {
                success: true,
                message: format!(
                    "Deployment updated successfully for session {}",
                    req.session_id
                ),
            })),
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
        let akash_ctx = self.state.akash.as_ref().ok_or_else(|| {
            Status::failed_precondition("Akash deployment context not initialized")
        })?;

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

        tracing::info!(
            "Topping up escrow for session {} with {} uakt",
            req.session_id,
            req.amount_uakt
        );

        // Create deployer and top up escrow
        let deployer = akash_ctx.create_deployer(self.state.s.clone());

        match deployer.topup_escrow(&workflow, req.amount_uakt).await {
            Ok(()) => Ok(Response::new(OperationResult {
                success: true,
                message: format!(
                    "Escrow topped up with {} uakt for session {}",
                    req.amount_uakt, req.session_id
                ),
            })),
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
        let deployment_status = match AkashWorkflowStatus::try_from(workflow.status)
            .unwrap_or(AkashWorkflowStatus::Unspecified)
        {
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
            price_amount: workflow
                .provider
                .as_ref()
                .map(|p| p.bid_price_uakt.to_string())
                .unwrap_or_default(),
            created_at: workflow.created_at.as_ref().map(|t| t.seconds).unwrap_or(0),
            closed_on: workflow
                .completed_at
                .as_ref()
                .map(|t| t.seconds)
                .unwrap_or(0),
        });

        Ok(Response::new(LeaseStatusResponse {
            lease,
            endpoints: workflow.service_endpoints,
            balance_remaining_uakt: 0, // TODO: Query from chain
            deployment_status: deployment_status.to_string(),
        }))
    }

    /// Register deployment service endpoints as LLM providers
    async fn register_deployment_providers(
        &self,
        request: Request<RegisterDeploymentProvidersRequest>,
    ) -> Result<Response<RegisterDeploymentProvidersResponse>, Status> {
        use ho_std::types::ergors::orch::v1::{InferenceProviderConfig, InferenceProviderType};

        let req = request.into_inner();

        // Get workflow
        let workflow = self
            .state
            .s
            .get_akash_workflow_by_id_or_label(&req.session_id)
            .await
            .map_err(|e| Status::not_found(format!("Workflow not found: {}", e)))?;

        if workflow.service_endpoints.is_empty() {
            return Ok(Response::new(RegisterDeploymentProvidersResponse {
                success: false,
                message: "No service endpoints available in deployment. Deploy and wait for endpoints first.".to_string(),
                provider_labels: vec![],
                registered_count: 0,
            }));
        }

        // Get current provider config
        let mut router_config = self
            .state
            .s
            .get_proxy_router_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load router config: {}", e)))?
            .unwrap_or_default();

        let mut registered_labels = Vec::new();
        let mut errors = Vec::new();

        for endpoint in &workflow.service_endpoints {
            // Skip endpoints without a model name — they are not inference providers
            if endpoint.model_name.is_empty() {
                tracing::debug!(
                    "Skipping non-inference service '{}' (no model_name)",
                    endpoint.service_name
                );
                continue;
            }

            // Build provider label
            let label = if req.label_prefix.is_empty() {
                endpoint.service_name.clone()
            } else {
                format!("{}-{}", req.label_prefix, endpoint.service_name)
            };

            // Check if label already exists
            if router_config.providers.contains_key(&label) {
                errors.push(format!("Provider '{}' already exists", label));
                continue;
            }

            // Create provider config with default_model in metadata for restart persistence
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("default_model".to_string(), endpoint.model_name.clone());

            let provider_config = InferenceProviderConfig {
                provider_id: label.clone(),
                base_url: endpoint.external_uri.clone(),
                api_key_ref: String::new(), // Keyless provider
                enabled: true,
                provider_type: InferenceProviderType::Custom as i32,
                metadata,
                ..Default::default()
            };

            router_config.providers.insert(label.clone(), provider_config);

            // Add catch-all model route for this provider
            let wildcard_pattern = format!("{}/*", label);
            router_config.model_routes.insert(wildcard_pattern, label.clone());

            registered_labels.push(label);
        }

        // Save updated config and update in-memory state
        if !registered_labels.is_empty() {
            // Persist to storage
            self.state
                .s
                .put_proxy_router_config(&router_config)
                .await
                .map_err(|e| Status::internal(format!("Failed to save router config: {}", e)))?;

            // Update in-memory proxy router
            {
                let mut pr = self.state.pr.write().await;
                pr.update_config(router_config);
            }

            // Register each provider in LlmRouter for call_provider_by_name
            for label in &registered_labels {
                let endpoint = workflow.service_endpoints.iter()
                    .find(|ep| {
                        let expected = if req.label_prefix.is_empty() {
                            ep.service_name.clone()
                        } else {
                            format!("{}-{}", req.label_prefix, ep.service_name)
                        };
                        &expected == label
                    });
                if let Some(ep) = endpoint {
                    use ho_std::types::ergors::orch::v1::LlmEntity;
                    let entity = LlmEntity {
                        name: label.clone(),
                        base_url: ep.external_uri.clone(),
                        // Provider responds to its own label name
                        models: vec![label.clone()],
                        // Actual upstream model name for substitution
                        default_model: ep.model_name.clone(),
                        priority: 0,
                        enabled: true,
                        default_strategy: 0,
                        timeout_seconds: 60,
                        max_retries: 3,
                    };
                    if let Err(e) = self.state.r.register_provider(&entity).await {
                        tracing::warn!("Failed to register '{}' in LLM router: {}", label, e);
                    }
                }
            }

            tracing::info!(
                "Registered {} providers from deployment {}: {:?}",
                registered_labels.len(),
                req.session_id,
                registered_labels
            );
        }

        let message = if !errors.is_empty() {
            format!(
                "Registered {} providers. Warnings: {}",
                registered_labels.len(),
                errors.join(", ")
            )
        } else {
            format!("Successfully registered {} providers", registered_labels.len())
        };

        Ok(Response::new(RegisterDeploymentProvidersResponse {
            success: !registered_labels.is_empty(),
            message,
            provider_labels: registered_labels.clone(),
            registered_count: registered_labels.len() as u32,
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

        match self
            .state
            .s
            .add_trusted_provider(&req.address, &req.label)
            .await
        {
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
            Ok(list) => Ok(Response::new(ListTrustedProvidersResponse {
                providers: list.providers,
            })),
            Err(e) => {
                tracing::error!("Failed to list trusted providers: {}", e);
                Err(Status::internal(format!(
                    "Failed to list trusted providers: {}",
                    e
                )))
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

        // Simple ingestion path: chunk and store without embeddings
        if req.skip_embeddings {
            return self.rag_ingest_simple(req.content, req.uri, req.doc_type, req.tags).await;
        }

        // Check if embedder is configured
        let rag_config = match self.state.s.get_rag_config().await {
            Ok(Some(config)) => config,
            Ok(None) => {
                return Ok(Response::new(RagIngestResponse {
                    success: false,
                    chunk_count: 0,
                    chunk_ids: vec![],
                    message: "Embedder not configured. Use 'ergors rag configure' first."
                        .to_string(),
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
            Ok(rag) => match rag.ingest(doc, None).await {
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
            },
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
                                        content_preview: r.content[..r.content.len().min(200)]
                                            .to_string(),
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
        let (embedder_configured, endpoint, model, dimension) =
            match self.state.s.get_rag_config().await {
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
            Ok(count) => Ok(Response::new(RagOperationResult {
                success: true,
                message: format!("Deleted {} chunks from source '{}'", count, req.source_uri),
            })),
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
        match self
            .state
            .s
            .set_rag_config(&req.endpoint, &req.model, req.dimension)
            .await
        {
            Ok(()) => Ok(Response::new(RagOperationResult {
                success: true,
                message: format!(
                    "Embedder configured: {} ({}, {} dims)",
                    req.endpoint, req.model, req.dimension
                ),
            })),
            Err(e) => {
                tracing::error!("Failed to configure RAG: {}", e);
                Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!("Failed to configure: {}", e),
                }))
            }
        }
    }

    // ============ RLM Methods ============

    /// Execute RLM query with agentic code execution
    async fn rlm_query(
        &self,
        request: Request<RlmQueryRequest>,
    ) -> Result<Response<RlmQueryResponse>, Status> {
        let req = request.into_inner();

        // Load RLM config
        let rlm_config = self
            .state
            .s
            .get_rlm_config()
            .await
            .map_err(|e| Status::internal(format!("Failed to load RLM config: {}", e)))?
            .ok_or_else(|| {
                Status::failed_precondition("RLM not configured. Use 'ergors ask rlm configure'")
            })?;

        // Documents are now accessed on-demand via callbacks (DocumentAccessTrait).
        // The Python REPL discovers documents via list_documents() / search_document().
        let documents: Vec<ho_std::types::ergors::orch::v1::Document> = vec![];

        // Get RLM service from ErgorsAppState
        #[cfg(not(feature = "rlm"))]
        return Err(Status::unimplemented(
            "RLM feature not enabled. Rebuild with --features rlm"
        ));

        #[cfg(feature = "rlm")]
        {
            let rlm_service = self.state.rlm.as_ref().ok_or_else(|| {
                Status::unavailable("RLM service not initialized. Check startup logs.")
            })?;

            let start = std::time::Instant::now();

            let rlm_query = ergors_rlm::RlmQuery {
                query: req.query,
                guild_id: String::new(),
                max_iterations: if rlm_config.max_iterations > 0 {
                    rlm_config.max_iterations
                } else {
                    10
                },
                max_sub_calls: if rlm_config.max_sub_calls > 0 {
                    rlm_config.max_sub_calls
                } else {
                    50
                },
                primary_model: "default".to_string(),
                sub_model: "default".to_string(),
            };

            match rlm_service.query(rlm_query, documents.into_iter().map(ergors_rlm::Document::from).collect()).await {
                Ok(response) => Ok(Response::new(RlmQueryResponse {
                    answer: response.answer,
                    source_uris: response.source_uris,
                    iterations: response.iterations,
                    sub_llm_calls: response.sub_llm_calls,
                    cost_usd: response.cost_usd,
                    latency_ms: start.elapsed().as_millis() as u64,
                })),
                Err(e) => {
                    tracing::error!("RLM query failed: {}", e);
                    Err(Status::internal(format!("RLM query failed: {}", e)))
                }
            }
        }
    }

    /// Configure RLM provider selection
    async fn rlm_configure(
        &self,
        request: Request<RlmConfigureRequest>,
    ) -> Result<Response<RagOperationResult>, Status> {
        let req = request.into_inner();

        // Validate primary provider exists by checking ProxyRouter config
        let router_config = self.state.s.get_proxy_router_config().await
            .map_err(|e| Status::internal(format!("Failed to load router config: {}", e)))?;

        if let Some(config) = router_config {
            // Check if primary provider exists
            if !config.providers.contains_key(&req.primary_provider_label) {
                return Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!(
                        "Primary provider '{}' not found. Available providers: {}",
                        req.primary_provider_label,
                        config.providers.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                }));
            }

            // Check secondary provider if specified
            if !req.secondary_provider_label.is_empty()
                && !config.providers.contains_key(&req.secondary_provider_label) {
                return Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!(
                        "Secondary provider '{}' not found. Available providers: {}",
                        req.secondary_provider_label,
                        config.providers.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                }));
            }
        } else {
            return Ok(Response::new(RagOperationResult {
                success: false,
                message: "No providers configured. Use 'ergors provider add' first.".to_string(),
            }));
        }

        // Build config
        let config = ho_std::types::ergors::orch::v1::RlmConfig {
            primary_provider_label: req.primary_provider_label.clone(),
            secondary_provider_label: req.secondary_provider_label.clone(),
            max_iterations: req.max_iterations.unwrap_or(10),
            max_sub_calls: req.max_sub_calls.unwrap_or(50),
            updated_at: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                nanos: 0,
            }),
        };

        // Store configuration
        match self.state.s.set_rlm_config(&config).await {
            Ok(()) => Ok(Response::new(RagOperationResult {
                success: true,
                message: format!(
                    "RLM configured: primary={}, secondary={}",
                    config.primary_provider_label,
                    if config.secondary_provider_label.is_empty() {
                        "none"
                    } else {
                        &config.secondary_provider_label
                    }
                ),
            })),
            Err(e) => {
                tracing::error!("Failed to configure RLM: {}", e);
                Ok(Response::new(RagOperationResult {
                    success: false,
                    message: format!("Failed to configure: {}", e),
                }))
            }
        }
    }

    /// Get current RLM configuration
    async fn rlm_get_config(
        &self,
        _request: Request<RlmGetConfigRequest>,
    ) -> Result<Response<RlmGetConfigResponse>, Status> {
        match self.state.s.get_rlm_config().await {
            Ok(Some(config)) => Ok(Response::new(RlmGetConfigResponse {
                configured: true,
                config: Some(config),
            })),
            Ok(None) => Ok(Response::new(RlmGetConfigResponse {
                configured: false,
                config: None,
            })),
            Err(e) => {
                tracing::error!("Failed to get RLM config: {}", e);
                Err(Status::internal(format!("Failed to get RLM config: {}", e)))
            }
        }
    }

    // ============ Document Storage (Non-RAG) ============

    /// Ingest a document into storage
    async fn ingest_document(
        &self,
        request: Request<IngestDocumentRequest>,
    ) -> Result<Response<IngestDocumentResponse>, Status> {
        use ho_std::document::DocumentStorage;

        let req = request.into_inner();

        tracing::info!("Ingesting document: name={}, size={} bytes", req.name, req.content.len());

        // Store document
        let snapshot = self.state.s.cs.latest_snapshot();
        let mut state_delta = cnidarium::StateDelta::new(snapshot);

        let doc_id = DocumentStorage::store_document(
            &mut state_delta,
            &req.content,
            req.name,
            req.source,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to store document: {}", e);
            Status::internal(format!("Failed to store document: {}", e))
        })?;

        // Commit state changes
        self.state.s.cs.commit(state_delta).await.map_err(|e| {
            tracing::error!("Failed to commit document: {}", e);
            Status::internal(format!("Failed to commit document: {}", e))
        })?;

        tracing::info!("Document ingested: id={}", doc_id);

        Ok(Response::new(IngestDocumentResponse {
            document_id: doc_id.to_string(),
        }))
    }

    /// Retrieve a document by ID
    async fn retrieve_document(
        &self,
        request: Request<RetrieveDocumentRequest>,
    ) -> Result<Response<RetrieveDocumentResponse>, Status> {
        use ho_std::document::{DocumentId, DocumentStorage};

        let req = request.into_inner();

        tracing::debug!("Retrieving document: id={}", req.document_id);

        let doc_id = DocumentId::from_hex(req.document_id.clone()).map_err(|e| {
            tracing::warn!("Invalid document ID: {}", e);
            Status::invalid_argument(format!("Invalid document ID: {}", e))
        })?;

        let snapshot = self.state.s.cs.latest_snapshot();

        let (content, metadata) = DocumentStorage::retrieve_document(&snapshot, &doc_id)
            .await
            .map_err(|e| {
                if e.to_string().contains("not found") {
                    Status::not_found(format!("Document not found: {}", req.document_id))
                } else {
                    tracing::error!("DocumentStorage: Failed to retrieve document: {}", e);
                    Status::internal(format!("DocumentStorage: Failed to retrieve document: {}", e))
                }
            })?;

        let metadata_json = serde_json::to_vec(&metadata).map_err(|e| {
            tracing::error!("Failed to serialize metadata: {}", e);
            Status::internal(format!("Failed to serialize metadata: {}", e))
        })?;

        tracing::debug!("Document retrieved: id={}, size={} bytes", req.document_id, content.len());

        Ok(Response::new(RetrieveDocumentResponse {
            content,
            metadata_json,
        }))
    }

    /// List all documents with pagination
    async fn list_documents(
        &self,
        request: Request<ListDocumentsRequest>,
    ) -> Result<Response<ListDocumentsResponse>, Status> {
        use ho_std::document::DocumentStorage;
        use ho_std::types::ergors::orch::v1::DocumentInfo;

        let req = request.into_inner();

        let limit = req.limit.map(|l| l as usize);
        let offset = req.offset.map(|o| o as usize);

        tracing::debug!("Listing documents: limit={:?}, offset={:?}", limit, offset);

        let snapshot = self.state.s.cs.latest_snapshot();

        let documents = DocumentStorage::list_documents(&snapshot, limit, offset)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list documents: {}", e);
                Status::internal(format!("Failed to list documents: {}", e))
            })?;

        let document_infos: Vec<DocumentInfo> = documents
            .into_iter()
            .filter_map(|(doc_id, metadata)| {
                match serde_json::to_vec(&metadata) {
                    Ok(metadata_json) => Some(DocumentInfo {
                        document_id: doc_id.to_string(),
                        metadata_json,
                    }),
                    Err(e) => {
                        tracing::warn!("Failed to serialize metadata for {}: {}", doc_id, e);
                        None
                    }
                }
            })
            .collect();

        tracing::debug!("Listed {} documents", document_infos.len());

        Ok(Response::new(ListDocumentsResponse {
            documents: document_infos,
        }))
    }

    /// Delete a document by ID
    async fn delete_document(
        &self,
        request: Request<DeleteDocumentRequest>,
    ) -> Result<Response<DeleteDocumentResponse>, Status> {
        use ho_std::document::{DocumentId, DocumentStorage};

        let req = request.into_inner();

        tracing::info!("Deleting document: id={}", req.document_id);

        let doc_id = DocumentId::from_hex(req.document_id.clone()).map_err(|e| {
            tracing::warn!("Invalid document ID: {}", e);
            Status::invalid_argument(format!("Invalid document ID: {}", e))
        })?;

        let snapshot = self.state.s.cs.latest_snapshot();
        let mut state_delta = cnidarium::StateDelta::new(snapshot);

        DocumentStorage::delete_document(&mut state_delta, &doc_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete document: {}", e);
                Status::internal(format!("Failed to delete document: {}", e))
            })?;

        // Commit state changes
        self.state.s.cs.commit(state_delta).await.map_err(|e| {
            tracing::error!("Failed to commit document deletion: {}", e);
            Status::internal(format!("Failed to commit deletion: {}", e))
        })?;

        tracing::info!("Document deleted: id={}", req.document_id);

        Ok(Response::new(DeleteDocumentResponse { success: true }))
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

        let client = AkashClient::from_akash_config(&akash_config)
            .map_err(|e| Status::internal(format!("Failed to create client: {}", e)))?;

        tracing::debug!(
            "Querying balance for {} (denom: {}) via {}",
            req.address,
            req.denom,
            client.rest_endpoint()
        );

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
        tracing::info!(
            "Registering SDL template contract: {}",
            req.contract_address
        );

        match self
            .state
            .s
            .register_sdl_template_contract(&req.contract_address, req.label, req.code_id)
            .await
        {
            Ok(()) => Ok(Response::new(RegisterSdlTemplateResponse {
                success: true,
                message: format!("SDL template contract registered: {}", req.contract_address),
            })),
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
        tracing::info!(
            "Getting SDL template from contract {}",
            req.contract_address
        );

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
                    let template_json_bytes = serde_json::to_vec(&template_json).map_err(|e| {
                        Status::internal(format!("Failed to serialize template JSON: {}", e))
                    })?;
                    let template_json_struct: pbjson_types::Struct =
                        serde_json::from_slice(&template_json_bytes).map_err(|e| {
                            Status::internal(format!("Failed to convert template JSON: {}", e))
                        })?;

                    Ok(Response::new(GetSdlTemplateResponse {
                        sdl_template,
                        template_json: Some(template_json_struct),
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to query SDL template: {}", e);
                    Err(Status::internal(format!(
                        "Failed to query SDL template: {}",
                        e
                    )))
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
        tracing::info!(
            "Getting SDL defaults from contract {}",
            req.contract_address
        );

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
                    Err(Status::internal(format!(
                        "Failed to query SDL defaults: {}",
                        e
                    )))
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
        tracing::info!(
            "Rendering SDL template from contract {}",
            req.contract_address
        );

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
                Ok((rendered_sdl, used_variables)) => {
                    Ok(Response::new(RenderSdlTemplateResponse {
                        rendered_sdl,
                        used_variables,
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to render SDL template: {}", e);
                    Err(Status::internal(format!(
                        "Failed to render SDL template: {}",
                        e
                    )))
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

        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("Chain config is required"))?;

        if config.chain_id.is_empty() {
            return Err(Status::invalid_argument("chain_id cannot be empty"));
        }

        tracing::info!(
            "Setting chain config for: {} ({})",
            config.chain_name,
            config.chain_id
        );

        match self.state.s.put_chain_config(&config).await {
            Ok(()) => Ok(Response::new(SetChainConfigResponse {
                success: true,
                message: format!("Chain config stored for: {}", config.chain_id),
            })),
            Err(e) => {
                tracing::error!("Failed to store chain config: {}", e);
                Err(Status::internal(format!(
                    "Failed to store chain config: {}",
                    e
                )))
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
                Err(Status::internal(format!(
                    "Failed to get chain config: {}",
                    e
                )))
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
                Err(Status::internal(format!(
                    "Failed to list chain configs: {}",
                    e
                )))
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
                Err(Status::internal(format!(
                    "Failed to delete chain config: {}",
                    e
                )))
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
                let enabled = self
                    .state
                    .s
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

        // Hot-start the gateway if the manager is running (no engine restart needed)
        if let Some(ref gm) = self.state.gm {
            if let Err(e) = gm.start_one(&req.gateway_id).await {
                tracing::warn!("Gateway {} enabled but failed to hot-start: {}. Will start on next engine restart.", req.gateway_id, e);
            }
        }

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
        let req = request.into_inner();
        tracing::info!("Configuring Discord gateway");

        // Get node pubkey from NetworkManifold (populated at runtime with actual key)
        let node_pubkey = {
            let nm = self.state.nm.lock().await;
            nm.identity()
                .public_key
                .clone()
                .ok_or_else(|| Status::internal("Node public key not available for encryption"))?
        };

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
            let (encrypted_value, nonce) =
                encrypt_gateway_secret(&req.bot_token, &node_pubkey).map_err(Status::internal)?;

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
            config
                .settings
                .insert("bot_token_encrypted".to_string(), "true".to_string());
            config.settings.remove("bot_token"); // Remove any plaintext token
        }

        if let Some(respond_mentions) = req.respond_to_mentions {
            config.settings.insert(
                "respond_to_mentions".to_string(),
                respond_mentions.to_string(),
            );
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
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            if !guilds.contains(&req.guild_id) {
                guilds.push(req.guild_id.clone());
                config
                    .settings
                    .insert("allowed_guild_ids".to_string(), guilds.join(","));

                self.state
                    .s
                    .put_gateway_config(&config)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("Failed to save gateway config: {}", e))
                    })?;
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

            config
                .settings
                .insert("allowed_guild_ids".to_string(), guilds.join(","));

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
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            let allowed_channel_ids: Vec<String> = config
                .settings
                .get("allowed_channel_ids")
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            Ok(Response::new(GetDiscordConfigResponse {
                token_configured: config.settings.get("bot_token_encrypted").map(|s| s == "true").unwrap_or(false),
                allowed_guild_ids,
                allowed_channel_ids,
                respond_to_mentions: config
                    .settings
                    .get("respond_to_mentions")
                    .map(|s| s == "true")
                    .unwrap_or(true),
                respond_to_dms: config
                    .settings
                    .get("respond_to_dms")
                    .map(|s| s == "true")
                    .unwrap_or(false),
            }))
        } else {
            Ok(Response::new(GetDiscordConfigResponse {
                token_configured: false,
                allowed_guild_ids: vec![],
                allowed_channel_ids: vec![],
                respond_to_mentions: true,
                respond_to_dms: false,
            }))
        }
    }

    // Task worktree management methods removed - use deploy/orchestrator instead

    // async fn add_workspace(
    //     &self,
    //     _request: Request<AddWorkspaceRequest>,
    // ) -> Result<Response<AddWorkspaceResponse>, Status> {
    //     Err(Status::unimplemented("Workspace management moved to commands"))
    // }

    // async fn get_workspace(
    //     &self,
    //     _request: Request<GetWorkspaceRequest>,
    // ) -> Result<Response<GetWorkspaceResponse>, Status> {
    //     Err(Status::unimplemented("Workspace management moved to commands"))
    // }

    // async fn list_workspaces(
    //     &self,
    //     _request: Request<ListWorkspacesRequest>,
    // ) -> Result<Response<ListWorkspacesResponse>, Status> {
    //     Err(Status::unimplemented("Workspace management moved to commands"))
    // }

    // async fn remove_workspace(
    //     &self,
    //     _request: Request<RemoveWorkspaceRequest>,
    // ) -> Result<Response<OperationResult>, Status> {
    //     Err(Status::unimplemented("Workspace management moved to commands"))
    // }

    // async fn create_task_worktree(
    //     &self,
    //     _request: Request<CreateTaskWorktreeRequest>,
    // ) -> Result<Response<CreateTaskWorktreeResponse>, Status> {
    //     Err(Status::unimplemented("Task worktree management moved to deploy/orchestrator"))
    // }

    // async fn list_task_worktrees(
    //     &self,
    //     _request: Request<ListTaskWorktreesRequest>,
    // ) -> Result<Response<ListTaskWorktreesResponse>, Status> {
    //     Err(Status::unimplemented("Task worktree management moved to deploy/orchestrator"))
    // }

    // async fn complete_task_worktree(
    //     &self,
    //     _request: Request<CompleteTaskWorktreeRequest>,
    // ) -> Result<Response<CompleteTaskWorktreeResponse>, Status> {
    //     Err(Status::unimplemented("Task worktree management moved to deploy/orchestrator"))
    // }

    // async fn fail_task_worktree(
    //     &self,
    //     _request: Request<FailTaskWorktreeRequest>,
    // ) -> Result<Response<OperationResult>, Status> {
    //     Err(Status::unimplemented("Task worktree management moved to deploy/orchestrator"))
    // }

    // async fn resolve_conflict(
    //     &self,
    //     _request: Request<ResolveConflictRequest>,
    // ) -> Result<Response<ResolveConflictResponse>, Status> {
    //     Err(Status::unimplemented("Conflict resolution moved to deploy/orchestrator"))
    // }

    // ============ CLI Key Management ============

    async fn register_cli_key(
        &self,
        request: Request<RegisterCliKeyRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        if req.public_key_hex.is_empty() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Public key hex is required".to_string(),
            }));
        }

        // Validate hex is valid (should be 64 hex chars = 32 bytes for Ed25519)
        if hex::decode(&req.public_key_hex).is_err() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Invalid hex encoding for public key".to_string(),
            }));
        }

        match self
            .state
            .s
            .add_authorized_cli_key(&req.public_key_hex, &req.label)
            .await
        {
            Ok(()) => {
                // Update runtime set
                self.authorized_keys.add(&req.public_key_hex);
                tracing::info!("Registered CLI key: {} ({})", req.public_key_hex, req.label);
                Ok(Response::new(OperationResult {
                    success: true,
                    message: format!("CLI key {} registered", req.public_key_hex),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to register CLI key: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to register CLI key: {}", e),
                }))
            }
        }
    }

    async fn revoke_cli_key(
        &self,
        request: Request<RevokeCliKeyRequest>,
    ) -> Result<Response<OperationResult>, Status> {
        let req = request.into_inner();

        if req.public_key_hex.is_empty() {
            return Ok(Response::new(OperationResult {
                success: false,
                message: "Public key hex is required".to_string(),
            }));
        }

        match self
            .state
            .s
            .remove_authorized_cli_key(&req.public_key_hex)
            .await
        {
            Ok(removed) => {
                if removed {
                    self.authorized_keys.remove(&req.public_key_hex);
                    tracing::info!("Revoked CLI key: {}", req.public_key_hex);
                    Ok(Response::new(OperationResult {
                        success: true,
                        message: format!("CLI key {} revoked", req.public_key_hex),
                    }))
                } else {
                    Ok(Response::new(OperationResult {
                        success: false,
                        message: format!("CLI key {} not found", req.public_key_hex),
                    }))
                }
            }
            Err(e) => {
                tracing::error!("Failed to revoke CLI key: {}", e);
                Ok(Response::new(OperationResult {
                    success: false,
                    message: format!("Failed to revoke CLI key: {}", e),
                }))
            }
        }
    }

    async fn list_cli_keys(
        &self,
        _request: Request<ListCliKeysRequest>,
    ) -> Result<Response<ListCliKeysResponse>, Status> {
        match self.state.s.get_authorized_cli_keys().await {
            Ok(list) => Ok(Response::new(list)),
            Err(e) => {
                tracing::error!("Failed to list CLI keys: {}", e);
                Err(Status::internal(format!(
                    "Failed to list CLI keys: {}",
                    e
                )))
            }
        }
    }
}

/// Start the gRPC management server with Ed25519 auth interceptor
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    service: ManagementServiceImpl,
    rlm_service: Option<crate::client::RlmDocService>,
    authorized_keys: crate::auth::grpc::AuthorizedCliKeys,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ho_std::types::ergors::management::v1::management_service_server::ManagementServiceServer;

    tracing::info!("Starting gRPC management server on {}", addr);

    let interceptor = crate::auth::grpc::create_grpc_auth_interceptor(authorized_keys);

    // Configure message size limits BEFORE applying interceptor (100MB)
    let configured_service = ManagementServiceServer::new(service)
        .max_decoding_message_size(100 * 1024 * 1024) // 100MB limit
        .max_encoding_message_size(100 * 1024 * 1024); // 100MB limit

    // Manually apply interceptor using InterceptedService
    use tonic::codegen::InterceptedService;
    let intercepted_service = InterceptedService::new(configured_service, interceptor);

    // Configure server with larger HTTP/2 window sizes
    let mut server = tonic::transport::Server::builder()
        .initial_stream_window_size(100 * 1024 * 1024) // 100MB
        .initial_connection_window_size(100 * 1024 * 1024) // 100MB
        .add_service(intercepted_service);

    // Add RLM document service if provided
    if let Some(rlm_svc) = rlm_service {
        tracing::info!("Registering RLM document service");
        let rlm_server = rlm_svc.into_server()
            .max_decoding_message_size(100 * 1024 * 1024) // 100MB limit
            .max_encoding_message_size(100 * 1024 * 1024); // 100MB limit
        server = server.add_service(rlm_server);
    }

    server.serve(addr).await?;

    Ok(())
}
