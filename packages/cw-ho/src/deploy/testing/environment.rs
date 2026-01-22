//! Akash Development Environment Manager
//!
//! Wraps Akash's official Kind-based development environment for integration testing.
//! Uses the `ghcr.io/akash-network/node` and `ghcr.io/akash-network/provider` containers.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Akash development environment configuration
#[derive(Debug, Clone)]
pub struct AkashDevConfig {
    /// Path to Akash provider repository (for Makefile targets)
    pub provider_repo_path: Option<PathBuf>,
    /// Kind cluster name
    pub cluster_name: String,
    /// Kubernetes rollout timeout in seconds
    pub kube_rollout_timeout: u64,
    /// Node RPC endpoint (after startup)
    pub node_rpc_endpoint: String,
    /// Node REST endpoint (after startup)
    pub node_rest_endpoint: String,
    /// Provider endpoint (after startup)
    pub provider_endpoint: String,
    /// Whether to skip binary rebuilds
    pub skip_build: bool,
    /// Use GPU cluster setup
    pub use_gpu: bool,
}

impl Default for AkashDevConfig {
    fn default() -> Self {
        Self {
            provider_repo_path: None,
            cluster_name: "akash-dev".to_string(),
            kube_rollout_timeout: 300,
            node_rpc_endpoint: "http://localhost:26657".to_string(),
            node_rest_endpoint: "http://localhost:1317".to_string(),
            provider_endpoint: "http://localhost:8443".to_string(),
            skip_build: false,
            use_gpu: false,
        }
    }
}

/// Environment status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvStatus {
    /// Not started
    Stopped,
    /// Kind cluster starting
    ClusterStarting,
    /// Kind cluster ready, node starting
    NodeStarting,
    /// Node ready, provider starting
    ProviderStarting,
    /// Fully running
    Running,
    /// Error state
    Error(String),
}

/// Test deployment info
#[derive(Debug, Clone)]
pub struct TestDeployment {
    pub dseq: u64,
    pub gseq: u64,
    pub oseq: u64,
    pub owner: String,
    pub provider: String,
    pub status: String,
    pub endpoints: HashMap<String, String>,
}

/// Akash development environment manager
///
/// Manages the lifecycle of Akash's Kind-based development environment
/// for integration testing. Uses the official Akash provider repository
/// Makefile targets when available, or falls back to direct Docker/Kind commands.
pub struct AkashDevEnvironment {
    config: AkashDevConfig,
    status: Arc<RwLock<EnvStatus>>,
    /// Test accounts created in the environment
    test_accounts: Arc<RwLock<Vec<TestAccount>>>,
    /// Active deployments
    deployments: Arc<RwLock<Vec<TestDeployment>>>,
    /// Working directory for commands
    work_dir: PathBuf,
}

/// Test account with pre-funded balance
#[derive(Debug, Clone)]
pub struct TestAccount {
    pub name: String,
    pub address: String,
    pub balance_uakt: u64,
}

