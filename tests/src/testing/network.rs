//! Network Topology Simulation
//!
//! Simulates multi-node ERGORS network topologies for testing
//! grant request workflows and peer-to-peer communication.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Network topology configuration
#[derive(Debug, Clone)]
pub struct NetworkTopologyConfig {
    /// Number of nodes to simulate
    pub node_count: usize,
    /// Network latency simulation (min, max) in milliseconds
    pub latency_range_ms: (u64, u64),
    /// Packet loss rate (0.0 - 1.0)
    pub packet_loss_rate: f32,
    /// Enable network partitioning simulation
    pub enable_partitioning: bool,
    /// Default grant acceptance mode for nodes
    pub default_grant_mode: GrantAcceptanceMode,
}

impl Default for NetworkTopologyConfig {
    fn default() -> Self {
        Self {
            node_count: 5,
            latency_range_ms: (10, 100),
            packet_loss_rate: 0.0,
            enable_partitioning: false,
            default_grant_mode: GrantAcceptanceMode::Whitelist,
        }
    }
}

/// Grant acceptance modes for simulated nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantAcceptanceMode {
    /// Accept all grant requests
    AcceptAll,
    /// Reject all grant requests
    RejectAll,
    /// Accept only whitelisted requesters
    Whitelist,
    /// Require manual approval (queued)
    Manual,
}

/// Simulated network node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedNode {
    /// Node identifier
    pub id: String,
    /// Node public key (32 bytes, hex encoded)
    pub pubkey: String,
    /// Node address
    pub address: String,
    /// Grant acceptance mode
    pub grant_mode: GrantAcceptanceMode,
    /// Whitelist of allowed requester pubkeys
    pub whitelist: Vec<String>,
    /// Pending grant requests
    pub pending_requests: Vec<GrantRequest>,
    /// Approved grants
    pub approved_grants: Vec<GrantRequest>,
    /// Rejected requests
    pub rejected_requests: Vec<GrantRequest>,
    /// Node status
    pub status: NodeStatus,
    /// Connected peers
    pub connected_peers: Vec<String>,
}

/// Node status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Online,
    Offline,
    Partitioned,
    Syncing,
}

/// Grant request between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantRequest {
    pub id: u64,
    pub requester_pubkey: String,
    pub requester_address: String,
    pub granter_pubkey: String,
    pub granter_address: String,
    pub grant_type: GrantTypeRequest,
    pub duration_seconds: u64,
    pub spend_limit_uakt: u64,
    pub purpose: String,
    pub status: GrantRequestStatus,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
    pub rejection_reason: Option<String>,
}

/// Grant type request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantTypeRequest {
    AuthzOnly,
    FeegrantOnly,
    AuthzAndFeegrant,
}

/// Grant request status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}

/// Network message between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Grant request message
    GrantRequest(GrantRequest),
    /// Grant response message
    GrantResponse {
        request_id: u64,
        approved: bool,
        reason: Option<String>,
    },
    /// Peer discovery
    PeerAnnounce {
        node_id: String,
        pubkey: String,
        address: String,
    },
    /// Heartbeat
    Heartbeat { node_id: String, timestamp: u64 },
}

/// Network event for testing verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    pub timestamp: u64,
    pub event_type: NetworkEventType,
    pub source_node: String,
    pub target_node: Option<String>,
    pub details: serde_json::Value,
}

/// Network event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkEventType {
    NodeJoined,
    NodeLeft,
    MessageSent,
    MessageReceived,
    MessageDropped,
    GrantRequested,
    GrantApproved,
    GrantRejected,
    PartitionCreated,
    PartitionHealed,
}

/// Multi-node network topology simulator
///
/// Simulates an ERGORS network with multiple nodes for testing
/// grant request workflows, peer discovery, and network resilience.
pub struct NetworkTopology {
    config: NetworkTopologyConfig,
    nodes: Arc<RwLock<HashMap<String, SimulatedNode>>>,
    events: Arc<RwLock<Vec<NetworkEvent>>>,
    request_counter: Arc<RwLock<u64>>,
    partitions: Arc<RwLock<Vec<Vec<String>>>>,
}

