//! Test Network Orchestrator
//!
//! Orchestrates multiple ERGORS node instances for E2E testing.
//! Handles node lifecycle, configuration generation, and network coordination.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Test node role in the network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestNodeRole {
    /// Coordinator node - manages deployments and grants
    Coordinator,
    /// Executor node - requests grants and performs deployments
    Executor,
    /// Referee node - validates and monitors
    Referee,
}

impl std::fmt::Display for TestNodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestNodeRole::Coordinator => write!(f, "coordinator"),
            TestNodeRole::Executor => write!(f, "executor"),
            TestNodeRole::Referee => write!(f, "referee"),
        }
    }
}

/// Configuration for a test node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestNodeConfig {
    /// Node identifier
    pub id: String,
    /// Node role
    pub role: TestNodeRole,
    /// Home directory for this node
    pub home_dir: PathBuf,
    /// gRPC port
    pub grpc_port: u16,
    /// HTTP API port
    pub http_port: u16,
    /// P2P port for network communication
    pub p2p_port: u16,
    /// Seed nodes to connect to (for non-coordinator nodes)
    pub seed_nodes: Vec<String>,
    /// Grant acceptance mode
    pub grant_mode: String,
    /// Test wallet mnemonic (for testing only)
    pub test_mnemonic: Option<String>,
}

impl TestNodeConfig {
    /// Create a new test node config
    pub fn new(id: &str, role: TestNodeRole, base_port: u16) -> Self {
        Self {
            id: id.to_string(),
            role,
            home_dir: PathBuf::new(), // Set later
            grpc_port: base_port,
            http_port: base_port + 1,
            p2p_port: base_port + 2,
            seed_nodes: Vec::new(),
            grant_mode: "accept_all".to_string(),
            test_mnemonic: None,
        }
    }

    /// Get the node's P2P address
    pub fn p2p_addr(&self) -> String {
        format!("127.0.0.1:{}", self.p2p_port)
    }

    /// Get the node's gRPC address
    pub fn grpc_addr(&self) -> String {
        format!("127.0.0.1:{}", self.grpc_port)
    }

    /// Get the node's HTTP API address
    pub fn http_addr(&self) -> String {
        format!("http://127.0.0.1:{}", self.http_port)
    }
}

/// Running test node instance
pub struct TestNodeInstance {
    /// Node configuration
    pub config: TestNodeConfig,
    /// Child process handle
    process: Option<Child>,
    /// Whether the node is healthy
    pub healthy: bool,
}

impl TestNodeInstance {
    /// Create a new instance (not yet started)
    pub fn new(config: TestNodeConfig) -> Self {
        Self {
            config,
            process: None,
            healthy: false,
        }
    }

    /// Check if the node process is running
    pub fn is_running(&self) -> bool {
        self.process.is_some()
    }

    /// Get the process ID if running
    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.id())
    }
}

/// Test network orchestrator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Base directory for all test nodes
    pub base_dir: PathBuf,
    /// Path to ergors binary
    pub ergors_binary: PathBuf,
    /// Starting port for nodes (each node uses 3 consecutive ports)
    pub base_port: u16,
    /// Number of executor nodes
    pub executor_count: usize,
    /// Include a referee node
    pub include_referee: bool,
    /// Node startup timeout
    pub startup_timeout_secs: u64,
    /// Health check interval
    pub health_check_interval_secs: u64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            base_dir: std::env::temp_dir().join("ergors-e2e-test"),
            ergors_binary: PathBuf::from("target/release/ergors"),
            base_port: 50100,
            executor_count: 2,
            include_referee: false,
            startup_timeout_secs: 30,
            health_check_interval_secs: 5,
        }
    }
}

/// Test network orchestrator
///
/// Manages a network of ERGORS nodes for E2E testing.
/// Handles node lifecycle, configuration, and coordination.
pub struct TestNetworkOrchestrator {
    config: OrchestratorConfig,
    nodes: Arc<RwLock<HashMap<String, TestNodeInstance>>>,
    coordinator_id: Option<String>,
}

impl TestNetworkOrchestrator {
    /// Create a new orchestrator with default config
    pub fn new() -> Self {
        Self::with_config(OrchestratorConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: OrchestratorConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            coordinator_id: None,
        }
    }

