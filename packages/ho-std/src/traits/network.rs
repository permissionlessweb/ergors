//! Network-related traits for CW-HO system

use crate::commonware::error::CommonwareNetworkResult;
use crate::commonware::identity::NodePrivKey;
use crate::error::HoResult;
use crate::traits::NetworkConfigTrait;
use async_trait::async_trait;
use std::net::SocketAddr;

/// Core trait for network node identity
pub trait NodeIdentityTrait {
    type HostOS;
    type NodeType;
    type PrivateKey;
    type PublicKey;

    /// Create a new node identity
    fn new_node(
        host: String,
        p2p_port: u16,
        api_port: u16,
        user: String,
        os: Self::HostOS,
        ssh_port: u16,
        node_type: Self::NodeType,
        private_key: Self::PrivateKey,
    ) -> Self;

    /// Generate a fresh keypair
    fn generate_keypair<R: rand::RngCore + rand::CryptoRng>(
        &mut self,
        rng: &mut R,
    ) -> CommonwareNetworkResult<()>;

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
    type NodeInfo;
    type Connection;

    /// Get all nodes in topology
    fn nodes(&self) -> &[Self::NodeInfo];

    /// Get all connections in topology
    fn connections(&self) -> &[Self::Connection];

    /// Get online nodes only
    fn online_nodes(&self) -> Vec<&Self::NodeInfo>;

    /// Add a node to topology
    fn add_node(&mut self, node: Self::NodeInfo);

    /// Remove a node from topology
    fn remove_node(&mut self, node_id: &str);

    /// Add a connection
    fn add_connection(&mut self, connection: Self::Connection);

    /// Remove a connection
    fn remove_connection(&mut self, from_node: &str, to_node: &str);
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
