//! Shared trait implementations for common CW-HO operations
//!
//! This module provides concrete implementations for traits that can be reused
//! across different parts of the system, following the fractal organizational patterns.

use crate::error::{HoError, Result};
use crate::traits::*;
use crate::utils::{FileOps, IdGenerator, NetworkUtils};
use async_trait::async_trait;
use ho_std::shim::Timestamp;
use ho_std::types::cw_ho::{
    network::v1::{NetworkConfig, NetworkMessage, NodeAnnounce, NodeIdentity, TetrahedralPing},
    orchestration::v1::{Message, PromptContext, PromptRequest, PromptResponse, TokenUsage},
    types::v1::{Connection, NetworkTopology, NodeInfo, NodeType},
};
use std::collections::HashMap;
use std::path::Path;

/// Shared implementation for file-based operations
pub struct FileShareImpl;

impl FileShareImpl {
    /// Share a file by copying it to a shared location
    pub fn share_file<P1: AsRef<Path>, P2: AsRef<Path>>(
        source: P1,
        shared_path: P2,
    ) -> Result<String> {
        let content = FileOps::read_string(&source)?;
        let shared_file_path = shared_path.as_ref().join(
            source
                .as_ref()
                .file_name()
                .ok_or_else(|| HoError::from("Invalid source file path".to_string()))?,
        );

        FileOps::write_string(&shared_file_path, &content)?;
        Ok(shared_file_path.to_string_lossy().to_string())
    }

    /// Save file with automatic directory creation
    pub fn save_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
        FileOps::write_string(path, content)
    }

    /// Create a backup of a file
    pub fn backup_file<P: AsRef<Path>>(path: P) -> Result<String> {
        let content = FileOps::read_string(&path)?;
        let backup_path = format!(
            "{}.backup.{}",
            path.as_ref().to_string_lossy(),
            IdGenerator::timestamp_seconds()
        );
        FileOps::write_string(&backup_path, &content)?;
        Ok(backup_path)
    }

    /// Sync files between two directories
    pub fn sync_files<P1: AsRef<Path>, P2: AsRef<Path>>(
        source_dir: P1,
        target_dir: P2,
        extension: Option<&str>,
    ) -> Result<Vec<String>> {
        let files = FileOps::list_files(&source_dir, extension)?;
        let mut synced_files = Vec::new();

        for file in files {
            let relative_path = file
                .strip_prefix(&source_dir)
                .map_err(|e| HoError::from(format!("Failed to get relative path: {}", e)))?;

            let target_file = target_dir.as_ref().join(relative_path);
            let content = FileOps::read_string(&file)?;
            FileOps::write_string(&target_file, &content)?;

            synced_files.push(target_file.to_string_lossy().to_string());
        }

        Ok(synced_files)
    }
}

/// Shared implementation for network operations
pub struct NetworkShareImpl;

impl NetworkShareImpl {
    /// Create a standard node announcement message
    pub fn create_node_announcement(identity: &NodeIdentity) -> NetworkMessage {
        let announce = NodeAnnounce {
            node_id: format!("{}:{}", identity.host, identity.p2p_port),
            role: NodeType::from_str_name(&identity.node_type).unwrap_or(NodeType::Unspecified)
                as i32,
            capabilities: vec![
                "llm_processing".to_string(),
                "task_coordination".to_string(),
            ],

            load_factor: 0.5.to_string(), // Default load factor
        };

        NetworkMessage {
            message_type: Some(
                ho_std::types::cw_ho::network::v1::network_message::MessageType::NodeAnnounce(
                    announce,
                ),
            ),
        }
    }

    /// Create a tetrahedral ping message
    pub fn create_ping(from_node: &str, topology: NetworkTopology) -> NetworkMessage {
        let ping = TetrahedralPing {
            from_node: from_node.to_string(),
            timestamp: IdGenerator::timestamp_seconds(),
            network_topology: Some(topology),
        };

        NetworkMessage {
            message_type: Some(ho_std::types::cw_ho::network::v1::network_message::MessageType::TetrahedralPing(ping)),
        }
    }

