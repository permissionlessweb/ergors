//! SDL Template Management for Akash Deployments
//!
//! This module provides functionality for:
//! - Parsing SDL templates to detect variables
//! - Variable substitution with validation
//! - Creating ConfiguredSdl records for deployment
//! - Querying SDL templates from CosmWasm contracts

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{ConfiguredSdl, SdlVariable};
use pbjson_types::Timestamp;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::SystemTime;

#[cfg(feature = "cw")]
use {
    cnidarium::Storage,
    cosmwasm_std,
    ho_std::wasm::runtime::WasmRuntime,
};

/// Variable placeholder pattern: ${VAR_NAME} or ${VAR_NAME:default}
const VAR_PATTERN: &str = r"\$\{([A-Z_][A-Z0-9_]*)(:[^}]*)?\}";

/// Ollama inference provider SDL template
const OLLAMA_SDL_TEMPLATE: &str = r#"---
version: "2.0"

services:
  ollama:
    image: ollama/ollama:${OLLAMA_VERSION:latest}
    expose:
      - port: ${EXPOSE_PORT:11434}
        as: 80
        to:
          - global: true
    env:
      - OLLAMA_HOST=0.0.0.0
      - OLLAMA_MODELS=/root/.ollama/models

profiles:
  compute:
    ollama:
      resources:
        cpu:
          units: ${CPU:4}
        memory:
          size: ${MEMORY:16Gi}
        storage:
          - size: ${STORAGE:50Gi}
        gpu:
          units: ${GPU_COUNT:1}
          attributes:
            vendor:
              nvidia:
  placement:
    akash:
      pricing:
        ollama:
          denom: uakt
          amount: 10000

deployment:
  ollama:
    akash:
      profile: ollama
      count: ${REPLICAS:1}
"#;

/// vLLM inference provider SDL template
const VLLM_SDL_TEMPLATE: &str = r#"---
version: "2.0"

services:
  vllm:
    image: vllm/vllm-openai:${VLLM_VERSION:latest}
    expose:
      - port: ${EXPOSE_PORT:8000}
        as: 80
        to:
          - global: true
    env:
      - MODEL=${MODEL_NAME:meta-llama/Llama-2-7b-chat-hf}
      - MAX_MODEL_LEN=${MAX_MODEL_LEN:4096}
    args:
      - --model
      - ${MODEL_NAME:meta-llama/Llama-2-7b-chat-hf}
      - --tensor-parallel-size
      - "${TENSOR_PARALLEL:1}"

profiles:
  compute:
    vllm:
      resources:
        cpu:
          units: ${CPU:8}
        memory:
          size: ${MEMORY:32Gi}
        storage:
          - size: ${STORAGE:100Gi}
        gpu:
          units: ${GPU_COUNT:1}
          attributes:
            vendor:
              nvidia:
  placement:
    akash:
      pricing:
        vllm:
          denom: uakt
          amount: 15000

deployment:
  vllm:
    akash:
      profile: vllm
      count: ${REPLICAS:1}
"#;

/// TGI (Text Generation Inference) SDL template
const TGI_SDL_TEMPLATE: &str = r#"---
version: "2.0"

services:
  tgi:
    image: ghcr.io/huggingface/text-generation-inference:${TGI_VERSION:latest}
    expose:
      - port: ${EXPOSE_PORT:80}
        as: 80
        to:
          - global: true
    env:
      - MODEL_ID=${MODEL_NAME:meta-llama/Llama-2-7b-chat-hf}
      - HUGGING_FACE_HUB_TOKEN=${HF_TOKEN:}
      - MAX_INPUT_LENGTH=${MAX_INPUT_LENGTH:4096}
      - MAX_TOTAL_TOKENS=${MAX_TOTAL_TOKENS:8192}
    args:
      - --model-id
      - ${MODEL_NAME:meta-llama/Llama-2-7b-chat-hf}

