//! gRPC client for engine management
//!
//! Provides a wrapper around the tonic-generated client.

use anyhow::{Context, Result};
use ho_std::types::ergors::management::v1::{
    management_service_client::ManagementServiceClient as ProtoClient,
    // Workspace types
    AddWorkspaceRequest,
    AddWorkspaceResponse,
    // Akash deployment types
    AdvanceAkashDeploymentRequest,
    AdvanceAkashDeploymentResponse,
    ApproveGrantRequest,
    CancelAkashDeploymentRequest,
    ConfigureProxyRoutesRequest,
    CompleteTaskWorktreeRequest,
    CompleteTaskWorktreeResponse,
    ConfigData,
    ConfigUpdate,
    CreateAkashDeploymentRequest,
    CreateAkashDeploymentResponse,
    CreateTaskWorktreeRequest,
    CreateTaskWorktreeResponse,
    Empty,
    EngineState,
    EngineStatus,
    FailTaskWorktreeRequest,
    GetAkashDeploymentRequest,
    GetAkashDeploymentResponse,
    GetWorkspaceRequest,
    GetWorkspaceResponse,
    ListAkashDeploymentsRequest,
    ListAkashDeploymentsResponse,
    ListGrantRequestsRequest,
    ListGrantRequestsResponse,
    ListTaskWorktreesRequest,
    ListTaskWorktreesResponse,
    ListWorkspacesRequest,
    ListWorkspacesResponse,
    NodeIdRequest,
    NodeTypeRequest,
    OperationResult,
    PeerAddress,
    ProviderConfig,
    ProviderList,
    ProviderName,
    ProviderTestResult,
    QueryAkashBidsRequest,
    QueryAkashBidsResponse,
    QueryBalanceRequest,
    QueryBalanceResponse,
    RemoveWorkspaceRequest,
    // SDL template types
    ListSdlTemplatesRequest,
    ListSdlTemplatesResponse,
    GetSdlTemplateRequest,
    GetSdlTemplateResponse,
    GetSdlDefaultsRequest,
    GetSdlDefaultsResponse,
    RenderSdlTemplateRequest,
    RenderSdlTemplateResponse,
    RequestGrantRequest,
    RequestGrantResponse,
    RevokeGrantRequest,
    SelectAkashProviderRequest,
    SelectAkashProviderResponse,
    SetWorkflowEndpointsRequest,
    SetWorkflowEndpointsResponse,
    ShutdownRequest,
    SyncWorkspaceRequest,
    SyncWorkspaceResponse,
    TokenIdRequest,
    TokenLabel,
    TokenList,
    TokenResponse,
};
use ho_std::types::ergors::network::v1::{NetworkTopology, NodeIdentity, NodeType};
use tonic::transport::Channel;

/// Management client wrapping the generated tonic client
pub struct ManagementClient {
    inner: ProtoClient<Channel>,
}

impl ManagementClient {
    /// Connect to the engine gRPC server
    pub async fn connect(addr: &str) -> Result<Self> {
        let inner = ProtoClient::connect(addr.to_string())
            .await
            .context("Failed to connect to engine. Is it running?")?;

        Ok(Self { inner })
    }

    // ============ Lifecycle ============

    /// Get engine status
    pub async fn get_status(&mut self) -> Result<EngineStatus> {
        let response = self
            .inner
            .get_status(Empty {})
            .await
            .context("Failed to get engine status")?;

        Ok(response.into_inner())
    }

    /// Shutdown the engine
    pub async fn shutdown(&mut self, force: bool) -> Result<OperationResult> {
        let response = self
            .inner
            .shutdown(ShutdownRequest { force })
            .await
            .context("Failed to send shutdown request")?;

        Ok(response.into_inner())
    }

    // ============ Node Identity ============

    /// Get node identity
    pub async fn get_node_identity(&mut self) -> Result<NodeIdentity> {
        let response = self
            .inner
            .get_node_identity(Empty {})
            .await
            .context("Failed to get node identity")?;

        Ok(response.into_inner())
    }