impl NetworkTopology {
    /// Create a new network topology with default configuration
    pub fn new() -> Self {
        Self::with_config(NetworkTopologyConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: NetworkTopologyConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            request_counter: Arc::new(RwLock::new(0)),
            partitions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Initialize the network with configured number of nodes
    pub async fn init(&self) -> Result<()> {
        info!(
            "Initializing network topology with {} nodes",
            self.config.node_count
        );

        for i in 0..self.config.node_count {
            let node = self.create_node(&format!("node_{}", i)).await?;
            debug!("Created node: {} ({})", node.id, node.address);
        }

        // Connect all nodes in a mesh
        self.connect_all_nodes().await?;

        info!("Network topology initialized");
        Ok(())
    }

    /// Create a new simulated node
    pub async fn create_node(&self, id: &str) -> Result<SimulatedNode> {
        let (pubkey, address) = generate_node_identity(id);

        let node = SimulatedNode {
            id: id.to_string(),
            pubkey: pubkey.clone(),
            address,
            grant_mode: self.config.default_grant_mode,
            whitelist: Vec::new(),
            pending_requests: Vec::new(),
            approved_grants: Vec::new(),
            rejected_requests: Vec::new(),
            status: NodeStatus::Online,
            connected_peers: Vec::new(),
        };

        self.nodes
            .write()
            .await
            .insert(id.to_string(), node.clone());

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::NodeJoined,
            source_node: id.to_string(),
            target_node: None,
            details: serde_json::json!({"pubkey": pubkey}),
        })
        .await;

        Ok(node)
    }

    /// Connect all nodes in a mesh topology
    async fn connect_all_nodes(&self) -> Result<()> {
        let node_ids: Vec<String> = self.nodes.read().await.keys().cloned().collect();

        let mut nodes = self.nodes.write().await;
        for id in &node_ids {
            if let Some(node) = nodes.get_mut(id) {
                node.connected_peers = node_ids
                    .iter()
                    .filter(|peer_id| *peer_id != id)
                    .cloned()
                    .collect();
            }
        }

        Ok(())
    }

    /// Get a node by ID
    pub async fn get_node(&self, id: &str) -> Option<SimulatedNode> {
        self.nodes.read().await.get(id).cloned()
    }

    /// Get a node by pubkey
    pub async fn get_node_by_pubkey(&self, pubkey: &str) -> Option<SimulatedNode> {
        self.nodes
            .read()
            .await
            .values()
            .find(|n| n.pubkey == pubkey)
            .cloned()
    }

    /// List all nodes
    pub async fn list_nodes(&self) -> Vec<SimulatedNode> {
        self.nodes.read().await.values().cloned().collect()
    }

    /// Set node's grant acceptance mode
    pub async fn set_grant_mode(&self, node_id: &str, mode: GrantAcceptanceMode) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        node.grant_mode = mode;
        info!("Set grant mode for '{}' to {:?}", node_id, mode);

