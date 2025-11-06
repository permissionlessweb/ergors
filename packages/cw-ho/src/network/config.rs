//! Network configuration types
use ho_std::commonware::error::{CommonwareNetworkError, CommonwareNetworkResult};
use ho_std::prelude::*;
use serde::{Deserialize, Serialize};

// Newtype wrapper for NetworkConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CwHoNetworkConfig(pub NetworkConfig);

impl ho_std::traits::NetworkConfigTrait for CwHoNetworkConfig {
    /// Validate the network config.
    fn validate(&self) -> CommonwareNetworkResult<()> {
        if self.channels.is_none() {};
        if self.connection_timeout_ms < 100 {}
        if self.listen_port == 0 {};
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
        todo!()
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
        todo!()
    }
}

impl std::ops::Deref for CwHoNetworkConfig {
    type Target = NetworkConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for CwHoNetworkConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