    /// Generate new node identity
    pub async fn generate_node_identity(
        &mut self,
        node_type: NodeType,
    ) -> Result<(Vec<u8>, String, String)> {
        let response = self
            .inner
            .generate_node_identity(NodeTypeRequest {
                node_type: node_type.into(),
            })
            .await
            .context("Failed to generate node identity")?;

        let inner = response.into_inner();
        Ok((inner.public_key, inner.node_id, inner.mnemonic_phrase))
    }

    // ============ Configuration ============

    /// Get current configuration
    pub async fn get_config(&mut self) -> Result<ConfigData> {
        let response = self
            .inner
            .get_config(Empty {})
            .await
            .context("Failed to get configuration")?;

        Ok(response.into_inner())
    }

    /// Update a configuration value
    pub async fn update_config(&mut self, key: &str, value: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .update_config(ConfigUpdate {
                key: key.to_string(),
                value: value.to_string(),
            })
            .await
            .context("Failed to update configuration")?;

        Ok(response.into_inner())
    }

    // ============ Network ============

    /// Get network topology
    pub async fn get_network_topology(&mut self) -> Result<NetworkTopology> {
        let response = self
            .inner
            .get_network_topology(Empty {})
            .await
            .context("Failed to get network topology")?;

        Ok(response.into_inner())
    }

    /// Add a peer
    pub async fn add_peer(&mut self, address: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .add_peer(PeerAddress {
                address: address.to_string(),
            })
            .await
            .context("Failed to add peer")?;

        Ok(response.into_inner())
    }

    /// Remove a peer
    pub async fn remove_peer(&mut self, node_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .remove_peer(NodeIdRequest {
                node_id: node_id.to_string(),
            })
            .await
            .context("Failed to remove peer")?;

        Ok(response.into_inner())
    }

    // ============ Providers ============

    /// List configured providers
    pub async fn list_providers(&mut self) -> Result<ProviderList> {
        let response = self
            .inner
            .list_providers(Empty {})
            .await
            .context("Failed to list providers")?;

        Ok(response.into_inner())
    }

    /// Configure a provider
    pub async fn configure_provider(
        &mut self,
        name: &str,
        api_key: &str,
        set_as_default: bool,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .configure_provider(ProviderConfig {
                name: name.to_string(),
                api_key: api_key.to_string(),
                set_as_default,
            })
            .await
            .context("Failed to configure provider")?;

        Ok(response.into_inner())
    }

    /// Test a provider
    pub async fn test_provider(&mut self, name: &str) -> Result<ProviderTestResult> {
        let response = self
            .inner
            .test_provider(ProviderName {
                name: name.to_string(),
            })
            .await
            .context("Failed to test provider")?;

        Ok(response.into_inner())
    }

    // ============ Auth Tokens ============

    /// Register a new auth token
    pub async fn register_auth_token(&mut self, label: &str) -> Result<TokenResponse> {
        let response = self
            .inner
            .register_auth_token(TokenLabel {
                label: label.to_string(),
            })
            .await
            .context("Failed to register auth token")?;

        Ok(response.into_inner())
    }

    /// Revoke an auth token
    pub async fn revoke_auth_token(&mut self, token_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .revoke_auth_token(TokenIdRequest {
                token_id: token_id.to_string(),
            })
            .await
            .context("Failed to revoke auth token")?;

        Ok(response.into_inner())
    }

    /// List auth tokens
    pub async fn list_auth_tokens(&mut self) -> Result<TokenList> {
        let response = self
            .inner
            .list_auth_tokens(Empty {})
            .await
            .context("Failed to list auth tokens")?;

        Ok(response.into_inner())
    }

    // ============ Workspace Management ============

    /// Add a new workspace
    pub async fn add_workspace(
        &mut self,
        name: &str,
        remote_url: Option<&str>,
    ) -> Result<AddWorkspaceResponse> {
        let response = self
            .inner
            .add_workspace(AddWorkspaceRequest {
                name: name.to_string(),
                remote_url: remote_url.map(|s| s.to_string()).unwrap_or_default(),
            })
            .await
            .context("Failed to add workspace")?;

        Ok(response.into_inner())
    }