profiles:
  compute:
    tgi:
      resources:
        cpu:
          units: ${CPU:8}
        memory:
          size: ${MEMORY:32Gi}
        storage:
          - size: ${STORAGE:100Gi}
        gpu:
          units: ${GPU_COUNT:1}
          attributes:
            vendor:
              nvidia:
  placement:
    akash:
      pricing:
        tgi:
          denom: uakt
          amount: 15000

deployment:
  tgi:
    akash:
      profile: tgi
      count: ${REPLICAS:1}
"#;

/// Common SDL variables for inference providers
pub mod common_vars {
    pub const CPU: &str = "CPU";
    pub const MEMORY: &str = "MEMORY";
    pub const STORAGE: &str = "STORAGE";
    pub const GPU_COUNT: &str = "GPU_COUNT";
    pub const GPU_VENDOR: &str = "GPU_VENDOR";
    pub const GPU_MODEL: &str = "GPU_MODEL";
    pub const IMAGE: &str = "IMAGE";
    pub const REPLICAS: &str = "REPLICAS";
    pub const EXPOSE_PORT: &str = "EXPOSE_PORT";
    pub const ENV_VARS: &str = "ENV_VARS";
    pub const API_KEY_SECRET: &str = "API_KEY_SECRET";
    pub const MODEL_NAME: &str = "MODEL_NAME";
}

/// SDL Template manager for variable extraction and substitution
pub struct SdlTemplateManager {
    var_regex: Regex,
}

impl SdlTemplateManager {
    pub fn new() -> Self {
        Self {
            var_regex: Regex::new(VAR_PATTERN).expect("Invalid regex pattern"),
        }
    }

