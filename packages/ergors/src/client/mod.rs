//! gRPC client for engine management
//!
//! Provides a wrapper around the tonic-generated client.

use anyhow::{Context, Result};
use commonware_codec::Encode;
use commonware_cryptography::{blake3::Blake3, Hasher, Signer};
use ho_std::keys::commonware::NodePrivKey;
use ho_std::types::ergors::management::v1::{
    management_service_client::ManagementServiceClient as ProtoClient,
    AddDiscordAllowedGuildRequest,
    // Akash deployment types
    ApproveGrantRequest,
    CancelAkashDeploymentRequest,
    ConfigData,
    ConfigUpdate,
    ConfigureDiscordGatewayRequest,
    ConfigureProxyRoutesRequest,
    CosmosKeyInfo,
    CreateAkashDeploymentRequest,
    CreateAkashDeploymentResponse,
    DeleteCosmosKeyRequest,
    DisableGatewayRequest,
    Empty,
    EnableGatewayRequest,
    EngineState,
    EngineStatus,
    GatewayStatusResponse,
    GetAkashDeploymentRequest,
    GetAkashDeploymentResponse,
    GetDiscordConfigRequest,
    GetDiscordConfigResponse,
    GetGatewayStatusRequest,
    // Key address query types
    GetKeyAddressRequest,
    GetKeyAddressResponse,
    GetSdlDefaultsRequest,
    GetSdlDefaultsResponse,
    GetSdlTemplateRequest,
    GetSdlTemplateResponse,
    ImportCosmosKeyRequest,
    ImportCosmosKeyResponse,
    ListAkashDeploymentsRequest,
    ListAkashDeploymentsResponse,
    // Gateway types
    ListGatewaysRequest,
    ListGatewaysResponse,
    ListGrantRequestsRequest,
    ListGrantRequestsResponse,
    // SDL template types
    ListSdlTemplatesRequest,
    ListSdlTemplatesResponse,
    NodeIdRequest,
    NodeTypeRequest,
    OperationResult,
    PeerAddress,
    AssignProviderRoleRequest,
    UnassignProviderRoleRequest,
    ProviderConfig,
    ProviderList,
    ProviderName,
    ProviderTestResult,
    RemoveProviderRequest,
    QueryAkashBidsRequest,
    QueryAkashBidsResponse,
    QueryBalanceRequest,
    QueryBalanceResponse,
    RemoveDiscordAllowedGuildRequest,
    RenderSdlTemplateRequest,
    RenderSdlTemplateResponse,
    RequestGrantRequest,
    RequestGrantResponse,
    RevokeGrantRequest,
    SelectAkashProviderRequest,
    SelectAkashProviderResponse,
    SetDefaultCosmosKeyRequest,
    SetWorkflowEndpointsRequest,
    SetWorkflowEndpointsResponse,
    ShutdownRequest,
    TokenIdRequest,
    TokenLabel,
    TokenList,
    TokenResponse,
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
};
use ho_std::types::ergors::network::v1::{NetworkTopology, NodeIdentity, NodeType};
use ho_std::types::ergors::orch::v1::{
    EngineRole,
    EngineRoleConfig,
    AddTrustedProviderRequest,
    AkashWorkflowOptions,
    CloseAkashDeploymentRequest,
    CloseAkashLeaseRequest,
    // Certificate management types
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
    RagStatusRequest,
    RagStatusResponse,
    // RLM types
    RlmConfigureRequest,
    RlmQueryRequest,
    RlmQueryResponse,
    RemoveTrustedProviderRequest,
    RevokeAkashCertificateRequest,
    // Automated workflow types
    RunAkashDeploymentRequest,
    RunAkashDeploymentResponse,
    TopupAkashEscrowRequest,
    UpdateAkashDeploymentRequest,
};
use tonic::transport::Channel;
use tonic::service::interceptor::InterceptedService;

