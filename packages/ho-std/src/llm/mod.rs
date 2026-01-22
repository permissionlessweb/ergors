mod api_keys;
mod cost;
mod encrypted_keys;
mod joints;
mod macros;
mod prompt;
mod providers;
mod router;
pub mod state_ext;
use anyhow::Result;

// pub use macros::{find_entity, registered_entities, LlmEntityDescriptor};
pub use api_keys::*;
pub use cost::*;
pub use encrypted_keys::*;
pub use joints::*;
pub use prompt::*;
pub use providers::*;
pub use router::*;
pub use state_ext::{StateReadExt, StateWriteExt};

use {
    crate::{constants::*, orchestrate::*, traits::LlmModelTrait},
    camino::Utf8Path,
};

impl LlmRouterConfig {
    pub fn new(data_dir: &Utf8Path) -> Self {
        let mut neurons = Self::default();
        neurons.api_keys_file = data_dir.join(LLM_API_KEYS_FILE).to_string();
        neurons.default_entity = LlmModel::AkashMl as u32;
        neurons.default_strategy = ModelSelectionStrategy::Unspecified.into();
        neurons.timeout_seconds = 60;
        neurons.entities = vec![
            LlmModel::AkashMl.default_entity(),
            LlmModel::Grok.default_entity(),
            LlmModel::Anthropic.default_entity(),
            LlmModel::OpenAi.default_entity(),
        ];
        tracing::debug!("LlmRouterConfig: {:#?}", neurons);
        neurons
    }
    pub fn update_default_entity(&mut self, model: LlmModel) {
        self.default_entity = model as u32;
    }
    pub fn update_default_strategy(&mut self, strategy: ModelSelectionStrategy) {
        self.default_strategy = strategy.into();
    }
    pub fn add_entity(&mut self, entity: LlmEntity) {
        if !self.entities.contains(&entity) {
            self.entities.push(entity);
        }
    }
    pub fn remove_entity(&mut self, e_name: String) -> Result<()> {
        if let Some(e) = self.entities.iter().position(|e| e.name == e_name) {
            self.entities.remove(e);
        };
        Ok(())
    }
}

impl LlmModelTrait for LlmModel {
    /// (default_model, all_available_models)
    fn models(&self) -> (String, Vec<String>) {
        let all: Vec<String> = match self {
            LlmModel::AkashMl => AKASHML_MODELS,
            LlmModel::KimiResearch => KIMI_RESEARCH_MODELS,
            LlmModel::Grok => GROK_MODELS,
            LlmModel::OllamaLocal => OLLAMA_LOCAL_MODELS,
            LlmModel::OpenAi => OPENAI_MODELS,
            LlmModel::Anthropic => ANTHROPIC_MODELS,
            LlmModel::Custom => EXTERNAL_MODELS,
        }
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        (all.first().cloned().unwrap_or_default(), all)
    }
    fn default_base_url(&self) -> String {
        match self {
            LlmModel::AkashMl => AKASH_CHAT_BASE_URL.to_string(),
            LlmModel::KimiResearch => KIMI_RESEARCH_BASE_URL.to_string(),
            LlmModel::Grok => GROK_BASE_URL.to_string(),
            LlmModel::OpenAi => OPENAI_BASE_URL.to_string(),
            LlmModel::Anthropic => ANTHROPIC_BASE_URL.to_string(),
            _ => String::new(),
        }
    }
    fn default_entity(&self) -> LlmEntity {
        LlmEntity {
            name: self.as_str_name().into(),
            base_url: self.default_base_url(),
            models: self.models().1,
            default_model: self.models().0,
            priority: 1,
            enabled: true,
            default_strategy: ModelSelectionStrategy::Priority.into(),
            timeout_seconds: 696969,
            max_retries: 2,
        }
    }
}
