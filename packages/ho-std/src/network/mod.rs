use crate::error::HoResult;
use crate::llm::HoError;
use crate::prelude::{MessageType, NetworkConfig, NetworkMessage, NetworkTopology, Response};
use crate::traits::Message as _;
use crate::traits::{NetworkConfigTrait, NetworkMessageTrait, NetworkTopologyTrait};

use crate::commonware::error::{CommonwareNetworkError, CommonwareNetworkResult};

impl NetworkTopologyTrait for NetworkTopology {
    type NodeInfo = crate::types::cw_ho::types::v1::NodeInfo;
    type Connection = crate::types::cw_ho::types::v1::Connection;

    fn nodes(&self) -> &[Self::NodeInfo] {
        &self.nodes
    }

    fn connections(&self) -> &[Self::Connection] {
        &self.connections
    }

    fn online_nodes(&self) -> Vec<&Self::NodeInfo> {
        self.nodes.iter().filter(|node| node.online).collect()
    }

    fn add_node(&mut self, node: Self::NodeInfo) {
        self.nodes.push(node);
    }

    fn remove_node(&mut self, node_id: &str) {
        self.nodes.retain(|node| node.node_id != node_id);
    }

    fn add_connection(&mut self, connection: Self::Connection) {
        self.connections.push(connection);
    }

    fn remove_connection(&mut self, from_node: &str, to_node: &str) {
        self.connections
            .retain(|conn| !(conn.from_node_id == from_node && conn.to_node_id == to_node));
    }
}

impl NetworkMessageTrait for NetworkMessage {
    type MessageType = MessageType;
    type ResultType = HoResult<()>;

    fn message_type(&self) -> &Self::MessageType {
        // This is a simplified implementation - would need actual message type enum
        static DEFAULT: std::sync::LazyLock<MessageType> =
            std::sync::LazyLock::new(|| MessageType::Response(Response::default()));
        &DEFAULT
    }

    fn to_bytes(&self) -> HoResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode(&mut buf)
            .map_err(|e| crate::error::HoError::Serialization(e.to_string()))?;
        Ok(buf)
    }

    fn from_bytes(bytes: &[u8]) -> HoResult<Self> {
        Self::decode(bytes).map_err(|e| crate::error::HoError::DeSerialization(e.to_string()))
    }

    fn channel(&self) -> HoResult<u8> {
        todo!()
    }
}

/// Network helper functions
pub struct NetworkUtils;

impl NetworkUtils {
    /// Parse address string into host and port
    pub fn parse_address(address: &str) -> HoResult<(String, u16)> {
        let parts: Vec<&str> = address.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(HoError::Network(format!(
                "Invalid address format: {}",
                address
            )));
        }

        let port = parts[0]
            .parse::<u16>()
            .map_err(|_| HoError::Network(format!("Invalid port in address: {}", address)))?;

        Ok((parts[1].to_string(), port))
    }

    /// Format host and port into address string
    pub fn format_address(host: &str, port: u16) -> String {
        format!("{}:{}", host, port)
    }

    /// Validate port number is in valid range
    pub fn validate_port(port: u32) -> HoResult<u16> {
        if port == 0 || port > 65535 {
            return Err(HoError::Network(format!("Invalid port number: {}", port)));
        }
        Ok(port as u16)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_network_utils() {
        let (host, port) = NetworkUtils::parse_address("127.0.0.1:8080").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8080);

        let address = NetworkUtils::format_address(&host, port);
        assert_eq!(address, "127.0.0.1:8080");
    }
}
