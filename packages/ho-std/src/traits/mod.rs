//! # ERGORS Trait System
//!
//! This module provides traits used for the ERGORS engine.
//!
//! ## Architecture Overview
//!
//! ### Key Components
//!
//! - [`config`] - Configuration management and validation traits
//! - [`file_ops`] - Utilities for interacting with files
//! - [`llm`] - LLM provider routing, prompt handling, and response processing traits
//! - [`network`] - Network node identity, topology, and message handling traits
//! - [`orchestrator`] - Cosmic task management and fractal orchestration traits
//! - [`storage`] - Data persistence, querying, and snapshot management traits
//!
//! ## Usage Pattern
//!
//! ### TRAITS -> STRUCTS -> CONFIGS -> IMPLEMENTATIONS
//!
//! // Storage-related traits for ERGORS system
// Configuration-related traits for ERGORS system
// Network-related traits for ERGORS system

mod domain;
pub mod file_ops;
pub mod message;

pub use {domain::*, file_ops::*, message::*};

use crate::{error::HoResult, keys::commonware::NodePrivKey, types::ergors::network::v1::*};

use async_trait::async_trait;
use camino::Utf8Path;
use cnidarium::StateRead;
use reqwest::Client;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::{any::Any, collections::BTreeMap};
use uuid::Uuid;

// Network-related traits for ERGORS system
// Category: network
/// Core trait for network node identity
pub trait NodeIdentityTrait {
    type HostOS;
    type NodeType;
    type PrivateKey;
    type PublicKey;

    /// Create a new node identity
    fn new() -> Self
    where
        Self: Sized;

    /// Generate a fresh keypair
    fn generate_keypair<R: rand::RngCore + rand::CryptoRng>(&mut self, rng: &mut R)
        -> HoResult<()>;

    /// Set keypair from existing keys
    fn set_keypair(&mut self, private_key: Self::PrivateKey);

    /// Get P2P identity address
    fn p2p_identity(&self) -> String;

    /// Get P2P socket address
    fn p2p_address(&self) -> SocketAddr;

    /// Get API address
    fn api_address(&self) -> String;

    /// Get display-friendly identifier
    fn display_id(&self) -> String;

    fn get_private_key_from_env() -> NodePrivKey;
    fn private_key_from_hex(hex_string: &str) -> Option<NodePrivKey>;
}

// Network-related traits for ERGORS system
// Category: network
/// Core trait for network topology management
pub trait NetworkTopologyTrait {
    /// Create a new empty topology
    fn new() -> Self
    where
        Self: Sized;

    type NodeInfo;
    type Connection;

    /// Get all nodes in topology
    fn nodes(&self) -> &[Self::NodeInfo];

    // /// Get all nodes of a specific type
    fn nodes_by_type(&self, node_type: NodeType) -> Vec<&NodeInfo>;

    /// Get online nodes only
    fn online_nodes(&self) -> Vec<&Self::NodeInfo>;

    /// Get all connections in topology
    fn connections(&self) -> &[Self::Connection];

    /// Add a node to topology
    fn add_node(&mut self, node: Self::NodeInfo);

    /// Remove a node from topology
    fn remove_node(&mut self, node_id: &str);

    /// Add a connection
    fn add_connection(&mut self, connection: Self::Connection);

    /// Remove a connection
    fn remove_connection(&mut self, from_node: &str, to_node: &str);
    fn count_nodes_by_type(&self) -> Vec<(String, usize)> {
        vec![]
    }

    /// Check if a connection exists
    fn has_connection(&self, from: &str, to: &str) -> bool;

    // /// Get statistics about the topology
    fn stats(&self) -> TopologyStatsResponse {
        TopologyStatsResponse {
            // total_nodes: self.nodes.len(),
            // online_nodes: self.online_nodes().len(),
            // total_connections: self.connections.len(),
            // is_complete: self.is_complete_tetrahedron(),
            // nodes_by_type: self.count_nodes_by_type(),
            max_message_size: todo!(),
            max_peers: todo!(),
            connection_timeout: todo!(),
        }
    }