    /// Extract all variable names from an SDL template
    pub fn extract_variables(&self, template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for cap in self.var_regex.captures_iter(template) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().to_string();
                if seen.insert(name.clone()) {
                    vars.push(name);
                }
            }
        }

        vars
    }

    /// Extract variables with their default values (if specified in ${VAR:default} format)
    pub fn extract_variables_with_defaults(&self, template: &str) -> Vec<(String, Option<String>)> {
        let mut vars = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for cap in self.var_regex.captures_iter(template) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str().to_string();
                let default_val = cap.get(2).map(|m| {
                    let s = m.as_str();
                    // Remove leading ':' from default value
                    s.strip_prefix(':').unwrap_or(s).to_string()
                });

                if seen.insert(name.clone()) {
                    vars.push((name, default_val));
                }
            }
        }

        vars
    }

    /// Substitute variables in template with provided values
    pub fn substitute_variables(
        &self,
        template: &str,
        values: &HashMap<String, String>,
    ) -> Result<String> {
        let mut result = template.to_string();
        let vars_in_template = self.extract_variables_with_defaults(template);

        // Check for missing required variables
        for (var_name, default) in &vars_in_template {
            if !values.contains_key(var_name) && default.is_none() {
                return Err(anyhow!("Missing required variable: {}", var_name));
            }
        }

        // Replace variables with values
        for (var_name, default) in vars_in_template {
            let value = values
                .get(&var_name)
                .cloned()
                .or(default)
                .ok_or_else(|| anyhow!("No value for variable: {}", var_name))?;

            // Replace ${VAR_NAME} and ${VAR_NAME:default} patterns
            let patterns = vec![
                format!(r"\$\{{{}\}}", var_name),
                format!(r"\$\{{{}:[^}}]*\}}", var_name),
            ];

            for pattern in patterns {
                let re = Regex::new(&pattern)?;
                result = re.replace_all(&result, value.as_str()).to_string();
            }
        }

        Ok(result)
    }

    /// Validate an SDL template has all required variables defined
    pub fn validate_template(&self, template: &str, variables: &[SdlVariable]) -> Result<()> {
        let template_vars = self.extract_variables(template);
        let defined_vars: std::collections::HashSet<_> =
            variables.iter().map(|v| v.name.clone()).collect();

        // Check for undefined variables in template
        for var in &template_vars {
            if !defined_vars.contains(var) {
                return Err(anyhow!(
                    "Variable '{}' used in template but not defined in variable list",
                    var
                ));
            }
        }

        Ok(())
    }

    /// Create SdlVariable definitions from a template with optional metadata
    pub fn create_variable_definitions(
        &self,
        template: &str,
        descriptions: &HashMap<String, String>,
        types: &HashMap<String, String>,
    ) -> Vec<SdlVariable> {
        let vars_with_defaults = self.extract_variables_with_defaults(template);

        vars_with_defaults
            .into_iter()
            .map(|(name, default)| {
                let required = default.is_none();
                SdlVariable {
                    name: name.clone(),
                    description: descriptions.get(&name).cloned().unwrap_or_default(),
                    default_value: default.unwrap_or_default(),
                    var_type: types.get(&name).cloned().unwrap_or_else(|| "string".to_string()),
                    required,
                }
            })
            .collect()
    }

    /// Configure an SDL template with values and create a ConfiguredSdl record
    pub fn configure_sdl(
        &self,
        template_name: &str,
        template: &str,
        values: &HashMap<String, String>,
    ) -> Result<ConfiguredSdl> {
        // Substitute variables
        let resolved_content = self.substitute_variables(template, values)?;

        // Compute content hash
        let mut hasher = Sha256::new();
        hasher.update(resolved_content.as_bytes());
        let content_hash = hasher.finalize().to_vec();

        // Create timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        Ok(ConfiguredSdl {
            template_name: template_name.to_string(),
            resolved_content,
            variable_values: values.clone(),
            content_hash,
            configured_at: Some(Timestamp {
                seconds: now.as_secs() as i64,
                nanos: now.subsec_nanos() as i32,
            }),
        })
    }

    /// Parse YAML SDL and extract service resources for variable defaults
    pub fn suggest_variable_defaults_from_sdl(&self, sdl_yaml: &str) -> HashMap<String, String> {
        let mut defaults = HashMap::new();

        // Parse YAML to extract common resource patterns
        if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(sdl_yaml) {
            // Try to extract from profiles.compute.*.resources
            if let Some(profiles) = yaml.get("profiles") {
                if let Some(compute) = profiles.get("compute") {
                    if let Some(compute_map) = compute.as_mapping() {
                        for (_, profile_value) in compute_map {
                            if let Some(resources) = profile_value.get("resources") {
                                // Extract CPU
                                if let Some(cpu) = resources.get("cpu").and_then(|c| c.get("units")) {
                                    if let Some(cpu_val) = cpu.as_str() {
                                        defaults.insert(common_vars::CPU.to_string(), cpu_val.to_string());
                                    } else if let Some(cpu_val) = cpu.as_u64() {
                                        defaults.insert(common_vars::CPU.to_string(), cpu_val.to_string());
                                    } else if let Some(cpu_val) = cpu.as_f64() {
                                        defaults.insert(common_vars::CPU.to_string(), cpu_val.to_string());
                                    }
                                }
                                // Extract Memory
                                if let Some(memory) = resources.get("memory").and_then(|m| m.get("size")) {
                                    if let Some(mem_val) = memory.as_str() {
                                        defaults.insert(common_vars::MEMORY.to_string(), mem_val.to_string());
                                    }
                                }
                                // Extract Storage
                                if let Some(storage) = resources.get("storage") {
                                    if let Some(storage_arr) = storage.as_sequence() {
                                        if let Some(first) = storage_arr.first() {
                                            if let Some(size) = first.get("size").and_then(|s| s.as_str()) {
                                                defaults.insert(common_vars::STORAGE.to_string(), size.to_string());
                                            }
                                        }
                                    }
                                }
                                // Extract GPU
                                if let Some(gpu) = resources.get("gpu") {
                                    if let Some(units) = gpu.get("units").and_then(|u| u.as_u64()) {
                                        defaults.insert(common_vars::GPU_COUNT.to_string(), units.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Try to extract from services.*.image
            if let Some(services) = yaml.get("services") {
                if let Some(services_map) = services.as_mapping() {
                    for (_, service_value) in services_map {
                        if let Some(image) = service_value.get("image").and_then(|i| i.as_str()) {
                            defaults.insert(common_vars::IMAGE.to_string(), image.to_string());
                        }
                        if let Some(count) = service_value.get("count").and_then(|c| c.as_u64()) {
                            defaults.insert(common_vars::REPLICAS.to_string(), count.to_string());
                        }
                    }
                }
            }
        }

        defaults
    }

    /// Query SDL template from a CosmWasm contract
    #[cfg(feature = "cw")]
    pub async fn query_template_from_contract(
        &self,
        wasm_runtime: &WasmRuntime,
        storage: &Storage,
        contract_address: &str,
    ) -> Result<(String, serde_json::Value)> {
        

        // Create QueryMsg::GetTemplate
        let query_msg = serde_json::json!({
            "get_template": {}
        });

        let query_bytes = serde_json::to_vec(&query_msg)?;

        // Query the contract
        let result = wasm_runtime
            .query_contract(storage, contract_address.to_string(), query_bytes)
            .await
            .map_err(|e| anyhow!("Failed to query template contract: {}", e))?;

        // Extract response from ContractResult
        let response_binary = match result {
            cosmwasm_std::ContractResult::Ok(binary) => binary,
            cosmwasm_std::ContractResult::Err(err) => {
                return Err(anyhow!("Contract query failed: {}", err));
            }
        };

        // Parse TemplateResponse
        let response: serde_json::Value = serde_json::from_slice(&response_binary)?;

        let sdl_template = response["sdl_template"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing sdl_template in response"))?
            .to_string();

        let template_json = response["template_json"].clone();

        Ok((sdl_template, template_json))
    }

    /// Query rendered SDL from contract with variable substitution
    #[cfg(feature = "cw")]
    pub async fn query_rendered_sdl_from_contract(
        &self,
        wasm_runtime: &WasmRuntime,
        storage: &Storage,
        contract_address: &str,
        variables: Option<HashMap<String, String>>,
    ) -> Result<(String, HashMap<String, String>)> {
        

        // Create QueryMsg::RenderSdl
        let query_msg = serde_json::json!({
            "render_sdl": {
                "variables": variables
            }
        });

        let query_bytes = serde_json::to_vec(&query_msg)?;

        // Query the contract
        let result = wasm_runtime
            .query_contract(storage, contract_address.to_string(), query_bytes)
            .await
            .map_err(|e| anyhow!("Failed to query contract: {}", e))?;

        // Extract response from ContractResult
        let response_binary = match result {
            cosmwasm_std::ContractResult::Ok(binary) => binary,
            cosmwasm_std::ContractResult::Err(err) => {
                return Err(anyhow!("Contract query failed: {}", err));
            }
        };

        // Parse RenderedSdlResponse
        let response: serde_json::Value = serde_json::from_slice(&response_binary)?;

        let rendered_sdl = response["rendered_sdl"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing rendered_sdl in response"))?
            .to_string();

        let used_variables: HashMap<String, String> = serde_json::from_value(
            response["used_variables"].clone()
        ).unwrap_or_default();

        Ok((rendered_sdl, used_variables))
    }

    /// Query variable defaults from contract
    #[cfg(feature = "cw")]
    pub async fn query_defaults_from_contract(
        &self,
        wasm_runtime: &WasmRuntime,
        storage: &Storage,
        contract_address: &str,
    ) -> Result<HashMap<String, String>> {
        

        // Create QueryMsg::GetDefaults
        let query_msg = serde_json::json!({
            "get_defaults": {}
        });

        let query_bytes = serde_json::to_vec(&query_msg)?;

        // Query the contract
        let result = wasm_runtime
            .query_contract(storage, contract_address.to_string(), query_bytes)
            .await
            .map_err(|e| anyhow!("Failed to query contract: {}", e))?;

        // Extract response from ContractResult
        let response_binary = match result {
            cosmwasm_std::ContractResult::Ok(binary) => binary,
            cosmwasm_std::ContractResult::Err(err) => {
                return Err(anyhow!("Contract query failed: {}", err));
            }
        };

        // Parse DefaultsResponse
        let response: serde_json::Value = serde_json::from_slice(&response_binary)?;

        let defaults: HashMap<String, String> = serde_json::from_value(
            response["defaults"].clone()
        ).map_err(|e| anyhow!("Failed to parse defaults: {}", e))?;

        Ok(defaults)
    }

    /// Configure SDL from contract-sourced template
    #[cfg(feature = "cw")]
    pub async fn configure_sdl_from_contract(
        &self,
        wasm_runtime: &WasmRuntime,
        storage: &Storage,
        contract_address: &str,
        template_name: &str,
        user_values: &HashMap<String, String>,
    ) -> Result<ConfiguredSdl> {
        // Query defaults from contract
        let defaults = self
            .query_defaults_from_contract(wasm_runtime, storage, contract_address)
            .await?;

        // Merge user values with defaults
        let final_values = merge_with_defaults(user_values, &defaults);

        // Query rendered SDL from contract
        let (rendered_sdl, used_variables) = self
            .query_rendered_sdl_from_contract(
                wasm_runtime,
                storage,
                contract_address,
                Some(final_values.clone()),
            )
            .await?;

        // Compute content hash
        let mut hasher = Sha256::new();
        hasher.update(rendered_sdl.as_bytes());
        let content_hash = hasher.finalize().to_vec();

        // Create timestamp
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        Ok(ConfiguredSdl {
            template_name: template_name.to_string(),
            resolved_content: rendered_sdl,
            variable_values: used_variables,
            content_hash,
            configured_at: Some(Timestamp {
                seconds: now.as_secs() as i64,
                nanos: now.subsec_nanos() as i32,
            }),
        })
    }
}

impl Default for SdlTemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to merge user values with defaults
pub fn merge_with_defaults(
    user_values: &HashMap<String, String>,
    defaults: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = defaults.clone();
    for (key, value) in user_values {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

/// Create an inference provider SDL template with common variables
pub fn create_inference_sdl_template(provider_type: &str) -> String {
    match provider_type {
        "ollama" => OLLAMA_SDL_TEMPLATE.to_string(),
        "vllm" => VLLM_SDL_TEMPLATE.to_string(),
        "tgi" => TGI_SDL_TEMPLATE.to_string(),
        _ => {
            // Generic inference provider template
            r#"---
version: "2.0"

services:
  inference:
    image: ${IMAGE}
    expose:
      - port: ${EXPOSE_PORT:8080}
        as: 80
        to:
          - global: true
    env:
      - MODEL=${MODEL_NAME}

profiles:
  compute:
    inference:
      resources:
        cpu:
          units: ${CPU:2}
        memory:
          size: ${MEMORY:4Gi}
        storage:
          - size: ${STORAGE:20Gi}
        gpu:
          units: ${GPU_COUNT:1}
          attributes:
            vendor:
              nvidia:
  placement:
    akash:
      pricing:
        inference:
          denom: uakt
          amount: 10000

deployment:
  inference:
    akash:
      profile: inference
      count: ${REPLICAS:1}
"#.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_variables() {
        let manager = SdlTemplateManager::new();
        let template = r#"
            cpu: ${CPU}
            memory: ${MEMORY}
            storage: ${STORAGE}
            image: ${IMAGE}
        "#;

        let vars = manager.extract_variables(template);
        assert_eq!(vars.len(), 4);
        assert!(vars.contains(&"CPU".to_string()));
        assert!(vars.contains(&"MEMORY".to_string()));
        assert!(vars.contains(&"STORAGE".to_string()));
        assert!(vars.contains(&"IMAGE".to_string()));
    }

    #[test]
    fn test_extract_variables_with_defaults() {
        let manager = SdlTemplateManager::new();
        let template = r#"
            cpu: ${CPU:2}
            memory: ${MEMORY}
            port: ${EXPOSE_PORT:8080}
        "#;

        let vars = manager.extract_variables_with_defaults(template);
        assert_eq!(vars.len(), 3);

        let cpu_var = vars.iter().find(|(n, _)| n == "CPU").unwrap();
        assert_eq!(cpu_var.1, Some("2".to_string()));

        let mem_var = vars.iter().find(|(n, _)| n == "MEMORY").unwrap();
        assert_eq!(mem_var.1, None);

        let port_var = vars.iter().find(|(n, _)| n == "EXPOSE_PORT").unwrap();
        assert_eq!(port_var.1, Some("8080".to_string()));
    }

    #[test]
    fn test_substitute_variables() {
        let manager = SdlTemplateManager::new();
        let template = "cpu: ${CPU}\nmemory: ${MEMORY:4Gi}";

        let mut values = HashMap::new();
        values.insert("CPU".to_string(), "4".to_string());

        let result = manager.substitute_variables(template, &values).unwrap();
        assert!(result.contains("cpu: 4"));
        assert!(result.contains("memory: 4Gi"));
    }

    #[test]
    fn test_substitute_missing_required_variable() {
        let manager = SdlTemplateManager::new();
        let template = "cpu: ${CPU}\nmemory: ${MEMORY}";

        let mut values = HashMap::new();
        values.insert("CPU".to_string(), "4".to_string());
        // MEMORY is missing and has no default

        let result = manager.substitute_variables(template, &values);
        assert!(result.is_err());
    }

    #[test]
    fn test_configure_sdl() {
        let manager = SdlTemplateManager::new();
        let template = "cpu: ${CPU}\nmemory: ${MEMORY:4Gi}";

        let mut values = HashMap::new();
        values.insert("CPU".to_string(), "8".to_string());
        values.insert("MEMORY".to_string(), "16Gi".to_string());

        let configured = manager
            .configure_sdl("test-template", template, &values)
            .unwrap();

        assert_eq!(configured.template_name, "test-template");
        assert!(configured.resolved_content.contains("cpu: 8"));
        assert!(configured.resolved_content.contains("memory: 16Gi"));
        assert!(!configured.content_hash.is_empty());
        assert!(configured.configured_at.is_some());
    }

    #[test]
    fn test_create_variable_definitions() {
        let manager = SdlTemplateManager::new();
        let template = "cpu: ${CPU:2}\nmemory: ${MEMORY}";

        let mut descriptions = HashMap::new();
        descriptions.insert("CPU".to_string(), "Number of CPU units".to_string());
        descriptions.insert("MEMORY".to_string(), "Memory size".to_string());

        let mut types = HashMap::new();
        types.insert("CPU".to_string(), "integer".to_string());
        types.insert("MEMORY".to_string(), "string".to_string());

        let vars = manager.create_variable_definitions(template, &descriptions, &types);

        assert_eq!(vars.len(), 2);

        let cpu_var = vars.iter().find(|v| v.name == "CPU").unwrap();
        assert_eq!(cpu_var.default_value, "2");
        assert!(!cpu_var.required);
        assert_eq!(cpu_var.var_type, "integer");

        let mem_var = vars.iter().find(|v| v.name == "MEMORY").unwrap();
        assert!(mem_var.required);
    }

    #[test]
    fn test_merge_with_defaults() {
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "2".to_string());
        defaults.insert("MEMORY".to_string(), "4Gi".to_string());

        let mut user_values = HashMap::new();
        user_values.insert("CPU".to_string(), "8".to_string());

        let merged = merge_with_defaults(&user_values, &defaults);

        assert_eq!(merged.get("CPU").unwrap(), "8");
        assert_eq!(merged.get("MEMORY").unwrap(), "4Gi");
    }
}
