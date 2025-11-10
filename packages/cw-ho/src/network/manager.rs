//! Minimal network manager implementation using commonware libraries
use commonware_codec::DecodeExt;
use ho_std::commonware::error::{CommonwareNetworkError, CommonwareNetworkResult};
use ho_std::llm::HoResult;
use ho_std::prelude::*;

use bytes::Bytes;
use commonware_cryptography::{ed25519, Signer};
use commonware_runtime::{tokio::Context, Metrics, Spawner};

use chrono;
use ho_std::traits::{NetworkMessageTrait, NodeIdentityTrait};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time;
use tracing::info;

use commonware_p2p::{authenticated, Manager, Recipients};

use governor::Quota;
use std::num::NonZeroU32;

use ho_std::commonware::identity::NodePubkey;

use crate::network::topology::NetworkTopology;
use crate::CwHoNetworkManifold;

/// Peer information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub public_key: NodePubkey,
    pub node_info: NodeInfo,
    pub last_seen: std::time::Instant,
}

impl CwHoNetworkManifold {
    /// Initialize and start the network
    pub async fn new(
        // config: NetworkConfig,
        identity: NodeIdentity,
        context: Context,
    ) -> Self {
        // Validate config
        // config.validate().map_err(|e| CommonwareNetworkError::P2P(e))?;
        if identity.private_key.is_none() {
            panic!("{}", CommonwareNetworkError::NodePrivKeyNotFound)
        }

        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // We'll initialize the network components inside a spawned task
        // For now, create empty containers that will be filled later
        let channel_senders = HashMap::new();
        let channel_receivers = HashMap::new();

        // Create topology
        let mut topology = NetworkTopology::new();

        // Add ourselves to the topology
        let our_info = NodeInfo {
            node_id: format!(
                "{:?}",
                identity
                    .public_key
                    .clone()
                    .expect("unidentifiable (public key not set)")
            ),
            node_type: identity.node_type.clone(),
            online: true,
            last_seen: chrono::Utc::now().timestamp() as u64,
        };
        topology.add_node(our_info);

        // Network will be started separately using start_network method
        // Background tasks and announcements will be handled there
        Self {
            identity,
            context,
            network_running: Arc::new(RwLock::new(false)),
            channel_senders,
            channel_receivers,
            peers: Arc::new(RwLock::new(HashMap::new())),
            topology: Arc::new(RwLock::new(topology)),
            event_tx,
            event_rx: Some(event_rx),
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the network using commonware runtime pattern
    pub async fn start_network(&mut self, config: &NetworkConfig) -> CommonwareNetworkResult<()> {
        // Get the private key
        let private_key = self
            .identity
            .private_key
            .as_ref()
            .ok_or_else(|| CommonwareNetworkError::NotInitialized)?
            .clone();

        // Parse listen address
        let listen_addr = self.identity.p2p_address();

        // Create commonware config using the signer from private key
        // First convert Vec<u8> to PrivateKey
        let private_key_bytes: &[u8] = private_key.as_slice();
        let ed25519_private_key = ed25519::PrivateKey::decode(private_key_bytes)
            .map_err(|_| CommonwareNetworkError::NodePrivKeyNotFound)?;
        let public_key = ed25519_private_key.public_key();
        let namespace = b"cw-ho-network";

        let commonware_config = authenticated::lookup::Config::recommended(
            ed25519_private_key,
            namespace,
            listen_addr,
            listen_addr, // dialable address same as listen for now
            10485760,
        );

        // Create network instance and oracle using our stored context
        let (mut network, mut oracle) = authenticated::lookup::Network::new(
            self.context.with_label("cw-ho-network"),
            commonware_config,
        );

        oracle
            .update(0, vec![(public_key, listen_addr)].into())
            .await;

        // Register channels and get senders/receivers
        let rate_quota = Quota::per_second(NonZeroU32::new(100).unwrap());
        let channels = config.channels.expect("channels does not exist");
        // Channel 0: Discovery
        let (_discovery_sender, _discovery_receiver) =
            network.register(0, rate_quota, channels.discovery_buffer.try_into().unwrap());

        // Channel 1: Tasks
        let (_task_sender, _task_receiver) =
            network.register(1, rate_quota, channels.task_buffer.try_into().unwrap());

        // Channel 2: State
        let (_state_sender, _state_receiver) =
            network.register(2, rate_quota, channels.state_buffer.try_into().unwrap());

        // Channel 3: Health
        let (_health_sender, _health_receiver) =
            network.register(3, rate_quota, channels.health_buffer.try_into().unwrap());

        // Start the network
        let network_handle = network.start();

        // Store network handle for future shutdown
        *self.network_running.write().await = true;

        // TODO: Store senders/receivers for use by the manager
        // TODO: Start background message processing tasks

        // Spawn background task to monitor network
        let shutdown = self.shutdown.clone();
        let network_running = self.network_running.clone();
        self.context.clone().spawn(move |_| async move {
            // Wait for shutdown signal
            while !*shutdown.read().await {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Shutdown network
            network_handle.abort();
            *network_running.write().await = false;
        });

        info!("🌐 Network started in background");
        Ok(())
    }

    /// Send a message to specific node type(s)
    pub async fn send_to_role(
        &mut self,
        role: NodeType,
        msg: NetworkMessage,
    ) -> CommonwareNetworkResult<()> {
        let peers = self.peers.read().await;
        let targets: Vec<ed25519::PublicKey> = peers
            .values()
            .filter(|p| &p.node_info.node_type == role.as_str_name())
            .map(|p| p.public_key.0.clone())
            .collect();

        if targets.is_empty() {
            return Err(CommonwareNetworkError::NoPeersForRole(
                role.as_str_name().to_string(),
            ));
        }

        let channel = msg.channel()?;
        let bytes = self.serialize_message(&msg)?;

        let sender = self.channel_senders.get_mut(&channel).ok_or_else(|| {
            CommonwareNetworkError::ChannelError(format!("Channel {} not found", channel))
        })?;

        use commonware_p2p::Sender;
        sender
            .send(Recipients::Some(targets), bytes, false)
            .await
            .map_err(|e| CommonwareNetworkError::P2P(format!("{:?}", e)))?;

        Ok(())
    }

    /// Broadcast a message to all peers
    pub async fn broadcast(&mut self, msg: NetworkMessage) -> CommonwareNetworkResult<()> {
        let channel = msg.channel()?;
        let bytes = self.serialize_message(&msg)?;

        // TODO: Use commonware-broadcast for efficient dissemination
        // if msg.is_broadcast() {
        //     let mut broadcast = self.broadcast.write().await;
        //     // TODO: Implement broadcast integration
        // }

        let sender = self.channel_senders.get_mut(&channel).ok_or_else(|| {
            CommonwareNetworkError::ChannelError(format!("Channel {} not found", channel))
        })?;

        use commonware_p2p::Sender;
        sender
            .send(Recipients::All, bytes, false)
            .await
            .map_err(|e| CommonwareNetworkError::P2P(format!("{:?}", e)))?;

        Ok(())
    }

    /// Request data with timeout
    pub async fn request(
        &mut self,
        peer: ed25519::PublicKey,
        req: NetworkMessage,
        timeout: Duration,
    ) -> CommonwareNetworkResult<NetworkMessage> {
        let msgtype = &req.message_type;

        let _request_id = match &msgtype.clone().expect("always will have MessageType") {
            MessageType::Request(r) => &r.request_id,
            _ => &uuid::Uuid::new_v4().to_string(),
        };
        // TODO: Register with collector
        // let mut collector = self.collector.write().await;
        // TODO: Implement collector integration for request-response

        // Send request
        let channel = req.channel()?;
        let bytes = self.serialize_message(&req)?;

        let sender = self.channel_senders.get_mut(&channel).expect("yuh");

        use commonware_p2p::Sender;
        sender.send(Recipients::One(peer), bytes, true).await?;

        // Wait for response with timeout
        tokio::time::timeout(timeout, async {
            // TODO: Wait for response using collector
            Err(CommonwareNetworkError::CollectorTimeout)
        })
        .await
        .map_err(|_| CommonwareNetworkError::CollectorTimeout)?
    }

    /// Get current network topology
    pub async fn get_topology(&self) -> NetworkTopology {
        self.topology.read().await.clone()
    }

    /// Subscribe to network events
    pub fn subscribe(&mut self) -> mpsc::UnboundedReceiver<NetworkEvent> {
        self.event_rx.take().expect("Event receiver already taken")
    }

    /// Announce this node to the network
    async fn announce_node(&mut self) -> CommonwareNetworkResult<()> {
        let msg = MessageType::NodeAnnounce(NodeAnnounce {
            node_id: hex::encode(&self.identity.public_key.clone().expect("no pubkey set yet")),
            role: NodeType::from_str_name(&self.identity.node_type.clone())
                .expect("always have a valid node type")
                .into(),
            capabilities: vec!["minimal".to_string()], // Simplified capabilities
            load_factor: 0.0.to_string(),
        });

        self.broadcast(NetworkMessage {
            message_type: Some(msg),
        })
        .await
    }

    /// Start background tasks
    async fn start_background_tasks(&mut self) {
        // Start message receivers for each channel
        for channel in 0..4 {
            if let Some(receiver) = self.channel_receivers.remove(&channel) {
                self.spawn_channel_handler(channel, receiver);
            }
        }

        // Start periodic tasks
        self.spawn_periodic_tasks();
    }

    /// Spawn handler for a specific channel
    fn spawn_channel_handler(
        &self,
        channel: u8,
        mut receiver: authenticated::lookup::Receiver<ed25519::PublicKey>,
    ) {
        let peers = self.peers.clone();
        let _topology = self.topology.clone();
        let event_tx = self.event_tx.clone();
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            while !*shutdown.read().await {
                use commonware_p2p::Receiver;
                match receiver.recv().await {
                    Ok((peer_key, bytes)) => {
                        // Process message
                        if let Ok(msg) = Self::deserialize_message(&bytes) {
                            // Update peer info
                            if let Some(peer_info) = peers.read().await.get(peer_key.borrow()) {
                                let mut peer_info = peer_info.clone();
                                peer_info.last_seen = std::time::Instant::now();
                                peers.write().await.insert(peer_key.clone(), peer_info);
                            }
                            // Send event
                            let _ = event_tx.send(NetworkEvent {
                                event_type: Some(EventType::MessageReceived(MessageReceived {
                                    from: peer_key.to_vec(),
                                    message: Some(msg),
                                    channel: channel.into(),
                                })),
                            });
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(NetworkEvent {
                            event_type: Some(EventType::Error(NetworkError {
                                error: format!("Channel {} error: {:?}", channel, e),
                            })),
                        });
                    }
                }
            }
        });
    }

    /// Spawn periodic maintenance tasks
    fn spawn_periodic_tasks(&self) {
        let peers = self.peers.clone();
        let topology = self.topology.clone();
        let event_tx = self.event_tx.clone();
        let _identity = self.identity.clone();
        let shutdown = self.shutdown.clone();

        // Periodic health check
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(30));

            while !*shutdown.read().await {
                interval.tick().await;

                // Clean up stale peers
                let now = std::time::Instant::now();
                let mut peers_write = peers.write().await;
                let stale_peers: Vec<ed25519::PublicKey> = peers_write
                    .iter()
                    .filter(|(_, info)| {
                        now.duration_since(info.last_seen) > Duration::from_secs(120)
                    })
                    .map(|(key, _)| key.clone())
                    .collect();

                for peer_key in stale_peers {
                    if let Some(peer_info) = peers_write.remove(&peer_key) {
                        let mut topo = topology.write().await;
                        topo.remove_node(&peer_info.node_info.node_id);

                        let _ = event_tx.send(NetworkEvent {
                            event_type: Some(EventType::PeerDisconnected(PeerDisconnected {
                                peer_id: peer_key.to_vec(),
                                reason: "Timeout".to_string(),
                            })),
                        });
                    }
                }
            }
        });
    }

    /// Serialize a network message
    fn serialize_message(&self, msg: &NetworkMessage) -> HoResult<Bytes> {
        let json = serde_json::to_vec(msg)?;
        Ok(Bytes::from(json))
    }

    /// Deserialize a network message
    fn deserialize_message(bytes: &Bytes) -> HoResult<NetworkMessage> {
        let msg = serde_json::from_slice(bytes)?;
        Ok(msg)
    }

    /// Shutdown the network manager
    pub async fn shutdown(&mut self) {
        // Set shutdown flag to stop all background tasks
        *self.shutdown.write().await = true;

        // Mark network as not running
        *self.network_running.write().await = false;

        // The commonware network will see the shutdown flag and gracefully stop
        // All spawned tasks (channel handlers, periodic tasks) will also see the flag and exit
    }
}
