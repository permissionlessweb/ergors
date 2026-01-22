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

use crate::{
    error::HoResult,
    keys::commonware::{NodePrivKey, NodePubkey},
    types::ergors::network::v1::*,
};

use async_trait::async_trait;
use camino::Utf8Path;
use cnidarium::StateRead;
use commonware_cryptography::ed25519;
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

    /// Set only the public key (private key managed by custody)
    fn set_public_key(&mut self, public_key: &Self::PublicKey);

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

// ============================================
// Custody-backed Node Identity traits
// Category: custody, security
// ============================================

/// Custody backend type for node identity key management
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeIdentityCustodyBackend {
    /// Plaintext storage (legacy, insecure for production)
    Plaintext,
    /// Password-encrypted using ChaCha20Poly1305 + Argon2
    PasswordEncrypted,
    /// Encrypted using the node's own key (for API keys etc.)
    NodeKeyEncrypted,
    /// Threshold custody with distributed key shares
    Threshold,
    /// Remote custody service via gRPC
    RemoteCustody(String),
}

impl Default for NodeIdentityCustodyBackend {
    fn default() -> Self {
        Self::PasswordEncrypted
    }
}

/// Core trait for custody-backed node identity operations.
///
/// This trait abstracts over different custody backends (password-encrypted,
/// threshold, HSM, etc.) while providing a uniform interface for node identity
/// operations like signing and key access.
///
/// # Security
///
/// - Private keys are never stored in plaintext at rest
/// - Decryption happens on-demand and keys can be cached with TTL
/// - All key access operations can be audited
#[async_trait]
pub trait NodeIdentityCustody: Send + Sync {
    /// Get the custody backend type
    fn backend(&self) -> NodeIdentityCustodyBackend;

    /// Get the public key (always available without decryption)
    fn public_key(&self) -> HoResult<NodePubkey>;

    /// Get the private key, decrypting if necessary.
    ///
    /// This operation may require user interaction (password entry)
    /// or network calls (remote custody) depending on the backend.
    async fn get_private_key(&self) -> HoResult<NodePrivKey>;

    /// Sign a message using ed25519 with optional namespace.
    ///
    /// This is the primary operation for network authentication.
    /// The namespace is prepended to the message before signing.
    async fn sign_ed25519(
        &self,
        namespace: Option<&[u8]>,
        message: &[u8],
    ) -> HoResult<ed25519::Signature>;

    /// Export SSH keys to the specified directory for git operations.
    ///
    /// Creates id_ed25519 and id_ed25519.pub files in the directory.
    async fn export_ssh_keys(&self, ssh_dir: &Path) -> HoResult<()>;

    /// Check if the private key is currently cached/unlocked
    fn is_unlocked(&self) -> bool;

    /// Lock the custody, clearing any cached key material
    async fn lock(&self);

    /// Get the raw 32-byte private key bytes for encryption operations.
    ///
    /// This is used for deriving encryption keys for API keys etc.
    async fn get_key_bytes(&self) -> HoResult<[u8; 32]>;
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
    fn cost(&self) -> f64;

    /// Get latency
    fn latency_ms(&self) -> u64;

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

// ============================================
// Session-related traits for ERGORS system
// Category: session
// ============================================

use crate::types::ergors::management::v1::{
    CreateSessionRequest, FractalSession, QuerySessionsRequest, SessionPropagation,
    SessionStateSnapshot, SessionStatus, SessionUpdate, SpawnChildRequest,
};

/// Core trait for fractal session lifecycle management
/// Provides self-similar session operations at any hierarchy level
#[async_trait]
pub trait SessionTrait {
    type Session;
    type StateSnapshot;
    type Metrics;

    // === Lifecycle Operations ===

    /// Create a new session with optional parent for hierarchy
    async fn create(&self, request: CreateSessionRequest) -> HoResult<Self::Session>;

    /// Get a session by ID
    async fn get(&self, session_id: &str) -> HoResult<Option<Self::Session>>;

    /// Update session labels, metadata, and tags
    async fn update(
        &self,
        session_id: &str,
        labels: Option<HashMap<String, String>>,
        metadata: Option<HashMap<String, String>>,
        tags: Option<Vec<String>>,
    ) -> HoResult<Self::Session>;

    /// Delete a session (optionally cascade to children)
    async fn delete(&self, session_id: &str, cascade: bool) -> HoResult<()>;

    // === State Management ===

    /// Pause session execution, capturing state snapshot
    async fn pause(&self, session_id: &str, cascade: bool) -> HoResult<Self::StateSnapshot>;

    /// Resume a paused session from its state snapshot
    async fn resume(&self, session_id: &str, cascade: bool) -> HoResult<Self::Session>;