    /// Check if the topology forms a complete tetrahedral structure for node
    /// TODO: implement direct wqieries from storage, implement epoch trigger on each request
    fn is_complete_tetrahedron(&self) -> bool {
        let online_nodes = self.online_nodes();

        // // Need exactly 4 nodes (one of each type)
        // if online_nodes.len() != 4 {
        //     return false;
        // }

        // // Check we have one of each type
        // let types: Vec<NodeType> = online_nodes
        //     .iter()
        //     .map(|n| NodeType::from_str_name(&n.clone()).unwrap())
        //     .collect();

        // let has_coordinator = types.contains(&NodeType::Coordinator);
        // let has_executor = types.contains(&NodeType::Executor);
        // let has_referee = types.contains(&NodeType::Referee);
        // let has_development = types.contains(&NodeType::Development);

        // if !(has_coordinator && has_executor && has_referee && has_development) {
        //     return false;
        // }

        // // Check each node is connected to all others (6 edges for 4 nodes)
        // let expected_connections = 6;
        // let actual_connections = self.connections().len();

        // actual_connections >= expected_connections
        false
    }

    // /// Get the nearest node of a specific type
    fn nearest_node_of_type(&self, node_type: NodeType) -> Option<&NodeInfo> {
        self.nodes_by_type(node_type).into_iter().find(|n| n.online)
    }
}

// Network-related traits for ERGORS system
// Category: network
/// Core trait for network message handling
pub trait NetworkMessageTrait {
    type MessageType;
    type ResultType;

    /// Get the message type
    fn message_type(&self) -> &Self::MessageType;

    /// Serialize message to bytes
    fn to_bytes(&self) -> HoResult<Vec<u8>>;

    /// Deserialize message from bytes
    fn from_bytes(bytes: &[u8]) -> HoResult<Self>
    where
        Self: Sized;

    /// Return channel message type identifier
    fn channel(&self) -> HoResult<u8>;
}

// Network-related traits for ERGORS system
// Category: network
/// Core trait for minimal network management
#[async_trait]
pub trait NetworkManagerTrait {
    type Config: NetworkConfigTrait;
    type Identity: NodeIdentityTrait;
    type Topology: NetworkTopologyTrait;
    type Message: NetworkMessageTrait;
    type Context;

    /// Create a new network manager
    async fn new(
        config: Self::Config,
        identity: Self::Identity,
        context: Self::Context,
    ) -> HoResult<Self>
    where
        Self: Sized;

    /// Start the network
    async fn start_network(&mut self, config: Self::Config) -> HoResult<()>;

    /// Stop the network
    async fn stop_network(&mut self) -> HoResult<()>;

    /// Get current network topology
    async fn get_topology(&self) -> Self::Topology;

    /// Send a message to a peer
    async fn send_message(&mut self, peer_id: &str, message: Self::Message) -> HoResult<()>;

    /// Broadcast a message to all peers
    async fn broadcast_message(&mut self, message: Self::Message) -> HoResult<()>;

    /// Handle incoming message
    async fn handle_message(&mut self, from_peer: &str, message: Self::Message) -> HoResult<()>;

    /// Check if connected to a specific peer
    fn is_connected_to_peer(&self, peer_id: &str) -> bool;
}

// Configuration-related traits for ERGORS system
// Category: config
/// Core trait for application configuration
pub trait HoConfigTrait {
    type Identity;
    type StorageConfig;
    type LLMConfig;
    type HoConfigResult;

    fn load<P: AsRef<Path> + std::fmt::Display>(path: P) -> HoResult<Self>
    where
        Self: Sized;
    fn save<P: AsRef<Path>>(&self, path: P) -> HoResult<()>;

    /// Get network configuration
    fn new(home_dir: &Utf8Path) -> Self;
    /// Get network configuration
    fn from_file(path: &str) -> HoResult<Self>
    where
        Self: Sized;