    /// Get workspace details
    pub async fn get_workspace(&mut self, workspace_id: &str) -> Result<GetWorkspaceResponse> {
        let response = self
            .inner
            .get_workspace(GetWorkspaceRequest {
                workspace_id: workspace_id.to_string(),
            })
            .await
            .context("Failed to get workspace")?;

        Ok(response.into_inner())
    }

    /// List all workspaces
    pub async fn list_workspaces(&mut self, limit: u32) -> Result<ListWorkspacesResponse> {
        let response = self
            .inner
            .list_workspaces(ListWorkspacesRequest { limit, offset: 0 })
            .await
            .context("Failed to list workspaces")?;

        Ok(response.into_inner())
    }

    /// Remove a workspace
    pub async fn remove_workspace(
        &mut self,
        workspace_id: &str,
        force: bool,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .remove_workspace(RemoveWorkspaceRequest {
                workspace_id: workspace_id.to_string(),
                force,
            })
            .await
            .context("Failed to remove workspace")?;

        Ok(response.into_inner())
    }

    /// Sync workspace with remote
    pub async fn sync_workspace(
        &mut self,
        workspace_id: &str,
        remote_name: &str,
        push: bool,
        fetch: bool,
    ) -> Result<SyncWorkspaceResponse> {
        let response: tonic::Response<SyncWorkspaceResponse> = self
            .inner
            .sync_workspace(SyncWorkspaceRequest {
                workspace_id: workspace_id.to_string(),
                remote_name: remote_name.to_string(),
                push,
                fetch,
            })
            .await
            .context("Failed to sync workspace")?;

        Ok(response.into_inner())
    }

    // ============ Task Worktree Management ============

    /// Create a new task worktree
    pub async fn create_task_worktree(
        &mut self,
        workspace_id: &str,
        task_id: &str,
        assigned_node_id: Option<&str>,
    ) -> Result<CreateTaskWorktreeResponse> {
        let response = self
            .inner
            .create_task_worktree(CreateTaskWorktreeRequest {
                workspace_id: workspace_id.to_string(),
                task_id: task_id.to_string(),
                assigned_node_id: assigned_node_id.map(|s| s.to_string()).unwrap_or_default(),
            })
            .await
            .context("Failed to create task worktree")?;

        Ok(response.into_inner())
    }

    /// List task worktrees
    pub async fn list_task_worktrees(
        &mut self,
        workspace_id: Option<&str>,
        assigned_node_id: Option<&str>,
    ) -> Result<ListTaskWorktreesResponse> {
        let response = self
            .inner
            .list_task_worktrees(ListTaskWorktreesRequest {
                workspace_id: workspace_id.map(|s| s.to_string()).unwrap_or_default(),
                assigned_node_id: assigned_node_id.map(|s| s.to_string()).unwrap_or_default(),
                status: 0,
            })
            .await
            .context("Failed to list task worktrees")?;

        Ok(response.into_inner())
    }

    /// Complete a task worktree
    pub async fn complete_task_worktree(
        &mut self,
        task_id: &str,
        commit_message: &str,
        merge_to_main: bool,
    ) -> Result<CompleteTaskWorktreeResponse> {
        let response = self
            .inner
            .complete_task_worktree(CompleteTaskWorktreeRequest {
                task_id: task_id.to_string(),
                commit_message: commit_message.to_string(),
                merge_to_main,
            })
            .await
            .context("Failed to complete task worktree")?;

        Ok(response.into_inner())
    }

    /// Fail/abandon a task worktree
    pub async fn fail_task_worktree(
        &mut self,
        task_id: &str,
        reason: &str,
        cleanup: bool,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .fail_task_worktree(FailTaskWorktreeRequest {
                task_id: task_id.to_string(),
                reason: reason.to_string(),
                cleanup,
            })
            .await
            .context("Failed to fail task worktree")?;

        Ok(response.into_inner())
    }

    // ============ Akash Deployment Management ============