    /// Initialize the test network
    ///
    /// Creates directories, generates configs, but doesn't start nodes yet.
    pub async fn init(&mut self) -> Result<()> {
        info!("Initializing test network orchestrator");

        // Create base directory
        std::fs::create_dir_all(&self.config.base_dir)?;

        // Generate node configs
        let mut port = self.config.base_port;

        // Coordinator node
        let coordinator_config = self.create_node_config("coordinator", TestNodeRole::Coordinator, port)?;
        port += 10;
        self.coordinator_id = Some("coordinator".to_string());

        let coordinator_p2p = coordinator_config.p2p_addr();

        {
            let mut nodes = self.nodes.write().await;
            nodes.insert("coordinator".to_string(), TestNodeInstance::new(coordinator_config));
        }

        // Executor nodes
        for i in 0..self.config.executor_count {
            let id = format!("executor_{}", i);
            let mut config = self.create_node_config(&id, TestNodeRole::Executor, port)?;
            config.seed_nodes.push(coordinator_p2p.clone());
            port += 10;

            let mut nodes = self.nodes.write().await;
            nodes.insert(id, TestNodeInstance::new(config));
        }

        // Referee node (optional)
        if self.config.include_referee {
            let mut config = self.create_node_config("referee", TestNodeRole::Referee, port)?;
            config.seed_nodes.push(coordinator_p2p.clone());

            let mut nodes = self.nodes.write().await;
            nodes.insert("referee".to_string(), TestNodeInstance::new(config));
        }

        info!("Test network initialized with {} nodes", self.nodes.read().await.len());
        Ok(())
    }

    /// Create a node configuration
    fn create_node_config(&self, id: &str, role: TestNodeRole, base_port: u16) -> Result<TestNodeConfig> {
        let home_dir = self.config.base_dir.join(id);
        std::fs::create_dir_all(&home_dir)?;

        let mut config = TestNodeConfig::new(id, role, base_port);
        config.home_dir = home_dir.clone();

        // Generate test mnemonic for the node
        config.test_mnemonic = Some(generate_test_mnemonic(id));

        // Write node config file
        self.write_node_config(&config)?;

        Ok(config)
    }

    /// Write the TOML config file for a node
    fn write_node_config(&self, config: &TestNodeConfig) -> Result<()> {
        let config_content = format!(
            r#"# ERGORS Test Node Configuration
# Auto-generated for E2E testing

[network]
listen_addr = "0.0.0.0:{}"
seeds = {:?}
node_type = "{}"

[identity]
node_id = "{}"
public_key = ""

[storage]
data_dir = "{}/data"

[llm]
# Test configuration - no real providers

[grant]
acceptance_mode = "{}"
"#,
            config.p2p_port,
            config.seed_nodes,
            config.role,
            config.id,
            config.home_dir.display(),
            config.grant_mode
        );

        let config_path = config.home_dir.join("config.toml");
        std::fs::write(&config_path, config_content)?;
        debug!("Wrote config for node '{}' to {:?}", config.id, config_path);

        Ok(())
    }

    /// Start all nodes in the network
    pub async fn start_all(&mut self) -> Result<()> {
        info!("Starting all test nodes");

        // Start coordinator first
        let coord_id = self.coordinator_id.clone();
        if let Some(ref id) = coord_id {
            self.start_node(id).await?;
            // Wait for coordinator to be ready
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // Start other nodes
        let node_ids: Vec<String> = {
            let nodes = self.nodes.read().await;
            nodes.keys()
                .filter(|id| Some(*id) != coord_id.as_ref())
                .cloned()
                .collect()
        };

        for id in node_ids {
            self.start_node(&id).await?;
        }

        // Wait for all nodes to be healthy
        self.wait_for_healthy().await?;

        info!("All test nodes started and healthy");
        Ok(())
    }

    /// Start a specific node
    pub async fn start_node(&mut self, node_id: &str) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes.get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        if node.is_running() {
            warn!("Node '{}' is already running", node_id);
            return Ok(());
        }

        info!("Starting node '{}'", node_id);

        let child = Command::new(&self.config.ergors_binary)
            .arg("start")
            .arg("--home")
            .arg(&node.config.home_dir)
            .arg("--grpc-port")
            .arg(node.config.grpc_port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        info!("Node '{}' started with PID {}", node_id, child.id());
        node.process = Some(child);

        Ok(())
    }

    /// Stop a specific node
    pub async fn stop_node(&mut self, node_id: &str) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes.get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        if let Some(mut process) = node.process.take() {
            info!("Stopping node '{}'", node_id);
            process.kill()?;
            process.wait()?;
            node.healthy = false;
        }

        Ok(())
    }

    /// Stop all nodes
    pub async fn stop_all(&mut self) -> Result<()> {
        info!("Stopping all test nodes");

        let node_ids: Vec<String> = self.nodes.read().await.keys().cloned().collect();

        for id in node_ids {
            if let Err(e) = self.stop_node(&id).await {
                error!("Failed to stop node '{}': {}", id, e);
            }
        }

        Ok(())
    }

    /// Wait for all nodes to be healthy
    async fn wait_for_healthy(&self) -> Result<()> {
        let timeout = Duration::from_secs(self.config.startup_timeout_secs);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for nodes to be healthy"));
            }

