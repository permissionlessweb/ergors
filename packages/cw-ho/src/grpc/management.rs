//! ManagementService gRPC implementation
//!
//! Provides the server-side implementation of the management gRPC service.

use crate::session::{SessionManager, SessionManagerConfig};
use crate::ErgorsAppState;
use async_stream::try_stream;
use ho_std::traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait};
use ho_std::types::ergors::management::v1::{
    management_service_server::ManagementService,
    // Workspace types
    AddWorkspaceRequest,
    AddWorkspaceResponse,
    // Akash deployment types
    AdvanceAkashDeploymentRequest,
    AdvanceAkashDeploymentResponse,
    CancelAkashDeploymentRequest,
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
    FractalSession,
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
    TokenInfo,
    TokenLabel,
    TokenList,
    TokenResponse,
    UpdateSessionRequest,
    UpdateSessionResponse,
};
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashWorkflowStatus, AkashWorkflowStep, ConfiguredSdl,
};
use ho_std::types::ergors::network::v1::{NetworkTopology, NodeIdentity, NodeType};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
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
            .map(|pk| hex::encode(pk))
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
            .map(|pk| hex::encode(pk))
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
        let identity = self.state.c.identity();

        Ok(Response::new(NodeIdentity {
            host: identity.host.clone(),
            p2p_port: identity.p2p_port,
            api_port: identity.api_port,
            user: identity.user.clone(),
            os: identity.os,
            ssh_port: identity.ssh_port,
            node_type: identity.node_type.clone(),
            public_key: identity.public_key.clone(),
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
                let metrics = session.metrics.clone();
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
                    .map(|pk| hex::encode(pk))
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
                    .map(|pk| hex::encode(pk))
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

        // Use engine config as defaults, override with request params
        let akash_config = self.state.c.0.akash.as_ref();
        let chain_id = if !req.chain_id.is_empty() {
            req.chain_id
        } else if let Some(cfg) = akash_config {
            if cfg.chain_id.is_empty() { "akashnet-2".to_string() } else { cfg.chain_id.clone() }
        } else {
            "akashnet-2".to_string()
        };
        let node_endpoint = if !req.node_endpoint.is_empty() {
            req.node_endpoint
        } else if let Some(cfg) = akash_config {
            if cfg.rpc_endpoint.is_empty() { "https://rpc-akash.ecostake.com:443".to_string() } else { cfg.rpc_endpoint.clone() }
        } else {
            "https://rpc-akash.ecostake.com:443".to_string()
        };

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
                configured_at: Some(now.clone()),
            })
        } else {
            None
        };

        let workflow = AkashDeploymentWorkflow {
            session_id: session_id.clone(),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            selected_key_name: req.key_name,
            account_address: String::new(),
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
            created_at: Some(now.clone()),
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

        let workflow = self
            .state
            .s
            .get_akash_workflow(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workflow: {}", e)))?;

        Ok(Response::new(GetAkashDeploymentResponse { workflow }))
    }

    async fn advance_akash_deployment(
        &self,
        request: Request<AdvanceAkashDeploymentRequest>,
    ) -> Result<Response<AdvanceAkashDeploymentResponse>, Status> {
        let req = request.into_inner();

        let mut workflow = self
            .state
            .s
            .get_akash_workflow(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workflow: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Workflow not found: {}", req.session_id)))?;

        // Advance to next step
        let current = workflow.current_step;
        let next_step = current + 1;

        // Validate step transition
        if current >= AkashWorkflowStep::Complete as i32 {
            return Ok(Response::new(AdvanceAkashDeploymentResponse {
                success: false,
                workflow: Some(workflow),
                error_message: "Workflow is already complete".to_string(),
            }));
        }

        workflow.current_step = next_step;
        workflow.status = AkashWorkflowStatus::Running as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Mark as complete if we reached the complete step
        if next_step >= AkashWorkflowStep::Complete as i32 {
            workflow.status = AkashWorkflowStatus::Completed as i32;
            workflow.completed_at = workflow.updated_at.clone();
        }

        // Persist
        self.state.s.put_akash_workflow(&workflow).await.ok();

        Ok(Response::new(AdvanceAkashDeploymentResponse {
            success: true,
            workflow: Some(workflow),
            error_message: String::new(),
        }))
    }

    async fn query_akash_bids(
        &self,
        request: Request<QueryAkashBidsRequest>,
    ) -> Result<Response<QueryAkashBidsResponse>, Status> {
        let req = request.into_inner();

        let workflow = self
            .state
            .s
            .get_akash_workflow(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workflow: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Workflow not found: {}", req.session_id)))?;

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

        let mut workflow = self
            .state
            .s
            .get_akash_workflow(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workflow: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Workflow not found: {}", req.session_id)))?;

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

        let mut workflow = self
            .state
            .s
            .get_akash_workflow(&req.session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get workflow: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Workflow not found: {}", req.session_id)))?;

        workflow.status = AkashWorkflowStatus::Cancelled as i32;
        workflow.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            nanos: 0,
        });

        // Persist
        self.state.s.put_akash_workflow(&workflow).await.ok();

        Ok(Response::new(OperationResult {
            success: true,
            message: format!("Deployment {} cancelled", req.session_id),
        }))
    }
}

/// Start the gRPC management server
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    service: ManagementServiceImpl,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ho_std::types::ergors::management::v1::management_service_server::ManagementServiceServer;

    tracing::info!("Starting gRPC management server on {}", addr);

    tonic::transport::Server::builder()
        .add_service(ManagementServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