    /// Get network configuration
    fn file_path(&self) -> &str;
    /// Get network configuration
    fn network(&self) -> &NetworkConfig;

    /// Get node identity
    fn identity(&self) -> &Self::Identity;

    /// Get storage configuration
    fn storage(&self) -> &Self::StorageConfig;

    /// Get LLM configuration
    fn llm(&self) -> &Self::LLMConfig;

    /// Validate configuration
    fn validate(&self) -> Self::HoConfigResult;

    /// Set network config
    fn set_network_config(&mut self, config: NetworkConfig);

    /// Set identity
    fn set_identity(&mut self, identity: Self::Identity);

    /// Set storage config
    fn set_storage_config(&mut self, config: Self::StorageConfig);

    /// Set LLM config
    fn set_llm_config(&mut self, config: Self::LLMConfig);
}

// Configuration-related traits for ERGORS system
// Category: config
/// Core trait for LLM router configuration
pub trait LLMRouterConfigTrait {
    /// Get default provider name
    fn default_provider(&self) -> &str;

    /// Get timeout in seconds
    fn timeout_seconds(&self) -> u32;

    /// Get retry attempts
    fn retry_attempts(&self) -> u32;

    /// Remove provider
    fn remove_provider(&mut self, name: &str);

    /// Set default provider
    fn set_default_provider(&mut self, name: String);

    /// Set timeout
    fn set_timeout(&mut self, timeout: u32);

    /// Set retry attempts
    fn set_retry_attempts(&mut self, attempts: u32);