    /// Mark session as successfully completed
    async fn complete(
        &self,
        session_id: &str,
        result: Option<pbjson_types::Struct>,
    ) -> HoResult<Self::Session>;

    /// Mark session as failed with error details
    async fn fail(
        &self,
        session_id: &str,
        error: &str,
        error_code: Option<&str>,
    ) -> HoResult<Self::Session>;

    // === Status Queries ===

    /// Get current session status
    fn status(&self, session: &Self::Session) -> SessionStatus;

    /// Check if session is currently active
    fn is_active(&self, session: &Self::Session) -> bool;

    /// Check if session is a root session (no parent)
    fn is_root(&self, session: &Self::Session) -> bool;

    /// Check if session can be modified
    fn is_mutable(&self, session: &Self::Session) -> bool {
        let status = self.status(session);
        matches!(
            status,
            SessionStatus::Created | SessionStatus::Active | SessionStatus::Paused
        )
    }
}

/// Trait for fractal session hierarchy operations
/// Supports parent/child relationships and recursive metrics aggregation
#[async_trait]
pub trait FractalSessionTrait: SessionTrait {
    // === Hierarchy Operations ===

    /// Spawn a child session linked to parent
    async fn spawn_child(
        &self,
        parent_session_id: &str,
        request: SpawnChildRequest,
    ) -> HoResult<Self::Session>;

    /// Get parent session (None if root)
    async fn get_parent(&self, session_id: &str) -> HoResult<Option<Self::Session>>;

    /// Get direct children of a session
    async fn get_children(&self, session_id: &str) -> HoResult<Vec<Self::Session>>;

    /// Get the root session of any session in the hierarchy
    async fn get_root(&self, session_id: &str) -> HoResult<Self::Session>;

    /// Get all ancestors from session to root (root first)
    async fn get_ancestors(&self, session_id: &str) -> HoResult<Vec<Self::Session>>;

    /// Get all descendants (BFS order) with optional depth limit
    async fn get_descendants(
        &self,
        session_id: &str,
        max_depth: Option<u32>,
    ) -> HoResult<Vec<Self::Session>>;

    // === Fractal Metrics ===

    /// Rollup metrics from all descendants into parent
    async fn rollup_metrics(&self, session_id: &str) -> HoResult<Self::Metrics>;

    /// Get fractal depth (0 for root)
    fn fractal_depth(&self, session: &Self::Session) -> u32;

    /// Get direct child count
    fn child_count(&self, session: &Self::Session) -> u32;

    /// Get total descendant count (all nested children)
    fn descendant_count(&self, session: &Self::Session) -> u32;

    // === Propagation Rules ===

    /// Check if labels should inherit to children
    fn should_inherit_labels(&self, session: &Self::Session) -> bool;

    /// Check if metadata should inherit to children
    fn should_inherit_metadata(&self, session: &Self::Session) -> bool;

    /// Check if participants should inherit to children
    fn should_inherit_participants(&self, session: &Self::Session) -> bool;

    /// Get propagation configuration
    fn propagation(&self, session: &Self::Session) -> Option<&SessionPropagation>;
}

/// Trait for cross-node session coordination
/// Supports distributed session management across tetrahedral network
#[async_trait]
pub trait SessionCoordinationTrait: FractalSessionTrait {
    type Topology: NetworkTopologyTrait;

    // === Cross-Node Operations ===

    /// Sync session state to another node
    async fn sync_to_node(
        &self,
        session_id: &str,
        target_node_id: &str,
        full_sync: bool,
    ) -> HoResult<String>;

    /// Migrate session ownership to another node
    async fn migrate_to_node(
        &self,
        session_id: &str,
        target_node_id: &str,
        migrate_children: bool,
    ) -> HoResult<Self::Session>;

    // === Node Ownership ===

    /// Get owning node ID
    fn owner_node_id(&self, session: &Self::Session) -> &str;

    /// Get owning node type
    fn owner_node_type(&self, session: &Self::Session) -> NodeType;

    /// Check if this node owns the session
    fn is_local_owner(&self, session: &Self::Session) -> bool;

    // === Distributed Locking ===

    /// Acquire distributed lock on session
    async fn acquire_lock(&self, session_id: &str) -> HoResult<SessionLock>;

    /// Release distributed lock
    async fn release_lock(&self, lock: SessionLock) -> HoResult<()>;

    /// Check if session is locked
    async fn is_locked(&self, session_id: &str) -> HoResult<bool>;

    // === Notifications ===