        Ok(())
    }

    /// Add pubkey to node's whitelist
    pub async fn whitelist_add(&self, node_id: &str, requester_pubkey: &str) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        if !node.whitelist.contains(&requester_pubkey.to_string()) {
            node.whitelist.push(requester_pubkey.to_string());
            info!("Added {} to {}'s whitelist", requester_pubkey, node_id);
        }

        Ok(())
    }

    /// Remove pubkey from node's whitelist
    pub async fn whitelist_remove(&self, node_id: &str, requester_pubkey: &str) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        node.whitelist.retain(|pk| pk != requester_pubkey);
        info!("Removed {} from {}'s whitelist", requester_pubkey, node_id);

        Ok(())
    }

    /// Submit a grant request from one node to another
    pub async fn submit_grant_request(
        &self,
        requester_id: &str,
        granter_id: &str,
        grant_type: GrantTypeRequest,
        duration_seconds: u64,
        spend_limit_uakt: u64,
        purpose: &str,
    ) -> Result<GrantRequest> {
        // Check if nodes can communicate
        if !self.can_communicate(requester_id, granter_id).await {
            return Err(anyhow!("Nodes cannot communicate (partitioned or offline)"));
        }

        // Simulate packet loss
        if should_drop_packet(&self.config) {
            self.record_event(NetworkEvent {
                timestamp: current_timestamp(),
                event_type: NetworkEventType::MessageDropped,
                source_node: requester_id.to_string(),
                target_node: Some(granter_id.to_string()),
                details: serde_json::json!({"type": "grant_request"}),
            })
            .await;
            return Err(anyhow!("Message dropped (simulated packet loss)"));
        }

        // Simulate latency
        simulate_latency(&self.config).await;

        let mut counter = self.request_counter.write().await;
        *counter += 1;
        let request_id = *counter;

        let nodes = self.nodes.read().await;
        let requester = nodes
            .get(requester_id)
            .ok_or_else(|| anyhow!("Requester '{}' not found", requester_id))?;
        let granter = nodes
            .get(granter_id)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_id))?;

        let request = GrantRequest {
            id: request_id,
            requester_pubkey: requester.pubkey.clone(),
            requester_address: requester.address.clone(),
            granter_pubkey: granter.pubkey.clone(),
            granter_address: granter.address.clone(),
            grant_type,
            duration_seconds,
            spend_limit_uakt,
            purpose: purpose.to_string(),
            status: GrantRequestStatus::Pending,
            created_at: current_timestamp(),
            resolved_at: None,
            rejection_reason: None,
        };

        drop(nodes);

        // Process the request based on granter's mode
        let result = self.process_grant_request(granter_id, request).await?;

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::GrantRequested,
            source_node: requester_id.to_string(),
            target_node: Some(granter_id.to_string()),
            details: serde_json::json!({
                "request_id": result.id,
                "grant_type": format!("{:?}", result.grant_type),
                "status": format!("{:?}", result.status)
            }),
        })
        .await;

        Ok(result)
    }

    /// Process a grant request based on node's acceptance mode
    async fn process_grant_request(
        &self,
        granter_id: &str,
        mut request: GrantRequest,
    ) -> Result<GrantRequest> {
        let mut nodes = self.nodes.write().await;
        let granter = nodes
            .get_mut(granter_id)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_id))?;

        match granter.grant_mode {
            GrantAcceptanceMode::AcceptAll => {
                request.status = GrantRequestStatus::Approved;
                request.resolved_at = Some(current_timestamp());
                granter.approved_grants.push(request.clone());

                info!(
                    "Grant request {} auto-approved (AcceptAll mode)",
                    request.id
                );
            }
            GrantAcceptanceMode::RejectAll => {
                request.status = GrantRequestStatus::Rejected;
                request.resolved_at = Some(current_timestamp());
                request.rejection_reason = Some("Node configured to reject all requests".to_string());
                granter.rejected_requests.push(request.clone());

                info!(
                    "Grant request {} auto-rejected (RejectAll mode)",
                    request.id
                );
            }
            GrantAcceptanceMode::Whitelist => {
                if granter.whitelist.contains(&request.requester_pubkey) {
                    request.status = GrantRequestStatus::Approved;
                    request.resolved_at = Some(current_timestamp());
                    granter.approved_grants.push(request.clone());

                    info!(
                        "Grant request {} approved (whitelisted requester)",
                        request.id
                    );
                } else {
                    request.status = GrantRequestStatus::Rejected;
                    request.resolved_at = Some(current_timestamp());
                    request.rejection_reason = Some("Requester not in whitelist".to_string());
                    granter.rejected_requests.push(request.clone());

                    info!(
                        "Grant request {} rejected (not whitelisted)",
                        request.id
                    );
                }
            }
            GrantAcceptanceMode::Manual => {
                // Queue for manual approval
                granter.pending_requests.push(request.clone());
                info!("Grant request {} queued for manual approval", request.id);
            }
        }

        Ok(request)
    }

    /// Manually approve a pending grant request
    pub async fn approve_request(&self, granter_id: &str, request_id: u64) -> Result<GrantRequest> {
        let mut nodes = self.nodes.write().await;
        let granter = nodes
            .get_mut(granter_id)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_id))?;

        let idx = granter
            .pending_requests
            .iter()
            .position(|r| r.id == request_id)
            .ok_or_else(|| anyhow!("Request {} not found in pending", request_id))?;

        let mut request = granter.pending_requests.remove(idx);
        request.status = GrantRequestStatus::Approved;
        request.resolved_at = Some(current_timestamp());
        granter.approved_grants.push(request.clone());

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::GrantApproved,
            source_node: granter_id.to_string(),
            target_node: Some(request.requester_pubkey.clone()),
            details: serde_json::json!({"request_id": request_id}),
        })
        .await;

        info!("Grant request {} manually approved", request_id);
        Ok(request)
    }

    /// Manually reject a pending grant request
    pub async fn reject_request(
        &self,
        granter_id: &str,
        request_id: u64,
        reason: &str,
    ) -> Result<GrantRequest> {
        let mut nodes = self.nodes.write().await;
        let granter = nodes
            .get_mut(granter_id)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_id))?;

        let idx = granter
            .pending_requests
            .iter()
            .position(|r| r.id == request_id)
            .ok_or_else(|| anyhow!("Request {} not found in pending", request_id))?;

        let mut request = granter.pending_requests.remove(idx);
        request.status = GrantRequestStatus::Rejected;
        request.resolved_at = Some(current_timestamp());
        request.rejection_reason = Some(reason.to_string());
        granter.rejected_requests.push(request.clone());

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::GrantRejected,
            source_node: granter_id.to_string(),
            target_node: Some(request.requester_pubkey.clone()),
            details: serde_json::json!({"request_id": request_id, "reason": reason}),
        })
        .await;

        info!("Grant request {} manually rejected: {}", request_id, reason);
        Ok(request)
    }

    /// Check if two nodes can communicate
    pub async fn can_communicate(&self, node_a: &str, node_b: &str) -> bool {
        let nodes = self.nodes.read().await;

        // Check if both nodes exist and are online
        let node_a_online = nodes
            .get(node_a)
            .map(|n| n.status == NodeStatus::Online)
            .unwrap_or(false);
        let node_b_online = nodes
            .get(node_b)
            .map(|n| n.status == NodeStatus::Online)
            .unwrap_or(false);

        if !node_a_online || !node_b_online {
            return false;
        }

        // Check partitioning
        if self.config.enable_partitioning {
            let partitions = self.partitions.read().await;
            for partition in partitions.iter() {
                let a_in_partition = partition.contains(&node_a.to_string());
                let b_in_partition = partition.contains(&node_b.to_string());
                if a_in_partition != b_in_partition {
                    return false;
                }
            }
        }

        true
    }

    /// Set node status
    pub async fn set_node_status(&self, node_id: &str, status: NodeStatus) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow!("Node '{}' not found", node_id))?;

        node.status = status;
        info!("Set node '{}' status to {:?}", node_id, status);

        Ok(())
    }

    /// Create a network partition
    pub async fn create_partition(&self, partition_nodes: Vec<String>) -> Result<()> {
        if !self.config.enable_partitioning {
            return Err(anyhow!("Partitioning is not enabled"));
        }

        self.partitions.write().await.push(partition_nodes.clone());

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::PartitionCreated,
            source_node: "network".to_string(),
            target_node: None,
            details: serde_json::json!({"nodes": partition_nodes}),
        })
        .await;

        info!("Created network partition with {} nodes", partition_nodes.len());
        Ok(())
    }

    /// Heal all network partitions
    pub async fn heal_partitions(&self) -> Result<()> {
        self.partitions.write().await.clear();

        self.record_event(NetworkEvent {
            timestamp: current_timestamp(),
            event_type: NetworkEventType::PartitionHealed,
            source_node: "network".to_string(),
            target_node: None,
            details: serde_json::json!({}),
        })
        .await;

        info!("All network partitions healed");
        Ok(())
    }

    /// Get all network events
    pub async fn get_events(&self) -> Vec<NetworkEvent> {
        self.events.read().await.clone()
    }

    /// Get events filtered by type
    pub async fn get_events_by_type(&self, event_type: NetworkEventType) -> Vec<NetworkEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Clear event history
    pub async fn clear_events(&self) {
        self.events.write().await.clear();
    }

    /// Record a network event
    async fn record_event(&self, event: NetworkEvent) {
        self.events.write().await.push(event);
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        let nodes = self.nodes.read().await;
        let events = self.events.read().await;

        let online_count = nodes.values().filter(|n| n.status == NodeStatus::Online).count();
        let total_approved = nodes.values().map(|n| n.approved_grants.len()).sum();
        let total_rejected = nodes.values().map(|n| n.rejected_requests.len()).sum();
        let total_pending = nodes.values().map(|n| n.pending_requests.len()).sum();

        NetworkStats {
            total_nodes: nodes.len(),
            online_nodes: online_count,
            total_events: events.len(),
            total_grants_approved: total_approved,
            total_grants_rejected: total_rejected,
            total_grants_pending: total_pending,
        }
    }
}

