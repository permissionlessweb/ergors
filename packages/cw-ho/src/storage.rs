use crate::ErgorsAppState;
use axum::{extract::State, Json};
use cnidarium::{StateRead, StateWrite, Storage as CnidariumStorage};
use futures::StreamExt;
use ho_std::error::error_json;
use ho_std::llm::{HoError, HoResult};
use ho_std::types::ergors::{
    management::v1::{
        FractalSession, QuerySessionsRequest, SessionStateSnapshot, SessionStatus, SessionType,
    },
    orch::v1::*,
    proxy::v1::*,
    storage::v1::*,
};
use std::path::Path;
use tracing::{debug, info, warn};
use uuid::Uuid;

const PROMPT_PREFIX: &str = "prompts/";
const SESSION_INDEX_PREFIX: &str = "sessions/";
const USER_INDEX_PREFIX: &str = "users/";
const TIMESTAMP_INDEX_PREFIX: &str = "timestamps/";
const OP_PREFIX: &str = "operations/";
const API_KEY_PREFIX: &str = "custody/api_keys/";
const COSMOS_KEY_STORE_KEY: &str = "custody/cosmos_key_store";
const AKASH_WORKFLOW_PREFIX: &str = "akash_workflows/";
// const HEADSTASH: &str = "headstash/";
const PROXY_SESSION_PREFIX: &str = "proxy_sessions/";
const PROXY_CLIENT_INDEX_PREFIX: &str = "proxy_sessions_by_client/";

// Git Workspace Storage Prefixes
pub const WORKSPACE_PREFIX: &str = "workspaces/";
pub const TASK_WORKTREE_PREFIX: &str = "task_worktrees/";
pub const WORKTREE_BY_WORKSPACE_PREFIX: &str = "worktrees_by_workspace/";
pub const WORKTREE_BY_NODE_PREFIX: &str = "worktrees_by_node/";

// Fractal Session Storage Prefixes
const FRACTAL_SESSION_PREFIX: &str = "fractal_sessions/";
const SESSION_BY_PARENT_PREFIX: &str = "sessions_by_parent/";
const SESSION_BY_ROOT_PREFIX: &str = "sessions_by_root/";
const SESSION_BY_OWNER_PREFIX: &str = "sessions_by_owner/";
const SESSION_BY_STATUS_PREFIX: &str = "sessions_by_status/";
const SESSION_BY_TYPE_PREFIX: &str = "sessions_by_type/";
const SESSION_BY_LABEL_PREFIX: &str = "sessions_by_label/";
const SESSION_BY_TAG_PREFIX: &str = "sessions_by_tag/";
const SESSION_STATE_PREFIX: &str = "session_states/";
const SESSION_LOCK_PREFIX: &str = "session_locks/";

// Open Responses Storage Prefix
const OPEN_RESPONSE_PREFIX: &str = "open_responses/";

// Custom Authenticator Storage Prefixes
const AUTHENTICATOR_PREFIX: &str = "authenticators/";
const AUTHENTICATOR_META_PREFIX: &str = "authenticators/metadata/";

/// Defines the storage used for this CwHo. implemenations in ./storage.rs
pub struct ErgorsStorage {
    pub cs: CnidariumStorage,
}

impl ErgorsStorage {
    pub async fn new<P: AsRef<Path>>(data_dir: P, prefixes: Vec<String>) -> HoResult<Self> {
        info!("📂 Initializing Cnidarium storage");
        let path = data_dir.as_ref();
        std::fs::create_dir_all(path)?;
        Ok(Self {
            cs: CnidariumStorage::load(path.to_path_buf(), prefixes).await?,
        })
    }

    pub async fn put_prompt_w_ctx(
        &self,
        prompt: &PromptResponse,
        original_request: Option<&PromptRequest>,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let id = hex::encode(prompt.id.clone());
        // Serialize the prompt response
        let prompt_data = serde_json::to_vec(prompt)?;
        let prompt_key = format!("{}{}", PROMPT_PREFIX, id.clone());

        // Store the main prompt record
        delta.put_raw(prompt_key.clone(), prompt_data);

        // Create indexes for efficient querying
        let timestamp_key = format!(
            "{}{:020}:{}",
            TIMESTAMP_INDEX_PREFIX,
            prompt
                .timestamp
                .expect("should always have timestamp")
                .nanos,
            id
        );
        delta.put_raw(timestamp_key, prompt.id.clone());

        // Create context-based indexes if original request is provided
        if let Some(request) = original_request {
            if let Some(ref context) = request.context {
                // Index by session_id
                let sid = context.session_id.clone();
                let uid = context.user_id.clone();

                let session_key = format!("{}{}:{}", SESSION_INDEX_PREFIX, sid, id);
                delta.put_raw(session_key, prompt.id.clone());
                debug!("Created session index for {}: {}", sid, id);

                // Index by user_id

                let user_key = format!("{}{}:{}", USER_INDEX_PREFIX, uid, id);
                delta.put_raw(user_key, prompt.id.clone());
                debug!("Created user index for {}: {}", uid, id);
            }
        }

        debug!("Storing prompt {} with timestamp index", id);

        // Commit the changes
        self.cs.commit(delta).await?;

        info!(
            "💾 Successfully stored prompt: {} with key: {}",
            id, prompt_key
        );

        // Debug: Let's try to immediately read it back to verify storage
        match self
            .get_prompt(&Uuid::from_slice(&prompt.id).unwrap())
            .await
        {
            Ok(Some(_)) => info!("✅ Verified prompt {} can be read back immediately", id),
            Ok(None) => warn!("⚠️ Prompt {} not found immediately after storage", id),
            Err(e) => warn!("❌ Error reading prompt {} back: {}", id, e),
        }

        Ok(())
    }