/// Client-side interceptor that signs gRPC requests with an Ed25519 key.
/// When `signing_key` is `None`, no auth headers are added (local connections).
#[derive(Clone)]
pub struct ClientAuthInterceptor {
    signing_key: Option<NodePrivKey>,
}

impl tonic::service::Interceptor for ClientAuthInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(key) = &self.signing_key {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string();

            let message = Blake3::hash(timestamp.as_bytes());
            let signature = key.sign(None, &message);
            let pubkey_hex = hex::encode(key.id().0.encode());
            let sig_hex = hex::encode(signature.encode());

            req.metadata_mut()
                .insert("x-timestamp", timestamp.parse().unwrap());
            req.metadata_mut()
                .insert("x-signature", sig_hex.parse().unwrap());
            req.metadata_mut()
                .insert("x-public-key", pubkey_hex.parse().unwrap());
        }
        Ok(req)
    }
}

/// Management client wrapping the generated tonic client with auth interceptor
pub struct ManagementClient {
    inner: ProtoClient<InterceptedService<Channel, ClientAuthInterceptor>>,
}

impl ManagementClient {
    /// Connect to the engine gRPC server with optional signing key for remote auth
    pub async fn connect(addr: &str, signing_key: Option<NodePrivKey>) -> Result<Self> {
        tracing::debug!("Configuring gRPC client with 100MB message/window limits");

        // Configure endpoint with larger initial window sizes for HTTP/2
        let endpoint = Channel::from_shared(addr.to_string())
            .context("Invalid gRPC address")?
            .initial_stream_window_size(100 * 1024 * 1024) // 100MB
            .initial_connection_window_size(100 * 1024 * 1024); // 100MB

        let channel = endpoint
            .connect()
            .await
            .context("Failed to connect to engine. Is it running?")?;

        let interceptor = ClientAuthInterceptor { signing_key };
        let inner = ProtoClient::with_interceptor(channel, interceptor)
            .max_decoding_message_size(100 * 1024 * 1024) // 100MB limit
            .max_encoding_message_size(100 * 1024 * 1024); // 100MB limit
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

    /// Get cosmos bech32 address for a stored key
    pub async fn get_key_address(
        &mut self,
        key_name: &str,
        address_prefix: &str,
        coin_type: u32,
        account_index: u32,
    ) -> Result<GetKeyAddressResponse> {
        let response = self
            .inner
            .get_key_address(GetKeyAddressRequest {
                key_name: key_name.to_string(),
                address_prefix: address_prefix.to_string(),
                coin_type,
                account_index,
            })
            .await
            .context("Failed to get key address")?;

        Ok(response.into_inner())
    }

    // ============ Cosmos Key Management ============

    /// List all stored cosmos keys
    pub async fn list_cosmos_keys(&mut self) -> Result<Vec<CosmosKeyInfo>> {
        let response = self
            .inner
            .list_cosmos_keys(Empty {})
            .await
            .context("Failed to list cosmos keys")?;

        Ok(response.into_inner().keys)
    }

    /// Import a cosmos key from mnemonic
    pub async fn import_cosmos_key(
        &mut self,
        mnemonic: &str,
        label: &str,
        key_name: &str,
        chain_id: &str,
        address_prefix: &str,
        make_default: bool,
        password: &str,
    ) -> Result<ImportCosmosKeyResponse> {
        let response = self
            .inner
            .import_cosmos_key(ImportCosmosKeyRequest {
                mnemonic: mnemonic.to_string(),
                label: label.to_string(),
                key_name: key_name.to_string(),
                chain_id: chain_id.to_string(),
                address_prefix: address_prefix.to_string(),
                make_default,
                password: password.to_string(),
            })
            .await
            .context("Failed to import cosmos key")?;

        Ok(response.into_inner())
    }

    /// Delete a cosmos key
    pub async fn delete_cosmos_key(&mut self, key_name: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .delete_cosmos_key(DeleteCosmosKeyRequest {
                key_name: key_name.to_string(),
            })
            .await
            .context("Failed to delete cosmos key")?;

        Ok(response.into_inner())
    }

    /// Set default cosmos key
    pub async fn set_default_cosmos_key(&mut self, key_name: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .set_default_cosmos_key(SetDefaultCosmosKeyRequest {
                key_name: key_name.to_string(),
            })
            .await
            .context("Failed to set default cosmos key")?;

        Ok(response.into_inner())
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
        base_url: Option<&str>,
        set_as_default: bool,
        no_key: bool,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .configure_provider(ProviderConfig {
                name: name.to_string(),
                api_key_ref: api_key.to_string(),
                set_as_default,
                base_url: base_url.unwrap_or("").to_string(),
                no_key,
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

    /// Remove a provider (requires custody password)
    pub async fn remove_provider(
        &mut self,
        name: &str,
        custody_password: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .remove_provider(RemoveProviderRequest {
                name: name.to_string(),
                custody_password: custody_password.to_string(),
            })
            .await
            .context("Failed to remove provider")?;

        Ok(response.into_inner())
    }

    // ============ Provider Role Assignments ============

    /// Assign a provider to an engine role
    pub async fn assign_provider_role(
        &mut self,
        provider_name: &str,
        role: EngineRole,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .assign_provider_role(AssignProviderRoleRequest {
                provider_name: provider_name.to_string(),
                role: role as i32,
            })
            .await
            .context("Failed to assign provider role")?;

        Ok(response.into_inner())
    }

    /// Unassign a provider from an engine role
    pub async fn unassign_provider_role(
        &mut self,
        provider_name: &str,
        role: EngineRole,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .unassign_provider_role(UnassignProviderRoleRequest {
                provider_name: provider_name.to_string(),
                role: role as i32,
            })
            .await
            .context("Failed to unassign provider role")?;

        Ok(response.into_inner())
    }

    /// List all provider role assignments
    pub async fn list_provider_roles(&mut self) -> Result<EngineRoleConfig> {
        let response = self
            .inner
            .list_provider_roles(Empty {})
            .await
            .context("Failed to list provider roles")?;

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
        label: &str,
        model_name: &str,
        model_map: &[(String, String)],
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
                label: label.to_string(),
                model_name: model_name.to_string(),
                model_map: model_map.iter().cloned().collect(),
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

    /// Query bids for a deployment
    pub async fn query_akash_bids(&mut self, session_id: &str) -> Result<QueryAkashBidsResponse> {
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
    pub async fn cancel_akash_deployment(&mut self, session_id: &str) -> Result<OperationResult> {
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
        model_routes: std::collections::HashMap<String, String>,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .configure_proxy_routes(ConfigureProxyRoutesRequest { model_routes })
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
    pub async fn query_balance(
        &mut self,
        address: &str,
        denom: &str,
    ) -> Result<QueryBalanceResponse> {
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
    pub async fn get_sdl_template(
        &mut self,
        contract_address: &str,
    ) -> Result<GetSdlTemplateResponse> {
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
    pub async fn get_sdl_defaults(
        &mut self,
        contract_address: &str,
    ) -> Result<GetSdlDefaultsResponse> {
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

    // ============ Automated Workflow Methods ============

    /// Run automated deployment workflow
    pub async fn run_akash_deployment(
        &mut self,
        session_id: &str,
        interactive_bid: bool,
        min_balance_uakt: u64,
        trusted_providers: Vec<String>,
        request_grant_from: &str,
        grant_duration_seconds: u64,
        grant_spend_limit_uakt: u64,
        key_password: &str,
    ) -> Result<RunAkashDeploymentResponse> {
        let response = self
            .inner
            .run_akash_deployment(RunAkashDeploymentRequest {
                session_id: session_id.to_string(),
                options: Some(AkashWorkflowOptions {
                    min_balance_uakt,
                    bid_wait_blocks: 2,
                    trusted_providers,
                    max_retries: 3,
                    interactive_bid,
                    request_grant_from: request_grant_from.to_string(),
                    grant_duration_seconds,
                    grant_spend_limit_uakt,
                }),
                key_password: key_password.to_string(),
            })
            .await
            .context("Failed to run Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Close an active lease
    pub async fn close_akash_lease(&mut self, session_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .close_akash_lease(CloseAkashLeaseRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to close Akash lease")?;

        Ok(response.into_inner())
    }

    /// Close a deployment (also closes any active leases)
    pub async fn close_akash_deployment(&mut self, session_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .close_akash_deployment(CloseAkashDeploymentRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to close Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Update a deployment with new SDL
    pub async fn update_akash_deployment(
        &mut self,
        session_id: &str,
        sdl_content: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .update_akash_deployment(UpdateAkashDeploymentRequest {
                session_id: session_id.to_string(),
                sdl_content: sdl_content.to_string(),
            })
            .await
            .context("Failed to update Akash deployment")?;

        Ok(response.into_inner())
    }

    /// Top up escrow account for a deployment
    pub async fn topup_akash_escrow(
        &mut self,
        session_id: &str,
        amount_uakt: u64,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .topup_akash_escrow(TopupAkashEscrowRequest {
                session_id: session_id.to_string(),
                amount_uakt,
            })
            .await
            .context("Failed to top up Akash escrow")?;

        Ok(response.into_inner())
    }

    /// Get lease status
    pub async fn get_lease_status(&mut self, session_id: &str) -> Result<LeaseStatusResponse> {
        let response = self
            .inner
            .get_lease_status(GetLeaseStatusRequest {
                session_id: session_id.to_string(),
            })
            .await
            .context("Failed to get lease status")?;

        Ok(response.into_inner())
    }

    /// Register deployment service endpoints as LLM providers
    pub async fn register_deployment_providers(
        &mut self,
        session_id: &str,
        label_prefix: Option<&str>,
    ) -> Result<RegisterDeploymentProvidersResponse> {
        let response = self
            .inner
            .register_deployment_providers(RegisterDeploymentProvidersRequest {
                session_id: session_id.to_string(),
                label_prefix: label_prefix.unwrap_or_default().to_string(),
            })
            .await
            .context("Failed to register deployment providers")?;

        Ok(response.into_inner())
    }

    /// Add trusted provider
    pub async fn add_trusted_provider(
        &mut self,
        address: &str,
        label: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .add_trusted_provider(AddTrustedProviderRequest {
                address: address.to_string(),
                label: label.to_string(),
            })
            .await
            .context("Failed to add trusted provider")?;

        Ok(response.into_inner())
    }

    /// Remove trusted provider
    pub async fn remove_trusted_provider(&mut self, address: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .remove_trusted_provider(RemoveTrustedProviderRequest {
                address: address.to_string(),
            })
            .await
            .context("Failed to remove trusted provider")?;

        Ok(response.into_inner())
    }

    /// List trusted providers
    pub async fn list_trusted_providers(&mut self) -> Result<ListTrustedProvidersResponse> {
        let response = self
            .inner
            .list_trusted_providers(ListTrustedProvidersRequest {})
            .await
            .context("Failed to list trusted providers")?;

        Ok(response.into_inner())
    }

    // ============ Akash Certificate Management Methods ============

    /// Create a new Akash mTLS certificate
    pub async fn create_akash_certificate(
        &mut self,
        key_name: &str,
        account_index: u32,
    ) -> Result<CreateAkashCertificateResponse> {
        let response = self
            .inner
            .create_akash_certificate(CreateAkashCertificateRequest {
                key_name: key_name.to_string(),
                account_index,
            })
            .await
            .context("Failed to create certificate")?;

        Ok(response.into_inner())
    }

    /// Revoke an Akash certificate
    pub async fn revoke_akash_certificate(
        &mut self,
        key_name: &str,
        account_index: u32,
        serial: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .revoke_akash_certificate(RevokeAkashCertificateRequest {
                key_name: key_name.to_string(),
                account_index,
                serial: serial.to_string(),
            })
            .await
            .context("Failed to revoke certificate")?;

        Ok(response.into_inner())
    }

    /// List certificates for an address
    pub async fn list_akash_certificates(
        &mut self,
        key_name: &str,
        account_index: u32,
        address: &str,
    ) -> Result<ListAkashCertificatesResponse> {
        let response = self
            .inner
            .list_akash_certificates(ListAkashCertificatesRequest {
                key_name: key_name.to_string(),
                account_index,
                address: address.to_string(),
            })
            .await
            .context("Failed to list certificates")?;

        Ok(response.into_inner())
    }

    // ============ RAG Vector Database Methods ============

    /// Ingest document into vector database
    pub async fn rag_ingest(
        &mut self,
        content: &str,
        uri: &str,
        doc_type: &str,
        tags: Vec<String>,
        skip_embeddings: bool,
    ) -> Result<RagIngestResponse> {
        let response = self
            .inner
            .rag_ingest(RagIngestRequest {
                content: content.to_string(),
                uri: uri.to_string(),
                doc_type: doc_type.to_string(),
                tags,
                skip_embeddings,
            })
            .await
            .context("Failed to ingest document")?;

        Ok(response.into_inner())
    }

    /// Query vector database
    pub async fn rag_query(
        &mut self,
        query: &str,
        top_k: usize,
        verify: bool,
    ) -> Result<RagQueryResponse> {
        let response = self
            .inner
            .rag_query(RagQueryRequest {
                query: query.to_string(),
                top_k: top_k as u32,
                verify,
            })
            .await
            .context("Failed to query vector database")?;

        Ok(response.into_inner())
    }

    /// Get RAG status
    pub async fn rag_status(&mut self) -> Result<RagStatusResponse> {
        let response = self
            .inner
            .rag_status(RagStatusRequest {})
            .await
            .context("Failed to get RAG status")?;

        Ok(response.into_inner())
    }

    /// Delete chunks by source URI
    pub async fn rag_delete(&mut self, source_uri: &str) -> Result<RagOperationResult> {
        let response = self
            .inner
            .rag_delete(RagDeleteRequest {
                source_uri: source_uri.to_string(),
            })
            .await
            .context("Failed to delete chunks")?;

        Ok(response.into_inner())
    }

    /// List ingested sources
    pub async fn rag_list_sources(&mut self, limit: u32) -> Result<RagListSourcesResponse> {
        let response = self
            .inner
            .rag_list_sources(RagListSourcesRequest { limit })
            .await
            .context("Failed to list sources")?;

        Ok(response.into_inner())
    }

    /// Configure embedder endpoint
    pub async fn rag_configure(
        &mut self,
        endpoint: &str,
        model: &str,
        dimension: u32,
    ) -> Result<RagOperationResult> {
        let response = self
            .inner
            .rag_configure(RagConfigureRequest {
                endpoint: endpoint.to_string(),
                model: model.to_string(),
                dimension,
            })
            .await
            .context("Failed to configure embedder")?;

        Ok(response.into_inner())
    }

    // ============ RLM (Recursive Language Model) ============

    /// Execute RLM query with agentic code execution
    pub async fn rlm_query(
        &mut self,
        query: &str,
        source_prefix: &str,
        limit: usize,
    ) -> Result<RlmQueryResponse> {
        let response = self
            .inner
            .rlm_query(RlmQueryRequest {
                query: query.to_string(),
                source_uri_prefix: source_prefix.to_string(),
                limit: limit as u32,
                guild_id: String::new(), // Not used for CLI
                max_iterations: 0, // Use server defaults
                max_sub_calls: 0,
                allowed_models: vec![],
            })
            .await
            .context("Failed to execute RLM query")?;

        Ok(response.into_inner())
    }

    /// Configure RLM provider selection
    pub async fn rlm_configure(
        &mut self,
        primary: &str,
        secondary: Option<&str>,
        max_iterations: Option<u32>,
        max_sub_calls: Option<u32>,
    ) -> Result<RagOperationResult> {
        let response = self
            .inner
            .rlm_configure(RlmConfigureRequest {
                primary_provider_label: primary.to_string(),
                secondary_provider_label: secondary.unwrap_or_default().to_string(),
                max_iterations,
                max_sub_calls,
            })
            .await
            .context("Failed to configure RLM")?;

        Ok(response.into_inner())
    }

    /// Get current RLM configuration
    pub async fn rlm_get_config(&mut self) -> Result<RlmGetConfigResponse> {
        let response = self
            .inner
            .rlm_get_config(RlmGetConfigRequest {})
            .await
            .context("Failed to get RLM config")?;

        Ok(response.into_inner())
    }

    // ============ Document Storage (Non-RAG) ============

    /// Ingest a document into storage
    pub async fn ingest_document(
        &mut self,
        content: Vec<u8>,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<String> {
        use ho_std::types::ergors::orch::v1::IngestDocumentRequest;

        let response = self
            .inner
            .ingest_document(IngestDocumentRequest {
                content,
                name: name.into(),
                source: source.into(),
            })
            .await
            .context("Failed to ingest document")?;

        Ok(response.into_inner().document_id)
    }

    /// Retrieve a document by ID
    pub async fn retrieve_document(
        &mut self,
        document_id: &str,
    ) -> Result<(Vec<u8>, ho_std::document::DocumentMetadata)> {
        use ho_std::types::ergors::orch::v1::RetrieveDocumentRequest;

        let response = self
            .inner
            .retrieve_document(RetrieveDocumentRequest {
                document_id: document_id.to_string(),
            })
            .await
            .context("Mgmt: Failed to retrieve document")?;

        let inner = response.into_inner();

        // Deserialize metadata
        let metadata: ho_std::document::DocumentMetadata =
            serde_json::from_slice(&inner.metadata_json)
                .context("Failed to deserialize metadata")?;

        Ok((inner.content, metadata))
    }

    /// List all documents with pagination
    pub async fn list_documents(
        &mut self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<(ho_std::document::DocumentId, ho_std::document::DocumentMetadata)>> {
        use ho_std::types::ergors::orch::v1::ListDocumentsRequest;

        let response = self
            .inner
            .list_documents(ListDocumentsRequest {
                limit: limit.map(|l| l as u32),
                offset: offset.map(|o| o as u32),
            })
            .await
            .context("Failed to list documents")?;

        let mut documents = Vec::new();
        for doc in response.into_inner().documents {
            let doc_id = ho_std::document::DocumentId::from_hex(doc.document_id)
                .context("Invalid document ID in response")?;
            let metadata: ho_std::document::DocumentMetadata =
                serde_json::from_slice(&doc.metadata_json)
                    .context("Failed to deserialize metadata")?;
            documents.push((doc_id, metadata));
        }

        Ok(documents)
    }

    /// Delete a document by ID
    pub async fn delete_document(&mut self, document_id: &str) -> Result<()> {
        use ho_std::types::ergors::orch::v1::DeleteDocumentRequest;

        self.inner
            .delete_document(DeleteDocumentRequest {
                document_id: document_id.to_string(),
            })
            .await
            .context("Failed to delete document")?;

        Ok(())
    }

    // ============ Gateway Management ============

    /// List all registered gateways
    pub async fn list_gateways(&mut self) -> Result<ListGatewaysResponse> {
        let response = self
            .inner
            .list_gateways(ListGatewaysRequest {})
            .await
            .context("Failed to list gateways")?;

        Ok(response.into_inner())
    }

    /// Get gateway status
    pub async fn get_gateway_status(&mut self, gateway_id: &str) -> Result<GatewayStatusResponse> {
        let response = self
            .inner
            .get_gateway_status(GetGatewayStatusRequest {
                gateway_id: gateway_id.to_string(),
            })
            .await
            .context("Failed to get gateway status")?;

        Ok(response.into_inner())
    }

    /// Enable a gateway
    pub async fn enable_gateway(&mut self, gateway_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .enable_gateway(EnableGatewayRequest {
                gateway_id: gateway_id.to_string(),
            })
            .await
            .context("Failed to enable gateway")?;

        Ok(response.into_inner())
    }

    /// Disable a gateway
    pub async fn disable_gateway(&mut self, gateway_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .disable_gateway(DisableGatewayRequest {
                gateway_id: gateway_id.to_string(),
            })
            .await
            .context("Failed to disable gateway")?;

        Ok(response.into_inner())
    }

    /// Configure Discord gateway
    pub async fn configure_discord_gateway(
        &mut self,
        bot_token: &str,
        respond_to_mentions: Option<bool>,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .configure_discord_gateway(ConfigureDiscordGatewayRequest {
                bot_token: bot_token.to_string(),
                respond_to_mentions,
            })
            .await
            .context("Failed to configure Discord gateway")?;

        Ok(response.into_inner())
    }

    /// Add Discord allowed guild
    pub async fn add_discord_allowed_guild(&mut self, guild_id: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .add_discord_allowed_guild(AddDiscordAllowedGuildRequest {
                guild_id: guild_id.to_string(),
            })
            .await
            .context("Failed to add Discord allowed guild")?;

        Ok(response.into_inner())
    }

    /// Remove Discord allowed guild
    pub async fn remove_discord_allowed_guild(
        &mut self,
        guild_id: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .remove_discord_allowed_guild(RemoveDiscordAllowedGuildRequest {
                guild_id: guild_id.to_string(),
            })
            .await
            .context("Failed to remove Discord allowed guild")?;

        Ok(response.into_inner())
    }

    /// Get Discord configuration (token redacted)
    pub async fn get_discord_config(&mut self) -> Result<GetDiscordConfigResponse> {
        let response = self
            .inner
            .get_discord_config(GetDiscordConfigRequest {})
            .await
            .context("Failed to get Discord config")?;

        Ok(response.into_inner())
    }

    // ============ CLI Key Management ============

    pub async fn register_cli_key(
        &mut self,
        pubkey_hex: &str,
        label: &str,
    ) -> Result<OperationResult> {
        let response = self
            .inner
            .register_cli_key(RegisterCliKeyRequest {
                public_key_hex: pubkey_hex.to_string(),
                label: label.to_string(),
            })
            .await
            .context("Failed to register CLI key")?;

        Ok(response.into_inner())
    }

    pub async fn revoke_cli_key(&mut self, pubkey_hex: &str) -> Result<OperationResult> {
        let response = self
            .inner
            .revoke_cli_key(RevokeCliKeyRequest {
                public_key_hex: pubkey_hex.to_string(),
            })
            .await
            .context("Failed to revoke CLI key")?;

        Ok(response.into_inner())
    }

    pub async fn list_cli_keys(&mut self) -> Result<ListCliKeysResponse> {
        let response = self
            .inner
            .list_cli_keys(ListCliKeysRequest {})
            .await
            .context("Failed to list CLI keys")?;

        Ok(response.into_inner())
    }

    /// Create a mock client for testing (panics if called)
    #[cfg(test)]
    pub fn mock() -> Self {
        // This is a placeholder for tests that don't actually call client methods
        // If a test tries to use this client, it will panic
        panic!("Mock client called - this should not be used in actual test execution");
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

pub mod grpc;
pub mod rlm;
pub mod sentinel;

// Re-export key types and functions
pub use crate::auth::grpc::{create_grpc_auth_interceptor, AuthorizedCliKeys};
pub use grpc::{start_grpc_server, ManagementServiceImpl};
pub use rlm::{load_documents_by_prefix, RlmDocService};