impl Default for NetworkTopology {
    fn default() -> Self {
        Self::new()
    }
}

/// Network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub total_events: usize,
    pub total_grants_approved: usize,
    pub total_grants_rejected: usize,
    pub total_grants_pending: usize,
}

// ==================== Helper Functions ====================

/// Generate deterministic node identity
fn generate_node_identity(id: &str) -> (String, String) {
    let hash = simple_hash(id);

    // Generate 32-byte pubkey
    let pubkey_bytes: Vec<u8> = (0..32).map(|i| ((hash >> (i % 8)) & 0xFF) as u8).collect();
    let pubkey = hex::encode(&pubkey_bytes);

    // Generate address
    let addr_bytes: Vec<u8> = (0..20)
        .map(|i| ((hash >> ((i + 10) % 8)) & 0xFF) as u8)
        .collect();
    let address = format!("akash1{}", hex::encode(&addr_bytes));

    (pubkey, address)
}

/// Simple hash function
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Simulate network latency
async fn simulate_latency(config: &NetworkTopologyConfig) {
    let (min, max) = config.latency_range_ms;
    let latency = if min == max {
        min
    } else {
        min + (rand::random::<u64>() % (max - min))
    };
    tokio::time::sleep(std::time::Duration::from_millis(latency)).await;
}