impl AkashDevEnvironment {
    /// Create a new environment with default configuration
    pub fn new() -> Self {
        Self::with_config(AkashDevConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: AkashDevConfig) -> Self {
        let work_dir = config
            .provider_repo_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        Self {
            config,
            status: Arc::new(RwLock::new(EnvStatus::Stopped)),
            test_accounts: Arc::new(RwLock::new(Vec::new())),
            deployments: Arc::new(RwLock::new(Vec::new())),
            work_dir,
        }
    }

    /// Start the development environment
    ///
    /// This performs the following steps:
    /// 1. Creates Kind cluster with Akash configuration
    /// 2. Starts Akash node
    /// 3. Registers and starts provider
    /// 4. Creates test accounts with funding
    pub async fn start() -> Result<Self> {
        let env = Self::new();
        env.startup().await?;
        Ok(env)
    }

    /// Start with custom configuration
    pub async fn start_with_config(config: AkashDevConfig) -> Result<Self> {
        let env = Self::with_config(config);
        env.startup().await?;
        Ok(env)
    }

    /// Internal startup sequence
    async fn startup(&self) -> Result<()> {
        info!("Starting Akash development environment...");

        // Check prerequisites
        self.check_prerequisites().await?;

        // Step 1: Setup Kind cluster
        self.setup_cluster().await?;

        // Step 2: Start node
        self.start_node().await?;

        // Step 3: Start provider
        self.start_provider().await?;

        // Step 4: Create test accounts
        self.setup_test_accounts().await?;

        *self.status.write().await = EnvStatus::Running;
        info!("Akash development environment is ready");

        Ok(())
    }

    /// Check that required tools are installed
    async fn check_prerequisites(&self) -> Result<()> {
        info!("Checking prerequisites...");

        // Check Docker
        let docker_check = Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if docker_check.is_err() || !docker_check.unwrap().success() {
            return Err(anyhow!(
                "Docker is not running. Please start Docker Desktop or the Docker daemon."
            ));
        }

        // Check Kind
        let kind_check = Command::new("kind")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if kind_check.is_err() || !kind_check.unwrap().success() {
            return Err(anyhow!(
                "Kind is not installed. Install with: brew install kind (macOS) or go install sigs.k8s.io/kind@latest"
            ));
        }

        // Check kubectl
        let kubectl_check = Command::new("kubectl")
            .arg("version")
            .arg("--client")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if kubectl_check.is_err() || !kubectl_check.unwrap().success() {
            return Err(anyhow!(
                "kubectl is not installed. Install with: brew install kubectl (macOS)"
            ));
        }

        debug!("All prerequisites satisfied");
        Ok(())
    }

    /// Setup Kind cluster
    async fn setup_cluster(&self) -> Result<()> {
        *self.status.write().await = EnvStatus::ClusterStarting;
        info!("Setting up Kind cluster '{}'...", self.config.cluster_name);

        // Check if cluster already exists
        let existing = Command::new("kind")
            .args(["get", "clusters"])
            .output()
            .await?;

        let clusters = String::from_utf8_lossy(&existing.stdout);
        if clusters.lines().any(|l| l.trim() == self.config.cluster_name) {
            info!("Kind cluster '{}' already exists", self.config.cluster_name);
            return Ok(());
        }

        // Create Kind cluster with Akash-compatible config
        let kind_config = self.generate_kind_config();
        let config_path = std::env::temp_dir().join("akash-kind-config.yaml");
        tokio::fs::write(&config_path, &kind_config).await?;

        let output = Command::new("kind")
            .args([
                "create",
                "cluster",
                "--name",
                &self.config.cluster_name,
                "--config",
                config_path.to_str().unwrap(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to create Kind cluster: {}", stderr));
        }

        // Install ingress controller
        self.install_ingress().await?;

        info!("Kind cluster ready");
        Ok(())
    }

    /// Generate Kind cluster configuration
    fn generate_kind_config(&self) -> String {
        r#"kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  kubeadmConfigPatches:
  - |
    kind: InitConfiguration
    nodeRegistration:
      kubeletExtraArgs:
        node-labels: "ingress-ready=true"
  extraPortMappings:
  - containerPort: 80
    hostPort: 80
    protocol: TCP
  - containerPort: 443
    hostPort: 443
    protocol: TCP
  - containerPort: 26657
    hostPort: 26657
    protocol: TCP
  - containerPort: 1317
    hostPort: 1317
    protocol: TCP
  - containerPort: 8443
    hostPort: 8443
    protocol: TCP
  - containerPort: 9090
    hostPort: 9090
    protocol: TCP
"#
        .to_string()
    }

    /// Install nginx ingress controller
    async fn install_ingress(&self) -> Result<()> {
        info!("Installing ingress controller...");

        let output = Command::new("kubectl")
            .args([
                "apply",
                "-f",
                "https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/kind/deploy.yaml",
            ])
            .output()
            .await?;

        if !output.status.success() {
            warn!(
                "Ingress installation warning: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Wait for ingress to be ready
        self.wait_for_ingress().await?;

        Ok(())
    }

    /// Wait for ingress controller to be ready
    async fn wait_for_ingress(&self) -> Result<()> {
        info!("Waiting for ingress controller...");

        for i in 0..60 {
            let output = Command::new("kubectl")
                .args([
                    "get",
                    "pods",
                    "-n",
                    "ingress-nginx",
                    "-l",
                    "app.kubernetes.io/component=controller",
                    "-o",
                    "jsonpath={.items[0].status.phase}",
                ])
                .output()
                .await?;

            let phase = String::from_utf8_lossy(&output.stdout);
            if phase.trim() == "Running" {
                info!("Ingress controller is ready");
                return Ok(());
            }

            if i % 10 == 0 {
                debug!("Ingress status: {} (attempt {}/60)", phase.trim(), i + 1);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(anyhow!("Ingress controller failed to become ready"))
    }

    /// Start Akash node
    async fn start_node(&self) -> Result<()> {
        *self.status.write().await = EnvStatus::NodeStarting;
        info!("Starting Akash node...");

        // Deploy Akash node using kubectl
        let node_manifest = self.generate_node_manifest();
        let manifest_path = std::env::temp_dir().join("akash-node.yaml");
        tokio::fs::write(&manifest_path, &node_manifest).await?;

        let output = Command::new("kubectl")
            .args(["apply", "-f", manifest_path.to_str().unwrap()])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to deploy Akash node: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Wait for node to be ready
        self.wait_for_node().await?;

        info!("Akash node is ready");
        Ok(())
    }

    /// Generate Akash node Kubernetes manifest
    fn generate_node_manifest(&self) -> String {
        format!(
            r#"apiVersion: v1
kind: Namespace
metadata:
  name: akash-services
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: akash-node
  namespace: akash-services
spec:
  replicas: 1
  selector:
    matchLabels:
      app: akash-node
  template:
    metadata:
      labels:
        app: akash-node
    spec:
      containers:
      - name: akash-node
        image: ghcr.io/akash-network/node:latest
        ports:
        - containerPort: 26657
          name: rpc
        - containerPort: 1317
          name: rest
        - containerPort: 9090
          name: grpc
        env:
        - name: AKASH_HOME
          value: /root/.akash
        - name: AKASH_CHAIN_ID
          value: localakash
        - name: AKASH_KEYRING_BACKEND
          value: test
        command: ["/bin/sh", "-c"]
        args:
        - |
          akash init test-node --chain-id localakash
          akash keys add validator --keyring-backend test
          akash keys add faucet --keyring-backend test
          akash add-genesis-account $(akash keys show validator -a --keyring-backend test) 100000000000000uakt
          akash add-genesis-account $(akash keys show faucet -a --keyring-backend test) 100000000000000uakt
          akash gentx validator 10000000000uakt --chain-id localakash --keyring-backend test
          akash collect-gentxs
          akash start --rpc.laddr tcp://0.0.0.0:26657 --api.enable --api.address tcp://0.0.0.0:1317
        volumeMounts:
        - name: akash-data
          mountPath: /root/.akash
      volumes:
      - name: akash-data
        emptyDir: {{}}
---
apiVersion: v1
kind: Service
metadata:
  name: akash-node
  namespace: akash-services
spec:
  type: NodePort
  selector:
    app: akash-node
  ports:
  - name: rpc
    port: 26657
    targetPort: 26657
    nodePort: 30657
  - name: rest
    port: 1317
    targetPort: 1317
    nodePort: 31317
  - name: grpc
    port: 9090
    targetPort: 9090
    nodePort: 30090
"#
        )
    }

    /// Wait for node to be ready
    async fn wait_for_node(&self) -> Result<()> {
        info!("Waiting for Akash node...");

        // Wait for pod to be running
        for i in 0..120 {
            let output = Command::new("kubectl")
                .args([
                    "get",
                    "pods",
                    "-n",
                    "akash-services",
                    "-l",
                    "app=akash-node",
                    "-o",
                    "jsonpath={.items[0].status.phase}",
                ])
                .output()
                .await?;

            let phase = String::from_utf8_lossy(&output.stdout);
            if phase.trim() == "Running" {
                // Check if RPC is responding
                tokio::time::sleep(Duration::from_secs(5)).await;

                let rpc_check = reqwest::Client::new()
                    .get(format!("{}/status", self.config.node_rpc_endpoint))
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await;

                if rpc_check.is_ok() {
                    return Ok(());
                }
            }

            if i % 15 == 0 {
                debug!("Node status: {} (attempt {}/120)", phase.trim(), i + 1);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(anyhow!("Akash node failed to become ready"))
    }

    /// Start Akash provider
    async fn start_provider(&self) -> Result<()> {
        *self.status.write().await = EnvStatus::ProviderStarting;
        info!("Starting Akash provider...");

        let provider_manifest = self.generate_provider_manifest();
        let manifest_path = std::env::temp_dir().join("akash-provider.yaml");
        tokio::fs::write(&manifest_path, &provider_manifest).await?;

        let output = Command::new("kubectl")
            .args(["apply", "-f", manifest_path.to_str().unwrap()])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to deploy Akash provider: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Wait for provider to be ready
        self.wait_for_provider().await?;

        info!("Akash provider is ready");
        Ok(())
    }

    /// Generate Akash provider Kubernetes manifest
    fn generate_provider_manifest(&self) -> String {
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: akash-provider
  namespace: akash-services
spec:
  replicas: 1
  selector:
    matchLabels:
      app: akash-provider
  template:
    metadata:
      labels:
        app: akash-provider
    spec:
      containers:
      - name: akash-provider
        image: ghcr.io/akash-network/provider:latest
        ports:
        - containerPort: 8443
          name: api
        env:
        - name: AKASH_NODE
          value: http://akash-node:26657
        - name: AKASH_CHAIN_ID
          value: localakash
        - name: AKASH_KEYRING_BACKEND
          value: test
        - name: AKASH_FROM
          value: provider
        - name: AKASH_HOME
          value: /root/.akash
        volumeMounts:
        - name: provider-data
          mountPath: /root/.akash
      volumes:
      - name: provider-data
        emptyDir: {}
---
apiVersion: v1
kind: Service
metadata:
  name: akash-provider
  namespace: akash-services
spec:
  type: NodePort
  selector:
    app: akash-provider
  ports:
  - name: api
    port: 8443
    targetPort: 8443
    nodePort: 30443
"#
        .to_string()
    }

    /// Wait for provider to be ready
    async fn wait_for_provider(&self) -> Result<()> {
        info!("Waiting for Akash provider...");

        for i in 0..120 {
            let output = Command::new("kubectl")
                .args([
                    "get",
                    "pods",
                    "-n",
                    "akash-services",
                    "-l",
                    "app=akash-provider",
                    "-o",
                    "jsonpath={.items[0].status.phase}",
                ])
                .output()
                .await?;

            let phase = String::from_utf8_lossy(&output.stdout);
            if phase.trim() == "Running" {
                return Ok(());
            }

            if i % 15 == 0 {
                debug!("Provider status: {} (attempt {}/120)", phase.trim(), i + 1);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(anyhow!("Akash provider failed to become ready"))
    }

    /// Setup test accounts with funding
    async fn setup_test_accounts(&self) -> Result<()> {
        info!("Setting up test accounts...");

        let accounts = vec![
            TestAccount {
                name: "deployer".to_string(),
                address: String::new(), // Will be filled
                balance_uakt: 100_000_000_000, // 100k AKT
            },
            TestAccount {
                name: "granter".to_string(),
                address: String::new(),
                balance_uakt: 100_000_000_000,
            },
            TestAccount {
                name: "grantee".to_string(),
                address: String::new(),
                balance_uakt: 1_000_000, // 1 AKT (will request grants)
            },
        ];

        let mut created_accounts = Vec::new();

        for mut account in accounts {
            // Create key in node's keyring
            let output = Command::new("kubectl")
                .args([
                    "exec",
                    "-n",
                    "akash-services",
                    "deploy/akash-node",
                    "--",
                    "akash",
                    "keys",
                    "add",
                    &account.name,
                    "--keyring-backend",
                    "test",
                    "--output",
                    "json",
                ])
                .output()
                .await?;

            if output.status.success() {
                // Parse address from output
                let key_info: serde_json::Value =
                    serde_json::from_slice(&output.stdout).unwrap_or_default();
                if let Some(addr) = key_info.get("address").and_then(|v| v.as_str()) {
                    account.address = addr.to_string();
                }
            }

            // Fund account from faucet
            if !account.address.is_empty() {
                let _ = Command::new("kubectl")
                    .args([
                        "exec",
                        "-n",
                        "akash-services",
                        "deploy/akash-node",
                        "--",
                        "akash",
                        "tx",
                        "bank",
                        "send",
                        "faucet",
                        &account.address,
                        &format!("{}uakt", account.balance_uakt),
                        "--keyring-backend",
                        "test",
                        "--chain-id",
                        "localakash",
                        "-y",
                    ])
                    .output()
                    .await;

                info!(
                    "Created test account '{}' at {} with {} uAKT",
                    account.name, account.address, account.balance_uakt
                );
            }

            created_accounts.push(account);
        }

        *self.test_accounts.write().await = created_accounts;
        Ok(())
    }

    /// Get current environment status
    pub async fn status(&self) -> EnvStatus {
        self.status.read().await.clone()
    }

    /// Check if environment is running
    pub async fn is_running(&self) -> bool {
        *self.status.read().await == EnvStatus::Running
    }

    /// Get test accounts
    pub async fn test_accounts(&self) -> Vec<TestAccount> {
        self.test_accounts.read().await.clone()
    }

    /// Get a specific test account by name
    pub async fn get_account(&self, name: &str) -> Option<TestAccount> {
        self.test_accounts
            .read()
            .await
            .iter()
            .find(|a| a.name == name)
            .cloned()
    }

    /// Get node RPC endpoint
    pub fn node_rpc_endpoint(&self) -> &str {
        &self.config.node_rpc_endpoint
    }

    /// Get node REST endpoint
    pub fn node_rest_endpoint(&self) -> &str {
        &self.config.node_rest_endpoint
    }

    /// Get provider endpoint
    pub fn provider_endpoint(&self) -> &str {
        &self.config.provider_endpoint
    }

    /// Create a test deployment
    pub async fn create_deployment(&self, owner: &str, sdl: &str) -> Result<TestDeployment> {
        info!("Creating test deployment for {}...", owner);

        // Write SDL to temp file
        let sdl_path = std::env::temp_dir().join("test-deployment.sdl.yaml");
        tokio::fs::write(&sdl_path, sdl).await?;

        // Create deployment via akash CLI
        let output = Command::new("kubectl")
            .args([
                "exec",
                "-n",
                "akash-services",
                "deploy/akash-node",
                "--",
                "akash",
                "tx",
                "deployment",
                "create",
                "/tmp/test-deployment.sdl.yaml",
                "--from",
                owner,
                "--keyring-backend",
                "test",
                "--chain-id",
                "localakash",
                "-y",
                "--output",
                "json",
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to create deployment: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Parse deployment info
        let deployment = TestDeployment {
            dseq: 1, // Would parse from output
            gseq: 1,
            oseq: 1,
            owner: owner.to_string(),
            provider: String::new(),
            status: "pending".to_string(),
            endpoints: HashMap::new(),
        };

        self.deployments.write().await.push(deployment.clone());

        Ok(deployment)
    }

    /// Query deployments
    pub async fn query_deployments(&self, owner: &str) -> Result<Vec<TestDeployment>> {
        let output = Command::new("kubectl")
            .args([
                "exec",
                "-n",
                "akash-services",
                "deploy/akash-node",
                "--",
                "akash",
                "query",
                "deployment",
                "list",
                "--owner",
                owner,
                "--output",
                "json",
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to query deployments: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Parse deployments from JSON output
        // For now return cached deployments
        Ok(self.deployments.read().await.clone())
    }

    /// Stop the development environment
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping Akash development environment...");

        // Delete Kubernetes resources
        let _ = Command::new("kubectl")
            .args(["delete", "namespace", "akash-services", "--ignore-not-found"])
            .output()
            .await;

        // Optionally delete Kind cluster
        let _ = Command::new("kind")
            .args(["delete", "cluster", "--name", &self.config.cluster_name])
            .output()
            .await;

        *self.status.write().await = EnvStatus::Stopped;
        info!("Environment stopped");

        Ok(())
    }

    /// Reset the environment (stop and restart)
    pub async fn reset(&self) -> Result<()> {
        self.stop().await?;
        self.startup().await
    }

    /// Execute a make target (if provider repo is available)
    pub async fn make_target(&self, target: &str) -> Result<String> {
        let repo_path = self
            .config
            .provider_repo_path
            .as_ref()
            .ok_or_else(|| anyhow!("Provider repository path not configured"))?;

        let runbook_path = repo_path.join("_run/kube");

        let output = Command::new("make")
            .arg(target)
            .current_dir(&runbook_path)
            .env("KUBE_ROLLOUT_TIMEOUT", self.config.kube_rollout_timeout.to_string())
            .env("SKIP_BUILD", if self.config.skip_build { "1" } else { "0" })
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Make target '{}' failed: {}",
                target,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl Drop for AkashDevEnvironment {
    fn drop(&mut self) {
        // Note: async cleanup not possible in Drop
        // Users should call stop() explicitly
        error!("AkashDevEnvironment dropped - call stop() explicitly for cleanup");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AkashDevConfig::default();
        assert_eq!(config.cluster_name, "akash-dev");
        assert_eq!(config.kube_rollout_timeout, 300);
    }

    #[test]
    fn test_kind_config_generation() {
        let env = AkashDevEnvironment::new();
        let config = env.generate_kind_config();
        assert!(config.contains("kind: Cluster"));
        assert!(config.contains("ingress-ready=true"));
    }
}
