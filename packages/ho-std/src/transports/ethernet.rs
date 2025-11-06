// //! Ethernet-based Transport Implementation
// //!
// //! High-performance Ethernet transport using TCP/UDP with multicast discovery
// //! for local network deployments. Optimized for data centers and local clusters.

// use async_trait::async_trait;
// use ho_std::types::constants::*;
// use serde::{Deserialize, Serialize};
// use std::{
//     collections::HashMap,
//     net::SocketAddr,
//     sync::Arc,
//     time::{Duration, SystemTime},
// };
// use tokio::{
//     io::{AsyncReadExt, AsyncWriteExt},
//     net::{TcpListener, TcpStream, UdpSocket as TokioUdpSocket},
//     sync::{broadcast, RwLock},
//     task::JoinHandle,
//     time::interval,
// };
// use tracing::{debug, error, info, warn};

// use crate::types::constants::GOLDEN_RATIO;

// /// Maximum message size for Ethernet transport
// const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MiB

// /// Ethernet transport configuration
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct EthernetConfig {
//     /// Local binding address for TCP connections
//     pub bind_address: SocketAddr,
//     /// UDP multicast address for peer discovery
//     pub multicast_address: SocketAddr,
//     /// Maximum concurrent connections
//     pub max_connections: usize,
//     /// Connection timeout
//     pub connection_timeout: Duration,
//     /// Discovery interval for broadcasting presence
//     pub discovery_interval: Duration,
//     /// Keep-alive interval for TCP connections
//     pub keepalive_interval: Duration,
// }

// impl Default for EthernetConfig {
//     fn default() -> Self {
//         Self {
//             bind_address: "0.0.0.0:8000".parse().unwrap(),
//             multicast_address: "224.0.0.251:8001".parse().unwrap(), // mDNS-like
//             max_connections: 100,
//             connection_timeout: Duration::from_secs(10),
//             discovery_interval: Duration::from_millis((1000.0 * GOLDEN_RATIO as f64) as u64), // ~1618ms
//             keepalive_interval: Duration::from_secs(30),
//         }
//     }
// }

// /// Ethernet transport address
// #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// pub struct EthernetAddress {
//     pub socket_addr: SocketAddr,
//     pub node_id: String,
// }

// impl std::fmt::Display for EthernetAddress {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}@{}", self.node_id, self.socket_addr)
//     }
// }

// /// Connection state for tracking peers
// #[derive(Debug, Clone)]
// struct ConnectionState {
//     stream: Arc<RwLock<TcpStream>>,
//     last_activity: SystemTime,
//     message_count: u64,
//     bytes_sent: u64,
//     bytes_received: u64,
// }

// /// Ethernet transport implementation
// pub struct EthernetTransport {
//     config: Option<EthernetConfig>,
//     local_address: Option<EthernetAddress>,

//     // Connection management
//     connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
//     udp_socket: Option<Arc<TokioUdpSocket>>,

//     // Discovery and heartbeat tasks
//     discovery_task: Option<JoinHandle<()>>,
//     heartbeat_task: Option<JoinHandle<()>>,
//     message_handler_task: Option<JoinHandle<()>>,

//     // Event handling
//     message_sender: broadcast::Sender<(NodeInfo, SacredMessage)>,
//     message_receiver: broadcast::Receiver<(NodeInfo, SacredMessage)>,

//     // Health tracking
//     health: Arc<RwLock<TransportHealth>>,

//     // Running state
//     is_running: Arc<RwLock<bool>>,
// }

// impl Default for EthernetTransport {
//     fn default() -> Self {
//         Self::new()
//     }
// }

// impl EthernetTransport {
//     /// Create a new Ethernet transport
//     pub fn new() -> Self {
//         let (message_sender, message_receiver) = broadcast::channel(1000);

//         Self {
//             config: None,
//             local_address: None,
//             connections: Arc::new(RwLock::new(HashMap::new())),
//             udp_socket: None,
//             discovery_task: None,
//             heartbeat_task: None,
//             message_handler_task: None,
//             message_sender,
//             message_receiver,
//             health: Arc::new(RwLock::new(TransportHealth {
//                 is_connected: false,
//                 peer_count: 0,
//                 golden_ratio_compliance: 0.0,
//                 error_rate: 0.0,
//                 last_error: None,
//             })),
//             is_running: Arc::new(RwLock::new(false)),
//         }
//     }

//     /// Start TCP listener for incoming connections
//     async fn start_tcp_listener(&mut self, config: &EthernetConfig) -> NetworkResult<()> {
//         let listener = TcpListener::bind(config.bind_address)
//             .await
//             .map_err(|e| NetworkError::TransportError(e.into()))?;

//         info!("🌐 TCP listener started on {}", config.bind_address);

//         let connections = Arc::clone(&self.connections);
//         let message_sender = self.message_sender.clone();
//         let is_running = Arc::clone(&self.is_running);

//         let handler = tokio::spawn(async move {
//             while *is_running.read().await {
//                 match listener.accept().await {
//                     Ok((stream, addr)) => {
//                         info!("🤝 New TCP connection from {}", addr);

//                         let connections = Arc::clone(&connections);
//                         let message_sender = message_sender.clone();

//                         tokio::spawn(async move {
//                             if let Err(e) = Self::handle_tcp_connection(
//                                 stream,
//                                 addr,
//                                 connections,
//                                 message_sender,
//                             )
//                             .await
//                             {
//                                 warn!("🚫 TCP connection error: {}", e);
//                             }
//                         });
//                     }
//                     Err(e) => {
//                         error!("🚫 TCP accept error: {}", e);
//                         tokio::time::sleep(Duration::from_millis(100)).await;
//                     }
//                 }
//             }
//         });

//         self.message_handler_task = Some(handler);
//         Ok(())
//     }

//     /// Handle individual TCP connection
//     async fn handle_tcp_connection(
//         stream: TcpStream,
//         addr: SocketAddr,
//         connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
//         message_sender: broadcast::Sender<(NodeInfo, SacredMessage)>,
//     ) -> NetworkResult<()> {
//         let peer_id = addr.to_string();

//         // Add to connections
//         {
//             let mut connections_guard = connections.write().await;
//             connections_guard.insert(
//                 peer_id.clone(),
//                 ConnectionState {
//                     stream: Arc::new(RwLock::new(stream)),
//                     last_activity: SystemTime::now(),
//                     message_count: 0,
//                     bytes_sent: 0,
//                     bytes_received: 0,
//                 },
//             );
//         }

//         // Message handling loop
//         let mut buffer = vec![0u8; MAX_MESSAGE_SIZE];
//         loop {
//             let connection = {
//                 let connections_guard = connections.read().await;
//                 connections_guard.get(&peer_id).cloned()
//             };

//             if let Some(conn) = connection {
//                 let mut stream_guard = conn.stream.write().await;
//                 match stream_guard.read(&mut buffer).await {
//                     Ok(0) => {
//                         info!("👋 Peer {} disconnected", peer_id);
//                         break;
//                     }
//                     Ok(n) => {
//                         // Deserialize and forward message
//                         match Self::deserialize_message(&buffer[..n]) {
//                             Ok((node_info, message)) => {
//                                 debug!("📨 Received message from {}", peer_id);
//                                 let _ = message_sender.send((node_info, message));
//                             }
//                             Err(e) => {
//                                 warn!("🚫 Failed to deserialize message from {}: {}", peer_id, e);
//                             }
//                         }
//                     }
//                     Err(e) => {
//                         warn!("🚫 TCP read error from {}: {}", peer_id, e);
//                         break;
//                     }
//                 }
//             } else {
//                 break;
//             }
//         }

//         // Remove from connections
//         {
//             let mut connections_guard = connections.write().await;
//             connections_guard.remove(&peer_id);
//         }

//         Ok(())
//     }

//     /// Start UDP discovery service
//     async fn start_discovery_service(&mut self, config: &EthernetConfig) -> NetworkResult<()> {
//         let socket = TokioUdpSocket::bind("0.0.0.0:0")
//             .await
//             .map_err(|e| NetworkError::TransportError(e.into()))?;

//         socket
//             .set_broadcast(true)
//             .map_err(|e| NetworkError::TransportError(e.into()))?;

//         let socket = Arc::new(socket);
//         self.udp_socket = Some(Arc::clone(&socket));

//         let multicast_addr = config.multicast_address;
//         let discovery_interval = config.discovery_interval;
//         let local_addr = self.local_address.clone();
//         let is_running = Arc::clone(&self.is_running);

//         // Discovery broadcast task
//         let discovery_socket = Arc::clone(&socket);
//         let discovery_task = tokio::spawn(async move {
//             let mut interval = interval(discovery_interval);

//             while *is_running.read().await {
//                 interval.tick().await;

//                 if let Some(ref addr) = local_addr {
//                     let discovery_message = Self::create_discovery_message(addr);
//                     match discovery_socket
//                         .send_to(&discovery_message, multicast_addr)
//                         .await
//                     {
//                         Ok(_) => debug!("📡 Discovery broadcast sent"),
//                         Err(e) => warn!("🚫 Discovery broadcast failed: {}", e),
//                     }
//                 }
//             }
//         });

//         self.discovery_task = Some(discovery_task);

//         // Discovery listener task
//         let listener_socket = Arc::clone(&socket);
//         let message_sender = self.message_sender.clone();
//         let is_running = Arc::clone(&self.is_running);
//         let listener_task = tokio::spawn(async move {
//             let mut buffer = vec![0u8; 1024];

//             while *is_running.read().await {
//                 match listener_socket.recv_from(&mut buffer).await {
//                     Ok((n, addr)) => {
//                         debug!("📡 Discovery message received from {}", addr);
//                         if let Ok(peer_info) = Self::parse_discovery_message(&buffer[..n]) {
//                             debug!("🔍 Discovered peer: {}", peer_info.node_id);
//                             // Could emit discovery events here
//                         }
//                     }
//                     Err(e) => {
//                         warn!("🚫 UDP recv error: {}", e);
//                         tokio::time::sleep(Duration::from_millis(100)).await;
//                     }
//                 }
//             }
//         });

//         self.heartbeat_task = Some(listener_task);

//         info!("🔍 UDP discovery service started on {}", multicast_addr);
//         Ok(())
//     }

//     /// Create discovery message
//     fn create_discovery_message(local_addr: &EthernetAddress) -> Vec<u8> {
//         let discovery = serde_json::json!({
//             "type": "discovery",
//             "node_id": local_addr.node_id,
//             "address": local_addr.socket_addr.to_string(),
//             "protocol_version": PROTOCOL_VERSION,
//             "timestamp": SystemTime::now()
//                 .duration_since(SystemTime::UNIX_EPOCH)
//                 .unwrap()
//                 .as_secs(),
//             "capabilities": ["sacred-geometry", "golden-ratio", "tetrahedral-topology"]
//         });

//         serde_json::to_vec(&discovery).unwrap_or_default()
//     }

//     /// Parse discovery message
//     fn parse_discovery_message(data: &[u8]) -> NetworkResult<NodeInfo> {
//         let value: serde_json::Value = serde_json::from_slice(data)
//             .map_err(|e| NetworkError::InvalidMessage(e.to_string()))?;

//         let node_id = value["node_id"]
//             .as_str()
//             .ok_or_else(|| NetworkError::InvalidMessage("Missing node_id".to_string()))?;

//         let address = value["address"]
//             .as_str()
//             .ok_or_else(|| NetworkError::InvalidMessage("Missing address".to_string()))?;

//         Ok(NodeInfo {
//             node_id: node_id.to_string(),
//             node_type: NodeType::Development, // Default
//             tetrahedral_position: TetrahedralPosition::Development,
//             capabilities: value["capabilities"]
//                 .as_array()
//                 .map(|caps| {
//                     caps.iter()
//                         .filter_map(|c| c.as_str().map(|s| s.to_string()))
//                         .collect()
//                 })
//                 .unwrap_or_default(),
//             endpoint: Some(address.to_string()),
//         })
//     }

//     /// Serialize message for network transmission
//     fn serialize_message(node_info: &NodeInfo, message: &SacredMessage) -> NetworkResult<Vec<u8>> {
//         let envelope = serde_json::json!({
//             "node_info": node_info,
//             "message": message
//         });

//         serde_json::to_vec(&envelope).map_err(NetworkError::SerializationError)
//     }

//     /// Deserialize message from network
//     fn deserialize_message(data: &[u8]) -> NetworkResult<(NodeInfo, SacredMessage)> {
//         let envelope: serde_json::Value =
//             serde_json::from_slice(data).map_err(NetworkError::SerializationError)?;

//         let node_info: NodeInfo = serde_json::from_value(envelope["node_info"].clone())
//             .map_err(NetworkError::SerializationError)?;

//         let message: SacredMessage = serde_json::from_value(envelope["message"].clone())
//             .map_err(NetworkError::SerializationError)?;

//         Ok((node_info, message))
//     }

//     /// Send message to a specific TCP connection
//     async fn send_to_connection(&self, peer_id: &str, data: &[u8]) -> NetworkResult<()> {
//         let connections_guard = self.connections.read().await;
//         let connection = connections_guard.get(peer_id).cloned();

//         if let Some(conn) = connection {
//             let mut stream_guard = conn.stream.write().await;
//             stream_guard
//                 .write_all(data)
//                 .await
//                 .map_err(|e| NetworkError::TransportError(e.into()))?;
//             stream_guard
//                 .flush()
//                 .await
//                 .map_err(|e| NetworkError::TransportError(e.into()))?;
//             Ok(())
//         } else {
//             Err(NetworkError::ConnectionError(format!(
//                 "No connection to peer {}",
//                 peer_id
//             )))
//         }
//     }
// }

// #[async_trait]
// impl GeometricTransport for EthernetTransport {
//     type Config = EthernetConfig;
//     type Address = EthernetAddress;

//     async fn initialize(&mut self, config: Self::Config) -> NetworkResult<()> {
//         info!(
//             "🚀 Initializing Ethernet transport with config: {:?}",
//             config
//         );

//         // Set local address
//         self.local_address = Some(EthernetAddress {
//             socket_addr: config.bind_address,
//             node_id: format!("eth-{}", config.bind_address.port()),
//         });

//         // Start TCP listener
//         self.start_tcp_listener(&config).await?;

//         // Start discovery service
//         self.start_discovery_service(&config).await?;

//         self.config = Some(config);

//         {
//             let mut running = self.is_running.write().await;
//             *running = true;
//         }

//         {
//             let mut health = self.health.write().await;
//             health.is_connected = true;
//         }

//         info!("✅ Ethernet transport initialized successfully");
//         Ok(())
//     }

//     async fn send_message(
//         &self,
//         message: SacredMessage,
//         targets: Vec<Self::Address>,
//         priority: MessagePriority,
//     ) -> NetworkResult<()> {
//         debug!(
//             "📤 Sending message to {} targets with priority {:?}",
//             targets.len(),
//             priority
//         );

//         // Serialize message
//         let node_info = NodeInfo {
//             node_id: self.local_address.as_ref().unwrap().node_id.clone(),
//             node_type: NodeType::Development,
//             tetrahedral_position: TetrahedralPosition::Development,
//             capabilities: vec!["ethernet".to_string()],
//             endpoint: Some(self.local_address.as_ref().unwrap().socket_addr.to_string()),
//         };

//         let data = Self::serialize_message(&node_info, &message)?;

//         // Send to each target
//         for target in targets {
//             match self
//                 .send_to_connection(&target.socket_addr.to_string(), &data)
//                 .await
//             {
//                 Ok(_) => debug!("✅ Message sent to {}", target),
//                 Err(e) => warn!("🚫 Failed to send to {}: {}", target, e),
//             }
//         }

//         Ok(())
//     }

//     async fn broadcast_message(
//         &self,
//         message: SacredMessage,
//         priority: MessagePriority,
//     ) -> NetworkResult<()> {
//         debug!("📢 Broadcasting message with priority {:?}", priority);

//         let connections = self.connections.read().await;
//         let targets: Vec<_> = connections.keys().cloned().collect();
//         drop(connections);

//         if targets.is_empty() {
//             warn!("📢 No connections available for broadcast");
//             return Ok(());
//         }

//         let node_info = NodeInfo {
//             node_id: self.local_address.as_ref().unwrap().node_id.clone(),
//             node_type: NodeType::Development,
//             tetrahedral_position: TetrahedralPosition::Development,
//             capabilities: vec!["ethernet".to_string()],
//             endpoint: Some(self.local_address.as_ref().unwrap().socket_addr.to_string()),
//         };

//         let data = Self::serialize_message(&node_info, &message)?;

//         let num_targets = targets.len();
//         for target in targets {
//             if let Err(e) = self.send_to_connection(&target, &data).await {
//                 warn!("🚫 Broadcast failed to {}: {}", target, e);
//             }
//         }

//         info!("📢 Broadcast completed to {} peers", num_targets);
//         Ok(())
//     }

//     async fn discover_peers(&self) -> NetworkResult<Vec<(NodeInfo, Self::Address)>> {
//         let connections = self.connections.read().await;
//         let mut peers = Vec::new();

//         for (addr_str, _conn) in connections.iter() {
//             if let Ok(socket_addr) = addr_str.parse::<SocketAddr>() {
//                 let node_info = NodeInfo {
//                     node_id: format!("peer-{}", socket_addr.port()),
//                     node_type: NodeType::Development,
//                     tetrahedral_position: TetrahedralPosition::Development,
//                     capabilities: vec!["ethernet".to_string()],
//                     endpoint: Some(addr_str.clone()),
//                 };

//                 let address = EthernetAddress {
//                     socket_addr,
//                     node_id: node_info.node_id.clone(),
//                 };

//                 peers.push((node_info, address));
//             }
//         }

//         debug!("🔍 Discovered {} peers", peers.len());
//         Ok(peers)
//     }

//     async fn local_address(&self) -> NetworkResult<Self::Address> {
//         self.local_address
//             .clone()
//             .ok_or_else(|| NetworkError::ConnectionError("Transport not initialized".to_string()))
//     }

//     async fn health_check(&self) -> NetworkResult<TransportHealth> {
//         let connections = self.connections.read().await;
//         let peer_count = connections.len();
//         drop(connections);

//         let mut health = self.health.write().await;
//         health.peer_count = peer_count;

//         // Calculate golden ratio compliance (simplified)
//         health.golden_ratio_compliance = if peer_count > 0 {
//             // In a real implementation, this would calculate based on actual resource usage
//             0.95 // Placeholder high compliance
//         } else {
//             0.0
//         };

//         Ok(health.clone())
//     }

//     async fn shutdown(&mut self) -> NetworkResult<()> {
//         info!("🛑 Shutting down Ethernet transport");

//         {
//             let mut running = self.is_running.write().await;
//             *running = false;
//         }

//         // Abort tasks
//         if let Some(task) = self.discovery_task.take() {
//             task.abort();
//         }
//         if let Some(task) = self.heartbeat_task.take() {
//             task.abort();
//         }
//         if let Some(task) = self.message_handler_task.take() {
//             task.abort();
//         }

//         // Close connections
//         let mut connections = self.connections.write().await;
//         connections.clear();

//         {
//             let mut health = self.health.write().await;
//             health.is_connected = false;
//             health.peer_count = 0;
//         }

//         info!("✅ Ethernet transport shutdown complete");
//         Ok(())
//     }
// }