/// Check if packet should be dropped
fn should_drop_packet(config: &NetworkTopologyConfig) -> bool {
    config.packet_loss_rate > 0.0 && rand::random::<f32>() < config.packet_loss_rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_init() {
        let network = NetworkTopology::with_config(NetworkTopologyConfig {
            node_count: 3,
            ..Default::default()
        });

        network.init().await.unwrap();

        let nodes = network.list_nodes().await;
        assert_eq!(nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_grant_request_accept_all() {
        let network = NetworkTopology::new();
        network.create_node("requester").await.unwrap();
        network.create_node("granter").await.unwrap();

        network
            .set_grant_mode("granter", GrantAcceptanceMode::AcceptAll)
            .await
            .unwrap();

        let request = network
            .submit_grant_request(
                "requester",
                "granter",
                GrantTypeRequest::AuthzAndFeegrant,
                86400,
                5_000_000,
                "Test deployment",
            )
            .await
            .unwrap();

        assert_eq!(request.status, GrantRequestStatus::Approved);
    }

    #[tokio::test]
    async fn test_grant_request_whitelist() {
        let network = NetworkTopology::new();
        let requester = network.create_node("requester").await.unwrap();
        network.create_node("granter").await.unwrap();

        network
            .set_grant_mode("granter", GrantAcceptanceMode::Whitelist)
            .await
            .unwrap();

        // Request without whitelist should be rejected
        let request = network
            .submit_grant_request(
                "requester",
                "granter",
                GrantTypeRequest::AuthzOnly,
                86400,
                0,
                "Test",
            )
            .await
            .unwrap();

        assert_eq!(request.status, GrantRequestStatus::Rejected);

        // Add to whitelist
        network
            .whitelist_add("granter", &requester.pubkey)
            .await
            .unwrap();

        // Now should be approved
        let request2 = network
            .submit_grant_request(
                "requester",
                "granter",
                GrantTypeRequest::AuthzOnly,
                86400,
                0,
                "Test",
            )
            .await
            .unwrap();

        assert_eq!(request2.status, GrantRequestStatus::Approved);
    }

    #[tokio::test]
    async fn test_manual_approval() {
        let network = NetworkTopology::new();
        network.create_node("requester").await.unwrap();
        network.create_node("granter").await.unwrap();

        network
            .set_grant_mode("granter", GrantAcceptanceMode::Manual)
            .await
            .unwrap();

        let request = network
            .submit_grant_request(
                "requester",
                "granter",
                GrantTypeRequest::AuthzAndFeegrant,
                86400,
                5_000_000,
                "Test",
            )
            .await
            .unwrap();

        assert_eq!(request.status, GrantRequestStatus::Pending);

        // Manually approve
        let approved = network.approve_request("granter", request.id).await.unwrap();
        assert_eq!(approved.status, GrantRequestStatus::Approved);
    }

    #[tokio::test]
    async fn test_node_offline() {
        let network = NetworkTopology::new();
        network.create_node("node_a").await.unwrap();
        network.create_node("node_b").await.unwrap();

        assert!(network.can_communicate("node_a", "node_b").await);

        network
            .set_node_status("node_b", NodeStatus::Offline)
            .await
            .unwrap();

        assert!(!network.can_communicate("node_a", "node_b").await);
    }

    #[test]
    fn test_deterministic_identity() {
        let (pk1, addr1) = generate_node_identity("test");
        let (pk2, addr2) = generate_node_identity("test");
        let (pk3, _) = generate_node_identity("other");

        assert_eq!(pk1, pk2);
        assert_eq!(addr1, addr2);
        assert_ne!(pk1, pk3);
    }
}
