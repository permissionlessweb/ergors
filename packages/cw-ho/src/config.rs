use crate::error::{CwHoError, Result};
use crate::traits::Wrap;
use crate::{CwHoConfig, CwHoLlmRouterConfig, CwHoNodeIdentity, CwHoStorageConfig};

use anyhow::Context;
use commonware_cryptography::ed25519;
use ho_std::llm::{HoError, HoResult};
use ho_std::orchestrate::HoConfig;
use ho_std::prelude::*;

use ho_std::traits::file_ops::ConfigLoaderTrait;
use ho_std::traits::{HoConfigTrait, LLMRouterConfigTrait, NetworkConfigTrait, NodeIdentityTrait};
use ho_std::utils::DefaultFileOps;

use crate::network::config::CwHoNetworkConfig;

// Network trait implementations for proto types
impl HoConfigTrait for CwHoConfig {
    type NetworkConfig = CwHoNetworkConfig;
    type Identity = CwHoNodeIdentity;
    type StorageConfig = CwHoStorageConfig;
    type LLMConfig = CwHoLlmRouterConfig;
    type HoConfigResult = Result<()>;

    fn network(&self) -> &Self::NetworkConfig {
        todo!()
    }

    fn identity(&self) -> &Self::Identity {
        CwHoNodeIdentity::wrap_ref(self.identity.as_ref().expect("ego is useful in moderation"))
    }

    fn storage(&self) -> &Self::StorageConfig {
        CwHoStorageConfig::wrap_ref(self.storage.as_ref().expect("ego is useful in moderation"))
    }

    fn llm(&self) -> &Self::LLMConfig {
        CwHoLlmRouterConfig::wrap_ref(self.llm.as_ref().expect("ego is useful in moderation"))
    }

    fn validate(&self) -> Self::HoConfigResult {
        self.network().validate()?;
        self.llm().validate()?;
        // Note: identity validation should be handled through the trait
        // self.identity().validate()?;
        //     // validate llm config
        //     // - self.llm().providers
        //     //      a. there should be atleast one provider
        //     // - self.llm().providers.expect("yes").cfg
        //     //    b. there should be atleast one entity: self.llm().providers.expect("yes").cfg
        //     // should be one of the available models
        //     self.llm().providers.expect("yes").cfg[0].default_model
        //     // should be one of the expected strategy enums
        //     self.llm().providers.expect("yes").cfg[0].default_strategy
        //     // should be a valid https url string
        //     self.llm().providers.expect("yes").cfg[0].endpoint
        //     // max_retries should never be more than 10
        //     self.llm().providers.expect("yes").cfg[0].max_retries
        //     // should be atleast one
        //     self.llm().providers.expect("yes").cfg[0].models
        //     // should never be more than 10 mins
        //     self.llm().providers.expect("yes").cfg[0].timeout_seconds
        //     // - self.llm().api_keys_file
        //     //      a. shpold exist and all llm entities should have an associeated api key in use
        //     //      b. max_retries should never be more than 10

        //     // validate network config
        //     // should  never be more than 10
        //     self.network().max_peers()
        //     // should never be more than 5 mins in ms
        //     self.network().connection_timeout_ms()

        //     self.network().listen_address()
        //     self.network().listen_port()
        //     // validate identity config
        //     self.identity().host
        //     // should be one of valid OS types
        //     self.identity().os()
        //     // should be one of valid node types
        //     self.identity().node_type
        // // _________ _________ _________ _________ _________
        //     // TODO: if no private or public key, create new one and add to the api-key.toml file used
        //     // should always have a private key
        //     self.identity().private_key
        //     // should always have a public key
        //     self.identity().public_key

        // _________ _________ _________ _________ _________
        // validate storage config

        Ok(())
    }

    fn set_network_config(&mut self, config: Self::NetworkConfig) {
        self.0.network = Some((*config).clone())
    }

    fn set_identity(&mut self, identity: Self::Identity) {
        self.0.identity = Some(identity.unwrap());
    }

