pub mod env;

use crate::commonware::error::{CommonwareNetworkError, CommonwareNetworkResult};
use crate::prelude::*;
use crate::types::cw_ho::network::v1::ChannelConfig;

impl crate::traits::NetworkConfigTrait for NetworkConfig {
    /// Validate the network config.
    fn validate(&self) -> CommonwareNetworkResult<()> {
        if self.channels.is_none() {};
        if self.connection_timeout_ms < 100 {}
        // Basic validation - could be expanded
        if self.listen_port == 0 {
            return Err(CommonwareNetworkError::ConfigError(
                "Listen port must be non-zero".to_string(),
            ));
        }
        if self.listen_address == "" {};
        if self.max_peers > 10 {};

        if let Some(l) = self.limits {
            if l.connection_timeout < 100 || l.max_message_size > 100000000 || l.max_peers > 10 {
                return Err(CommonwareNetworkError::ConfigError(
                    "Issue with nodes ports, please update".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn bootstrap_peers(&self) -> &[String] {
        &self.bootstrap_peers
    }

    fn listen_port(&self) -> u32 {
        self.listen_port
    }

    fn listen_address(&self) -> &str {
        &self.listen_address
    }

    fn max_peers(&self) -> u32 {
        self.max_peers
    }

    fn connection_timeout_ms(&self) -> u32 {
        self.connection_timeout_ms
    }

    fn is_discovery_enabled(&self) -> bool {
        self.enable_discovery
    }

    fn from_toml(&self) -> toml::Table {
        // Basic TOML representation
        let mut table = toml::Table::new();

        table.insert(
            "node_type".to_string(),
            toml::Value::String(self.node_type.clone()),
        );
        table.insert(
            "listen_port".to_string(),
            toml::Value::Integer(self.listen_port as i64),
        );
        table.insert(
            "listen_address".to_string(),
            toml::Value::String(self.listen_address.clone()),
        );
        table.insert(
            "max_peers".to_string(),
            toml::Value::Integer(self.max_peers as i64),
        );
        table.insert(
            "enable_discovery".to_string(),
            toml::Value::Boolean(self.enable_discovery),
        );
        table
    }

    fn new() -> Self {
        Self {
            node_type: NodeType::Executor.as_str_name().into(),
            bootstrap_peers: Default::default(),
            known_peers: Default::default(),
            listen_port: 69699,
            listen_address: "127.0.0.1".to_owned(),
            max_peers: 5,
            connection_timeout_ms: 1313131313,
            enable_discovery: true,
            limits: Default::default(),
            channels: Some(ChannelConfig::new()),
        }
    }
}

impl ChannelConfig {
    fn new() -> Self {
        Self {
            discovery_buffer: 100,
            task_buffer: 1000,
            state_buffer: 500,
            health_buffer: 50,
        }
    }
}
