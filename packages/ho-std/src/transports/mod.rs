//! Transport Implementations for Sacred Geometric Networking
//!
//! This module contains transport-specific implementations of the GeometricTransport trait,
//! enabling the network layer to operate over different physical communication mediums
//! while maintaining sacred geometric principles.

// pub mod bluetooth_mesh;
// pub mod ethernet;
pub mod ssh;
pub mod websocket;

// Re-export main transport types
// pub use ethernet::{EthernetAddress, EthernetConfig, EthernetTransport};
// pub use bluetooth_mesh::{BluetoothMeshTransport, BluetoothMeshConfig, BluetoothMeshAddress};
// pub use websocket::{WebSocketAddress, WebSocketConfig, WebSocketTransport};
// use websocket::{WebSocketConfig, WebSocketTransport};

use serde::{Deserialize, Serialize};

/// Transport type identifier for configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransportType {
    Ethernet,
    // BluetoothMesh,
    WebSocket,
    Custom(String),
}

/// Generic transport configuration that can be used to create any transport type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub transport_type: TransportType,
    pub config_json: serde_json::Value,
}

impl TransportConfig {
    // pub fn ethernet(config: EthernetConfig) -> Self {
    //     Self {
    //         transport_type: TransportType::Ethernet,
    //         config_json: serde_json::to_value(config).expect("Failed to serialize EthernetConfig"),
    //     }
    // }

    // pub fn bluetooth_mesh(config: BluetoothMeshConfig) -> Self {
    //     Self {
    //         transport_type: TransportType::BluetoothMesh,
    //         config_json: serde_json::to_value(config).expect("Failed to serialize BluetoothMeshConfig"),
    //     }
    // }

    // pub fn websocket(config: WebSocketConfig) -> Self {
    //     Self {
    //         transport_type: TransportType::WebSocket,
    //         config_json: serde_json::to_value(config).expect("Failed to serialize WebSocketConfig"),
    //     }
    // }
}

/// Factory function to create transport instances - simple like commonware
// pub fn create_ethernet_transport(config: EthernetConfig) -> EthernetTransport {

//     // Configuration will be applied during initialize()
//     EthernetTransport::new()
// }

/// Factory function for WebSocket transport
// pub fn create_websocket_transport(config: WebSocketConfig) -> WebSocketTransport {

//     // Configuration will be applied during initialize()
//     WebSocketTransport::new()
// }

/// Generic transport address that can hold any transport-specific address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportAddress {
    // Ethernet(EthernetAddress),
    // BluetoothMesh(BluetoothMeshAddress),
    // WebSocket(WebSocketAddress),
    Custom(String),
}

impl std::fmt::Display for TransportAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // TransportAddress::Ethernet(addr) => write!(f, "eth:{}", addr),
            // TransportAddress::BluetoothMesh(addr) => write!(f, "bt:{}", addr),
            // TransportAddress::WebSocket(addr) => write!(f, "ws:{}", addr),
            TransportAddress::Custom(addr) => write!(f, "custom:{}", addr),
        }
    }
}

// Test module
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // #[test]
    // fn test_create_ethernet_transport() {
    //     let config = EthernetConfig {
    //         bind_address: "127.0.0.1:8000".parse().unwrap(),
    //         multicast_address: "224.0.0.251:8001".parse().unwrap(),
    //         max_connections: 100,
    //         connection_timeout: Duration::from_secs(10),
    //         discovery_interval: Duration::from_secs(2),
    //         keepalive_interval: Duration::from_secs(30),
    //     };
    //     let _transport = create_ethernet_transport(config);
    //     // Just test that it creates without panicking
    //     assert!(true);
    // }
}
