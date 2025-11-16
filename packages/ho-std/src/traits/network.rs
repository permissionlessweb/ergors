//! Network-related traits for ERGORS system

use crate::error::HoResult;
use crate::keys::commonware::NodePrivKey;
use crate::traits::NetworkConfigTrait;
use crate::types::ergors::network::v1::*;
use async_trait::async_trait;
use std::net::SocketAddr;

/// Core trait for network node identity
pub trait NodeIdentityTrait {
    type HostOS;
    type NodeType;
    type PrivateKey;
    type PublicKey;

    /// Create a new node identity
    fn new() -> Self
    where
        Self: Sized;

    /// Generate a fresh keypair
    fn generate_keypair<R: rand::RngCore + rand::CryptoRng>(&mut self, rng: &mut R)
        -> HoResult<()>;

    /// Set keypair from existing keys
    fn set_keypair(&mut self, private_key: Self::PrivateKey);

    /// Get P2P identity address
    fn p2p_identity(&self) -> String;

    /// Get P2P socket address
    fn p2p_address(&self) -> SocketAddr;

    /// Get API address
    fn api_address(&self) -> String;

    /// Get display-friendly identifier
    fn display_id(&self) -> String;

    fn get_private_key_from_env() -> NodePrivKey;
    fn private_key_from_hex(hex_string: &str) -> Option<NodePrivKey>;
}

/// Core trait for network topology management
pub trait NetworkTopologyTrait {
    /// Create a new empty topology
    fn new() -> Self
    where
        Self: Sized;

    type NodeInfo;
    type Connection;

    /// Get all nodes in topology
    fn nodes(&self) -> &[Self::NodeInfo];

    // /// Get all nodes of a specific type
    fn nodes_by_type(&self, node_type: NodeType) -> Vec<&NodeInfo>;

    /// Get online nodes only
    fn online_nodes(&self) -> Vec<&Self::NodeInfo>;

    /// Get all connections in topology
    fn connections(&self) -> &[Self::Connection];

    /// Add a node to topology
    fn add_node(&mut self, node: Self::NodeInfo);

    /// Remove a node from topology
    fn remove_node(&mut self, node_id: &str);

    /// Add a connection
    fn add_connection(&mut self, connection: Self::Connection);

    /// Remove a connection
    fn remove_connection(&mut self, from_node: &str, to_node: &str);
    fn count_nodes_by_type(&self) -> Vec<(String, usize)> {
        // TODO: implement from access in storage root
        // let mut counts = HashMap::new();
        // for node in self.nodes.values() {
        //     *counts.entry(node.node_type.clone()).or_insert(0) += 1;
        // }
        // counts
        vec![]
    }

    /// Check if a connection exists
    fn has_connection(&self, from: &str, to: &str) -> bool;

    // /// Get statistics about the topology
    fn stats(&self) -> TopologyStatsResponse {
        TopologyStatsResponse {
            // total_nodes: self.nodes.len(),
            // online_nodes: self.online_nodes().len(),
            // total_connections: self.connections.len(),
            // is_complete: self.is_complete_tetrahedron(),
            // nodes_by_type: self.count_nodes_by_type(),
            max_message_size: todo!(),
            max_peers: todo!(),
            connection_timeout: todo!(),
        }
    }

    /// Check if the topology forms a complete tetrahedral structure for node
    /// TODO: implement direct wqieries from storage, implement epoch trigger on each request
    fn is_complete_tetrahedron(&self) -> bool {
        let online_nodes = self.online_nodes();

        // // Need exactly 4 nodes (one of each type)
        // if online_nodes.len() != 4 {
        //     return false;
        // }

        // // Check we have one of each type
        // let types: Vec<NodeType> = online_nodes
        //     .iter()
        //     .map(|n| NodeType::from_str_name(&n.clone()).unwrap())
        //     .collect();

        // let has_coordinator = types.contains(&NodeType::Coordinator);
        // let has_executor = types.contains(&NodeType::Executor);
        // let has_referee = types.contains(&NodeType::Referee);
        // let has_development = types.contains(&NodeType::Development);

        // if !(has_coordinator && has_executor && has_referee && has_development) {
        //     return false;
        // }

        // // Check each node is connected to all others (6 edges for 4 nodes)
        // let expected_connections = 6;
        // let actual_connections = self.connections().len();

        // actual_connections >= expected_connections
        false
    }

    // /// Get the nearest node of a specific type
    fn nearest_node_of_type(&self, node_type: NodeType) -> Option<&NodeInfo> {
        self.nodes_by_type(node_type).into_iter().find(|n| n.online)
    }
}

/// Core trait for network message handling
pub trait NetworkMessageTrait {
    type MessageType;
    type ResultType;

    /// Get the message type
    fn message_type(&self) -> &Self::MessageType;

    /// Serialize message to bytes
    fn to_bytes(&self) -> HoResult<Vec<u8>>;

    /// Deserialize message from bytes
    fn from_bytes(bytes: &[u8]) -> HoResult<Self>
    where
        Self: Sized;

    /// Return channel message type identifier
    fn channel(&self) -> HoResult<u8>;
}

/// Core trait for minimal network management
#[async_trait]
pub trait NetworkManagerTrait {
    type Config: NetworkConfigTrait;
    type Identity: NodeIdentityTrait;
    type Topology: NetworkTopologyTrait;
    type Message: NetworkMessageTrait;
    type Context;

    /// Create a new network manager
    async fn new(
        config: Self::Config,
        identity: Self::Identity,
        context: Self::Context,
    ) -> HoResult<Self>
    where
        Self: Sized;

    /// Start the network
    async fn start_network(&mut self, config: Self::Config) -> HoResult<()>;

    /// Stop the network
    async fn stop_network(&mut self) -> HoResult<()>;

    /// Get current network topology
    async fn get_topology(&self) -> Self::Topology;

    /// Send a message to a peer
    async fn send_message(&mut self, peer_id: &str, message: Self::Message) -> HoResult<()>;

    /// Broadcast a message to all peers
    async fn broadcast_message(&mut self, message: Self::Message) -> HoResult<()>;

    /// Handle incoming message
    async fn handle_message(&mut self, from_peer: &str, message: Self::Message) -> HoResult<()>;

    /// Get peer count
    fn peer_count(&self) -> usize;

    /// Check if connected to a specific peer
    fn is_connected_to_peer(&self, peer_id: &str) -> bool;
}
