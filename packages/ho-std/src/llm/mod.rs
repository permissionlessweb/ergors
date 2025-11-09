mod cost;
mod prompt;
use crate::prelude::LlmEntity;
use anyhow::Result;
pub use cost::*;
pub use prompt::*;

use {
    crate::{
        constants::*,
        orchestrate::ModelSelectionStrategy,
        prelude::{LlmModel, LlmRouterConfig},
        traits::LlmModelTrait,
    },
    camino::Utf8Path,
};

impl LlmRouterConfig {
    pub fn new(data_dir: &Utf8Path) -> Self {
        let mut neurons = Self::default();
        neurons.api_keys_file = data_dir.join(LLM_API_KEYS_FILE).to_string();
        neurons.default_entity = LlmModel::AkashChat as u32;
        neurons.default_strategy = ModelSelectionStrategy::Unspecified.into();
        neurons.entities = vec![LlmModel::AkashChat.default_entity()];
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
        match self.entities.iter().position(|e| e.name == e_name) {
            Some(e) => {
                self.entities.remove(e);
            }
            None => {}
        };
        Ok(())
    }
}

impl LlmModelTrait for LlmModel {
    /// (default_model, all_available_models)
    fn models(&self) -> (String, Vec<String>) {
        let all: Vec<String> = match self {
            LlmModel::AkashChat => AKASH_CHAT_MODELS,
            LlmModel::KimiResearch => KIMI_RESEARCH_MODELS,
            LlmModel::Grok => GROK_MODELS,
            LlmModel::OllamaLocal => OLLAMA_LOCAL_MODELS,
            LlmModel::OpenAi => OPENAI_MODELS,
            LlmModel::Anthropic => ANTHROPIC_MODELS,
            LlmModel::Custom { .. } => EXTERNAL_MODELS,
        }
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        (all.first().cloned().unwrap_or_default(), all)
    }
    fn default_base_url(&self) -> String {
        match self {
            LlmModel::AkashChat => AKASH_CHAT_BASE_URL.to_string(),
            LlmModel::KimiResearch => KIMI_RESEARCH_BASE_URL.to_string(),
            LlmModel::Grok => GROK_BASE_URL.to_string(),
            LlmModel::OpenAi => OPENAI_BASE_URL.to_string(),
            LlmModel::Anthropic => ANTHROPIC_BASE_URL.to_string(),
            // Any other variant (e.g., `Custom`) gets an empty string.
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
