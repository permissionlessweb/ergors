//! # CW-HO Trait Implementation Examples
//!
//! This module provides complete, working examples of how to implement and use
//! the CW-HO trait system. These examples demonstrate best practices and common
//! patterns for extending the system with custom implementations.
//!

use crate::prelude::*;
use crate::{error::HoResult, traits::*};
use async_trait::async_trait;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};
use uuid::Uuid;

/// # Example: Custom Storage Implementation
///
/// This example shows how to implement the `StorageTrait` for a custom storage backend.
/// This could be adapted for Redis, PostgreSQL, or any other storage system.
pub mod custom_storage {
    use super::*;
    use crate::types::cw_ho::orchestration::v1::*;

    /// A simple in-memory storage implementation for demonstration
    pub struct MemoryStorage {
        prompts: HashMap<String, Vec<u8>>,
        metrics: StorageMetrics,
    }

    #[derive(Debug, Clone)]
    pub struct SimpleQuery {
        pub limit: Option<u32>,
        pub session_id: Option<String>,
    }

    impl StorageQueryTrait for SimpleQuery {
        type Timestamp = chrono::DateTime<chrono::Utc>;

        fn limit(&self) -> Option<u32> {
            self.limit
        }

        fn session_id(&self) -> Option<&str> {
            self.session_id.as_deref()
        }

        fn user_id(&self) -> Option<&str> {
            None
        }

        fn start_time(&self) -> Option<&Self::Timestamp> {
            None
        }

        fn end_time(&self) -> Option<&Self::Timestamp> {
            None
        }

        fn offset(&self) -> Option<u32> {
            None
        }

        fn filters(&self) -> &HashMap<String, String> {
            static EMPTY: std::sync::LazyLock<HashMap<String, String>> =
                std::sync::LazyLock::new(|| HashMap::new());
            &EMPTY
        }

        fn set_session_id(&mut self, session_id: String) {
            self.session_id = Some(session_id);
        }

        fn set_user_id(&mut self, _user_id: String) {}

        fn set_time_range(&mut self, _start: Self::Timestamp, _end: Self::Timestamp) {}

        fn set_pagination(&mut self, limit: u32, _offset: u32) {
            self.limit = Some(limit);
        }

        fn add_filter(&mut self, _key: String, _value: String) {
            // Not supported in this simple example
        }

        fn data_file_path(&mut self, limit: u32, offset: u32) {
            todo!()
        }
    }

    #[derive(Debug, Clone)]
    pub struct SimpleSnapshot {
        pub id: String,
        pub data: HashMap<String, Vec<u8>>,
        pub created_at: chrono::DateTime<chrono::Utc>,
    }

    impl StorageSnapshotTrait for SimpleSnapshot {
        type Timestamp = chrono::DateTime<chrono::Utc>;

        fn id(&self) -> &str {
            &self.id
        }

        fn created_at(&self) -> &chrono::DateTime<chrono::Utc> {
            &self.created_at
        }

        fn state_root(&self) -> &str {
            &self.id // Use ID as state root for simplicity
        }

        fn version(&self) -> u64 {
            1 // Simple version
        }

        fn data(&self) -> &HashMap<String, Vec<u8>> {
            &self.data
        }

        fn set_state_root(&mut self, state_root: String) {
            self.id = state_root;
        }

        fn add_data(&mut self, key: String, value: Vec<u8>) {
            self.data.insert(key, value);
        }

        fn remove_data(&mut self, key: &str) {
            self.data.remove(key);
        }
    }

    #[derive(Debug, Clone)]
    pub struct StorageMetrics {
        pub total_entries: u64,
        pub storage_size_bytes: u64,
    }

    impl StorageMetricsTrait for StorageMetrics {
        type Timestamp = chrono::DateTime<chrono::Utc>;

        fn total_entries(&self) -> u64 {
            self.total_entries
        }

        fn storage_size_bytes(&self) -> u64 {
            self.storage_size_bytes
        }

        fn index_size_bytes(&self) -> u64 {
            self.storage_size_bytes / 10 // Assume index is 10% of storage
        }

        fn last_compaction(&self) -> &Self::Timestamp {
            static LAST_COMPACTION: std::sync::LazyLock<chrono::DateTime<chrono::Utc>> =
                std::sync::LazyLock::new(|| chrono::Utc::now());
            &LAST_COMPACTION
        }

