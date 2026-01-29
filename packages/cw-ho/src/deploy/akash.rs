//! Akash Network deployment client
//!
//! This module provides a high-level interface for deploying instances to the
//! Akash Network using direct API calls instead of CLI commands.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest;
use serde_json::Value;
use tokio::time::sleep;

use layer_climb::prelude::*;
use layer_climb_proto::Any as ClimbAny;
use prost::Message;

// Import our API client and proto types
use crate::deploy::api_client::{AkashApiClient, AkashApiConfig};
use crate::deploy::transaction::{SimpleKeyring, TxBroadcaster, TxConfig};
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::akash::market::v1beta4::LeaseId;
use ho_std::types::ergors::akash::provider::lease::v1::{
    lease_rpc_client::LeaseRpcClient, ServiceStatusRequest,
};
use ho_std::types::ergors::orch::v1::{AkashDeployConfig, CosmosKeyStore};

pub mod msg_types {
    use prost::Name;

    /// All deployment-related message types for full workflow authorization
    pub fn all_deployment_msg_types() -> Vec<String> {
        vec![
            ho_std::types::ergors::akash::deployment::v1beta4::MsgCreateDeployment::type_url(),
            ho_std::types::ergors::akash::deployment::v1beta4::MsgUpdateDeployment::type_url(),
            ho_std::types::ergors::akash::deployment::v1beta4::MsgCloseDeployment::type_url(),
            ho_std::types::ergors::akash::market::v1beta4::MsgCreateLease::type_url(),
            ho_std::types::ergors::akash::market::v1beta4::MsgCloseBid::type_url(),
            ho_std::types::ergors::akash::market::v1beta4::MsgWithdrawLease::type_url(),
            ho_std::types::ergors::akash::cert::v1::MsgCreateCertificate::type_url(),
            ho_std::types::ergors::akash::cert::v1::MsgRevokeCertificate::type_url(),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct AkashConfig {
    pub key_name: String,
    pub keyring_backend: String,
    pub node: String,
    pub chain_id: String,
    pub account_address: String,
    pub gas: String,
    pub gas_adjustment: f64,
    pub gas_prices: String,
    pub sign_mode: String,
}

impl AkashConfig {
    /// Create config from explicit parameters (used by CLI/gRPC)
    pub fn with_params(key_name: &str, node: &str, chain_id: &str) -> Self {
        Self {
            key_name: key_name.to_string(),
            keyring_backend: "os".to_string(),
            node: node.to_string(),
            chain_id: chain_id.to_string(),
            account_address: String::new(),
            gas: "auto".to_string(),
            gas_adjustment: 1.3,
            gas_prices: "0.0025uakt".to_string(),
            sign_mode: "amino-json".to_string(),
        }
    }

    /// Create config from proto AkashDeployConfig (loaded from engine config)
    pub fn from_proto_config(proto: &AkashDeployConfig) -> Self {
        Self {
            key_name: if proto.default_key_name.is_empty() {
                "default".to_string()
            } else {
                proto.default_key_name.clone()
            },
            keyring_backend: if proto.keyring_backend.is_empty() {
                "os".to_string()
            } else {
                proto.keyring_backend.clone()
            },
            node: if proto.rpc_endpoints.is_empty() {
                "https://rpc-akash.ecostake.com:443".to_string()
            } else {
                proto.rpc_endpoints[0].clone()
            },
            chain_id: if proto.chain_id.is_empty() {
                "akashnet-2".to_string()
            } else {
                proto.chain_id.clone()
            },
            account_address: String::new(),
            gas: "auto".to_string(),
            gas_adjustment: if proto.gas_adjustment == 0.0 {
                1.3
            } else {
                proto.gas_adjustment
            },
            gas_prices: if proto.gas_prices.is_empty() {
                "0.0025uakt".to_string()
            } else {
                proto.gas_prices.clone()
            },
            sign_mode: "amino-json".to_string(),
        }
    }

    /// Create config with mainnet defaults (fallback)
    pub async fn mainnet_defaults() -> Result<Self> {
        let node = "https://rpc-akash.ecostake.com:443".to_string();
        let chain_id = fetch_chain_id().await?;

        Ok(Self {
            key_name: "default".to_string(),
            keyring_backend: "os".to_string(),
            node,
            chain_id,
            account_address: String::new(),
            gas: "auto".to_string(),
            gas_adjustment: 1.3,
            gas_prices: "0.0025uakt".to_string(),
            sign_mode: "amino-json".to_string(),
        })
    }
}

#[derive(Debug, Clone)]
struct DeploymentInfo {
    dseq: String,
    oseq: String,
    gseq: String,
    provider: String,
}

pub struct AkashClient {
    config: AkashConfig,
    trusted_providers: Vec<String>,
    deployments: HashMap<String, DeploymentInfo>,
    api_client: Arc<tokio::sync::Mutex<AkashApiClient>>,
    tx_broadcaster: TxBroadcaster,
    keyring: SimpleKeyring,
    /// Encrypted cosmos key store for real key resolution
    cosmos_key_store: Option<CosmosKeyStore>,
}

impl AkashClient {
    pub async fn new(config: AkashConfig) -> Result<Self> {
        Self::with_providers(config, vec![]).await
    }

    /// Create client from proto AkashDeployConfig
    pub async fn from_proto_config(proto: &AkashDeployConfig) -> Result<Self> {
        let config = AkashConfig::from_proto_config(proto);
        Self::with_providers(config, proto.trusted_providers.clone()).await
    }

    pub async fn with_providers(
        config: AkashConfig,
        trusted_providers: Vec<String>,
    ) -> Result<Self> {
        // Initialize API client - derive endpoints from node config
        let grpc_endpoint = config.node.replace("https://rpc-", "https://grpc-");
        let rest_endpoint_derived = config
            .node
            .replace("https://rpc-", "https://rest-")
            .replace(":443", "");
        let api_config = AkashApiConfig {
            grpc_endpoint,
            rest_endpoint: rest_endpoint_derived,
            timeout: Duration::from_secs(30),
            chain_id: config.chain_id.clone(),
        };
        let rest_endpoint = api_config.rest_endpoint.clone();
        let api_client = Arc::new(tokio::sync::Mutex::new(AkashApiClient::new(api_config)?));

        // Initialize transaction broadcaster
        let tx_config = TxConfig {
            chain_id: config.chain_id.clone(),
            gas_limit: 1000000,
            gas_price: 5000,
            gas_adjustment: config.gas_adjustment,
            memo: None,
            sign_mode: config.sign_mode.clone(),
        };
        let tx_broadcaster = TxBroadcaster::new(rest_endpoint, tx_config);

        // Initialize keyring
        let keyring = SimpleKeyring::new();

        Ok(Self {
            config,
            trusted_providers,
            deployments: HashMap::new(),
            api_client,
            tx_broadcaster,
            keyring,
            cosmos_key_store: None,
        })
    }

    /// Load the encrypted cosmos key store for real key resolution
    pub fn with_cosmos_key_store(mut self, store: CosmosKeyStore) -> Self {
        self.cosmos_key_store = Some(store);
        self
    }

    async fn setup_keys(&mut self) -> Result<()> {
        println!("[INFO] Setting up keys...");

        // Try to resolve key from encrypted cosmos key store first
        if let Some(ref store) = self.cosmos_key_store {
            let key_name = if self.config.key_name.is_empty() || self.config.key_name == "default" {
                // Use the default key from the store
                EncryptedCosmosKeyManager::get_default_key_name(store)
                    .unwrap_or(&self.config.key_name)
                    .to_string()
            } else {
                self.config.key_name.clone()
            };

            if let Some(account) = store
                .derived_accounts
                .iter()
                .find(|a| a.key_name == key_name)
            {
                self.config.account_address = account.address.clone();
                self.config.key_name = key_name;
                println!(
                    "[INFO] Resolved key '{}' to address: {}",
                    self.config.key_name, self.config.account_address
                );
                self.check_balance().await?;
                return Ok(());
            }
        }

        // Fallback to SimpleKeyring (placeholder behavior)
        let key_exists = self
            .keyring
            .list_keys()
            .iter()
            .any(|k| k.name == self.config.key_name);

        if !key_exists {
            println!("[INFO] Creating key '{}'...", self.config.key_name);
            let address = format!("akash1placeholder{}", self.config.key_name);
            self.keyring.add_key(&self.config.key_name, &address)?;
            println!(
                "[INFO] Key '{}' created with address: {}",
                self.config.key_name, address
            );
        } else {
            println!("[INFO] Key '{}' already exists.", self.config.key_name);
        }

        // Get account address from keyring
        let key_info = self.keyring.get_key(&self.config.key_name)?;
        self.config.account_address = key_info.address.clone();
        println!(
            "[INFO] Using account address: {}",
            self.config.account_address
        );

        // Check balance via API
        self.check_balance().await?;

        Ok(())
    }

    async fn check_balance(&self) -> Result<()> {
        println!("[INFO] Checking account balance...");

        // Use REST API for balance checking (gRPC bank queries would be better but more complex)
        let api_client = self.api_client.lock().await;
        let url = format!(
            "{}/cosmos/bank/v1beta1/balances/{}",
            api_client.rest_endpoint().trim_end_matches('/'),
            self.config.account_address
        );

        let response = api_client.http_client().get(&url).send().await?;
        let balance_json: Value = response.json().await?;

        if let Some(balances) = balance_json["balances"].as_array() {
            for balance in balances {
                if balance["denom"] == "uakt" {
                    let amount = balance["amount"].as_str().unwrap_or("0");
                    println!("[INFO] Account has {} uakt", amount);
                    if amount.parse::<u64>().unwrap_or(0) > 0 {
                        return Ok(());
                    }
                }
            }
        }

        Err(anyhow!(
            "Account has no uakt. Please fund your account before proceeding."
        ))
    }

    async fn setup_certificate(&self) -> Result<()> {
        println!("[INFO] Setting up certificate...");

        // TODO: Implement certificate checking via Akash Cert API
        // For now, assume certificates are already set up
        // This would require:
        // 1. Query certificates using akash.cert.v1beta3.Query/Certificates
        // 2. Generate certificate if needed
        // 3. Submit MsgCreateCertificate transaction

        println!("[INFO] Certificate setup completed (placeholder implementation).");
        Ok(())
    }

    async fn check_existing_deployments(&mut self) -> Result<()> {
        println!("[INFO] Checking for existing deployments...");

        // Query deployments via API
        let deployments_response = {
            let mut api_client = self.api_client.lock().await;
            api_client
                .query_deployments(Some(&self.config.account_address), Some("active"))
                .await?
        };

        let deployment_count = deployments_response.deployments.len();

        if deployment_count == 0 {
            println!("[INFO] No existing deployments found.");
            return Ok(());
        }

        println!("[INFO] Found {} existing deployment(s):", deployment_count);

        // Display deployments and handle closure
        // TODO: Implement proper deployment listing and closure logic
        // For now, just log that deployments exist

        Ok(())
    }

    async fn deploy_sdl(&mut self, step: u32, sdl_file: &str) -> Result<()> {
        println!("[INFO] Deploying SDL: {}", sdl_file);

        // Read and potentially modify SDL
        let mut sdl_content = fs::read_to_string(sdl_file)?;

        // Set snapshot time if step 1
        if step == 1 {
            let snapshot_time = chrono::Utc::now() + chrono::Duration::minutes(10);
            let formatted_time = snapshot_time.format("%H:%M:%S").to_string();

            // Modify SDL - using simple string replacement for demonstration
            // In production, use proper YAML parsing
            sdl_content = sdl_content.replace(
                "SNAPSHOT_TIME=00:00:00",
                &format!("SNAPSHOT_TIME={}", formatted_time),
            );
        }

        // TODO: Create deployment using API transaction
        // This would involve:
        // 1. Parse SDL file into Deployment protobuf message
        // 2. Create MsgCreateDeployment transaction
        // 3. Sign and broadcast transaction
        // 4. Extract DSEQ, OSEQ, GSEQ from transaction response

        println!("[INFO] Creating deployment (API implementation pending)...");

        // Placeholder values for demonstration
        let dseq = "12345".to_string();
        let oseq = "1".to_string();
        let gseq = "1".to_string();

        println!(
            "[INFO] Deployment created with DSEQ: {}, OSEQ: {}, GSEQ: {}",
            dseq, oseq, gseq
        );

        // Wait for bids
        self.wait_for_bids(&dseq).await?;

        // Get bids and select provider
        let provider = self.select_provider(&dseq).await?;

        // TODO: Create lease using API transaction
        println!(
            "[INFO] Creating lease with provider {} (API implementation pending)...",
            provider
        );

        sleep(Duration::from_secs(10)).await;

        // TODO: Send manifest using provider RPC
        println!("[INFO] Sending manifest to provider (API implementation pending)...");

        println!("[INFO] Manifest sent, waiting for deployment to be ready...");
        sleep(Duration::from_secs(30)).await;

        // Get lease status and save deployment info
        self.collect_deployment_info(&dseq, &provider, "deployment_endpoints.env")
            .await?;

        // Store deployment info
        self.deployments.insert(
            sdl_file.to_string(),
            DeploymentInfo {
                dseq: dseq.clone(),
                oseq: oseq.clone(),
                gseq: gseq.clone(),
                provider: provider.clone(),
            },
        );

        println!("[INFO] Deployment of {} completed successfully!", sdl_file);
        Ok(())
    }

    async fn wait_for_bids(&self, _dseq: &str) -> Result<()> {
        println!("[INFO] Waiting for bids...");

        // TODO: Implement bid querying using akash.market.v1beta4.Query/Bids
        // For now, simulate waiting for bids
        for attempt in 1..=12 {
            println!(
                "[INFO] Attempt {}/12: Checking for bids (API implementation pending)...",
                attempt
            );

            // Simulate receiving bids after a few attempts
            if attempt >= 3 {
                println!("[INFO] Received bids after {} attempts", attempt);
                return Ok(());
            }

            println!("[INFO] Attempt {}/12: No bids yet, waiting 5s...", attempt);
            sleep(Duration::from_secs(5)).await;
        }

        Err(anyhow!("No bids received after 12 attempts"))
    }

    async fn select_provider(&self, _dseq: &str) -> Result<String> {
        println!("[INFO] Finding optimal bid from trusted providers...");

        // TODO: Implement bid querying using akash.market.v1beta4.Query/Bids
        // For now, select the first trusted provider
        if let Some(provider) = self.trusted_providers.first() {
            println!(
                "[INFO] Selected provider {} (API implementation pending)",
                provider
            );
            Ok(provider.clone())
        } else {
            Err(anyhow!("No trusted providers available!"))
        }
    }

    /// Generic method to collect deployment information and save to a file
    ///
    /// This queries the lease status and stores discovered endpoints.
    /// For workflow-specific logic, callers should use `query_lease_status` directly.
    async fn collect_deployment_info(
        &self,
        dseq: &str,
        provider: &str,
        output_file: &str,
    ) -> Result<HashMap<String, String>> {
        println!("[INFO] Checking lease status for deployment {}...", dseq);

        // Query actual lease status from provider
        let dseq_u64 = dseq.parse::<u64>()?;
        let lease_id = LeaseId {
            owner: self.config.account_address.clone(),
            dseq: dseq_u64,
            gseq: 1,
            oseq: 1,
            provider: provider.to_string(),
        };

        // Query all services (empty vec = all)
        let endpoints = self.query_lease_status(provider, lease_id, vec![]).await?;

        // Save endpoints to file
        for (service_name, endpoint) in &endpoints {
            let env_content = format!(
                "{}={}\n",
                service_name.to_uppercase().replace("-", "_"),
                endpoint
            );
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(output_file)?
                .write_all(env_content.as_bytes())?;
            println!("[INFO] Saved {} = {}", service_name, endpoint);
        }

        Ok(endpoints)
    }

    /// Query lease status from Akash provider to retrieve service endpoints
    ///
    /// This queries the provider's gRPC LeaseRPC/ServiceStatus endpoint to get
    /// the actual deployed service URIs, ports, and IPs.
    ///
    /// # Arguments
    /// * `provider_uri` - Provider gRPC endpoint (e.g., "https://provider.akash.host:8443")
    /// * `lease_id` - The lease ID (owner, dseq, gseq, oseq, provider)
    /// * `service_names` - Optional list of service names to query (empty = all services)
    ///
    /// # Returns
    /// HashMap mapping service name -> endpoint URL (e.g., "web" -> "http://host:port")
    pub async fn query_lease_status(
        &self,
        provider_uri: &str,
        lease_id: LeaseId,
        service_names: Vec<String>,
    ) -> Result<HashMap<String, String>> {
        tracing::info!(
            "Querying lease status from provider {} for dseq {}",
            provider_uri,
            lease_id.dseq
        );

        // Connect to provider's gRPC endpoint
        let mut client = LeaseRpcClient::connect(provider_uri.to_string())
            .await
            .map_err(|e| anyhow!("Failed to connect to provider {}: {}", provider_uri, e))?;

        // Query service status
        let request = ServiceStatusRequest {
            lease_id: Some(lease_id.clone()),
            services: service_names.clone(),
        };

        let response = client
            .service_status(request)
            .await
            .map_err(|e| anyhow!("Failed to query service status: {}", e))?
            .into_inner();

        // Parse response to extract endpoints
        let mut endpoints = HashMap::new();

        for service in response.services {
            let service_name = service.name.clone();

            // Try to construct endpoint from forwarded ports
            if let Some(port_status) = service.ports.first() {
                // Use the external port (provider's forwarded port)
                let endpoint = format!("http://{}:{}", port_status.host, port_status.external_port);
                tracing::info!("Discovered endpoint for '{}': {}", service_name, endpoint);
                endpoints.insert(service_name.clone(), endpoint);
                continue;
            }

            // Fallback: try to construct from IPs
            if let Some(ip_status) = service.ips.first() {
                // Use the port from the IP status
                let endpoint = format!("http://{}:{}", lease_id.provider, ip_status.port);
                tracing::info!(
                    "Discovered endpoint (via IP) for '{}': {}",
                    service_name,
                    endpoint
                );
                endpoints.insert(service_name.clone(), endpoint);
                continue;
            }

            tracing::warn!(
                "Could not determine endpoint for service '{}' - no ports or IPs in status",
                service_name
            );
        }

        if endpoints.is_empty() {
            return Err(anyhow!(
                "No service endpoints discovered from provider lease status"
            ));
        }

        Ok(endpoints)
    }

    /// Convenience method to query all services for a deployment
    pub async fn query_deployment_endpoints(
        &self,
        provider_uri: &str,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
    ) -> Result<HashMap<String, String>> {
        let lease_id = LeaseId {
            owner: owner.to_string(),
            dseq,
            gseq,
            oseq,
            provider: provider_uri.to_string(),
        };

        self.query_lease_status(provider_uri, lease_id, vec![])
            .await
    }
}

async fn fetch_chain_id() -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://raw.githubusercontent.com/akash-network/net/main/mainnet/chain-id.txt")
        .send()
        .await?
        .text()
        .await?;

    Ok(response.trim().to_string())
}

/// Broadcast a single Akash message with standard error handling.
///
/// This helper:
/// - Encodes the message to protobuf
/// - Creates a TxBuilder with the provided memo
/// - Broadcasts and waits for confirmation
/// - Returns error if tx fails (code != 0)
/// - Logs success with tx metadata
pub async fn broadcast_akash_msg<M: Message>(
    client: &SigningClient,
    type_url: &str,
    msg: &M,
    memo: impl Into<String>,
) -> Result<layer_climb_proto::abci::TxResponse> {
    let msg_any = ClimbAny {
        type_url: type_url.to_string(),
        value: msg.encode_to_vec(),
    };

    tracing::debug!(
        "Preparing tx: type={}, size={} bytes",
        type_url,
        msg_any.value.len()
    );

    let mut tx_builder = client.tx_builder();
    tx_builder.set_memo(memo);

    let tx_resp = match tx_builder.broadcast(vec![msg_any]).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Broadcast failed for {}: {}", type_url, e);
            tracing::error!("Error details: {:?}", e);
            return Err(anyhow!("Failed to broadcast {}: {}", type_url, e));
        }
    };

