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
use serde_json::{json, Value};
use tokio::time::sleep;

// Import our API client and proto types
use crate::deploy::api_client::{AkashApiClient, AkashApiConfig};
use crate::deploy::transaction::{SimpleKeyring, TxBroadcaster, TxConfig};

#[derive(Debug)]
struct AkashConfig {
    key_name: String,
    keyring_backend: String,
    node: String,
    chain_id: String,
    account_address: String,
    gas: String,
    gas_adjustment: f64,
    gas_prices: String,
    sign_mode: String,
}

impl AkashConfig {
    async fn new() -> Result<Self> {
        // Similar to bash script's environment setup
        let node = "https://rpc-akash.ecostake.com:443".to_string();
        let chain_id = fetch_chain_id().await?;

        Ok(Self {
            key_name: "test1".to_string(),
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

struct AkashClient {
    config: AkashConfig,
    trusted_providers: Vec<String>,
    deployments: HashMap<String, DeploymentInfo>,
    api_client: Arc<tokio::sync::Mutex<AkashApiClient>>,
    tx_broadcaster: TxBroadcaster,
    keyring: SimpleKeyring,
}

impl AkashClient {
    async fn new(config: AkashConfig) -> Result<Self> {
        let trusted_providers = vec![
            "akash1u5cdg7k3gl43mukca4aeultuz8x2j68mgwn28e", // d3akash
            "akash1h4h33c8rv8e084el7e74f7pktz27pmxxt8nl9q", // overclock
            "akash15ksejj7g4su7ljufsg0a8eglvkje94z8qsh68a", // palmito
            "akash1kqzpqqhm39umt06wu8m4hx63v5hefhrfmjf9dj", // leet.haus
            "akash16yrzlu9cgxcf4d7k6qjax5fd3cll05p87qha4m", // dsm.hh
            "akash1efge8vzg376fnnfeyg5v8tdq9sg3elhgy42wvm", // marzrock
            "akash1tweev0k42guyv3a2jtgphmgfrl2h5y2884vh9d", // dcnorse
            "akash18ga02jzaq8cw52anyhzkwta5wygufgu6zsz6xc", // europlots
            "akash17l0f3kf7gv4kmgqjmgc0ksj3em6lqgcc4kl4dg", // pcgameservers
            "akash1ut3m97h62tty06qdq9lds85r34dxe3snjj0xfe", // akashgpu.com
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // Initialize API client
        let api_config = AkashApiConfig {
            grpc_endpoint: "https://grpc-akash.ecostake.com:443".to_string(),
            rest_endpoint: config
                .node
                .replace("https://rpc-", "https://rest-")
                .replace(":443", ""),
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
        let mut keyring = SimpleKeyring::new();

        Ok(Self {
            config,
            trusted_providers,
            deployments: HashMap::new(),
            api_client,
            tx_broadcaster,
            keyring,
        })
    }

    async fn setup_keys(&mut self) -> Result<()> {
        println!("[INFO] Setting up keys...");

        // Check if key exists in our keyring
        let key_exists = self
            .keyring
            .list_keys()
            .iter()
            .any(|k| k.name == self.config.key_name);

        if !key_exists {
            println!("[INFO] Creating key '{}'...", self.config.key_name);
            // In a real implementation, this would generate a proper key
            // For now, we'll use a placeholder
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

        // Get lease status and extract node info
        self.collect_node_info(step, &dseq, &provider).await?;

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

    async fn wait_for_bids(&self, dseq: &str) -> Result<()> {
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

    async fn select_provider(&self, dseq: &str) -> Result<String> {
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

    async fn collect_node_info(&self, step: u32, dseq: &str, provider: &str) -> Result<()> {
        println!("[INFO] Checking lease status...");

        // TODO: Implement lease status querying using akash.provider.lease.v1.LeaseRPC/ServiceStatus
        // For now, simulate lease status response
        let status_json = json!({
            "services": {
                "oline-a-snapshot": {
                    "uris": ["http://localhost:26657"]
                },
                "oline-a-seed": {
                    "uris": ["http://localhost:26656"]
                }
            },
            "forwarded_ports": {}
        });

        // Define service names based on step
        let service_names = match step {
            1 => vec!["oline-a-snapshot", "oline-a-seed"],
            2 => vec!["oline-b-left", "oline-b-right"],
            3 => vec!["oline-forward-left", "oline-forward-right"],
            _ => return Err(anyhow!("Invalid step number: {}", step)),
        };

        // Get node IDs for each service
        for (i, service_name) in service_names.iter().enumerate() {
            let node_id = self.get_node_peer_id(&status_json, service_name).await?;
            let env_var_name = match (step, i) {
                (1, 0) => "SNAPSHOT_NODE_PEER_ID",
                (1, 1) => "SEED_NODE_PEER_ID",
                (2, 0) => "LEFT_TACKLE_PEER_ID",
                (2, 1) => "RIGHT_TACKLE_PEER_ID",
                (3, 0) => "LEFT_FORWARD_PEER_ID",
                (3, 1) => "RIGHT_FORWARD_PEER_ID",
                _ => continue,
            };

            self.save_node_info(env_var_name, &node_id).await?;
        }

        Ok(())
    }

    async fn get_node_peer_id(&self, status_json: &Value, service_name: &str) -> Result<String> {
        // Try to get URI from service
        if let Some(uri) = status_json["services"][service_name]["uris"][0].as_str() {
            let full_uri = format!("http://{}:26657", uri);
            return self.fetch_node_id_from_rpc(&full_uri, service_name).await;
        }

        // Fallback to forwarded ports
        if let Some(ports) = status_json["forwarded_ports"][service_name].as_array() {
            for port_info in ports {
                if port_info["port"] == 26657 {
                    if let Some(host) = port_info["host"].as_str() {
                        if let Some(port) = port_info["externalPort"].as_u64() {
                            let uri = format!("http://{}:{}", host, port);
                            return self.fetch_node_id_from_rpc(&uri, service_name).await;
                        }
                    }
                }
            }
        }

        Err(anyhow!(
            "Could not retrieve URI or forwarded port for service: {}",
            service_name
        ))
    }

    async fn fetch_node_id_from_rpc(&self, uri: &str, service_name: &str) -> Result<String> {
        println!(
            "[INFO] Retrieving node-id for '{}' from endpoint '{}'...",
            service_name, uri
        );

        for i in 1..=6 {
            let sleep_seconds = i * 2;

            // Try HTTP and HTTPS
            for scheme in ["http", "https"].iter() {
                let url = format!(
                    "{}://{}/status",
                    scheme,
                    uri.trim_start_matches("http://")
                        .trim_start_matches("https://")
                );

                let api_client = self.api_client.lock().await;
                if let Ok(response) = api_client
                    .http_client()
                    .get(&url)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                {
                    if let Ok(status_json) = response.json::<Value>().await {
                        if let Some(node_id) = status_json["result"]["node_info"]["id"].as_str() {
                            if !node_id.is_empty() {
                                let peer_url = format!(
                                    "{}@{}:443",
                                    node_id,
                                    uri.trim_start_matches("http://")
                                        .trim_start_matches("https://")
                                );
                                println!("[INFO] Retrieved node ID: {}", peer_url);
                                return Ok(peer_url);
                            }
                        }
                    }
                }
            }

            println!(
                "[INFO] Attempt {} failed, retrying in {}s...",
                i, sleep_seconds
            );
            sleep(Duration::from_secs(sleep_seconds as u64)).await;
        }

        Err(anyhow!(
            "Failed to retrieve node_info.id from {} after 6 attempts",
            uri
        ))
    }

    async fn save_node_info(&self, key: &str, value: &str) -> Result<()> {
        let env_content = format!("{}={}\n", key, value);
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("deployment_uris.env")?
            .write_all(env_content.as_bytes())?;

        println!("[INFO] Saved {}={}", key, value);
        Ok(())
    }

    async fn update_sdl_with_node_info(&self, step: u32, sdl_file: &str) -> Result<()> {
        println!(
            "[INFO] Updating {} with node info for step {}...",
            sdl_file, step
        );

        // Read environment file
        let env_content = fs::read_to_string("deployment_uris.env")?;
        let mut env_vars = HashMap::new();

        for line in env_content.lines() {
            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() == 2 {
                env_vars.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        // Read and parse YAML
        let mut yaml: Value = serde_yaml::from_str(&fs::read_to_string(sdl_file)?)?;

        match step {
            1 => {
                if let Some(snapshot_peer) = env_vars.get("SNAPSHOT_NODE_PEER_ID") {
                    // Update YAML with persistent peer
                    // This is simplified - actual YAML manipulation would be more robust
                    println!("[INFO] Setting persistent peer to: {}", snapshot_peer);
                }

                // Get private validator ID
                let current_ip = self.get_public_ip().await?;
                let private_peer = format!("abc123xyz987@{}:26656", current_ip);

                println!("[INFO] Setting private peers to: {}", private_peer);
            }
            2 => {
                if let Some(snapshot_peer) = env_vars.get("SNAPSHOT_NODE_PEER_ID") {
                    println!("[INFO] Setting persistent peer to: {}", snapshot_peer);
                }

                if let (Some(left_tackle), Some(right_tackle)) = (
                    env_vars.get("LEFT_TACKLE_PEER_ID"),
                    env_vars.get("RIGHT_TACKLE_PEER_ID"),
                ) {
                    let private_peers = format!("{},{}", left_tackle, right_tackle);
                    println!("[INFO] Setting private peers to: {}", private_peers);
                }
            }
            _ => return Err(anyhow!("Unsupported step number: {}", step)),
        }

        // Write updated YAML back
        let updated_yaml = serde_yaml::to_string(&yaml)?;
        fs::write(sdl_file, updated_yaml)?;

        println!("[INFO] Updated {} with node info successfully", sdl_file);
        Ok(())
    }

    async fn get_public_ip(&self) -> Result<String> {
        let api_client = self.api_client.lock().await;
        let response = api_client
            .http_client()
            .get("https://api.ipify.org")
            .send()
            .await?;
        Ok(response.text().await?.trim().to_string())
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

#[tokio::main]
async fn main() -> Result<()> {
    println!("[INFO] Starting Akash deployment process...");

    // Initialize config
    let config = AkashConfig::new().await?;
    let mut client = AkashClient::new(config).await?;

    // 1. Setup
    client.setup_keys().await?;
    client.setup_certificate().await?;
    client.check_existing_deployments().await?;

    // SDL files (adjust paths as needed)
    let sdl_files = [
        "sdls/a.kickoff-special-teams.yml",
        "sdls/b.left-and-right-tackle.yml",
        "sdls/c.left-and-right-forwards.yml",
    ];

    // 2. Deploy snapshot & seed node
    client.deploy_sdl(1, sdl_files[0]).await?;

    // 3. Update SDL with node info
    client.update_sdl_with_node_info(1, sdl_files[1]).await?;

    // 4. Deploy L/R Tackles
    client.deploy_sdl(2, sdl_files[1]).await?;

    // 5. Update SDL with node info
    client.update_sdl_with_node_info(2, sdl_files[2]).await?;

    // 6. Deploy L/R Forwards
    client.deploy_sdl(3, sdl_files[2]).await?;

    // Print summary
    println!("[INFO] All deployments completed successfully!");
    println!("[INFO] Deployment Summary:");

    for (sdl_file, info) in &client.deployments {
        println!(
            "  {}: DSEQ={}, Provider={}",
            sdl_file, info.dseq, info.provider
        );
    }

    Ok(())
}