        fn fragmentation_ratio(&self) -> f64 {
            0.1 // 10% fragmentation
        }

        fn update_metrics(
            &mut self,
            total_entries: u64,
            storage_size: u64,
            _index_size: u64,
            _fragmentation: f64,
        ) {
            self.total_entries = total_entries;
            self.storage_size_bytes = storage_size;
        }
    }

    #[async_trait]
    impl StorageTrait for MemoryStorage {
        type PromptResponse = PromptResponse;
        type PromptRequest = PromptRequest;
        type Query = SimpleQuery;
        type Snapshot = SimpleSnapshot;
        type Metrics = StorageMetrics;

        async fn new<P: AsRef<Path> + Send>(_path: P) -> HoResult<Self> {
            Ok(Self {
                prompts: HashMap::new(),
                metrics: StorageMetrics {
                    total_entries: 0,
                    storage_size_bytes: 0,
                },
            })
        }

        async fn store_prompt(&self, prompt: &Self::PromptResponse) -> HoResult<()> {
            // In a real implementation, this would be thread-safe

            Ok(())
        }

        async fn store_prompt_with_context(
            &self,
            prompt: &Self::PromptResponse,
            _request: Option<&Self::PromptRequest>,
        ) -> HoResult<()> {
            self.store_prompt(prompt).await
        }

        async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<Self::PromptResponse>> {
            println!("Retrieving prompt with ID: {}", id);
            Ok(None) // Simplified for example
        }

        async fn query_prompts(&self, query: &Self::Query) -> HoResult<Vec<Self::PromptResponse>> {
            println!("Querying prompts with limit: {:?}", query.limit);
            Ok(vec![]) // Simplified for example
        }

        async fn create_snapshot(&self) -> HoResult<Self::Snapshot> {
            Ok(SimpleSnapshot {
                id: Uuid::new_v4().to_string(),
                data: HashMap::new(),
                created_at: chrono::Utc::now(),
            })
        }

        async fn restore_from_snapshot(&self, snapshot: &Self::Snapshot) -> HoResult<()> {
            println!("Restoring from snapshot: {}", snapshot.id);
            Ok(())
        }

        async fn prune_storage(&self) -> HoResult<()> {
            println!("Pruning old storage data");
            Ok(())
        }

        async fn get_metrics(&self) -> HoResult<Self::Metrics> {
            Ok(self.metrics.clone())
        }

        async fn compact(&self) -> HoResult<()> {
            println!("Compacting storage");
            Ok(())
        }

        async fn health_check(&self) -> HoResult<()> {
            println!("Storage health check passed");
            Ok(())
        }

        async fn count(&self) -> HoResult<u64> {
            Ok(self.metrics.total_entries)
        }

        async fn clear_all(&self) -> HoResult<()> {
            println!("⚠️ Clearing all storage data");
            Ok(())
        }
    }
}

/// # Example: Custom LLM Provider Implementation
///
/// This example demonstrates how to implement a custom LLM provider that can be
/// used with the LLM router system.
pub mod custom_llm {
    use serde::Serialize;

    use crate::types::cw_ho::orchestration::v1::{PromptRequest, PromptResponse};

    use super::*;

    /// A mock LLM provider for demonstration
    pub struct MockLLMProvider {
        pub name: String,
        pub base_url: String,
        pub models: Vec<String>,
    }

    impl LLMProviderTrait for MockLLMProvider {
        type ProviderType = LlmModel;

        fn name(&self) -> &str {
            &self.name
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }

        fn api_key(&self) -> &str {
            ""
        }

        fn supported_models(&self) -> &[String] {
            &self.models
        }

        fn provider_type(&self) -> &Self::ProviderType {
            &LlmModel::OpenAi
        }

        fn set_api_key(&mut self, api_key: String) {}

        fn add_supported_model(&mut self, model: String) {
            self.models.push(model);
        }
    }

    #[derive(Debug, Clone)]
    pub struct MockLLMRouterConfig {
        pub config: LlmRouterConfig,
    }
    pub struct CustomLLMRouter {
        providers: HashMap<String, MockLLMProvider>,
        default_provider: String,
    }

    impl LLMRouterConfigTrait for MockLLMRouterConfig {
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
    }