            let mut all_healthy = true;

            {
                let mut nodes = self.nodes.write().await;
                for (id, node) in nodes.iter_mut() {
                    if !node.healthy {
                        match self.check_node_health(&node.config).await {
                            Ok(true) => {
                                node.healthy = true;
                                info!("Node '{}' is healthy", id);
                            }
                            Ok(false) => {
                                all_healthy = false;
                            }
                            Err(e) => {
                                debug!("Health check failed for '{}': {}", id, e);
                                all_healthy = false;
                            }
                        }
                    }
                }
            }

            if all_healthy {
                return Ok(());
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Check if a node is healthy via its gRPC endpoint
    async fn check_node_health(&self, config: &TestNodeConfig) -> Result<bool> {
        // Try to connect to the gRPC port
        let addr = config.grpc_addr();
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get coordinator node config
    pub async fn coordinator(&self) -> Option<TestNodeConfig> {
        if let Some(id) = &self.coordinator_id {
            self.nodes.read().await.get(id).map(|n| n.config.clone())
        } else {
            None
        }
    }

    /// Get all executor node configs
    pub async fn executors(&self) -> Vec<TestNodeConfig> {
        self.nodes.read().await
            .values()
            .filter(|n| n.config.role == TestNodeRole::Executor)
            .map(|n| n.config.clone())
            .collect()
    }

    /// Get node by ID
    pub async fn get_node(&self, id: &str) -> Option<TestNodeConfig> {
        self.nodes.read().await.get(id).map(|n| n.config.clone())
    }

    /// List all nodes
    pub async fn list_nodes(&self) -> Vec<TestNodeConfig> {
        self.nodes.read().await
            .values()
            .map(|n| n.config.clone())
            .collect()
    }

    /// Clean up all test resources
    pub async fn cleanup(&mut self) -> Result<()> {
        info!("Cleaning up test network");

        // Stop all nodes
        self.stop_all().await?;

        // Remove test directories
        if self.config.base_dir.exists() {
            std::fs::remove_dir_all(&self.config.base_dir)?;
        }

        info!("Test network cleanup complete");
        Ok(())
    }

    /// Get network statistics
    pub async fn stats(&self) -> NetworkStats {
        let nodes = self.nodes.read().await;
        let running = nodes.values().filter(|n| n.is_running()).count();
        let healthy = nodes.values().filter(|n| n.healthy).count();

        NetworkStats {
            total_nodes: nodes.len(),
            running_nodes: running,
            healthy_nodes: healthy,
            coordinator_healthy: self.coordinator_id.as_ref()
                .and_then(|id| nodes.get(id))
                .map(|n| n.healthy)
                .unwrap_or(false),
        }
    }
}

impl Default for TestNetworkOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestNetworkOrchestrator {
    fn drop(&mut self) {
        // Try to stop all nodes on drop
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.block_on(async {
                let _ = self.stop_all().await;
            });
        }
    }
}

/// Network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub running_nodes: usize,
    pub healthy_nodes: usize,
    pub coordinator_healthy: bool,
}

/// Generate a deterministic test mnemonic for a node
fn generate_test_mnemonic(node_id: &str) -> String {
    // Use a deterministic seed based on node_id for reproducible tests
    let words = [
        "abandon", "ability", "able", "about", "above", "absent",
        "absorb", "abstract", "absurd", "abuse", "access", "accident",
    ];

    // Simple hash to vary the first word
    let hash = node_id.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    let mut mnemonic = words.to_vec();
    mnemonic[0] = words[hash % words.len()];

    mnemonic.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_config_creation() {
        let config = TestNodeConfig::new("test", TestNodeRole::Executor, 50100);
        assert_eq!(config.grpc_port, 50100);
        assert_eq!(config.http_port, 50101);
        assert_eq!(config.p2p_port, 50102);
        assert_eq!(config.p2p_addr(), "127.0.0.1:50102");
    }

    #[test]
    fn test_deterministic_mnemonic() {
        let m1 = generate_test_mnemonic("node_1");
        let m2 = generate_test_mnemonic("node_1");
        let m3 = generate_test_mnemonic("node_2");

        assert_eq!(m1, m2);
        assert_ne!(m1, m3);
    }

    #[tokio::test]
    async fn test_orchestrator_init() {
        let mut orchestrator = TestNetworkOrchestrator::with_config(OrchestratorConfig {
            base_dir: std::env::temp_dir().join("ergors-test-orch"),
            executor_count: 2,
            include_referee: false,
            ..Default::default()
        });

        orchestrator.init().await.unwrap();

        let nodes = orchestrator.list_nodes().await;
        assert_eq!(nodes.len(), 3); // 1 coordinator + 2 executors

        // Cleanup
        orchestrator.cleanup().await.unwrap();
    }
}