    /// Validate network addresses
    pub fn validate_network_config(config: &NetworkConfig) -> Result<()> {
        NetworkUtils::validate_port(config.listen_port)?;

        if config.listen_address.is_empty() {
            return Err(HoError::Network(
                "Listen address cannot be empty".to_string(),
            ));
        }

        if config.max_peers == 0 {
            return Err(HoError::Network(
                "Max peers must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Create a standard network topology from node list
    pub fn create_topology(nodes: Vec<NodeInfo>) -> NetworkTopology {
        let mut connections = Vec::new();

        // Create mesh connections between all online nodes
        for (i, node1) in nodes.iter().enumerate() {
            if !node1.online {
                continue;
            }

            for node2 in nodes.iter().skip(i + 1) {
                if !node2.online {
                    continue;
                }

                connections.push(Connection {
                    from_node_id: node1.node_id.clone(),
                    to_node_id: node2.node_id.clone(),
                });

                // Add reverse connection for bidirectional mesh
                connections.push(Connection {
                    from_node_id: node2.node_id.clone(),
                    to_node_id: node1.node_id.clone(),
                });
            }
        }

        NetworkTopology { nodes, connections }
    }
}

/// Shared implementation for LLM operations
pub struct LlmShareImpl;

impl LlmShareImpl {
    /// Create a standard user message
    pub fn create_user_message(content: String) -> Message {
        Message {
            role: "user".to_string(),
            content,
        }
    }

    /// Create a standard assistant message
    pub fn create_assistant_message(content: String) -> Message {
        Message {
            role: "assistant".to_string(),
            content,
        }
    }

    /// Create a standard system message
    pub fn create_system_message(content: String) -> Message {
        Message {
            role: "system".to_string(),
            content,
        }
    }

    /// Create a prompt context with session tracking
    pub fn create_prompt_context(
        session_id: Option<String>,
        user_id: Option<String>,
    ) -> PromptContext {
        PromptContext {
            session_id: session_id.or_else(|| Some(IdGenerator::new_uuid_string())),
            user_id,
            thread_id: Some(IdGenerator::new_uuid_string()),
        }
    }

    /// Merge multiple prompt responses into a summary
    pub fn merge_responses(responses: Vec<PromptResponse>) -> Result<PromptResponse> {
        if responses.is_empty() {
            return Err(HoError::from("No responses to merge".to_string()));
        }

        let mut total_prompt_tokens = 0;
        let mut total_completion_tokens = 0;
        let mut total_cost = 0.0;
        let mut combined_response = String::new();
        let mut models = Vec::new();
        let mut providers = Vec::new();

        for response in &responses {
            if let Some(tokens) = &response.tokens_used {
                total_prompt_tokens += tokens.prompt;
                total_completion_tokens += tokens.completion;
            }

            if let Some(cost) = response.cost {
                total_cost += cost;
            }

            combined_response.push_str(&response.response);
            combined_response.push('\n');
            models.push(response.model.clone());
            providers.push(response.provider.clone());
        }

        Ok(PromptResponse {
            id: IdGenerator::new_uuid_bytes(),
            provider: format!("merged({})", providers.join(",")),
            model: format!("multi({})", models.join(",")),
            prompt: responses.first().unwrap().prompt.clone(),
            response: combined_response.trim().to_string(),
            timestamp: None,
            tokens_used: Some(TokenUsage {
                prompt: total_prompt_tokens,
                completion: total_completion_tokens,
                total: total_prompt_tokens + total_completion_tokens,
            }),
            cost: Some(total_cost),
            latency_ms: responses.iter().filter_map(|r| r.latency_ms).max(),
        })
    }

    /// Create a fractal prompt that includes context for recursive processing
    pub fn create_fractal_prompt(
        base_prompt: &str,
        context_responses: Vec<PromptResponse>,
        depth: u32,
    ) -> String {
        let mut fractal_prompt = format!(
            "FRACTAL PROCESSING DEPTH: {}\n\nBASE PROMPT:\n{}\n\n",
            depth, base_prompt
        );

        if !context_responses.is_empty() {
            fractal_prompt.push_str("PREVIOUS CONTEXT:\n");
            for (i, response) in context_responses.iter().enumerate() {
                fractal_prompt.push_str(&format!(
                    "Context {}: {} ({})\n{}\n\n",
                    i + 1,
                    response.provider,
                    response.model,
                    response.response
                ));
            }
        }

        fractal_prompt.push_str("GOLDEN RATIO GUIDANCE: Apply the golden ratio (φ ≈ 1.618) in your response structure, ensuring harmonic proportions between different sections.\n\n");
        fractal_prompt.push_str("GEOMETRIC COHERENCE: Maintain self-similarity across different scales of your response, reflecting the fractal nature of the processing.\n\n");

        fractal_prompt
    }
}