    #[async_trait]
    impl LLMRouterTrait for CustomLLMRouter {
        type Request = PromptRequest;
        type Response = PromptResponse;
        type Config = MockLLMRouterConfig;
        type Provider = MockLLMProvider;

        async fn new(config: &Self::Config) -> HoResult<Self>
        where
            Self: Sized,
        {
            let mut router = Self {
                providers: HashMap::new(),
                default_provider: config.default_provider().to_string(),
            };

            // Add providers from config (simplified)
            for provider in &config.config.entities {
                let mock_provider = MockLLMProvider {
                    name: provider.name.clone(),
                    base_url: provider.base_url.clone(),

                    models: provider.models.clone(),
                };
                router
                    .providers
                    .insert(provider.name.clone(), mock_provider);
            }

            Ok(router)
        }

        async fn process_request(
            &self,
            request: &Self::Request,
            model: &str,
        ) -> HoResult<Self::Response> {
            println!("Processing request with model: {}", model);

            // Create a mock response
            Ok(PromptResponse {
                id: Uuid::new_v4().into(),
                provider: self.default_provider.clone(),
                model: model.to_string(),
                prompt: "Mock prompt".to_string(),
                response: vec!["This is a mock response from the custom LLM provider".to_string()],
                timestamp: None,
                tokens_used: Some(TokenUsage {
                    prompt: 10,
                    completion: 20,
                    total: 30,
                }),
                cost: Some(0.001),
                latency_ms: Some(150),
                // context: todo!(),
            })
        }

        async fn route_to_provider(
            &self,
            provider_name: &str,
            request: &Self::Request,
        ) -> HoResult<Self::Response> {
            if let Some(_provider) = self.providers.get(provider_name) {
                self.process_request(request, "default-model").await
            } else {
                Err(crate::error::HoError::Llm(format!(
                    "Provider not found: {}",
                    provider_name
                )))
            }
        }

        fn providers(&self) -> Vec<&Self::Provider> {
            self.providers.values().collect()
        }

        fn get_provider(&self, name: &str) -> Option<&Self::Provider> {
            self.providers.get(name)
        }

        fn add_provider(&mut self, provider: Self::Provider) {
            let name = provider.name().to_string();
            self.providers.insert(name, provider);
        }

        fn remove_provider(&mut self, name: &str) {
            self.providers.remove(name);
        }

        async fn check_provider_health(&self, provider_name: &str) -> HoResult<bool> {
            println!("Checking health for provider: {}", provider_name);
            Ok(true) // Always healthy in this mock
        }
    }
}

/// # Example: Using Extension Traits with Proto Types
///
/// This example shows how to use the extension traits that are already implemented
/// for generated proto types.
pub mod extension_trait_usage {

    use super::*;

    /// Demonstrates using NodeIdentity extension methods
    pub async fn demonstrate_node_identity() -> HoResult<()> {
        println!("🔑 Demonstrating NodeIdentity extension traits");

        // Generate a fresh identity
        let mut identity = NodeIdentity {
            host: "192.168.1.100".to_string(),
            p2p_port: 26969,
            api_port: 8080,
            user: "demo-user".to_string(),
            os: HostOs::Linux as i32,
            ssh_port: 22,
            node_type: NodeType::Executor.as_str_name().into(),
            public_key: None,
            private_key: None,
        };

        // Use basic proto fields (extension traits would provide more functionality)
        // identity.generate_keypair(&mut rand::rngs::OsRng)?;

        println!("🌐 Host: {}:{}", identity.host, identity.p2p_port);
        println!("📡 P2P Port: {}", identity.p2p_port);
        println!("🔗 API Port: {}", identity.api_port);
        println!("🏷️  User: {}", identity.user);

        Ok(())
    }

    /// Demonstrates using CosmicContext extension methods
    pub fn demonstrate_cosmic_context() {
        println!("🌌 Demonstrating CosmicContext extension traits");

        let context = CosmicContext {
            task_id: "cosmic-task-42".to_string(),
            user_input: "Execute fractal orchestration with golden ratio scaling".to_string(),
            current_step: 1,
            total_steps: 7,
            fractal_level: 3,
            golden_ratio_state: 1.618.to_string(),
            previous_responses: vec![],
            cosmic_metadata: std::collections::HashMap::new(),
        };

        println!("📋 Task ID: {}", context.task_id);
        println!("📝 User Input: {}", context.user_input);
        println!("📊 Steps: {}/{}", context.current_step, context.total_steps);
        println!("🌀 Fractal Level: {}", context.fractal_level);
        println!("✨ Golden Ratio State: {}", context.golden_ratio_state);
    }