    fn set_storage_config(&mut self, config: Self::StorageConfig) {
        self.0.storage = Some(config.unwrap())
    }

    fn set_llm_config(&mut self, config: Self::LLMConfig) {
        self.0.llm = Some(config.unwrap());
    }

    fn default() -> Self {
        Self(HoConfig {
            network: Some(NetworkConfig::default()),
            identity: Some(NodeIdentity::default()),
            storage: Some(StorageConfig::default()),
            llm: Some(LlmRouterConfig::default()),
        })
    }

    fn file_path(&self) -> &str {
        todo!()
    }

    fn from_file(path: &str) -> HoResult<Self>
    where
        Self: Sized,
    {
        Ok(DefaultFileOps::from_toml_file(path)?)
    }

    fn load<P: AsRef<std::path::Path> + std::fmt::Display>(path: P) -> HoResult<Self>
    where
        Self: Sized,
    {
        let contents = std::fs::read_to_string(&path).map_err(|e| {
            HoError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "ho config file not found: {}. hint: run 'init' to create new config",
                    path.to_string()
                ),
            ))
        })?;
        Ok(toml::from_str(&contents)?)
    }

    fn save<P: AsRef<std::path::Path>>(&self, path: P) -> HoResult<()> {
        let contents = toml::to_string_pretty(&self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

impl NetworkConfigTrait for CwHoConfig {
    fn from_toml(&self) -> toml::Table {
        todo!()
    }

    fn validate(&self) -> ho_std::commonware::error::CommonwareNetworkResult<()> {
        todo!()
    }

    fn bootstrap_peers(&self) -> &[String] {
        todo!()
    }

    fn listen_port(&self) -> u32 {
        todo!()
    }

    fn listen_address(&self) -> &str {
        todo!()
    }

    fn max_peers(&self) -> u32 {
        todo!()
    }

    fn connection_timeout_ms(&self) -> u32 {
        todo!()
    }

    fn is_discovery_enabled(&self) -> bool {
        todo!()
    }
}

impl LLMRouterConfigTrait for CwHoLlmRouterConfig {
    fn default_provider(&self) -> &str {
        todo!()
    }

    fn timeout_seconds(&self) -> u32 {
        todo!()
    }

    fn retry_attempts(&self) -> u32 {
        todo!()
    }

    fn remove_provider(&mut self, name: &str) {
        todo!()
    }

    fn set_default_provider(&mut self, name: String) {
        todo!()
    }

    fn set_timeout(&mut self, timeout: u32) {
        todo!()
    }

    fn set_retry_attempts(&mut self, attempts: u32) {
        todo!()
    }
    fn validate(&self) -> HoResult<()> {
        Ok(())
    }
}

impl NodeIdentityTrait for CwHoNodeIdentity {
    type HostOS = HostOs;
    type NodeType = NodeType;
    type PrivateKey = ed25519::PrivateKey;
    type PublicKey = ed25519::PublicKey;

    fn new_node(
        host: String,
        p2p_port: u16,
        api_port: u16,
        user: String,
        os: Self::HostOS,
        ssh_port: u16,
        node_type: Self::NodeType,
        private_key: Self::PrivateKey,
    ) -> Self {
        todo!()
    }

    fn generate_keypair<R: rand::RngCore + rand::CryptoRng>(
        &mut self,
        rng: &mut R,
    ) -> ho_std::commonware::error::CommonwareNetworkResult<()> {
        todo!()
    }

    fn set_keypair(&mut self, private_key: Self::PrivateKey) {
        todo!()
    }

    fn p2p_identity(&self) -> String {
        todo!()
    }

    fn p2p_address(&self) -> std::net::SocketAddr {
        todo!()
    }

    fn api_address(&self) -> String {
        todo!()
    }

    fn display_id(&self) -> String {
        todo!()
    }

    fn get_private_key_from_env() -> ho_std::commonware::identity::NodePrivKey {
        todo!()
    }

    fn private_key_from_hex(hex_string: &str) -> Option<ho_std::commonware::identity::NodePrivKey> {
        todo!()
    }
}