    /// Create a new Akash deployment workflow
    pub async fn create_akash_deployment(
        &mut self,
        key_name: &str,
        hd_account_index: u32,
        sdl_content: &str,
        template_name: &str,
        sdl_variables: std::collections::HashMap<String, String>,
        node_endpoint: &str,
        chain_id: &str,
        auto_run: bool,
    ) -> Result<CreateAkashDeploymentResponse> {
        let response = self
            .inner
            .create_akash_deployment(CreateAkashDeploymentRequest {
                key_name: key_name.to_string(),
                hd_account_index,
                sdl_content: sdl_content.to_string(),
                template_name: template_name.to_string(),
                sdl_variables,
                node_endpoint: node_endpoint.to_string(),
                chain_id: chain_id.to_string(),
                auto_run,
            })
            .await
            .context("Failed to create Akash deployment")?;

        Ok(response.into_inner())
    }

    /// List Akash deployment workflows
    pub async fn list_akash_deployments(
        &mut self,
        status: i32,
        limit: u32,
    ) -> Result<ListAkashDeploymentsResponse> {
        let response = self
            .inner
            .list_akash_deployments(ListAkashDeploymentsRequest {
                status,
                limit,
                offset: 0,
            })
            .await
            .context("Failed to list Akash deployments")?;

        Ok(response.into_inner())
    }