    /// Notify all participants of session update
    async fn notify_participants(&self, session_id: &str, update: SessionUpdate) -> HoResult<()>;
}

/// Distributed lock for session operations
#[derive(Debug, Clone)]
pub struct SessionLock {
    pub session_id: String,
    pub lock_id: String,
    pub owner_node_id: String,
    pub acquired_at: std::time::SystemTime,
    pub expires_at: std::time::SystemTime,
}

impl SessionLock {
    /// Check if lock is still valid (not expired)
    pub fn is_valid(&self) -> bool {
        std::time::SystemTime::now() < self.expires_at
    }

    /// Get remaining time until expiration
    pub fn time_remaining(&self) -> Option<std::time::Duration> {
        self.expires_at
            .duration_since(std::time::SystemTime::now())
            .ok()
    }
}

/// Trait for session storage operations
/// Provides CRUD and indexing for fractal sessions
#[async_trait]
pub trait SessionStorageTrait {
    // === Core CRUD ===

    /// Store a session
    async fn put_session(&self, session: &FractalSession) -> HoResult<()>;

    /// Get a session by ID
    async fn get_session(&self, session_id: &str) -> HoResult<Option<FractalSession>>;

    /// Delete a session
    async fn delete_session(&self, session_id: &str) -> HoResult<()>;

    // === Query Operations ===

    /// Query sessions with filters
    async fn query_sessions(&self, query: &QuerySessionsRequest) -> HoResult<Vec<FractalSession>>;

    /// Count sessions matching query
    async fn count_sessions(&self, query: &QuerySessionsRequest) -> HoResult<u64>;

    // === Index Operations ===

    /// Get sessions by parent ID
    async fn get_sessions_by_parent(&self, parent_id: &str) -> HoResult<Vec<FractalSession>>;

    /// Get all sessions in a hierarchy by root ID
    async fn get_sessions_by_root(&self, root_id: &str) -> HoResult<Vec<FractalSession>>;

    /// Get sessions owned by a node
    async fn get_sessions_by_owner(&self, owner_node_id: &str) -> HoResult<Vec<FractalSession>>;

    /// Get sessions by status
    async fn get_sessions_by_status(&self, status: SessionStatus) -> HoResult<Vec<FractalSession>>;

    /// Get sessions by label key-value pair
    async fn get_sessions_by_label(&self, key: &str, value: &str) -> HoResult<Vec<FractalSession>>;

    /// Get sessions by tag
    async fn get_sessions_by_tag(&self, tag: &str) -> HoResult<Vec<FractalSession>>;

    // === State Snapshot Operations ===

    /// Store state snapshot for a session
    async fn put_state_snapshot(
        &self,
        session_id: &str,
        snapshot: &SessionStateSnapshot,
    ) -> HoResult<()>;

    /// Get latest state snapshot
    async fn get_state_snapshot(&self, session_id: &str) -> HoResult<Option<SessionStateSnapshot>>;

    /// Get state snapshot by version
    async fn get_state_snapshot_version(
        &self,
        session_id: &str,
        version: u64,
    ) -> HoResult<Option<SessionStateSnapshot>>;
}

/// Trait for session labeling and classification
/// Supports reinforcement learning classification
pub trait SessionLabelingTrait {
    // === Label Operations ===

    /// Add or update a label
    fn add_label(&mut self, key: &str, value: &str);

    /// Remove a label
    fn remove_label(&mut self, key: &str);

    /// Get a label value
    fn get_label(&self, key: &str) -> Option<&str>;

    /// Get all labels
    fn labels(&self) -> &HashMap<String, String>;

    // === Tag Operations ===

    /// Add a tag
    fn add_tag(&mut self, tag: &str);

    /// Remove a tag
    fn remove_tag(&mut self, tag: &str);

    /// Check if has tag
    fn has_tag(&self, tag: &str) -> bool;

    /// Get all tags
    fn tags(&self) -> &[String];

    // === Metadata Operations ===

    /// Set metadata value
    fn set_metadata(&mut self, key: &str, value: &str);

    /// Get metadata value
    fn get_metadata(&self, key: &str) -> Option<&str>;

    /// Get all metadata
    fn metadata(&self) -> &HashMap<String, String>;

    // === Classification ===

    /// Classify session for reinforcement learning
    fn classify_for_reinforcement(&self) -> SessionClassification;
}

/// Classification result for reinforcement learning
#[derive(Debug, Clone)]
pub struct SessionClassification {
    /// Success score (0.0-1.0)
    pub success_score: f64,
    /// Complexity score (0.0-1.0)
    pub complexity_score: f64,
    /// Learning value - how valuable for RL training
    pub learning_value: f64,
    /// Recommended labels based on analysis
    pub recommended_labels: Vec<(String, String)>,
    /// Recommended tags based on analysis
    pub recommended_tags: Vec<String>,
}