    /// Store Prompt to node storage.
    pub async fn put_prompt(&self, prompt: &PromptResponse) -> HoResult<()> {
        self.put_prompt_w_ctx(prompt, None).await
    }

    pub async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<PromptResponse>> {
        let snapshot = self.cs.latest_snapshot();
        let prompt_key = format!("{}{}", PROMPT_PREFIX, id);

        match snapshot.get_raw(&prompt_key).await {
            Ok(Some(data)) => {
                let prompt: PromptResponse = serde_json::from_slice(&data)?;
                Ok(Some(prompt))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get prompt {}: {}", id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Query Prompt to node storage
    pub async fn get_prompts(&self, query: &QueryRequest) -> HoResult<Vec<PromptResponse>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let limit = query.limit.unwrap_or(100).min(1000); // Cap at 1000

        info!(
            "🔍 Querying prompts with prefix '{}' and limit: {}",
            PROMPT_PREFIX, limit
        );

        // For now, let's implement a simple approach that scans all prompts
        // We'll use the prompt prefix to get all stored prompts
        let mut prompt_stream = snapshot.prefix_raw(PROMPT_PREFIX);
        let mut count = 0;
        let mut total_entries = 0;

        while let Some(entry_result) = prompt_stream.next().await {
            total_entries += 1;
            if count >= limit {
                break;
            }

            match entry_result {
                Ok((key, value)) => {
                    let key_str = String::from_utf8_lossy(key.as_bytes());
                    debug!(
                        "📋 Found entry with key: {}, value size: {} bytes",
                        key_str,
                        value.len()
                    );

                    // Deserialize the prompt response
                    match serde_json::from_slice::<PromptResponse>(&value) {
                        Ok(prompt) => {
                            let id = hex::encode(prompt.id.clone()).to_string();
                            debug!("✅ Successfully deserialized prompt: {}", id);

                            // Apply filters
                            let matches_filters = self.matches_query_filters(&prompt, query);
                            debug!(
                                "🔍 Prompt {} matches filters: {}",
                                id.to_string(),
                                matches_filters
                            );

                            if matches_filters {
                                results.push(prompt);
                                count += 1;
                                info!("➕ Added prompt to results, count now: {}", count);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize prompt from key {}: {}", key_str, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading from storage stream: {}", e);
                    continue;
                }
            }
        }

        // Sort by timestamp (most recent first)
        results.sort_by(|a, b| {
            let b_ts = b.timestamp.expect("always have one b_ts");
            let a_ts = a.timestamp.expect("always have one a_ts");

            // Compare seconds first, then nanoseconds
            b_ts.seconds
                .cmp(&a_ts.seconds)
                .then_with(|| b_ts.nanos.cmp(&a_ts.nanos))
        });

        info!(
            "🔍 Query scanned {} total entries, returned {} results",
            total_entries,
            results.len()
        );
        Ok(results)
    }

    // ===== Open Responses Session Storage =====

    /// Store an Open Responses session (request + response) for conversation continuity.
    pub async fn put_open_response(
        &self,
        response_id: &str,
        request: &PromptRequest,
        response_text: &[String],
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Store the conversation context: request messages + response
        let session_data = serde_json::json!({
            "request_messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            }).collect::<Vec<_>>(),
            "response_messages": response_text,
            "model": request.model,
            "system": request.system,
        });

        let key = format!("{}{}", OPEN_RESPONSE_PREFIX, response_id);
        delta.put_raw(key, serde_json::to_vec(&session_data)?);
        self.cs.commit(delta).await?;

        debug!("Stored Open Response session: {}", response_id);
        Ok(())
    }

    /// Load previous conversation context for `previous_response_id`.
    /// Returns messages to prepend to the current request.
    pub async fn get_open_response_context(
        &self,
        response_id: &str,
    ) -> HoResult<Vec<PromptMessage>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", OPEN_RESPONSE_PREFIX, response_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let session: serde_json::Value = serde_json::from_slice(&data)?;

                let mut messages = Vec::new();

                // Reconstruct request messages
                if let Some(req_msgs) = session.get("request_messages").and_then(|v| v.as_array())
                {
                    for msg in req_msgs {
                        messages.push(PromptMessage {
                            role: msg
                                .get("role")
                                .and_then(|v| v.as_str())
                                .unwrap_or("user")
                                .to_string(),
                            content: msg
                                .get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            tool_calls: vec![],
                            tool_result: None,
                            content_blocks: vec![],
                        });
                    }
                }

                // Add response as assistant message
                if let Some(resp_msgs) =
                    session.get("response_messages").and_then(|v| v.as_array())
                {
                    let combined: String = resp_msgs
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !combined.is_empty() {
                        messages.push(PromptMessage {
                            role: "assistant".to_string(),
                            content: combined,
                            tool_calls: vec![],
                            tool_result: None,
                            content_blocks: vec![],
                        });
                    }
                }

                Ok(messages)
            }
            Ok(None) => Ok(vec![]),
            Err(e) => {
                warn!(
                    "Failed to get open response context {}: {}",
                    response_id, e
                );
                Ok(vec![])
            }
        }
    }

    fn matches_query_filters(&self, prompt: &PromptResponse, query: &QueryRequest) -> bool {
        let prompt_nano = prompt.timestamp.expect("should have a time").nanos;
        // Apply time filters if specified
        let matches_time_filter = match (query.start_time, query.end_time) {
            (Some(start), Some(end)) => {
                prompt_nano >= start.nanos
                    && prompt.timestamp.expect("must have timestamp").nanos <= end.nanos
            }
            (Some(start), None) => prompt_nano >= start.nanos,
            (None, Some(end)) => prompt_nano <= end.nanos,
            (None, None) => true,
        };

        if !matches_time_filter {
            return false;
        }

        // Apply session_id filter if specified
        if let Some(_query_session_id) = &query.session_id {
            // if let Some(ref context) = prompt.context {
            //     if let Some(ref session_id) = context.session_id {
            //         if session_id != query_session_id {
            //             return false;
            //         }
            //     } else {
            //         return false; // No session_id in prompt, but filter requires it
            //     }
            // } else {
            //     return false; // No context in prompt, but filter requires session_id
            // }
        }

        // Apply user_id filter if specified
        if let Some(_query_user_id) = &query.user_id {
            // if let Some(ref context) = prompt.context {
            //     if let Some(ref user_id) = context.user_id {
            //         if user_id != query_user_id {
            //             return false;
            //         }
            //     } else {
            //         return false; // No user_id in prompt, but filter requires it
            //     }
            // } else {
            //     return false; // No context in prompt, but filter requires user_id
            // }
        }

        true
    }

    pub async fn health_check(&self) -> HoResult<()> {
        // Try to get the latest snapshot to verify storage is accessible
        let _snapshot = self.cs.latest_snapshot();

        // Try a simple read operation
        let test_key = "health_check";
        let snapshot = self.cs.latest_snapshot();

        match snapshot.get_raw(test_key).await {
            Ok(_) => Ok(()), // Whether it exists or not, storage is accessible
            Err(e) => {
                warn!("Storage health check failed: {}", e);
                Err(HoError::Storage(e.to_string()))
            }
        }
    }
    pub async fn prune_storage(&self) -> HoResult<()> {
        unimplemented!();
    }

    pub async fn create_snapshot(&self) -> HoResult<()> {
        // Create a named snapshot for backup/recovery
        let snapshot_name = format!("snapshot_{}", chrono::Utc::now().timestamp());

        // TODO: ensure we are accurately taking the snapshots (needs tests)
        let _snapshot = self.cs.latest_snapshot();
        info!("📸 Created logical snapshot: {}", snapshot_name);

        Ok(())
    }

    /// Store operation record (request only, response pending)
    pub async fn op_req(
        &self,
        id: &str,
        operation_type: &str,
        endpoint: &str,
        request_data: Vec<u8>,
        session_id: Option<String>,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let op_key = format!("{}{}", OP_PREFIX, id);
        let operation = OperationRecord {
            id: op_key.to_string(),
            operation_type: operation_type.to_string(),
            endpoint: endpoint.to_string(),
            request: request_data,
            response: None,
            error: None,
            started_at: Some(pbjson_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            completed_at: None,
            session_id,
        };

        let operation_data = serde_json::to_vec(&operation)?;
        delta.put_raw(op_key.clone(), operation_data);

        // Create timestamp index
        let timestamp_key = format!(
            "{}operations/{:020}:{}",
            TIMESTAMP_INDEX_PREFIX,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            id
        );
        delta.put_raw(timestamp_key, id.as_bytes().to_vec());

        self.cs.commit(delta).await?;

        debug!("📝 Stored operation request: {} ({})", id, operation_type);
        Ok(())
    }

    /// Update operation record with response
    pub async fn op_res(&self, id: &str, response_data: Vec<u8>) -> HoResult<()> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", OP_PREFIX, id);

        // Get existing operation
        let existing_data = snapshot
            .get_raw(&key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Operation not found: {}", id))?;

        let mut operation: OperationRecord = serde_json::from_slice(&existing_data)?;

        // Update with response
        operation.response = Some(response_data);
        operation.completed_at = Some(pbjson_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        });

        let mut delta = cnidarium::StateDelta::new(snapshot);
        let operation_data = serde_json::to_vec(&operation)?;
        delta.put_raw(key, operation_data);

        self.cs.commit(delta).await?;

        debug!("✅ Updated operation with response: {}", id);
        Ok(())
    }

    /// Update operation record with error
    pub async fn op_err(
        &self,
        id: &str,
        error_msg: &str,
        error_code: &str,
        stack_trace: Option<String>,
    ) -> HoResult<()> {
        let snapshot = self.cs.latest_snapshot();
        let op_key = format!("{}{}", OP_PREFIX, id);

        // Get existing operation
        let existing_data = snapshot
            .get_raw(&op_key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Operation not found: {}", id))?;

        let mut operation: OperationRecord = serde_json::from_slice(&existing_data)?;

        // Update with error
        operation.error = Some(ErrorResponse {
            error: error_msg.to_string(),
            code: error_code.to_string(),
            timestamp: Some(pbjson_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            stack_trace,
        });
        operation.completed_at = Some(pbjson_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        });

        let mut delta = cnidarium::StateDelta::new(snapshot);
        let operation_data = serde_json::to_vec(&operation)?;
        delta.put_raw(op_key, operation_data);

        self.cs.commit(delta).await?;

        warn!("❌ Recorded operation error: {} - {}", id, error_msg);
        Ok(())
    }

    /// q_ops: Query operations
    pub async fn q_ops(
        &self,
        operation_type: Option<&str>,
        limit: Option<u32>,
    ) -> HoResult<Vec<OperationRecord>> {
        let mut results = Vec::new();
        let mut count = 0;
        let mut operation_stream = self.cs.latest_snapshot().prefix_raw(OP_PREFIX);
        while let Some(entry_result) = operation_stream.next().await {
            if count >= limit.unwrap_or(100).min(1000) {
                break;
            }
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<OperationRecord>(&value) {
                        Ok(operation) => {
                            // Filter by operation type if specified
                            if let Some(op_type) = operation_type {
                                if operation.operation_type != op_type {
                                    continue;
                                }
                            }
                            results.push(operation);
                            count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to deserialize operation: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading operation stream: {}", e);
                    continue;
                }
            }
        }

        // Sort by timestamp (most recent first)
        results.sort_by(|a, b| {
            let b_ts = b.started_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let a_ts = a.started_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        info!("🔍 Query returned {} operations", results.len());
        Ok(results)
    }

    /// Get a specific operation by ID
    pub async fn q_op(&self, id: &str) -> HoResult<Option<OperationRecord>> {
        let snapshot = self.cs.latest_snapshot();
        let op_key = format!("{}{}", OP_PREFIX, id);

        match snapshot.get_raw(&op_key).await {
            Ok(Some(data)) => {
                let operation: OperationRecord = serde_json::from_slice(&data)?;
                Ok(Some(operation))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get operation {}: {}", id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Store encrypted API key
    pub async fn put_encrypted_api_key(
        &self,
        provider_name: &str,
        encrypted_key: &ho_std::types::ergors::storage::v1::EncryptedApiKey,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", API_KEY_PREFIX, provider_name);

        let data = serde_json::to_vec(encrypted_key)?;
        delta.put_raw(key.clone(), data);

        self.cs.commit(delta).await?;
        info!(
            "🔐 Stored encrypted API key for provider: {}",
            provider_name
        );
        Ok(())
    }

    /// Get encrypted API key
    pub async fn get_encrypted_api_key(
        &self,
        provider_name: &str,
    ) -> HoResult<Option<ho_std::types::ergors::storage::v1::EncryptedApiKey>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", API_KEY_PREFIX, provider_name);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let encrypted_key: ho_std::types::ergors::storage::v1::EncryptedApiKey =
                    serde_json::from_slice(&data)?;
                Ok(Some(encrypted_key))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!(
                    "Failed to get encrypted API key for {}: {}",
                    provider_name, e
                );
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Delete encrypted API key
    pub async fn delete_encrypted_api_key(&self, provider_name: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", API_KEY_PREFIX, provider_name);

        delta.delete(key);
        self.cs.commit(delta).await?;
        info!(
            "🗑️  Deleted encrypted API key for provider: {}",
            provider_name
        );
        Ok(())
    }

    /// List all providers with stored encrypted API keys
    pub async fn list_api_key_providers(&self) -> HoResult<Vec<String>> {
        let snapshot = self.cs.latest_snapshot();
        let mut providers = Vec::new();
        let mut stream = snapshot.prefix_raw(API_KEY_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((key, _)) => {
                    let key_str = String::from_utf8_lossy(key.as_bytes());
                    if let Some(provider) = key_str.strip_prefix(API_KEY_PREFIX) {
                        providers.push(provider.to_string());
                    }
                }
                Err(e) => {
                    warn!("Error reading API key provider stream: {}", e);
                    continue;
                }
            }
        }

        Ok(providers)
    }

    // ========================================
    // Cosmos Key Storage Methods
    // ========================================

    /// Store the cosmos key store (all encrypted cosmos keys)
    pub async fn put_cosmos_key_store(&self, store: &CosmosKeyStore) -> HoResult<()> {
        use ho_std::Message as _;
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let data = store.encode_to_vec();
        delta.put_raw(COSMOS_KEY_STORE_KEY.to_string(), data);
        self.cs.commit(delta).await?;
        info!(
            "🔐 Stored cosmos key store with {} keys",
            store.keys.len()
        );
        Ok(())
    }

    /// Get the cosmos key store
    pub async fn get_cosmos_key_store(&self) -> HoResult<Option<CosmosKeyStore>> {
        use ho_std::Message as _;
        let snapshot = self.cs.latest_snapshot();
        match snapshot.get_raw(COSMOS_KEY_STORE_KEY).await {
            Ok(Some(data)) => {
                let store = CosmosKeyStore::decode(data.as_slice()).map_err(|e| {
                    HoError::DeSerialization(format!("Failed to decode cosmos key store: {}", e))
                })?;
                Ok(Some(store))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get cosmos key store: {}", e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Check if cosmos key store exists
    pub async fn has_cosmos_key_store(&self) -> bool {
        let snapshot = self.cs.latest_snapshot();
        matches!(snapshot.get_raw(COSMOS_KEY_STORE_KEY).await, Ok(Some(_)))
    }

    // ========================================
    // Akash Workflow Storage Methods
    // ========================================

    /// Store an Akash deployment workflow
    pub async fn put_akash_workflow(&self, workflow: &AkashDeploymentWorkflow) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", AKASH_WORKFLOW_PREFIX, workflow.session_id);
        let data = serde_json::to_vec(workflow)?;
        delta.put_raw(key.clone(), data);
        self.cs.commit(delta).await?;
        info!(
            "💾 Stored Akash workflow: {} (step: {:?})",
            workflow.session_id, workflow.current_step
        );
        Ok(())
    }

    /// Get an Akash deployment workflow by session ID
    pub async fn get_akash_workflow(
        &self,
        session_id: &str,
    ) -> HoResult<Option<AkashDeploymentWorkflow>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", AKASH_WORKFLOW_PREFIX, session_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let workflow: AkashDeploymentWorkflow = serde_json::from_slice(&data)?;
                Ok(Some(workflow))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get Akash workflow {}: {}", session_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// List all Akash workflows
    pub async fn list_akash_workflows(&self) -> HoResult<Vec<AkashDeploymentWorkflow>> {
        let snapshot = self.cs.latest_snapshot();
        let mut workflows = Vec::new();
        let mut stream = snapshot.prefix_raw(AKASH_WORKFLOW_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_, data)) => {
                    match serde_json::from_slice::<AkashDeploymentWorkflow>(&data) {
                        Ok(workflow) => workflows.push(workflow),
                        Err(e) => warn!("Failed to deserialize Akash workflow: {}", e),
                    }
                }
                Err(e) => {
                    warn!("Error reading Akash workflow stream: {}", e);
                    continue;
                }
            }
        }

        // Sort by created_at (most recent first)
        workflows.sort_by(|a, b| {
            let b_ts = b.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let a_ts = a.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        Ok(workflows)
    }

    /// Delete an Akash workflow
    pub async fn delete_akash_workflow(&self, session_id: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", AKASH_WORKFLOW_PREFIX, session_id);
        delta.delete(key);
        self.cs.commit(delta).await?;
        info!("🗑️  Deleted Akash workflow: {}", session_id);
        Ok(())
    }

    // ========================================
    // Proxy Session Storage Methods
    // ========================================

    /// Store a proxy session to persistent storage.
    pub async fn put_proxy_session(&self, session: &ProxySession) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Main session record
        let session_key = format!("{}{}", PROXY_SESSION_PREFIX, session.session_id);
        let session_data = serde_json::to_vec(session)?;
        delta.put_raw(session_key.clone(), session_data);

        // Index by client type
        let client_type_name = match session.client_type {
            0 => "unspecified",
            1 => "claude_code",
            2 => "opencode",
            3 => "cursor",
            4 => "custom",
            _ => "unknown",
        };
        let client_index_key = format!(
            "{}{}:{}",
            PROXY_CLIENT_INDEX_PREFIX, client_type_name, session.session_id
        );
        delta.put_raw(client_index_key, session.session_id.as_bytes().to_vec());

        // Index by timestamp for efficient time-range queries
        if let Some(ref ts) = session.started_at {
            let ts_index_key = format!(
                "proxy_sessions_by_time/{:020}:{}",
                ts.seconds, session.session_id
            );
            delta.put_raw(ts_index_key, session.session_id.as_bytes().to_vec());
        }

        self.cs.commit(delta).await?;

        info!(
            "💾 Stored proxy session: {} (client: {}, model: {})",
            session.session_id, client_type_name, session.model
        );

        Ok(())
    }

    /// Get a proxy session by ID.
    pub async fn get_proxy_session(&self, session_id: &str) -> HoResult<Option<ProxySession>> {
        let snapshot = self.cs.latest_snapshot();
        let session_key = format!("{}{}", PROXY_SESSION_PREFIX, session_id);

        match snapshot.get_raw(&session_key).await {
            Ok(Some(data)) => {
                let session: ProxySession = serde_json::from_slice(&data)?;
                Ok(Some(session))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get proxy session {}: {}", session_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Query proxy sessions with filters.
    pub async fn query_proxy_sessions(
        &self,
        query: &QueryProxySessionsRequest,
    ) -> HoResult<Vec<ProxySession>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let limit = query.limit.max(1).min(1000) as usize;

        info!(
            "🔍 Querying proxy sessions with limit: {}, offset: {}",
            limit, query.offset
        );

        let mut stream = snapshot.prefix_raw(PROXY_SESSION_PREFIX);
        let mut count = 0;
        let mut skipped = 0;

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<ProxySession>(&value) {
                        Ok(session) => {
                            // Apply filters
                            if query.client_type != 0 && session.client_type != query.client_type {
                                continue;
                            }
                            if query.api_format != 0 && session.api_format != query.api_format {
                                continue;
                            }
                            if !query.model.is_empty() && session.model != query.model {
                                continue;
                            }

                            // Apply offset
                            if skipped < query.offset as usize {
                                skipped += 1;
                                continue;
                            }

                            // Apply limit
                            if count >= limit {
                                break;
                            }

                            results.push(session);
                            count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to deserialize proxy session: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading proxy session stream: {}", e);
                }
            }
        }

        // Sort by start time (most recent first)
        results.sort_by(|a, b| {
            let b_ts = b.started_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let a_ts = a.started_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        info!("🔍 Query returned {} proxy sessions", results.len());
        Ok(results)
    }

    /// Delete a proxy session.
    pub async fn delete_proxy_session(&self, session_id: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let session_key = format!("{}{}", PROXY_SESSION_PREFIX, session_id);
        delta.delete(session_key);
        self.cs.commit(delta).await?;
        info!("🗑️  Deleted proxy session: {}", session_id);
        Ok(())
    }

    // ========================================
    // Fractal Session Storage Methods
    // ========================================

    /// Store a fractal session with all indices.
    pub async fn put_fractal_session(&self, session: &FractalSession) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Main session record
        let session_key = format!("{}{}", FRACTAL_SESSION_PREFIX, session.session_id);
        let session_data = serde_json::to_vec(session)?;
        delta.put_raw(session_key.clone(), session_data);

        // Index by parent (for hierarchy traversal)
        if !session.parent_session_id.is_empty() {
            let parent_index_key = format!(
                "{}{}:{}",
                SESSION_BY_PARENT_PREFIX, session.parent_session_id, session.session_id
            );
            delta.put_raw(parent_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by root (for full hierarchy queries)
        if !session.root_session_id.is_empty() {
            let root_index_key = format!(
                "{}{}:{:04}:{}",
                SESSION_BY_ROOT_PREFIX,
                session.root_session_id,
                session.fractal_depth,
                session.session_id
            );
            delta.put_raw(root_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by owner node
        if !session.owner_node_id.is_empty() {
            let owner_index_key = format!(
                "{}{}:{}",
                SESSION_BY_OWNER_PREFIX, session.owner_node_id, session.session_id
            );
            delta.put_raw(owner_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by status
        let status_name = match SessionStatus::try_from(session.status) {
            Ok(s) => format!("{:?}", s).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let status_index_key = format!(
            "{}{}:{}",
            SESSION_BY_STATUS_PREFIX, status_name, session.session_id
        );
        delta.put_raw(status_index_key, session.session_id.as_bytes().to_vec());

        // Index by type
        let type_name = match SessionType::try_from(session.session_type) {
            Ok(t) => format!("{:?}", t).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let type_index_key = format!(
            "{}{}:{}",
            SESSION_BY_TYPE_PREFIX, type_name, session.session_id
        );
        delta.put_raw(type_index_key, session.session_id.as_bytes().to_vec());

        // Index by labels
        for (key, value) in &session.labels {
            let label_index_key = format!(
                "{}{}:{}:{}",
                SESSION_BY_LABEL_PREFIX, key, value, session.session_id
            );
            delta.put_raw(label_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by tags
        for tag in &session.tags {
            let tag_index_key = format!("{}{}:{}", SESSION_BY_TAG_PREFIX, tag, session.session_id);
            delta.put_raw(tag_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by timestamp
        if let Some(ref ts) = session.created_at {
            let ts_index_key = format!(
                "fractal_sessions_by_time/{:020}:{}",
                ts.seconds, session.session_id
            );
            delta.put_raw(ts_index_key, session.session_id.as_bytes().to_vec());
        }

        self.cs.commit(delta).await?;

        info!(
            "💾 Stored fractal session: {} (type: {}, depth: {}, parent: {})",
            session.session_id,
            type_name,
            session.fractal_depth,
            if session.parent_session_id.is_empty() {
                "none (root)"
            } else {
                &session.parent_session_id
            }
        );

        Ok(())
    }

    /// Get a fractal session by ID.
    pub async fn get_fractal_session(&self, session_id: &str) -> HoResult<Option<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let session_key = format!("{}{}", FRACTAL_SESSION_PREFIX, session_id);

        match snapshot.get_raw(&session_key).await {
            Ok(Some(data)) => {
                let session: FractalSession = serde_json::from_slice(&data)?;
                Ok(Some(session))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get fractal session {}: {}", session_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Get sessions by parent ID (direct children only).
    pub async fn get_sessions_by_parent(&self, parent_id: &str) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", SESSION_BY_PARENT_PREFIX, parent_id);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let session_id = String::from_utf8_lossy(&value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by parent stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Get all sessions in a hierarchy by root ID.
    pub async fn get_sessions_by_root(&self, root_id: &str) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", SESSION_BY_ROOT_PREFIX, root_id);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let session_id = String::from_utf8_lossy(&value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by root stream: {}", e);
                }
            }
        }

        // Sort by depth (ascending) for BFS order
        results.sort_by_key(|s| s.fractal_depth);
        Ok(results)
    }

    /// Get sessions owned by a node.
    pub async fn get_sessions_by_owner(
        &self,
        owner_node_id: &str,
    ) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", SESSION_BY_OWNER_PREFIX, owner_node_id);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let session_id = String::from_utf8_lossy(&value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by owner stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Get sessions by status.
    pub async fn get_sessions_by_status(
        &self,
        status: SessionStatus,
    ) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let status_name = format!("{:?}", status).to_lowercase();
        let prefix = format!("{}{}:", SESSION_BY_STATUS_PREFIX, status_name);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let session_id = String::from_utf8_lossy(&value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by status stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Get sessions by label key-value pair.
    pub async fn get_sessions_by_label(
        &self,
        key: &str,
        value: &str,
    ) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:{}:", SESSION_BY_LABEL_PREFIX, key, value);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, idx_value)) => {
                    let session_id = String::from_utf8_lossy(&idx_value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by label stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Get sessions by tag.
    pub async fn get_sessions_by_tag(&self, tag: &str) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", SESSION_BY_TAG_PREFIX, tag);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let session_id = String::from_utf8_lossy(&value);
                    if let Some(session) = self.get_fractal_session(&session_id).await? {
                        results.push(session);
                    }
                }
                Err(e) => {
                    warn!("Error reading session by tag stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Query fractal sessions with filters.
    pub async fn query_fractal_sessions(
        &self,
        query: &QuerySessionsRequest,
    ) -> HoResult<Vec<FractalSession>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let limit = if query.limit == 0 {
            100
        } else {
            query.limit.min(1000)
        } as usize;

        info!(
            "🔍 Querying fractal sessions with limit: {}, offset: {}",
            limit, query.offset
        );

        let mut stream = snapshot.prefix_raw(FRACTAL_SESSION_PREFIX);
        let mut count = 0;
        let mut skipped = 0;

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<FractalSession>(&value) {
                        Ok(session) => {
                            // Apply filters
                            if query.session_type != 0 && session.session_type != query.session_type
                            {
                                continue;
                            }
                            if query.status != 0 && session.status != query.status {
                                continue;
                            }
                            if !query.owner_node_id.is_empty()
                                && session.owner_node_id != query.owner_node_id
                            {
                                continue;
                            }
                            if !query.parent_session_id.is_empty()
                                && session.parent_session_id != query.parent_session_id
                            {
                                continue;
                            }
                            if !query.root_session_id.is_empty()
                                && session.root_session_id != query.root_session_id
                            {
                                continue;
                            }
                            if query.min_depth > 0 && session.fractal_depth < query.min_depth {
                                continue;
                            }
                            if query.max_depth > 0 && session.fractal_depth > query.max_depth {
                                continue;
                            }

                            // Apply label filters (AND logic)
                            let mut labels_match = true;
                            for (key, value) in &query.label_filters {
                                if session.labels.get(key) != Some(value) {
                                    labels_match = false;
                                    break;
                                }
                            }
                            if !labels_match {
                                continue;
                            }

                            // Apply tag filters (OR logic)
                            if !query.tag_filters.is_empty() {
                                let has_any_tag = query
                                    .tag_filters
                                    .iter()
                                    .any(|tag| session.tags.contains(tag));
                                if !has_any_tag {
                                    continue;
                                }
                            }

                            // Apply offset
                            if skipped < query.offset as usize {
                                skipped += 1;
                                continue;
                            }

                            // Apply limit
                            if count >= limit {
                                break;
                            }

                            results.push(session);
                            count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to deserialize fractal session: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading fractal session stream: {}", e);
                }
            }
        }

        // Sort by created_at (most recent first) by default
        results.sort_by(|a, b| {
            let b_ts = b.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let a_ts = a.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            if query.descending {
                b_ts.cmp(&a_ts)
            } else {
                a_ts.cmp(&b_ts)
            }
        });

        info!("🔍 Query returned {} fractal sessions", results.len());
        Ok(results)
    }

    /// Delete a fractal session and its indices.
    pub async fn delete_fractal_session(&self, session_id: &str) -> HoResult<()> {
        // First get the session to know what indices to delete
        let session = match self.get_fractal_session(session_id).await? {
            Some(s) => s,
            None => return Ok(()), // Already deleted
        };

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Delete main record
        let session_key = format!("{}{}", FRACTAL_SESSION_PREFIX, session_id);
        delta.delete(session_key);

        // Delete parent index
        if !session.parent_session_id.is_empty() {
            let parent_index_key = format!(
                "{}{}:{}",
                SESSION_BY_PARENT_PREFIX, session.parent_session_id, session_id
            );
            delta.delete(parent_index_key);
        }

        // Delete root index
        if !session.root_session_id.is_empty() {
            let root_index_key = format!(
                "{}{}:{:04}:{}",
                SESSION_BY_ROOT_PREFIX, session.root_session_id, session.fractal_depth, session_id
            );
            delta.delete(root_index_key);
        }

        // Delete owner index
        if !session.owner_node_id.is_empty() {
            let owner_index_key = format!(
                "{}{}:{}",
                SESSION_BY_OWNER_PREFIX, session.owner_node_id, session_id
            );
            delta.delete(owner_index_key);
        }

        // Delete status index
        let status_name = match SessionStatus::try_from(session.status) {
            Ok(s) => format!("{:?}", s).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let status_index_key =
            format!("{}{}:{}", SESSION_BY_STATUS_PREFIX, status_name, session_id);
        delta.delete(status_index_key);

        // Delete type index
        let type_name = match SessionType::try_from(session.session_type) {
            Ok(t) => format!("{:?}", t).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let type_index_key = format!("{}{}:{}", SESSION_BY_TYPE_PREFIX, type_name, session_id);
        delta.delete(type_index_key);

        // Delete label indices
        for (key, value) in &session.labels {
            let label_index_key = format!(
                "{}{}:{}:{}",
                SESSION_BY_LABEL_PREFIX, key, value, session_id
            );
            delta.delete(label_index_key);
        }

        // Delete tag indices
        for tag in &session.tags {
            let tag_index_key = format!("{}{}:{}", SESSION_BY_TAG_PREFIX, tag, session_id);
            delta.delete(tag_index_key);
        }

        self.cs.commit(delta).await?;
        info!("🗑️  Deleted fractal session: {}", session_id);
        Ok(())
    }

    /// Store a session state snapshot.
    pub async fn put_session_state_snapshot(
        &self,
        session_id: &str,
        snapshot: &SessionStateSnapshot,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        let key = format!(
            "{}{}:{}",
            SESSION_STATE_PREFIX, session_id, snapshot.state_version
        );
        let data = serde_json::to_vec(snapshot)?;
        delta.put_raw(key.clone(), data);

        // Also store as "latest" for quick access
        let latest_key = format!("{}{}:latest", SESSION_STATE_PREFIX, session_id);
        let latest_data = serde_json::to_vec(snapshot)?;
        delta.put_raw(latest_key, latest_data);

        self.cs.commit(delta).await?;
        info!(
            "📸 Stored session state snapshot: {} (version: {})",
            session_id, snapshot.state_version
        );
        Ok(())
    }

    /// Get the latest session state snapshot.
    pub async fn get_session_state_snapshot(
        &self,
        session_id: &str,
    ) -> HoResult<Option<SessionStateSnapshot>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}:latest", SESSION_STATE_PREFIX, session_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let state: SessionStateSnapshot = serde_json::from_slice(&data)?;
                Ok(Some(state))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get session state snapshot {}: {}", session_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Get a specific version of session state snapshot.
    pub async fn get_session_state_snapshot_version(
        &self,
        session_id: &str,
        version: u64,
    ) -> HoResult<Option<SessionStateSnapshot>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}:{}", SESSION_STATE_PREFIX, session_id, version);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let state: SessionStateSnapshot = serde_json::from_slice(&data)?;
                Ok(Some(state))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!(
                    "Failed to get session state snapshot {}:{}: {}",
                    session_id, version, e
                );
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Count fractal sessions matching query.
    pub async fn count_fractal_sessions(&self, query: &QuerySessionsRequest) -> HoResult<u64> {
        // For now, just query and count. Could be optimized with separate count indices.
        let sessions = self.query_fractal_sessions(query).await?;
        Ok(sessions.len() as u64)
    }

    // ========================================
    // Authenticator Registry Storage Methods
    // ========================================

    /// Register an authenticator contract for an endpoint label.
    /// This maps endpoint labels to CosmWasm contract addresses that will
    /// handle authorization decisions for that endpoint.
    pub async fn put_authenticator(
        &self,
        endpoint_label: &str,
        contract_address: &str,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", AUTHENTICATOR_PREFIX, endpoint_label);
        delta.put_raw(key.clone(), contract_address.as_bytes().to_vec());
        self.cs.commit(delta).await?;
        info!(
            "🔐 Registered authenticator for endpoint '{}': {}",
            endpoint_label, contract_address
        );
        Ok(())
    }

    /// Get the authenticator contract address for an endpoint label.
    /// Returns None if no authenticator is registered for this endpoint.
    pub async fn get_authenticator(&self, endpoint_label: &str) -> HoResult<Option<String>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", AUTHENTICATOR_PREFIX, endpoint_label);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let address = String::from_utf8(data)
                    .map_err(|e| HoError::Storage(format!("Invalid authenticator address: {}", e)))?;
                Ok(Some(address))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!(
                    "Failed to get authenticator for endpoint '{}': {}",
                    endpoint_label, e
                );
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Remove the authenticator for an endpoint label.
    pub async fn delete_authenticator(&self, endpoint_label: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", AUTHENTICATOR_PREFIX, endpoint_label);
        delta.delete(key);

        // Also delete metadata if it exists
        let meta_key = format!("{}{}", AUTHENTICATOR_META_PREFIX, endpoint_label);
        delta.delete(meta_key);

        self.cs.commit(delta).await?;
        info!("🗑️  Removed authenticator for endpoint '{}'", endpoint_label);
        Ok(())
    }

    /// Store metadata for an authenticator endpoint (e.g., description, created_at, etc.).
    pub async fn put_authenticator_metadata(
        &self,
        endpoint_label: &str,
        metadata: &str,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}{}", AUTHENTICATOR_META_PREFIX, endpoint_label);
        delta.put_raw(key, metadata.as_bytes().to_vec());
        self.cs.commit(delta).await?;
        debug!(
            "Stored metadata for authenticator endpoint '{}'",
            endpoint_label
        );
        Ok(())
    }

    /// Get metadata for an authenticator endpoint.
    pub async fn get_authenticator_metadata(
        &self,
        endpoint_label: &str,
    ) -> HoResult<Option<String>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}{}", AUTHENTICATOR_META_PREFIX, endpoint_label);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let metadata = String::from_utf8(data)
                    .map_err(|e| HoError::Storage(format!("Invalid metadata: {}", e)))?;
                Ok(Some(metadata))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!(
                    "Failed to get authenticator metadata for endpoint '{}': {}",
                    endpoint_label, e
                );
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// List all registered authenticator endpoint labels.
    pub async fn list_authenticators(&self) -> HoResult<Vec<(String, String)>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(AUTHENTICATOR_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((key, value)) => {
                    let key_str = String::from_utf8_lossy(key.as_bytes());
                    if let Some(endpoint_label) = key_str.strip_prefix(AUTHENTICATOR_PREFIX) {
                        // Skip metadata entries
                        if endpoint_label.starts_with("metadata/") {
                            continue;
                        }
                        let contract_address = String::from_utf8_lossy(&value).to_string();
                        results.push((endpoint_label.to_string(), contract_address));
                    }
                }
                Err(e) => {
                    warn!("Error reading authenticator stream: {}", e);
                    continue;
                }
            }
        }

        Ok(results)
    }

    /// Check if an endpoint has an authenticator registered.
    pub async fn has_authenticator(&self, endpoint_label: &str) -> bool {
        match self.get_authenticator(endpoint_label).await {
            Ok(Some(_)) => true,
            _ => false,
        }
    }
}

pub async fn handle_prune(
    State(state): State<ErgorsAppState>,
    Json(_request): Json<PromptRequest>,
) -> Json<serde_json::Value> {
    //TODO: prune all non-coordinator nodes storage state by bradcasting its cnidarium state to up to the coordinator node.
    info!("🔌 Step 1: snapshot, prepend metadata & broadcast to coordinator node");
    info!("🔌 Step 2: Dump snapshot of state and broadcast to coordinator node");
    match state.s.create_snapshot().await {
        Ok(_) => {}
        Err(_e) => return Json(error_json("ErgorsStorage snapshot failed", "STORAGE_ERROR")),
    };

    info!("🔌 Step 3: Prune node state");
    match state.s.prune_storage().await {
        Ok(_) => {}
        Err(_e) => return Json(error_json("ErgorsStorage prune failed", "STORAGE_ERROR")),
    };
    Json(error_json("Currently unimplemented", "INVALID_PROMPT"))
}