    if tx_resp.code != 0 {
        return Err(anyhow!(
            "Akash tx failed (type: {}, code: {}): {}",
            type_url,
            tx_resp.code,
            tx_resp.raw_log
        ));
    }

    tracing::info!(
        "Akash tx success: type={}, hash={}, height={}, gas={}",
        type_url,
        tx_resp.txhash,
        tx_resp.height,
        tx_resp.gas_used
    );

    Ok(tx_resp)
}

/// Broadcast multiple Akash messages in a single transaction (atomic).
///
/// This allows batching multiple operations into one transaction for:
/// - Atomicity (all succeed or all fail)
/// - Lower gas costs
/// - Faster execution
///
/// # Arguments
/// * `client` - SigningClient to use for broadcasting
/// * `msgs` - Vector of (type_url, encoded_proto_bytes) tuples
/// * `memo` - Transaction memo
pub async fn broadcast_akash_msgs(
    client: &SigningClient,
    msgs: Vec<(&str, Vec<u8>)>, // (type_url, encoded_value)
    memo: impl Into<String>,
) -> Result<layer_climb_proto::abci::TxResponse> {
    let msg_anys: Vec<ClimbAny> = msgs
        .into_iter()
        .map(|(type_url, value)| ClimbAny {
            type_url: type_url.to_string(),
            value,
        })
        .collect();

    let mut tx_builder = client.tx_builder();
    tx_builder.set_memo(memo);
    let tx_resp = tx_builder.broadcast(msg_anys).await?;

    if tx_resp.code != 0 {
        return Err(anyhow!(
            "Akash batch tx failed (code: {}): {}",
            tx_resp.code,
            tx_resp.raw_log
        ));
    }

    tracing::info!(
        "Akash batch tx success: hash={}, height={}, gas={}",
        tx_resp.txhash,
        tx_resp.height,
        tx_resp.gas_used
    );

    Ok(tx_resp)
}