    /// Demonstrates using FractalRequirements extension methods
    pub fn demonstrate_fractal_requirements() {
        println!("🔮 Demonstrating FractalRequirements extension traits");

        let requirements = FractalRequirements {
            recursion_depth: 5,
            self_similarity_threshold: 0.8,
            golden_ratio_compliance: true,
            fractal_dimension_target: 1.5,
            mobius_continuity: true,
            fractal_coherence: 0.9,
            expansion_criteria: vec!["complexity".to_string(), "elegance".to_string()],
            context: None,
        };

        println!("🔄 Recursion Depth: {}", requirements.recursion_depth);
        println!(
            "⚖️  Similarity Threshold: {}",
            requirements.self_similarity_threshold
        );
        println!(
            "📐 Golden Ratio Compliance: {}",
            requirements.golden_ratio_compliance
        );
        println!(
            "📏 Fractal Dimension Target: {}",
            requirements.fractal_dimension_target
        );
        println!("🌊 Möbius Continuity: {}", requirements.mobius_continuity);
        println!("🎯 Fractal Coherence: {}", requirements.fractal_coherence);
        println!(
            "📈 Expansion Criteria: {:?}",
            requirements.expansion_criteria
        );
    }
}

/// # Example: Generic Functions Using Traits
///
/// These functions demonstrate how to write generic code that works with any
/// implementation of the CW-HO traits.
pub mod generic_functions {

    use crate::orchestrate::PromptMessage;

    use super::*;

    /// A generic function that can work with any storage implementation
    pub async fn backup_prompts<S>(storage: &S) -> HoResult<()>
    where
        S: StorageTrait,
    {
        println!("🔄 Creating backup...");

        // Create snapshot
        let snapshot = storage.create_snapshot().await?;
        println!("📸 Snapshot created");

        // Get current metrics
        let metrics = storage.get_metrics().await?;
        println!("📊 Current metrics retrieved");

        // Perform health check
        storage.health_check().await?;
        println!("✅ Storage health verified");

        println!("💾 Backup completed successfully");
        Ok(())
    }

    /// A function that works with the concrete CustomLLMRouter implementation
    pub async fn test_llm_provider(
        router: &custom_llm::CustomLLMRouter,
        test_prompt: &str,
    ) -> HoResult<()> {
        println!("🧠 Testing LLM provider with prompt: '{}'", test_prompt);

        // Create a test request
        let request = PromptRequest {
            messages: vec![PromptMessage {
                role: "user".to_string(),
                content: test_prompt.to_string(),
            }],
            model: "test-model".to_string(),
            context: None,
            llm_config: None,
        };

        // Process the request
        let response = router.process_request(&request, "test-model").await?;

        println!("📤 Response received:");
        println!("   Provider: {}", response.provider());
        println!("   Model: {}", response.model());
        println!("   Response: {:#?}", response.response);

        let tokens = response.tokens_used();
        println!("   Tokens used: {}", tokens.total);

        Ok(())
    }
}

/// Run all examples to demonstrate the trait system in action
pub async fn run_all_examples() -> HoResult<()> {
    println!("🚀 Running CW-HO Trait System Examples\n");

    // Extension trait examples
    extension_trait_usage::demonstrate_node_identity().await?;
    println!();

    extension_trait_usage::demonstrate_cosmic_context();
    println!();

    extension_trait_usage::demonstrate_fractal_requirements();
    println!();

    // Custom storage example
    let storage = custom_storage::MemoryStorage::new("./test-data").await?;
    generic_functions::backup_prompts(&storage).await?;
    println!();

    // Custom LLM example
    let config = custom_llm::MockLLMRouterConfig { config: todo!() };

    let router = custom_llm::CustomLLMRouter::new(&config).await?;
    generic_functions::test_llm_provider(&router, "What is the golden ratio?").await?;

    println!("\n✅ All examples completed successfully!");
    Ok(())
}
