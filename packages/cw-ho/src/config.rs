use crate::traits::Wrap;
use crate::{CwHoLlmRouterConfig, ErgorsConfig};

use camino::Utf8Path;
use ho_std::llm::{HoError, HoResult};
use ho_std::types::ergors::{network::v1::*, orch::v1::*, storage::v1::*};

use ho_std::traits::file_ops::ConfigLoaderTrait;
use ho_std::traits::{HoConfigTrait, LLMRouterConfigTrait, NetworkConfigTrait, NodeIdentityTrait};
use ho_std::utils::DefaultFileOps;

// Network trait implementations for proto types
impl HoConfigTrait for ErgorsConfig {
    type Identity = NodeIdentity;
    type StorageConfig = StorageConfig;
    type LLMConfig = CwHoLlmRouterConfig;
    type HoConfigResult = HoResult<()>;

    fn new(home: &Utf8Path) -> Self {
        Self(HoConfig {
            network: Some(NetworkConfig::new()),
            identity: Some(NodeIdentity::new()),
            storage: Some(StorageConfig::new(home)),
            llm: Some(LlmRouterConfig::new(home)),
            home: home.as_str().into(),
        })
    }

    fn network(&self) -> &NetworkConfig {
        self.network.as_ref().expect("network config should exist")
    }

    fn identity(&self) -> &Self::Identity {
        self.identity
            .as_ref()
            .expect("ego is useful in moderation (cannot access node identity")
    }

    fn storage(&self) -> &Self::StorageConfig {
        self.storage
            .as_ref()
            .expect("memories seed ego (cannot find storage config)")
    }

    fn llm(&self) -> &Self::LLMConfig {
        CwHoLlmRouterConfig::wrap_ref(self.llm.as_ref().expect("ego is useful in moderation"))
    }

    fn validate(&self) -> Self::HoConfigResult {
        self.network().validate()?;
        self.llm().validate()?;
        // self.storage.validate
        // self.identity.validate
        Ok(())
    }

    fn set_network_config(&mut self, config: NetworkConfig) {
        self.0.network = Some(config)
    }

    fn set_identity(&mut self, identity: Self::Identity) {
        self.0.identity = Some(identity);
    }

    fn set_storage_config(&mut self, config: Self::StorageConfig) {
        self.0.storage = Some(config)
    }

    fn set_llm_config(&mut self, config: Self::LLMConfig) {
        self.0.llm = Some(config.unwrap());
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
        // validate each llm provider has keys defined in .env file
        for llm in &self.0.entities {}
        Ok(())
    }
}