    /// Validate LLM configuration
    fn validate(&self) -> HoResult<()> {
        if self.default_provider().is_empty() {
            return Err(crate::error::HoError::Cfg(
                "Default provider must be set".to_string(),
            ));
        }
        if self.timeout_seconds() == 0 {
            return Err(crate::error::HoError::Cfg(
                "Timeout must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

// Configuration-related traits for ERGORS system
// Category: config
/// Core trait for LLM configuration
pub trait LLMConfigTrait {
    /// Get temperature
    fn temperature(&self) -> f64;

    /// Get max tokens
    fn max_tokens(&self) -> u32;

    /// Get top-p value
    fn top_p(&self) -> f64;

    /// Get stop sequences
    fn stop_sequences(&self) -> &[String];

    /// Set temperature
    fn set_temperature(&mut self, temp: f64);

    /// Set max tokens
    fn set_max_tokens(&mut self, tokens: u32);

    /// Set top-p
    fn set_top_p(&mut self, top_p: f64);

    /// Add stop sequence
    fn add_stop_sequence(&mut self, sequence: String);

    /// Clear stop sequences
    fn clear_stop_sequences(&mut self);

    /// Validate LLM configuration
    fn validate(&self) -> HoResult<()> {
        if !(0.0..=2.0).contains(&self.temperature()) {
            return Err(crate::error::HoError::Cfg(
                "Temperature must be between 0.0 and 2.0".to_string(),
            ));
        }
        if self.max_tokens() == 0 {
            return Err(crate::error::HoError::Cfg(
                "Max tokens must be greater than 0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.top_p()) {
            return Err(crate::error::HoError::Cfg(
                "Top-p must be between 0.0 and 1.0".to_string(),
            ));
        }
        Ok(())
    }
}

// Configuration-related traits for ERGORS system
// Category: config
/// Core trait for network configuration
pub trait NetworkConfigTrait {
    /// Get bootstrap peers
    fn new() -> Self;
    /// Get bootstrap peers
    fn from_toml(&self) -> toml::Table;

    /// validate a network configuration is valid
    fn validate(&self) -> HoResult<()>;

    /// Get bootstrap peers
    fn bootstrap_peers(&self) -> &[String];

    /// Get listen port
    fn listen_port(&self) -> u32;

    /// Get listen address
    fn listen_address(&self) -> &str;

    /// Get defined limitations
    fn limits(&self) -> NetworkLimits;

    /// Get connection timeout
    fn connection_timeout_ms(&self) -> u32;

    /// Check if discovery is enabled
    fn is_discovery_enabled(&self) -> bool;
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait that all LLM providers must implement
#[async_trait]
pub trait LlmProviderTrait: Send + Sync {
    /// Get the provider name (e.g., "openai", "anthropic", "grok")
    fn name(&self) -> &str;

    /// Get the base URL for the provider's API
    fn base_url(&self) -> &str;

    /// Get all models supported by this provider
    fn supported_models(&self) -> &[&str];

    /// Call the provider's API with the given request
    async fn call(&self, client: &Client, request: &PromptRequest) -> HoResult<PromptResponse>;

    /// Validate that the provider has necessary credentials
    fn is_configured(&self) -> bool;

    // type ProviderType;
    // /// Get provider type
    // fn provider_type(&self) -> &Self::ProviderType;

    /// Check if model is supported
    fn supports_model(&self, model: &str) -> bool {
        self.supported_models()
            .contains(&model.to_string().as_str())
    }

    /// Set API key
    fn set_api_key(&mut self, api_key: String);

    /// Add supported model
    fn add_supported_model(&mut self, model: String);
}

// LLM-related traits for ERGORS system
// Category: llm
/// Trait for API request handlers
#[async_trait]
pub trait ApiJoint {
    // type Request: PromptRequestTrait;
    // type Response: PromptResponseTrait;
    // type Message: PromptRequestTrait;
    async fn handle_request<T>(
        provider: &T,
        client: &Client,
        request: &PromptRequest,
        base_url: &str,
        provider_name: &str,
    ) -> HoResult<PromptResponse>
    where
        T: ApiKeyProvider + Send + Sync;
}

// LLM-related traits for ERGORS system
// Category: llm
/// Trait for types that can provide API keys
#[async_trait]
pub trait ApiKeyProvider: Send + Sync {
    async fn get_api_key(&self) -> HoResult<String>;
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for LLM prompt requests
pub trait PromptRequestTrait {
    type Message;
    type Context;
    type Config;

    /// Get messages
    fn messages(&self) -> &[Self::Message];

    /// Get model name
    fn model(&self) -> &str;

    /// Get context
    fn context(&self) -> Option<&Self::Context>;

    /// Get LLM configuration
    fn llm_config(&self) -> Option<&Self::Config>;

    /// Add message to request
    fn add_message(&mut self, message: Self::Message);

    /// Set model
    fn set_model(&mut self, model: String);

    /// Set context
    fn set_context(&mut self, context: Self::Context);
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for LLM responses
pub trait PromptResponseTrait {
    type TokenUsage;
    type Context;
    type Timestamp;

    /// Get response ID
    fn id(&self) -> &Vec<u8>;

    /// Get provider name
    fn provider(&self) -> &str;

    /// Get model used
    fn model(&self) -> &str;

    /// Get original prompt
    fn prompt(&self) -> &str;

    /// Get timestamp
    fn timestamp(&self) -> &Self::Timestamp;

    /// Get token usage
    fn tokens_used(&self) -> &Self::TokenUsage;

    /// Get cost
    fn cost(&self) -> Option<f64>;

    /// Get latency
    fn latency_ms(&self) -> Option<u64>;

    // /// Get context
    // fn context(&self) -> Option<&Self::Context>;

    /// Set response content
    fn set_response(&mut self, response: Vec<String>);

    /// Set cost
    fn set_cost(&mut self, cost: f64);

    /// Set latency
    fn set_latency(&mut self, latency_ms: u64);
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for LLM messages
pub trait LlmMessageTrait {
    /// Get message role
    fn role(&self) -> &str;

    /// Get message content  
    fn content(&self) -> &str;

    /// Set role
    fn set_role(&mut self, role: String);

    /// Set content
    fn set_content(&mut self, content: String);

    /// Create user message
    fn user_message(content: String) -> Self;

    /// Create assistant message
    fn assistant_message(content: String) -> Self;

    /// Create system message
    fn system_message(content: String) -> Self;
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for prompt context
pub trait PromptContextTrait {
    /// Get session ID
    fn session_id(&self) -> Option<&str>;

    /// Get user ID
    fn user_id(&self) -> Option<&str>;

    /// Get thread ID
    fn thread_id(&self) -> Option<&str>;

    /// Get metadata
    fn metadata(&self) -> &HashMap<String, String>;

    /// Set session ID
    fn set_session_id(&mut self, session_id: String);

    /// Set user ID
    fn set_user_id(&mut self, user_id: String);

    /// Set thread ID
    fn set_thread_id(&mut self, thread_id: String);

    /// Add metadata
    fn add_metadata(&mut self, key: String, value: String);
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for token usage tracking
pub trait TokenUsageTrait {
    /// Get prompt tokens
    fn prompt_tokens(&self) -> u32;

    /// Get completion tokens
    fn completion_tokens(&self) -> u32;

    /// Get total tokens
    fn total_tokens(&self) -> u32;

    /// Set prompt tokens
    fn set_prompt_tokens(&mut self, tokens: u32);

    /// Set completion tokens
    fn set_completion_tokens(&mut self, tokens: u32);

    /// Update total tokens
    fn update_total(&mut self);
}

use crate::orchestrate::{LlmEntity, PromptRequest, PromptResponse};
/// Core trait that all LLM providers must implement
/// This allows for a modular, provider-agnostic approach to LLM routing
// LLM-related traits for ERGORS system
// Category: llm

/// Core trait for LLM providers.
/// LLMProviderTrait is implemented where we define the struct maintaining all llm instances available for a single hoe-instance. This means that different ho-providers may have different

#[async_trait]
pub trait LlmModelTrait {
    fn models(&self) -> (String, Vec<String>);
    fn default_base_url(&self) -> String;
    fn default_entity(&self) -> LlmEntity;
}

#[async_trait]
// LLM-related traits for ERGORS system
// Category: llm
pub trait LLMRouterTrait {
    type Request: PromptRequestTrait;
    type Response: PromptResponseTrait;
    type Config: LLMRouterConfigTrait;
    type Provider: LlmProviderTrait;

    /// Create new LLM router
    async fn new(config: &Self::Config) -> HoResult<Self>
    where
        Self: Sized;

    /// Process a prompt request
    async fn handle_request(
        &self,
        request: &Self::Request,
        model: &str,
    ) -> HoResult<Self::Response>;

    /// Route request to appropriate provider
    async fn route_to_provider(
        &self,
        provider_name: &str,
        request: &Self::Request,
    ) -> HoResult<Self::Response>;

    /// Get available providers
    fn providers(&self) -> Vec<&Self::Provider>;

    /// Get provider by name
    fn get_provider(&self, name: &str) -> Option<&Self::Provider>;

    /// Add provider
    fn add_provider(&mut self, provider: Self::Provider);

    /// Remove provider
    fn remove_provider(&mut self, name: &str);

    /// Check provider health
    async fn check_provider_health(&self, provider_name: &str) -> HoResult<bool>;
}

// LLM-related traits for ERGORS system
// Category: llm
/// Core trait for accessing API keys with support for multiple backends. Currently fetches api keys encrypted in storage
#[async_trait]
pub trait ApiKeyMethod: Send + Sync {
    /// Get API key for a specific provider
    async fn get_key(&self, provider: &str) -> HoResult<Option<String>>;

    /// Set/update API key for a provider (if supported by the backend)
    async fn set_key(&mut self, provider: &str, key: String) -> HoResult<()>;

    /// Check if a key exists for a provider
    async fn has_key(&self, provider: &str) -> bool {
        self.get_key(provider).await.ok().flatten().is_some()
    }

    /// Get all available providers with configured keys
    async fn available_providers(&self) -> Vec<String>;
}

// Storage-related traits for ERGORS system
// Category: storage
/// Write access to chain state.
pub trait StateWrite: StateRead + Send + Sync {
    /// Puts raw bytes into the verifiable key-value store with the given key.
    fn put_raw(&mut self, key: String, value: Vec<u8>);

    /// Delete a key from the verifiable key-value store.
    fn delete(&mut self, key: String);

    /// Puts raw bytes into the non-verifiable key-value store with the given key.
    fn nonverifiable_put_raw(&mut self, key: Vec<u8>, value: Vec<u8>);

    /// Delete a key from non-verifiable key-value storage.
    fn nonverifiable_delete(&mut self, key: Vec<u8>);

    /// Puts an object into the ephemeral object store with the given key.
    ///
    /// # Panics
    ///
    /// If the object is already present in the store, but its type is not the same as the type of
    /// `value`.
    fn object_put<T: Clone + Any + Send + Sync>(&mut self, key: &'static str, value: T);

    /// Deletes a key from the ephemeral object store.
    fn object_delete(&mut self, key: &'static str);

    /// Merge a set of object changes into this `StateWrite`.
    ///
    /// Unlike `object_put`, this avoids re-boxing values and messing up the downcasting.
    fn object_merge(&mut self, objects: BTreeMap<&'static str, Option<Box<dyn Any + Send + Sync>>>);

    // Record that an ABCI event occurred while building up this set of state changes.
    // fn record(&mut self, event: abci::Event);
}

impl<'a, S: StateWrite + Send + Sync> StateWrite for &'a mut S {
    fn put_raw(&mut self, key: String, value: jmt::OwnedValue) {
        (**self).put_raw(key, value)
    }

    fn delete(&mut self, key: String) {
        (**self).delete(key)
    }

    fn nonverifiable_delete(&mut self, key: Vec<u8>) {
        (**self).nonverifiable_delete(key)
    }

    fn nonverifiable_put_raw(&mut self, key: Vec<u8>, value: Vec<u8>) {
        (**self).nonverifiable_put_raw(key, value)
    }

    fn object_put<T: Clone + Any + Send + Sync>(&mut self, key: &'static str, value: T) {
        (**self).object_put(key, value)
    }

    fn object_delete(&mut self, key: &'static str) {
        (**self).object_delete(key)
    }

    fn object_merge(
        &mut self,
        objects: BTreeMap<&'static str, Option<Box<dyn Any + Send + Sync>>>,
    ) {
        (**self).object_merge(objects)
    }

    // fn record(&mut self, event: abci::Event) {
    //     (**self).record(event)
    // }
}

// Storage-related traits for ERGORS system
// Category: storage
/// Core trait for storage queries
pub trait StorageQueryTrait {
    type Timestamp;

    /// Get session ID filter
    fn session_id(&self) -> Option<&str>;

    /// Get user ID filter
    fn user_id(&self) -> Option<&str>;

    /// Get start time filter
    fn start_time(&self) -> Option<&Self::Timestamp>;

    /// Get end time filter
    fn end_time(&self) -> Option<&Self::Timestamp>;

    /// Get limit
    fn limit(&self) -> Option<u32>;

    /// Get offset
    fn offset(&self) -> Option<u32>;

    /// Get additional filters
    fn filters(&self) -> &std::collections::HashMap<String, String>;

    /// Set session ID filter
    fn set_session_id(&mut self, session_id: String);

    /// Set user ID filter
    fn set_user_id(&mut self, user_id: String);

    /// Set time range
    fn set_time_range(&mut self, start: Self::Timestamp, end: Self::Timestamp);

    /// Set pagination
    fn set_pagination(&mut self, limit: u32, offset: u32);

    /// Add filter
    fn add_filter(&mut self, key: String, value: String);

    /// Set pagination
    fn data_file_path(&mut self, limit: u32, offset: u32);
}

// Storage-related traits for ERGORS system
// Category: storage
/// Core trait for storage snapshots
pub trait StorageSnapshotTrait {
    type Timestamp;

    /// Get snapshot ID
    fn id(&self) -> &str;

    /// Get creation timestamp
    fn created_at(&self) -> &Self::Timestamp;

    /// Get state root
    fn state_root(&self) -> &str;

    /// Get version
    fn version(&self) -> u64;

    /// Get data
    fn data(&self) -> &std::collections::HashMap<String, Vec<u8>>;

    /// Set state root
    fn set_state_root(&mut self, root: String);

    /// Add data entry
    fn add_data(&mut self, key: String, value: Vec<u8>);

    /// Remove data entry
    fn remove_data(&mut self, key: &str);
}

// Storage-related traits for ERGORS system
// Category: storage
/// Core trait for storage metrics
pub trait StorageMetricsTrait {
    type Timestamp;

    /// Get total entries
    fn total_entries(&self) -> u64;

    /// Get storage size in bytes
    fn storage_size_bytes(&self) -> u64;

    /// Get index size in bytes
    fn index_size_bytes(&self) -> u64;

    /// Get last compaction time
    fn last_compaction(&self) -> &Self::Timestamp;

    /// Get fragmentation ratio
    fn fragmentation_ratio(&self) -> f64;

    /// Update metrics
    fn update_metrics(
        &mut self,
        entries: u64,
        storage_size: u64,
        index_size: u64,
        fragmentation: f64,
    );

    /// Check if compaction is needed
    fn needs_compaction(&self) -> bool {
        self.fragmentation_ratio() > 0.3 // 30% fragmentation threshold
    }
}

// Storage-related traits for ERGORS system
// Category: storage
/// Core trait for storage operations
#[async_trait]
pub trait StorageTrait {
    type PromptResponse;
    type PromptRequest;
    type Query: StorageQueryTrait;
    type Snapshot: StorageSnapshotTrait;
    type Metrics: StorageMetricsTrait;

    /// Initialize storage
    async fn new<P: AsRef<std::path::Path> + Send>(data_dir: P) -> HoResult<Self>
    where
        Self: Sized;

    /// Store a prompt response
    async fn put_prompt(&self, prompt: &Self::PromptResponse) -> HoResult<()>;

    /// Store prompt with context
    async fn put_prompt_w_ctx(
        &self,
        prompt: &Self::PromptResponse,
        request: Option<&Self::PromptRequest>,
    ) -> HoResult<()>;

    /// Get a prompt by ID
    async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<Self::PromptResponse>>;

    /// Query prompts
    async fn get_prompts(&self, query: &Self::Query) -> HoResult<Vec<Self::PromptResponse>>;

    /// Create a snapshot
    async fn create_snapshot(&self) -> HoResult<Self::Snapshot>;

    /// Restore from snapshot
    async fn restore_from_snapshot(&self, snapshot: &Self::Snapshot) -> HoResult<()>;

    /// Prune old data
    async fn prune_storage(&self) -> HoResult<()>;

    /// Get storage metrics
    async fn get_metrics(&self) -> HoResult<Self::Metrics>;

    /// Compact storage
    async fn compact(&self) -> HoResult<()>;

    /// Health check
    async fn health_check(&self) -> HoResult<()>;

    /// Get total stored items
    async fn count(&self) -> HoResult<u64>;

    /// Clear all data (dangerous operation)
    async fn clear_all(&self) -> HoResult<()>;
}

// Storage-related traits for ERGORS system
// Category: storage
/// Core trait for storage indexing
#[async_trait]
pub trait StorageIndexTrait {
    type Timestamp;

    /// Create index entry
    fn create_index(key: String, value: String) -> Self;

    /// Get key
    fn key(&self) -> &str;

    /// Get value
    fn value(&self) -> &str;

    /// Get creation time
    fn created_at(&self) -> &Self::Timestamp;

    /// Update index
    async fn update_index(&mut self, new_value: String) -> HoResult<()>;

    /// Check if index is expired
    fn is_expired(&self, ttl_seconds: u64) -> bool;
}

// Orchestrator-related traits for ERGORS system
// Category: orchestrator
pub trait FractalRequirementsExt {
    type FractalRequirements;
    fn new_default() -> Self::FractalRequirements;
}

// Orchestrator-related traits for ERGORS system
// Category: orchestrator
pub trait CosmicContextExt {
    type CosmicContext;
    fn new_context(task_id: String, prompt: &str, recursion_depth: u32) -> Self::CosmicContext;
}

// Orchestrator-related traits for ERGORS system
/// Core trait for cosmic task management
pub trait CosmicTaskTrait {
    type TaskType;
    type TaskStatus;
    // Orchestrator-related traits for ERGORS system
    // Category: orchestrator
    type FractalRequirements;
    type Timestamp;
    type StructData;

    /// Get task ID
    fn id(&self) -> &str;

    /// Get task type
    fn task_type(&self) -> &Self::TaskType;

    /// Get current status
    fn status(&self) -> &Self::TaskStatus;

    /// Get prompt
    fn prompt(&self) -> &str;

    /// Get fractal requirements
    fn fractal_requirements(&self) -> Option<&Self::FractalRequirements>;

    /// Get creation timestamp
    fn created_at(&self) -> &Self::Timestamp;

    /// Get update timestamp
    fn updated_at(&self) -> &Self::Timestamp;

    /// Get task result
    fn result(&self) -> Option<&Self::StructData>;

    /// Get error message
    fn error(&self) -> Option<&str>;

    /// Update task status
    fn set_status(&mut self, status: Self::TaskStatus);

    /// Set task result
    fn set_result(&mut self, result: Self::StructData);

    /// Set error message
    fn set_error(&mut self, error: String);
}

// Orchestrator-related traits for ERGORS system
// Category: orchestrator
/// Core trait for cosmic context management
pub trait CosmicContextTrait {
    type PromptResponse;
    type StructData;

    /// Create new context
    fn new_context(task_id: String, user_input: String, total_steps: u32) -> Self;

    /// Get task ID
    fn task_id(&self) -> &str;

    /// Get user input
    fn user_input(&self) -> &str;

    /// Get current step
    fn current_step(&self) -> u32;

    /// Get total steps
    fn total_steps(&self) -> u32;

    /// Get fractal level
    fn fractal_level(&self) -> u32;

    /// Get golden ratio state
    fn golden_ratio_state(&self) -> f64;

    /// Get previous responses
    fn previous_responses(&self) -> &[Self::PromptResponse];

    /// Get cosmic metadata
    fn cosmic_metadata(&self) -> &HashMap<String, Self::StructData>;

    /// Add previous response
    fn add_response(&mut self, response: Self::PromptResponse);

    /// Set metadata
    fn set_metadata(&mut self, key: String, value: Self::StructData);

    /// Increment step
    fn next_step(&mut self);

    /// Increment fractal level
    fn next_fractal_level(&mut self);
}

// Orchestrator-related traits for ERGORS system
// Category: orchestrator
/// Core trait for orchestrator execution
#[async_trait]
pub trait OrchestratorTrait {
    type Task: CosmicTaskTrait;
    type Context: CosmicContextTrait;
    type Config;

    /// Create new orchestrator
    async fn new(config: Self::Config) -> HoResult<Self>
    where
        Self: Sized;

    /// Execute a cosmic task
    async fn execute_task(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Execute recursive orchestration
    async fn execute_recursive_orchestration(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Execute fractal agent creation
    async fn execute_fractal_agent_creation(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Create fractal context
    fn create_fractal_context(&self, task_id: String, prompt: &str, depth: u32) -> Self::Context;

    /// Apply golden ratio scaling
    fn apply_golden_ratio_scaling(&self, value: f64) -> f64 {
        value * 1.618033988749894
    }
}