    /// Get Akash deployment workflow details
    pub async fn get_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<GetAkashDeploymentResponse> {
        let response = self
            .inner
            .get_akash_deployment(GetAkashDeploymentRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to get Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Advance Akash deployment to next step
    pub async fn advance_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<AdvanceAkashDeploymentResponse> {
        let response = self
            .inner
            .advance_akash_deployment(AdvanceAkashDeploymentRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to advance Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Query bids for a deployment
    pub async fn query_akash_bids(
        &mut self,
        session_id: &str,
    ) -> Result<QueryAkashBidsResponse> {
        let response = self
            .inner
            .query_akash_bids(QueryAkashBidsRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to query Akash bids")?;

        Ok(response.into_inner())
    }

    /// Select a provider for Akash deployment
    pub async fn select_akash_provider(
        &mut self,
        session_id: &str,
        provider_address: &str,
        bid_price_uakt: u64,
    ) -> Result<SelectAkashProviderResponse> {
        let response = self
            .inner
            .select_akash_provider(SelectAkashProviderRequest {
                session_id: session_id.to_string(),
                provider_address: provider_address.to_string(),
                bid_price_uakt,
            })
            .await
            .context("Failed to select Akash provider")?;

        Ok(response.into_inner())
    }

    /// Cancel an Akash deployment workflow
    pub async fn cancel_akash_deployment(
        &mut self,
        session_id: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .cancel_akash_deployment(CancelAkashDeploymentRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to cancel Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Set discovered endpoints for a deployment workflow
    pub async fn set_workflow_endpoints(
        &mut self,
        session_id: &str,
        endpoints: std::collections::HashMap<String, String>,
    ) -> Result<SetWorkflowEndpointsResponse> {
        let response = self
            .inner
            .set_workflow_endpoints(SetWorkflowEndpointsRequest {
                session_id: session_id.to_string(),
                endpoints,
            })
            .await
            .context("Failed to set workflow endpoints")?;

        Ok(response.into_inner())
    }

    /// Configure proxy routing dynamically
    pub async fn configure_proxy_routes(
        &mut self,
        openai_base_url: &str,
        anthropic_base_url: &str,
        ollama_base_url: &str,
        model_routes: std::collections::HashMap<String, String>,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .configure_proxy_routes(ConfigureProxyRoutesRequest {
                openai_base_url: openai_base_url.to_string(),
                anthropic_base_url: anthropic_base_url.to_string(),
                ollama_base_url: ollama_base_url.to_string(),
                model_routes,
            })
            .await
            .context("Failed to configure proxy routes")?;

        Ok(response.into_inner())
    }

    /// Request authz grant from coordinator
    pub async fn request_grant(
        &mut self,
        granter: &str,
        grantee: &str,
        msg_types: Vec<String>,
        allowance_amount: u64,
        reason: Option<&str>,
    ) -> Result<RequestGrantResponse> {
        let response = self
            .inner
            .request_grant(RequestGrantRequest {
                granter_address: granter.to_string(),
                grantee_address: grantee.to_string(),
                msg_types,
                allowance_amount,
                expiration: None,
                reason: reason.unwrap_or("").to_string(),
            })
            .await
            .context("Failed to request grant")?;

        Ok(response.into_inner())
    }

    /// Approve or reject pending grant request
    pub async fn approve_grant(
        &mut self,
        request_id: &str,
        approve: bool,
        reason: Option<&str>,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .approve_grant(ApproveGrantRequest {
                request_id: request_id.to_string(),
                approve,
                reason: reason.unwrap_or("").to_string(),
            })
            .await
            .context("Failed to approve grant")?;

        Ok(response.into_inner())
    }

    /// Revoke an existing grant
    pub async fn revoke_grant(
        &mut self,
        granter: &str,
        grantee: &str,
        msg_type: Option<&str>,
        revoke_feegrant: bool,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .revoke_grant(RevokeGrantRequest {
                granter_address: granter.to_string(),
                grantee_address: grantee.to_string(),
                msg_type: msg_type.unwrap_or("").to_string(),
                revoke_feegrant,
            })
            .await
            .context("Failed to revoke grant")?;

        Ok(response.into_inner())
    }

    /// List pending grant requests
    pub async fn list_grant_requests(
        &mut self,
        granter: Option<&str>,
        grantee: Option<&str>,
        status: Option<&str>,
    ) -> Result<ListGrantRequestsResponse> {
        let response = self
            .inner
            .list_grant_requests(ListGrantRequestsRequest {
                granter_address: granter.unwrap_or("").to_string(),
                grantee_address: grantee.unwrap_or("").to_string(),
                status: status.unwrap_or("").to_string(),
            })
            .await
            .context("Failed to list grant requests")?;

        Ok(response.into_inner())
    }

    /// Query account balance
    pub async fn query_balance(&mut self, address: &str, denom: &str) -> Result<QueryBalanceResponse> {
        let response = self
            .inner
            .query_balance(QueryBalanceRequest {
                address: address.to_string(),
                denom: denom.to_string(),
            })
            .await
            .context("Failed to query balance")?;

        Ok(response.into_inner())
    }

    // ============ SDL Template Management ============

    /// List deployed SDL template contracts
    pub async fn list_sdl_templates(&mut self) -> Result<ListSdlTemplatesResponse> {
        let response = self
            .inner
            .list_sdl_templates(ListSdlTemplatesRequest {})
            .await
            .context("Failed to list SDL templates")?;

        Ok(response.into_inner())
    }

    /// Get SDL template from contract
    pub async fn get_sdl_template(&mut self, contract_address: &str) -> Result<GetSdlTemplateResponse> {
        let response = self
            .inner
            .get_sdl_template(GetSdlTemplateRequest {
                contract_address: contract_address.to_string(),
            })
            .await
            .context("Failed to get SDL template")?;

        Ok(response.into_inner())
    }

    /// Get variable defaults from contract
    pub async fn get_sdl_defaults(&mut self, contract_address: &str) -> Result<GetSdlDefaultsResponse> {
        let response = self
            .inner
            .get_sdl_defaults(GetSdlDefaultsRequest {
                contract_address: contract_address.to_string(),
            })
            .await
            .context("Failed to get SDL defaults")?;

        Ok(response.into_inner())
    }

    /// Render SDL template with variables
    pub async fn render_sdl_template(
        &mut self,
        contract_address: &str,
        variables: std::collections::HashMap<String, String>,
    ) -> Result<RenderSdlTemplateResponse> {
        let response = self
            .inner
            .render_sdl_template(RenderSdlTemplateRequest {
                contract_address: contract_address.to_string(),
                variables,
            })
            .await
            .context("Failed to render SDL template")?;

        Ok(response.into_inner())
    }
}

/// Helper to format engine state for display
pub fn format_engine_state(state: i32) -> &'static str {
    match EngineState::try_from(state) {
        Ok(EngineState::Unspecified) => "unknown",
        Ok(EngineState::Starting) => "starting",
        Ok(EngineState::Running) => "running",
        Ok(EngineState::Degraded) => "degraded",
        Ok(EngineState::Stopping) => "stopping",
        Ok(EngineState::Stopped) => "stopped",
        Err(_) => "unknown",
    }
}

/// Helper to format uptime duration
pub fn format_uptime(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

// Re-export for commands module
pub use ho_std::types::ergors::network::v1::NodeType as NodeTypeProto;
