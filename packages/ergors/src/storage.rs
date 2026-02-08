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

// NOTE: Cnidarium prefixes must NOT have trailing slashes.
// The delimiter '/' is added when constructing full keys using the helper functions below.

/// Constructs a storage key from a prefix and a single key component.
/// Example: `storage_key("prompts", "abc123")` -> `"prompts/abc123"`
#[inline]
fn storage_key(prefix: &str, key: &str) -> String {
    format!("{}/{}", prefix, key)
}

/// Constructs a storage key from a prefix and two key components separated by `:`.
/// Example: `storage_key2("sessions", "sid", "pid")` -> `"sessions/sid:pid"`
#[inline]
fn storage_key2(prefix: &str, key1: &str, key2: &str) -> String {
    format!("{}/{}:{}", prefix, key1, key2)
}

/// Constructs a storage key from a prefix and three key components separated by `:`.
/// Example: `storage_key3("labels", "env", "prod", "sid")` -> `"labels/env:prod:sid"`
#[inline]
fn storage_key3(prefix: &str, key1: &str, key2: &str, key3: &str) -> String {
    format!("{}/{}:{}:{}", prefix, key1, key2, key3)
}

/// Constructs a query prefix for prefix iteration (ends with the separator).
/// Example: `query_prefix("sessions_by_parent", "root")` -> `"sessions_by_parent/root:"`
#[inline]
fn query_prefix(prefix: &str, key: &str) -> String {
    format!("{}/{}:", prefix, key)
}

/// Constructs a query prefix with two key components.
/// Example: `query_prefix2("labels", "env", "prod")` -> `"labels/env:prod:"`
#[inline]
fn query_prefix2(prefix: &str, key1: &str, key2: &str) -> String {
    format!("{}/{}:{}:", prefix, key1, key2)
}

const PROMPT_PREFIX: &str = "prompts";
const SESSION_INDEX_PREFIX: &str = "sessions";
const USER_INDEX_PREFIX: &str = "users";
const TIMESTAMP_INDEX_PREFIX: &str = "timestamps";
const OP_PREFIX: &str = "operations";
const API_KEY_PREFIX: &str = "custody/api_keys";
const COSMOS_KEY_STORE_KEY: &str = "custody/cosmos_key_store";
const AKASH_WORKFLOW_PREFIX: &str = "akash_workflows";
const AKASH_ENDPOINTS_PREFIX: &str = "akash_endpoints";
const AKASH_LABEL_INDEX_PREFIX: &str = "akash_labels";
const AKASH_ACTIVE_LABELS_PREFIX: &str = "akash_active_labels";
const TRUSTED_PROVIDERS_KEY: &str = "config/trusted_providers";
const AUTHORIZED_CLI_KEYS_KEY: &str = "config/authorized_cli_keys";
const AKASH_PROVIDER_INFO_PREFIX: &str = "akash_provider_info";
// const HEADSTASH: &str = "headstash";
const PROXY_SESSION_PREFIX: &str = "proxy_sessions";
const PROXY_CLIENT_INDEX_PREFIX: &str = "proxy_sessions_by_client";
const PROXY_ROUTER_CONFIG_PREFIX: &str = "proxy_router_config";
const PROXY_ROUTER_CONFIG_KEY: &str = "proxy_router_config/current";

// Git Workspace Storage Prefixes
pub const WORKSPACE_PREFIX: &str = "workspaces";
pub const TASK_WORKTREE_PREFIX: &str = "task_worktrees";
pub const WORKTREE_BY_WORKSPACE_PREFIX: &str = "worktrees_by_workspace";
pub const WORKTREE_BY_NODE_PREFIX: &str = "worktrees_by_node";

// Fractal Session Storage Prefixes
const FRACTAL_SESSION_PREFIX: &str = "fractal_sessions";
const SESSION_BY_PARENT_PREFIX: &str = "sessions_by_parent";
const SESSION_BY_ROOT_PREFIX: &str = "sessions_by_root";
const SESSION_BY_OWNER_PREFIX: &str = "sessions_by_owner";
const SESSION_BY_STATUS_PREFIX: &str = "sessions_by_status";
const SESSION_BY_TYPE_PREFIX: &str = "sessions_by_type";
const SESSION_BY_LABEL_PREFIX: &str = "sessions_by_label";
const SESSION_BY_TAG_PREFIX: &str = "sessions_by_tag";
const SESSION_STATE_PREFIX: &str = "session_states";

// Open Responses Storage Prefix
const OPEN_RESPONSE_PREFIX: &str = "open_responses";

// Custom Authenticator Storage Prefixes
const AUTHENTICATOR_PREFIX: &str = "authenticators";
const AUTHENTICATOR_META_PREFIX: &str = "authenticators/metadata";

// SDL Template Contract Storage Prefix
const SDL_TEMPLATE_CONTRACT_PREFIX: &str = "sdl_template_contracts";

// Cosmos Chain Configuration Storage Prefix
const COSMOS_CHAIN_CONFIG_PREFIX: &str = "cosmos_chain_configs";

// RAG vector database prefixes
const RAG_CONFIG_PREFIX: &str = "rag_config/";

// Gateway storage prefixes
const GATEWAY_CONFIG_PREFIX: &str = "gateway/config";
const GATEWAY_SESSIONS_PREFIX: &str = "gateway/sessions";
const GATEWAY_TOKENS_PREFIX: &str = "custody/gateway_tokens";

// Bootstrap session storage prefix
const BOOTSTRAP_SESSION_PREFIX: &str = "bootstrap_sessions";
const ENCRYPTED_SECRETS_PREFIX: &str = "custody/secrets";
const SECRET_ACCESS_LOG_PREFIX: &str = "audit/secret_access";

// Guild RAG storage prefixes
const GUILD_RAG_CONFIG_PREFIX: &str = "gateway/rag_config";
const GUILD_RAG_AUDIT_PREFIX: &str = "gateway/rag_audit";

// Generic Inbox storage prefixes
const INBOX_PREFIX: &str = "inbox";
const INBOX_STATUS_PREFIX: &str = "inbox_status";
const INBOX_SENDER_PREFIX: &str = "inbox_sender";
const INBOX_ACTION_PREFIX: &str = "inbox_action";
const INBOX_HISTORY_PREFIX: &str = "inbox_history";
const INBOX_CONFIG_KEY: &str = "config/inbox";
const INBOX_NEXT_ID_KEY: &str = "config/inbox_next_id";

/// Defines the storage used for this CwHo. implemenations in ./storage.rs
pub struct ErgorsStorage {
    pub cs: CnidariumStorage,
    /// Mutex to serialize write operations and prevent JMT key/value race conditions.
    /// cnidarium's JMT can get into inconsistent state if concurrent writes happen.
    write_lock: tokio::sync::Mutex<()>,
}

/// Cached provider info for human-readable display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedProviderInfo {
    pub address: String,
    pub host_uri: String,
    pub email: String,
    pub website: String,
    pub attributes: Vec<(String, String)>,
    pub cached_at: i64,
}

impl ErgorsStorage {
    pub async fn new<P: AsRef<Path>>(data_dir: P, prefixes: Vec<String>) -> HoResult<Self> {
        info!("📂 Initializing Cnidarium storage");
        let path = data_dir.as_ref();
        std::fs::create_dir_all(path)?;
        Ok(Self {
            cs: CnidariumStorage::load(path.to_path_buf(), prefixes).await?,
            write_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Commit a delta with write lock to prevent JMT race conditions.
    /// Retries on version conflicts (optimistic concurrency control).
    async fn commit_delta(&self, delta: cnidarium::StateDelta<cnidarium::Snapshot>) -> HoResult<()> {
        let _guard = self.write_lock.lock().await;

        match self.cs.commit(delta).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                // Check if this is a version conflict error
                if err_str.contains("forked from version") || err_str.contains("but the latest version is") {
                    warn!("Version conflict detected, will be retried by caller: {}", err_str);
                    Err(HoError::Storage(format!("Version conflict: {}", err_str)))
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Lock-first commit: acquires write lock, creates a fresh delta from the
    /// latest snapshot, applies mutations via the closure, then commits.
    /// Eliminates TOCTOU version conflicts by ensuring the snapshot is obtained
    /// while holding the lock.
    async fn locked_commit<F>(&self, apply: F) -> HoResult<()>
    where
        F: FnOnce(&mut cnidarium::StateDelta<cnidarium::Snapshot>),
    {
        let _guard = self.write_lock.lock().await;
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        apply(&mut delta);
        self.cs.commit(delta).await?;
        Ok(())
    }

    /// Lock-first commit with a read phase: acquires write lock, runs an async
    /// read from the latest snapshot, then applies mutations and commits.
    /// Use when you need to read existing state before writing.
    async fn locked_read_commit<R, F, Fut>(&self, read: R, apply: F) -> HoResult<()>
    where
        R: FnOnce(cnidarium::Snapshot) -> Fut,
        Fut: std::future::Future<Output = HoResult<Vec<(String, Vec<u8>)>>>,
        F: FnOnce(&mut cnidarium::StateDelta<cnidarium::Snapshot>, Vec<(String, Vec<u8>)>),
    {
        let _guard = self.write_lock.lock().await;
        let snapshot = self.cs.latest_snapshot();
        let read_results = read(snapshot).await?;
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        apply(&mut delta, read_results);
        self.cs.commit(delta).await?;
        Ok(())
    }

    pub async fn put_prompt_w_ctx(
        &self,
        prompt: &PromptResponse,
        original_request: Option<&PromptRequest>,
    ) -> HoResult<()> {
        let id = hex::encode(prompt.id.clone());
        let prompt_data = serde_json::to_vec(prompt)?;
        let prompt_key = storage_key(PROMPT_PREFIX, &id);
        let prompt_id = prompt.id.clone();

        let timestamp_key = format!(
            "{}{:020}:{}",
            TIMESTAMP_INDEX_PREFIX,
            prompt
                .timestamp
                .expect("should always have timestamp")
                .nanos,
            id
        );

        // Pre-compute context indexes
        let context_indexes: Vec<(String, Vec<u8>)> = if let Some(request) = original_request {
            if let Some(ref context) = request.context {
                let sid = context.session_id.clone();
                let uid = context.user_id.clone();
                let session_key = storage_key2(SESSION_INDEX_PREFIX, &sid, &id);
                let user_key = storage_key2(USER_INDEX_PREFIX, &uid, &id);
                debug!("Created session index for {}: {}", sid, id);
                debug!("Created user index for {}: {}", uid, id);
                vec![
                    (session_key, prompt_id.clone()),
                    (user_key, prompt_id.clone()),
                ]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        debug!("Storing prompt {} with timestamp index", id);

        self.locked_commit(|delta| {
            delta.put_raw(prompt_key.clone(), prompt_data);
            delta.put_raw(timestamp_key, prompt_id);
            for (key, value) in context_indexes {
                delta.put_raw(key, value);
            }
        }).await?;

        info!(
            "💾 Successfully stored prompt: {} with key: {}",
            id, prompt_key
        );

        Ok(())
    }

    /// Store Prompt to node storage.
    pub async fn put_prompt(&self, prompt: &PromptResponse) -> HoResult<()> {
        self.put_prompt_w_ctx(prompt, None).await
    }

    pub async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<PromptResponse>> {
        let snapshot = self.cs.latest_snapshot();
        let prompt_key = storage_key(PROMPT_PREFIX, &id.to_string());

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

        let key = storage_key(OPEN_RESPONSE_PREFIX, response_id);
        let data = serde_json::to_vec(&session_data)?;

        self.locked_commit(|delta| {
            delta.put_raw(key, data);
        }).await?;

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
        let key = storage_key(OPEN_RESPONSE_PREFIX, response_id);

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
        let op_key = storage_key(OP_PREFIX, id);
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
        let timestamp_key = format!(
            "{}operations/{:020}:{}",
            TIMESTAMP_INDEX_PREFIX,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            id
        );
        let id_bytes = id.as_bytes().to_vec();

        self.locked_commit(|delta| {
            delta.put_raw(op_key.clone(), operation_data);
            delta.put_raw(timestamp_key, id_bytes);
        }).await?;

        debug!("📝 Stored operation request: {} ({})", id, operation_type);
        Ok(())
    }

    /// Update operation record with response
    pub async fn op_res(&self, id: &str, response_data: Vec<u8>) -> HoResult<()> {
        let key = storage_key(OP_PREFIX, id);

        self.locked_read_commit(
            |snapshot| {
                let key = key.clone();
                async move {
                    let existing_data = snapshot
                        .get_raw(&key)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("Operation not found: {}", key))?;
                    Ok(vec![(key, existing_data)])
                }
            },
            |delta, read_results| {
                let (key, existing_data) = &read_results[0];
                if let Ok(mut operation) = serde_json::from_slice::<OperationRecord>(existing_data) {
                    operation.response = Some(response_data);
                    operation.completed_at = Some(pbjson_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    });
                    if let Ok(operation_data) = serde_json::to_vec(&operation) {
                        delta.put_raw(key.clone(), operation_data);
                    }
                }
            },
        ).await?;

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
        let op_key = storage_key(OP_PREFIX, id);
        let error_msg_owned = error_msg.to_string();
        let error_code = error_code.to_string();

        self.locked_read_commit(
            |snapshot| {
                let op_key = op_key.clone();
                async move {
                    let existing_data = snapshot
                        .get_raw(&op_key)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("Operation not found: {}", op_key))?;
                    Ok(vec![(op_key, existing_data)])
                }
            },
            |delta, read_results| {
                let (key, existing_data) = &read_results[0];
                if let Ok(mut operation) = serde_json::from_slice::<OperationRecord>(existing_data) {
                    operation.error = Some(ErrorResponse {
                        error: error_msg_owned,
                        code: error_code,
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
                    if let Ok(operation_data) = serde_json::to_vec(&operation) {
                        delta.put_raw(key.clone(), operation_data);
                    }
                }
            },
        ).await?;

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
        let op_key = storage_key(OP_PREFIX, id);

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
        let key = storage_key(API_KEY_PREFIX, provider_name);

        let data = serde_json::to_vec(encrypted_key)?;
        delta.put_raw(key.clone(), data);

        self.commit_delta(delta).await?;
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
        let key = storage_key(API_KEY_PREFIX, provider_name);

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
        let key = storage_key(API_KEY_PREFIX, provider_name);

        delta.delete(key);
        self.commit_delta(delta).await?;
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
        self.commit_delta(delta).await?;
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
        let key = storage_key(AKASH_WORKFLOW_PREFIX, &workflow.session_id);
        let data = serde_json::to_vec(workflow)?;
        delta.put_raw(key.clone(), data);

        // Manage label indices based on workflow status
        if !workflow.label.is_empty() {
            let is_active = matches!(
                workflow.status,
                0 | 1 // Pending or Running
            );

            let label_key = storage_key(AKASH_LABEL_INDEX_PREFIX, &workflow.label);
            let active_label_key = storage_key(AKASH_ACTIVE_LABELS_PREFIX, &workflow.label);

            if is_active {
                // Store label -> session_id mapping
                delta.put_raw(label_key, workflow.session_id.as_bytes().to_vec());

                // Store in active labels set for quick uniqueness checks
                delta.put_raw(active_label_key, workflow.session_id.as_bytes().to_vec());

                info!("🏷️  Indexed active deployment label: {} -> {}", workflow.label, workflow.session_id);
            } else {
                // Workflow is no longer active - remove from active labels
                // (keep historical label_key for lookups, only remove active_label_key)
                delta.delete(active_label_key);
                info!("🏷️  Deactivated label (status={}): {}", workflow.status, workflow.label);
            }
        }

        self.commit_delta(delta).await?;
        info!(
            "💾 Stored Akash workflow: {} (step: {:?}, label: {})",
            workflow.session_id,
            workflow.current_step,
            if workflow.label.is_empty() { "<none>" } else { &workflow.label }
        );
        Ok(())
    }

    /// Get an Akash deployment workflow by session ID
    pub async fn get_akash_workflow(
        &self,
        session_id: &str,
    ) -> HoResult<Option<AkashDeploymentWorkflow>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(AKASH_WORKFLOW_PREFIX, session_id);

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

    /// Get an Akash deployment workflow by label (O(1) lookup)
    pub async fn get_akash_workflow_by_label(
        &self,
        label: &str,
    ) -> HoResult<Option<AkashDeploymentWorkflow>> {
        let snapshot = self.cs.latest_snapshot();
        let label_key = storage_key(AKASH_LABEL_INDEX_PREFIX, label);

        match snapshot.get_raw(&label_key).await {
            Ok(Some(session_id_bytes)) => {
                let session_id = String::from_utf8(session_id_bytes)
                    .map_err(|e| HoError::Storage(format!("Invalid session_id in label index: {}", e)))?;
                self.get_akash_workflow(&session_id).await
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get Akash workflow by label {}: {}", label, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Check if a label is already in use by an active deployment
    /// Returns the session_id if label is in use, None otherwise
    pub async fn check_label_collision(&self, label: &str) -> HoResult<Option<String>> {
        let snapshot = self.cs.latest_snapshot();
        let active_label_key = storage_key(AKASH_ACTIVE_LABELS_PREFIX, label);

        match snapshot.get_raw(&active_label_key).await {
            Ok(Some(session_id_bytes)) => {
                let session_id = String::from_utf8(session_id_bytes)
                    .map_err(|e| HoError::Storage(format!("Invalid session_id in active labels: {}", e)))?;
                Ok(Some(session_id))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to check label collision for {}: {}", label, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Get an Akash deployment workflow by either session-id OR label.
    ///
    /// This helper tries to resolve the identifier as:
    /// 1. First as a session-id (UUID format check)
    /// 2. Then as a label (if not UUID format)
    ///
    /// Returns the workflow if found, or an error with helpful message if not found.
    pub async fn get_akash_workflow_by_id_or_label(
        &self,
        id_or_label: &str,
    ) -> HoResult<AkashDeploymentWorkflow> {
        // First try as session-id (direct lookup)
        if let Some(workflow) = self.get_akash_workflow(id_or_label).await? {
            return Ok(workflow);
        }

        // If not found as session-id, try as label
        match self.get_akash_workflow_by_label(id_or_label).await? {
            Some(workflow) => Ok(workflow),
            None => {
                // Not found by either session-id or label
                Err(HoError::Storage(format!(
                    "No deployment found with session-id or label: '{}'. Use 'ergors-cli deploy list' to see available deployments.",
                    id_or_label
                )))
            }
        }
    }

    /// Delete an Akash workflow
    pub async fn delete_akash_workflow(&self, session_id: &str) -> HoResult<()> {
        // Get workflow first to remove label index
        let workflow = self.get_akash_workflow(session_id).await?;

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(AKASH_WORKFLOW_PREFIX, session_id);
        delta.delete(key);

        // Delete label indices if workflow had a label
        if let Some(ref wf) = workflow {
            if !wf.label.is_empty() {
                let label_key = storage_key(AKASH_LABEL_INDEX_PREFIX, &wf.label);
                delta.delete(label_key);

                let active_label_key = storage_key(AKASH_ACTIVE_LABELS_PREFIX, &wf.label);
                delta.delete(active_label_key);

                info!("🏷️  Removed label index: {}", wf.label);
            }
        }

        // Also delete associated endpoints
        let endpoints_key = storage_key(AKASH_ENDPOINTS_PREFIX, session_id);
        delta.delete(endpoints_key);

        self.commit_delta(delta).await?;
        info!("🗑️  Deleted Akash workflow and endpoints: {}", session_id);
        Ok(())
    }

    /// Remove label from active index when deployment completes/fails
    /// (keeps historical label index for queries)
    pub async fn deactivate_deployment_label(&self, label: &str) -> HoResult<()> {
        if label.is_empty() {
            return Ok(());
        }

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let active_label_key = storage_key(AKASH_ACTIVE_LABELS_PREFIX, label);
        delta.delete(active_label_key);
        self.commit_delta(delta).await?;

        info!("🏷️  Deactivated label: {}", label);
        Ok(())
    }

    // ========================================
    // Akash Service Endpoints Storage
    // ========================================

    /// Store service endpoints for an Akash deployment
    pub async fn put_akash_endpoints(
        &self,
        session_id: &str,
        endpoints: &[AkashServiceEndpoint],
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(AKASH_ENDPOINTS_PREFIX, session_id);

        // Store as JSON for easy retrieval
        let data = serde_json::to_vec(endpoints)?;
        delta.put_raw(key.clone(), data);

        self.commit_delta(delta).await?;
        info!("💾 Stored {} endpoints for session: {}", endpoints.len(), session_id);
        Ok(())
    }

    /// Get service endpoints for an Akash deployment
    pub async fn get_akash_endpoints(
        &self,
        session_id: &str,
    ) -> HoResult<Vec<AkashServiceEndpoint>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(AKASH_ENDPOINTS_PREFIX, session_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let endpoints: Vec<AkashServiceEndpoint> = serde_json::from_slice(&data)?;
                Ok(endpoints)
            }
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to retrieve endpoints for {}: {}",
                session_id, e
            ))),
        }
    }

    /// Delete service endpoints for an Akash deployment
    pub async fn delete_akash_endpoints(&self, session_id: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(AKASH_ENDPOINTS_PREFIX, session_id);
        delta.delete(key);
        self.commit_delta(delta).await?;
        info!("🗑️  Deleted endpoints for session: {}", session_id);
        Ok(())
    }

    // ========================================
    // Trusted Providers Storage
    // ========================================

    /// Get list of trusted Akash providers
    pub async fn get_trusted_providers(
        &self,
    ) -> HoResult<ho_std::types::ergors::orch::v1::TrustedProviderList> {
        let snapshot = self.cs.latest_snapshot();

        match snapshot.get_raw(TRUSTED_PROVIDERS_KEY).await {
            Ok(Some(data)) => {
                let list: ho_std::types::ergors::orch::v1::TrustedProviderList =
                    serde_json::from_slice(&data)?;
                Ok(list)
            }
            Ok(None) => {
                // Return empty list if not set
                Ok(ho_std::types::ergors::orch::v1::TrustedProviderList {
                    providers: vec![],
                    updated_at: None,
                })
            }
            Err(e) => {
                warn!("Failed to get trusted providers: {}", e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Store trusted providers list
    pub async fn put_trusted_providers(
        &self,
        list: &ho_std::types::ergors::orch::v1::TrustedProviderList,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let data = serde_json::to_vec(list)?;
        delta.put_raw(TRUSTED_PROVIDERS_KEY.to_string(), data);
        self.commit_delta(delta).await?;
        info!(
            "💾 Stored {} trusted providers",
            list.providers.len()
        );
        Ok(())
    }

    /// Add a trusted provider
    pub async fn add_trusted_provider(&self, address: &str, label: &str) -> HoResult<()> {
        use ho_std::types::ergors::orch::v1::TrustedProvider;

        let mut list = self.get_trusted_providers().await?;

        // Check if already exists
        if list.providers.iter().any(|p| p.address == address) {
            info!("Provider {} already in trusted list", address);
            return Ok(());
        }

        // Add new provider
        let now = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        };

        list.providers.push(TrustedProvider {
            address: address.to_string(),
            label: label.to_string(),
            added_at: Some(now),
        });
        list.updated_at = Some(now);

        self.put_trusted_providers(&list).await?;
        info!("Added trusted provider: {} ({})", address, label);
        Ok(())
    }

    /// Remove a trusted provider
    pub async fn remove_trusted_provider(&self, address: &str) -> HoResult<bool> {
        let mut list = self.get_trusted_providers().await?;
        let original_len = list.providers.len();

        list.providers.retain(|p| p.address != address);

        if list.providers.len() == original_len {
            info!("Provider {} not found in trusted list", address);
            return Ok(false);
        }

        list.updated_at = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        });

        self.put_trusted_providers(&list).await?;
        info!("Removed trusted provider: {}", address);
        Ok(true)
    }

    /// Check if a provider is trusted
    pub async fn is_trusted_provider(&self, address: &str) -> HoResult<bool> {
        let list = self.get_trusted_providers().await?;
        Ok(list.providers.iter().any(|p| p.address == address))
    }

    /// Seed default trusted providers on first startup.
    ///
    /// This imports the hardcoded TRUSTED_PROVIDERS from reputation.rs
    /// into storage, skipping any that are already registered.
    /// Called once during engine initialization.
    pub async fn seed_default_trusted_providers(&self) -> HoResult<()> {
        use crate::deploy::reputation::TRUSTED_PROVIDERS;
        use ho_std::types::ergors::orch::v1::TrustedProvider;

        let mut list = self.get_trusted_providers().await?;

        // Collect existing addresses as owned strings to avoid borrow conflicts
        let existing_addresses: std::collections::HashSet<String> =
            list.providers.iter().map(|p| p.address.clone()).collect();

        let now = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        };

        let mut added_count = 0;
        for address in TRUSTED_PROVIDERS {
            if existing_addresses.contains(*address) {
                continue;
            }

            list.providers.push(TrustedProvider {
                address: address.to_string(),
                label: "default".to_string(),
                added_at: Some(now),
            });
            added_count += 1;
        }

        if added_count > 0 {
            list.updated_at = Some(now);
            self.put_trusted_providers(&list).await?;
            info!(
                "🌱 Seeded {} default trusted providers (total: {})",
                added_count,
                list.providers.len()
            );
        } else {
            debug!("All default trusted providers already registered");
        }

        Ok(())
    }

    // NOTE: Certificate storage functions removed - JWT authentication is used instead

    // ========================================
    // Akash Provider Info Cache
    // ========================================

    /// Store provider info in cache.
    pub async fn put_akash_provider_info(&self, info: &CachedProviderInfo) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(AKASH_PROVIDER_INFO_PREFIX, &info.address);
        let data = serde_json::to_vec(info)?;
        delta.put_raw(key.clone(), data);
        self.commit_delta(delta).await?;
        debug!("📋 Cached provider info for {}", info.address);
        Ok(())
    }

    /// Get provider info from cache.
    pub async fn get_akash_provider_info(
        &self,
        provider_address: &str,
    ) -> HoResult<Option<CachedProviderInfo>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(AKASH_PROVIDER_INFO_PREFIX, provider_address);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let info: CachedProviderInfo = serde_json::from_slice(&data)?;
                Ok(Some(info))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(HoError::Storage(format!(
                "Failed to retrieve provider info for {}: {}",
                provider_address, e
            ))),
        }
    }

    /// List all cached provider info.
    pub async fn list_akash_provider_info(&self) -> HoResult<Vec<CachedProviderInfo>> {
        let snapshot = self.cs.latest_snapshot();
        let mut providers = Vec::new();
        let mut stream = snapshot.prefix_raw(AKASH_PROVIDER_INFO_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_, data)) => {
                    match serde_json::from_slice::<CachedProviderInfo>(&data) {
                        Ok(info) => providers.push(info),
                        Err(e) => warn!("Failed to deserialize provider info: {}", e),
                    }
                }
                Err(e) => {
                    warn!("Error reading provider info stream: {}", e);
                    continue;
                }
            }
        }

        Ok(providers)
    }

    // ========================================
    // Proxy Router Configuration Storage
    // ========================================

    /// Store proxy router configuration (immutable audit log)
    ///
    /// This stores the current proxy router config in cnidarium, providing
    /// a deterministic, immutable log of all endpoint configuration changes.
    /// Each update increments the version number.
    pub async fn put_proxy_router_config(
        &self,
        config: &ho_std::types::ergors::orch::v1::ProxyRouterConfig,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Store current config
        let data = serde_json::to_vec(config)?;
        delta.put_raw(PROXY_ROUTER_CONFIG_KEY.to_string(), data.clone());

        // Also store versioned history entry for immutable audit trail
        let version_key = storage_key(PROXY_ROUTER_CONFIG_PREFIX, &format!("v{}", config.version));
        delta.put_raw(version_key.clone(), data);

        self.commit_delta(delta).await?;
        info!(
            "🔧 Stored proxy router config version {} ({} providers configured)",
            config.version,
            config.providers.len()
        );
        Ok(())
    }

    /// Get the current proxy router configuration
    pub async fn get_proxy_router_config(
        &self,
    ) -> HoResult<Option<ho_std::types::ergors::orch::v1::ProxyRouterConfig>> {
        let snapshot = self.cs.latest_snapshot();

        match snapshot.get_raw(PROXY_ROUTER_CONFIG_KEY).await {
            Ok(Some(data)) => {
                let config: ho_std::types::ergors::orch::v1::ProxyRouterConfig =
                    serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get proxy router config: {}", e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Get a specific version of the proxy router configuration (for audit)
    pub async fn get_proxy_router_config_version(
        &self,
        version: u64,
    ) -> HoResult<Option<ho_std::types::ergors::orch::v1::ProxyRouterConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(PROXY_ROUTER_CONFIG_PREFIX, &format!("v{}", version));

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let config: ho_std::types::ergors::orch::v1::ProxyRouterConfig =
                    serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get proxy router config version {}: {}", version, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// List all proxy router configuration versions (audit trail)
    pub async fn list_proxy_router_config_history(
        &self,
    ) -> HoResult<Vec<ho_std::types::ergors::orch::v1::ProxyRouterConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let mut stream = snapshot.prefix_raw(PROXY_ROUTER_CONFIG_PREFIX);
        let mut configs = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok((key, value)) => {
                    // Skip the "current" key, only get versioned entries
                    if key == PROXY_ROUTER_CONFIG_KEY {
                        continue;
                    }

                    if let Ok(config) =
                        serde_json::from_slice::<ho_std::types::ergors::orch::v1::ProxyRouterConfig>(&value)
                    {
                        configs.push(config);
                    }
                }
                Err(e) => {
                    warn!("Error iterating proxy router config history: {}", e);
                }
            }
        }

        // Sort by version descending (newest first)
        configs.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(configs)
    }

    // ========================================
    // Proxy Session Storage Methods
    // ========================================

    /// Store a proxy session to persistent storage.
    pub async fn put_proxy_session(&self, session: &ProxySession) -> HoResult<()> {
        let session_key = storage_key(PROXY_SESSION_PREFIX, &session.session_id);
        let session_data = serde_json::to_vec(session)?;

        let client_type_name = match session.client_type {
            0 => "unspecified",
            1 => "claude_code",
            2 => "opencode",
            3 => "cursor",
            4 => "custom",
            _ => "unknown",
        };
        let client_index_key = storage_key2(PROXY_CLIENT_INDEX_PREFIX, client_type_name, &session.session_id);
        let session_id_bytes = session.session_id.as_bytes().to_vec();

        let ts_index = session.started_at.as_ref().map(|ts| {
            format!(
                "proxy_sessions_by_time/{:020}:{}",
                ts.seconds, session.session_id
            )
        });

        self.locked_commit(|delta| {
            delta.put_raw(session_key.clone(), session_data);
            delta.put_raw(client_index_key, session_id_bytes.clone());
            if let Some(ts_key) = ts_index {
                delta.put_raw(ts_key, session_id_bytes);
            }
        }).await?;

        info!(
            "💾 Stored proxy session: {} (client: {}, model: {})",
            session.session_id, client_type_name, session.model
        );

        Ok(())
    }

    /// Get a proxy session by ID.
    pub async fn get_proxy_session(&self, session_id: &str) -> HoResult<Option<ProxySession>> {
        let snapshot = self.cs.latest_snapshot();
        let session_key = storage_key(PROXY_SESSION_PREFIX, session_id);

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
        let session_key = storage_key(PROXY_SESSION_PREFIX, session_id);
        delta.delete(session_key);
        self.commit_delta(delta).await?;
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
        let session_key = storage_key(FRACTAL_SESSION_PREFIX, &session.session_id);
        let session_data = serde_json::to_vec(session)?;
        delta.put_raw(session_key.clone(), session_data);

        // Index by parent (for hierarchy traversal)
        if !session.parent_session_id.is_empty() {
            let parent_index_key = storage_key2(SESSION_BY_PARENT_PREFIX, &session.parent_session_id, &session.session_id);
            delta.put_raw(parent_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by root (for full hierarchy queries)
        // Format: prefix/root_id:depth(4-digit padded):session_id
        if !session.root_session_id.is_empty() {
            let depth_str = format!("{:04}", session.fractal_depth);
            let root_index_key = storage_key3(SESSION_BY_ROOT_PREFIX, &session.root_session_id, &depth_str, &session.session_id);
            delta.put_raw(root_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by owner node
        if !session.owner_node_id.is_empty() {
            let owner_index_key = storage_key2(SESSION_BY_OWNER_PREFIX, &session.owner_node_id, &session.session_id);
            delta.put_raw(owner_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by status
        let status_name = match SessionStatus::try_from(session.status) {
            Ok(s) => format!("{:?}", s).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let status_index_key = storage_key2(SESSION_BY_STATUS_PREFIX, &status_name, &session.session_id);
        delta.put_raw(status_index_key, session.session_id.as_bytes().to_vec());

        // Index by type
        let type_name = match SessionType::try_from(session.session_type) {
            Ok(t) => format!("{:?}", t).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let type_index_key = storage_key2(SESSION_BY_TYPE_PREFIX, &type_name, &session.session_id);
        delta.put_raw(type_index_key, session.session_id.as_bytes().to_vec());

        // Index by labels
        for (key, value) in &session.labels {
            let label_index_key = storage_key3(SESSION_BY_LABEL_PREFIX, key, value, &session.session_id);
            delta.put_raw(label_index_key, session.session_id.as_bytes().to_vec());
        }

        // Index by tags
        for tag in &session.tags {
            let tag_index_key = storage_key2(SESSION_BY_TAG_PREFIX, tag, &session.session_id);
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

        self.commit_delta(delta).await?;

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
        let session_key = storage_key(FRACTAL_SESSION_PREFIX, session_id);

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
        let prefix = query_prefix(SESSION_BY_PARENT_PREFIX, parent_id);
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
        let prefix = query_prefix(SESSION_BY_ROOT_PREFIX, root_id);
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
        let prefix = query_prefix(SESSION_BY_OWNER_PREFIX, owner_node_id);
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
        let prefix = query_prefix(SESSION_BY_STATUS_PREFIX, &status_name);
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
        let prefix = query_prefix2(SESSION_BY_LABEL_PREFIX, key, value);
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
        let prefix = query_prefix(SESSION_BY_TAG_PREFIX, tag);
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
        let session_key = storage_key(FRACTAL_SESSION_PREFIX, session_id);
        delta.delete(session_key);

        // Delete parent index
        if !session.parent_session_id.is_empty() {
            let parent_index_key = storage_key2(SESSION_BY_PARENT_PREFIX, &session.parent_session_id, session_id);
            delta.delete(parent_index_key);
        }

        // Delete root index
        if !session.root_session_id.is_empty() {
            let depth_str = format!("{:04}", session.fractal_depth);
            let root_index_key = storage_key3(SESSION_BY_ROOT_PREFIX, &session.root_session_id, &depth_str, session_id);
            delta.delete(root_index_key);
        }

        // Delete owner index
        if !session.owner_node_id.is_empty() {
            let owner_index_key = storage_key2(SESSION_BY_OWNER_PREFIX, &session.owner_node_id, session_id);
            delta.delete(owner_index_key);
        }

        // Delete status index
        let status_name = match SessionStatus::try_from(session.status) {
            Ok(s) => format!("{:?}", s).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let status_index_key =
            storage_key2(SESSION_BY_STATUS_PREFIX, &status_name, session_id);
        delta.delete(status_index_key);

        // Delete type index
        let type_name = match SessionType::try_from(session.session_type) {
            Ok(t) => format!("{:?}", t).to_lowercase(),
            Err(_) => "unknown".to_string(),
        };
        let type_index_key = storage_key2(SESSION_BY_TYPE_PREFIX, &type_name, session_id);
        delta.delete(type_index_key);

        // Delete label indices
        for (key, value) in &session.labels {
            let label_index_key = storage_key3(SESSION_BY_LABEL_PREFIX, key, value, session_id);
            delta.delete(label_index_key);
        }

        // Delete tag indices
        for tag in &session.tags {
            let tag_index_key = storage_key2(SESSION_BY_TAG_PREFIX, tag, session_id);
            delta.delete(tag_index_key);
        }

        self.commit_delta(delta).await?;
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

        let key = storage_key2(SESSION_STATE_PREFIX, session_id, &snapshot.state_version.to_string());
        let data = serde_json::to_vec(snapshot)?;
        delta.put_raw(key.clone(), data);

        // Also store as "latest" for quick access
        let latest_key = storage_key2(SESSION_STATE_PREFIX, session_id, "latest");
        let latest_data = serde_json::to_vec(snapshot)?;
        delta.put_raw(latest_key, latest_data);

        self.commit_delta(delta).await?;
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
        let key = storage_key2(SESSION_STATE_PREFIX, session_id, "latest");

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
        let key = storage_key2(SESSION_STATE_PREFIX, session_id, &version.to_string());

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
        let key = storage_key(AUTHENTICATOR_PREFIX, endpoint_label);
        delta.put_raw(key.clone(), contract_address.as_bytes().to_vec());
        self.commit_delta(delta).await?;
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
        let key = storage_key(AUTHENTICATOR_PREFIX, endpoint_label);

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
        let key = storage_key(AUTHENTICATOR_PREFIX, endpoint_label);
        delta.delete(key);

        // Also delete metadata if it exists
        let meta_key = storage_key(AUTHENTICATOR_META_PREFIX, endpoint_label);
        delta.delete(meta_key);

        self.commit_delta(delta).await?;
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
        let key = storage_key(AUTHENTICATOR_META_PREFIX, endpoint_label);
        delta.put_raw(key, metadata.as_bytes().to_vec());
        self.commit_delta(delta).await?;
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
        let key = storage_key(AUTHENTICATOR_META_PREFIX, endpoint_label);

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

    // ============================================
    // SDL Template Contract Storage
    // ============================================

    /// Register an SDL template contract
    pub async fn register_sdl_template_contract(
        &self,
        contract_address: &str,
        label: Option<String>,
        code_id: u64,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Store contract info as JSON
        let contract_info = serde_json::json!({
            "contract_address": contract_address,
            "label": label,
            "code_id": code_id,
        });
        let info_bytes = serde_json::to_vec(&contract_info)?;

        let key = storage_key(SDL_TEMPLATE_CONTRACT_PREFIX, contract_address);
        delta.put_raw(key, info_bytes);

        self.commit_delta(delta).await?;
        info!("📝 Registered SDL template contract: {}", contract_address);
        Ok(())
    }

    /// Get SDL template contract info by address
    pub async fn get_sdl_template_contract(
        &self,
        contract_address: &str,
    ) -> HoResult<Option<(String, Option<String>, u64)>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(SDL_TEMPLATE_CONTRACT_PREFIX, contract_address);

        match snapshot.get_raw(&key).await? {
            Some(bytes) => {
                let contract_info: serde_json::Value = serde_json::from_slice(&bytes)?;
                let addr = contract_info["contract_address"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing contract_address"))?
                    .to_string();
                let label = contract_info["label"].as_str().map(|s| s.to_string());
                let code_id = contract_info["code_id"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing code_id"))?;
                Ok(Some((addr, label, code_id)))
            }
            None => Ok(None),
        }
    }

    /// List all registered SDL template contracts
    pub async fn list_sdl_template_contracts(
        &self,
    ) -> HoResult<Vec<(String, Option<String>, u64)>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = SDL_TEMPLATE_CONTRACT_PREFIX;
        let mut stream = snapshot.prefix_raw(prefix);
        let mut results = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok((_key, value)) => {
                    // All entries under this prefix are SDL template contracts
                    let contract_info: serde_json::Value = serde_json::from_slice(&value)?;
                    let addr = contract_info["contract_address"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing contract_address"))?
                        .to_string();
                    let label = contract_info["label"].as_str().map(|s| s.to_string());
                    let code_id = contract_info["code_id"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Missing code_id"))?;
                    results.push((addr, label, code_id));
                }
                Err(e) => {
                    warn!("Error reading SDL template contract stream: {}", e);
                    continue;
                }
            }
        }

        Ok(results)
    }

    // ============================================
    // Cosmos Chain Configuration Storage Methods
    // ============================================

    /// Store or update a Cosmos chain configuration in cnidarium
    pub async fn put_chain_config(
        &self,
        config: &ho_std::types::ergors::orch::v1::CosmosChainConfig,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Serialize config to JSON
        let config_bytes = serde_json::to_vec(config)?;

        // Store with chain_id as key
        let key = storage_key(COSMOS_CHAIN_CONFIG_PREFIX, &config.chain_id);
        delta.put_raw(key, config_bytes);

        self.commit_delta(delta).await?;
        info!(
            "Stored chain config for: {} ({})",
            config.chain_name, config.chain_id
        );
        Ok(())
    }

    /// Get a Cosmos chain configuration by chain ID
    pub async fn get_chain_config(
        &self,
        chain_id: &str,
    ) -> HoResult<Option<ho_std::types::ergors::orch::v1::CosmosChainConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(COSMOS_CHAIN_CONFIG_PREFIX, chain_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let config: ho_std::types::ergors::orch::v1::CosmosChainConfig =
                    serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Error getting chain config for {}: {}", chain_id, e);
                Err(e.into())
            }
        }
    }

    /// List all registered Cosmos chain configurations
    pub async fn list_chain_configs(
        &self,
    ) -> HoResult<Vec<ho_std::types::ergors::orch::v1::CosmosChainConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = COSMOS_CHAIN_CONFIG_PREFIX;
        let mut stream = snapshot.prefix_raw(prefix);
        let mut results = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<ho_std::types::ergors::orch::v1::CosmosChainConfig>(&value) {
                        Ok(config) => results.push(config),
                        Err(e) => {
                            warn!("Error deserializing chain config: {}", e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading chain config stream: {}", e);
                    continue;
                }
            }
        }

        Ok(results)
    }

    /// Delete a Cosmos chain configuration
    pub async fn delete_chain_config(&self, chain_id: &str) -> HoResult<()> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(COSMOS_CHAIN_CONFIG_PREFIX, chain_id);

        // Check if config exists before deleting
        match snapshot.get_raw(&key).await {
            Ok(None) => {
                return Err(anyhow::anyhow!("Chain config '{}' not found", chain_id).into());
            }
            Err(e) => return Err(e.into()),
            Ok(Some(_)) => {} // Exists, proceed with deletion
        }

        let mut delta = cnidarium::StateDelta::new(snapshot);
        delta.delete(key);

        self.commit_delta(delta).await?;
        info!("Deleted chain config for: {}", chain_id);
        Ok(())
    }

    // ===== RAG Vector Database Storage Methods =====

    /// Get RAG embedder configuration
    pub async fn get_rag_config(&self) -> HoResult<Option<RagConfigStored>> {
        let snapshot = self.cs.latest_snapshot();
        let key = format!("{}embedder", RAG_CONFIG_PREFIX);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let config: RagConfigStored = serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Error getting RAG config: {}", e);
                Ok(None)
            }
        }
    }

    /// Set RAG embedder configuration
    pub async fn set_rag_config(&self, endpoint: &str, model: &str, dimension: u32) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!("{}embedder", RAG_CONFIG_PREFIX);

        let config = RagConfigStored {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
            dimension,
        };

        delta.put_raw(key, serde_json::to_vec(&config)?);
        self.commit_delta(delta).await?;

        info!("RAG embedder configured: {} ({}, {} dims)", endpoint, model, dimension);
        Ok(())
    }

    /// Get RAG statistics
    pub async fn get_rag_stats(&self) -> HoResult<(u64, u64)> {
        // TODO: Implement actual chunk/source counting from rag_chunks prefix
        // For now, return zeros since we haven't ingested anything yet
        Ok((0, 0))
    }

    /// List ingested sources
    pub async fn list_rag_sources(&self, limit: usize) -> HoResult<(Vec<RagSourceInfoStored>, usize)> {
        // TODO: Implement actual source listing from rag_source_index prefix
        // For now, return empty list
        let _ = limit;
        Ok((vec![], 0))
    }

    // ============================================
    // Gateway Storage Methods
    // ============================================

    /// Get gateway configuration by ID.
    pub async fn get_gateway_config(
        &self,
        gateway_id: &str,
    ) -> HoResult<Option<ho_std::types::ergors::gateway::v1::GatewayConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(GATEWAY_CONFIG_PREFIX, gateway_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let config: ho_std::types::ergors::gateway::v1::GatewayConfig =
                    serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get gateway config {}: {}", gateway_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Store gateway configuration.
    pub async fn put_gateway_config(
        &self,
        config: &ho_std::types::ergors::gateway::v1::GatewayConfig,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(GATEWAY_CONFIG_PREFIX, &config.gateway_id);
        let data = serde_json::to_vec(config)?;
        delta.put_raw(key, data);
        self.commit_delta(delta).await?;
        info!("🌐 Stored gateway config: {}", config.gateway_id);
        Ok(())
    }

    /// List all gateway configurations.
    pub async fn list_gateway_configs(
        &self,
    ) -> HoResult<Vec<ho_std::types::ergors::gateway::v1::GatewayConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(GATEWAY_CONFIG_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<ho_std::types::ergors::gateway::v1::GatewayConfig>(
                        &value,
                    ) {
                        Ok(config) => results.push(config),
                        Err(e) => warn!("Failed to deserialize gateway config: {}", e),
                    }
                }
                Err(e) => warn!("Error reading gateway config stream: {}", e),
            }
        }

        Ok(results)
    }

    /// Get session ID for a gateway thread/channel.
    pub async fn get_gateway_session(
        &self,
        gateway_id: &str,
        thread_id: &str,
    ) -> HoResult<Option<String>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key2(GATEWAY_SESSIONS_PREFIX, gateway_id, thread_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let session_id = String::from_utf8(data)
                    .map_err(|e| HoError::Storage(format!("Invalid session ID: {}", e)))?;
                Ok(Some(session_id))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get gateway session {}:{}: {}", gateway_id, thread_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Get or create a session for a gateway thread.
    pub async fn get_or_create_gateway_session(
        &self,
        gateway_id: &str,
        thread_id: &str,
    ) -> HoResult<String> {
        if let Some(session_id) = self.get_gateway_session(gateway_id, thread_id).await? {
            return Ok(session_id);
        }
        self.create_gateway_session(gateway_id, thread_id).await
    }

    /// Create a new session for a gateway thread.
    pub async fn create_gateway_session(
        &self,
        gateway_id: &str,
        thread_id: &str,
    ) -> HoResult<String> {
        let session_id = format!("gw-{}-{}", gateway_id, Uuid::new_v4());
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key2(GATEWAY_SESSIONS_PREFIX, gateway_id, thread_id);
        delta.put_raw(key, session_id.as_bytes().to_vec());
        self.commit_delta(delta).await?;
        info!("🌐 Created gateway session: {} for {}:{}", session_id, gateway_id, thread_id);
        Ok(session_id)
    }

    /// Store an encrypted gateway token.
    pub async fn store_encrypted_gateway_token(
        &self,
        gateway_id: &str,
        encrypted_token: &[u8],
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(GATEWAY_TOKENS_PREFIX, gateway_id);
        delta.put_raw(key, encrypted_token.to_vec());
        self.commit_delta(delta).await?;
        info!("🔐 Stored encrypted token for gateway: {}", gateway_id);
        Ok(())
    }

    /// Get encrypted gateway token.
    pub async fn get_encrypted_gateway_token(&self, gateway_id: &str) -> HoResult<Option<Vec<u8>>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(GATEWAY_TOKENS_PREFIX, gateway_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => Ok(Some(data)),
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get encrypted gateway token {}: {}", gateway_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    // ============================================
    // Guild RAG Configuration Storage
    // ============================================

    /// Get RAG configuration for a Discord guild.
    pub async fn get_guild_rag_config(
        &self,
        guild_id: &str,
    ) -> HoResult<Option<ho_std::types::ergors::gateway::v1::GuildRagConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(GUILD_RAG_CONFIG_PREFIX, guild_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let config: ho_std::types::ergors::gateway::v1::GuildRagConfig =
                    serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get guild RAG config {}: {}", guild_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Store RAG configuration for a Discord guild.
    pub async fn put_guild_rag_config(
        &self,
        config: &ho_std::types::ergors::gateway::v1::GuildRagConfig,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(GUILD_RAG_CONFIG_PREFIX, &config.guild_id);
        let data = serde_json::to_vec(config)?;
        delta.put_raw(key, data);
        self.commit_delta(delta).await?;
        info!("🧠 Stored guild RAG config: {}", config.guild_id);
        Ok(())
    }

    /// List all guild RAG configurations.
    pub async fn list_guild_rag_configs(
        &self,
    ) -> HoResult<Vec<ho_std::types::ergors::gateway::v1::GuildRagConfig>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(GUILD_RAG_CONFIG_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<ho_std::types::ergors::gateway::v1::GuildRagConfig>(
                        &value,
                    ) {
                        Ok(config) => results.push(config),
                        Err(e) => warn!("Failed to deserialize guild RAG config: {}", e),
                    }
                }
                Err(e) => warn!("Error reading guild RAG config stream: {}", e),
            }
        }

        Ok(results)
    }

    /// Log a guild RAG audit event.
    pub async fn log_guild_rag_audit(
        &self,
        guild_id: &str,
        user_id: &str,
        operation: &str,
        source_uri: &str,
        success: bool,
        message: &str,
    ) -> HoResult<()> {
        use ho_std::types::ergors::gateway::v1::GuildRagAuditLog;

        let log = GuildRagAuditLog {
            guild_id: guild_id.to_string(),
            user_id: user_id.to_string(),
            operation: operation.to_string(),
            source_uri: source_uri.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            success,
            message: message.to_string(),
        };

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = format!(
            "{}/{}:{}",
            GUILD_RAG_AUDIT_PREFIX,
            guild_id,
            chrono::Utc::now().timestamp_millis()
        );
        let data = serde_json::to_vec(&log)?;
        delta.put_raw(key, data);
        self.commit_delta(delta).await?;

        if success {
            debug!("🧠 Guild RAG audit: {} {} {}", guild_id, operation, source_uri);
        } else {
            warn!("🧠 Guild RAG audit (failed): {} {} {} - {}", guild_id, operation, source_uri, message);
        }
        Ok(())
    }

    /// List RAG sources by URI prefix (for guild-scoped listing).
    pub async fn list_rag_sources_by_prefix(
        &self,
        uri_prefix: &str,
        limit: usize,
    ) -> HoResult<Vec<RagSourceInfoStored>> {
        // Query rag_source_index with the prefix
        let snapshot = self.cs.latest_snapshot();
        let storage_prefix = format!("rag_source_index/{}", uri_prefix);
        let mut stream = snapshot.prefix_raw(&storage_prefix);
        let mut results = Vec::new();

        while let Some(entry_result) = stream.next().await {
            if results.len() >= limit {
                break;
            }
            match entry_result {
                Ok((key, value)) => {
                    // Key format: rag_source_index/{source_uri}
                    // Value: list of chunk IDs
                    let key_str = String::from_utf8_lossy(key.as_bytes());
                    let uri = key_str.strip_prefix("rag_source_index/").unwrap_or(&key_str);

                    // Parse chunk IDs to get count
                    let chunk_ids: Vec<String> = serde_json::from_slice(&value).unwrap_or_default();

                    results.push(RagSourceInfoStored {
                        uri: uri.to_string(),
                        chunk_count: chunk_ids.len() as u32,
                        doc_type: "unknown".to_string(), // Would need to look up chunk metadata
                        ingested_at: "".to_string(),
                    });
                }
                Err(e) => warn!("Error reading RAG source index: {}", e),
            }
        }

        Ok(results)
    }

    /// Delete RAG source by URI (deletes all chunks for that source).
    pub async fn delete_rag_source(&self, source_uri: &str) -> HoResult<u32> {
        let snapshot = self.cs.latest_snapshot();
        let index_key = format!("rag_source_index/{}", source_uri);

        // Get chunk IDs for this source
        let chunk_ids: Vec<String> = match snapshot.get_raw(&index_key).await {
            Ok(Some(data)) => serde_json::from_slice(&data).unwrap_or_default(),
            Ok(None) => return Ok(0),
            Err(e) => {
                warn!("Failed to get source index {}: {}", source_uri, e);
                return Ok(0);
            }
        };

        let count = chunk_ids.len() as u32;

        // Delete chunks and index
        let mut delta = cnidarium::StateDelta::new(snapshot);
        for chunk_id in &chunk_ids {
            let chunk_key = format!("rag_chunks/{}", chunk_id);
            delta.delete(chunk_key);
        }
        delta.delete(index_key);
        self.commit_delta(delta).await?;

        info!("🧠 Deleted {} chunks for source: {}", count, source_uri);
        Ok(count)
    }

    // ===== Encrypted Secret Storage (v2 - unified, audited) =====

    /// Store an encrypted secret with audit logging.
    /// Uses the new EncryptedSecret proto type for unified secret management.
    pub async fn store_encrypted_secret(
        &self,
        secret: &EncryptedSecret,
        accessor: &str,
        purpose: &str,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(ENCRYPTED_SECRETS_PREFIX, &secret.secret_id);

        let data = serde_json::to_vec(secret)?;
        delta.put_raw(key, data);

        // Log the storage operation
        let log = SecretAccessLog {
            secret_id: secret.secret_id.clone(),
            accessed_at: Some(pbjson_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            accessor: accessor.to_string(),
            purpose: format!("store: {}", purpose),
            success: true,
            error: String::new(),
        };
        let log_key = format!(
            "{}/{}:{}",
            SECRET_ACCESS_LOG_PREFIX,
            secret.secret_id,
            chrono::Utc::now().timestamp_millis()
        );
        let log_data = serde_json::to_vec(&log)?;
        delta.put_raw(log_key, log_data);

        self.commit_delta(delta).await?;
        info!("🔐 Stored encrypted secret: {} (by {})", secret.secret_id, accessor);
        Ok(())
    }

    /// Get an encrypted secret by ID with audit logging.
    pub async fn get_encrypted_secret(
        &self,
        secret_id: &str,
        accessor: &str,
        purpose: &str,
    ) -> HoResult<Option<EncryptedSecret>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(ENCRYPTED_SECRETS_PREFIX, secret_id);

        let result = match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let mut secret: EncryptedSecret = serde_json::from_slice(&data)?;
                // Update access metadata
                secret.last_accessed_at = Some(pbjson_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                });
                secret.access_count += 1;
                Ok(Some(secret))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ho_std::error::HoError::Anyhow(e)),
        };

        // Log the access attempt (even if failed/not found)
        let success = result.as_ref().map(|r| r.is_some()).unwrap_or(false);
        let error = if result.is_err() {
            format!("{:?}", result.as_ref().err())
        } else if !success {
            "not_found".to_string()
        } else {
            String::new()
        };

        let log = SecretAccessLog {
            secret_id: secret_id.to_string(),
            accessed_at: Some(pbjson_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            accessor: accessor.to_string(),
            purpose: format!("get: {}", purpose),
            success,
            error,
        };

        // Store log in separate delta (don't fail main operation if logging fails)
        let log_key = format!(
            "{}/{}:{}",
            SECRET_ACCESS_LOG_PREFIX,
            secret_id,
            chrono::Utc::now().timestamp_millis()
        );
        if let Ok(log_data) = serde_json::to_vec(&log) {
            let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
            delta.put_raw(log_key, log_data);
            let _ = self.commit_delta(delta).await;
        }

        if success {
            debug!("🔐 Retrieved encrypted secret: {} (by {})", secret_id, accessor);
        }
        result
    }

    /// List all secret access logs for audit purposes.
    pub async fn list_secret_access_logs(
        &self,
        secret_id: Option<&str>,
        limit: usize,
    ) -> HoResult<Vec<SecretAccessLog>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = if let Some(id) = secret_id {
            format!("{}/{}:", SECRET_ACCESS_LOG_PREFIX, id)
        } else {
            format!("{}/", SECRET_ACCESS_LOG_PREFIX)
        };

        let mut stream = snapshot.prefix_raw(&prefix);
        let mut logs = Vec::new();

        while let Some(result) = stream.next().await {
            if logs.len() >= limit {
                break;
            }
            if let Ok((_key, data)) = result {
                if let Ok(log) = serde_json::from_slice::<SecretAccessLog>(&data) {
                    logs.push(log);
                }
            }
        }

        Ok(logs)
    }

    // ===== Bootstrap Session Storage =====

    /// Save bootstrap session state
    pub async fn save_bootstrap_state(
        &self,
        session_id: &str,
        state: &crate::deploy::BootstrapState,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(BOOTSTRAP_SESSION_PREFIX, session_id);
        let data = serde_json::to_vec(state)
            .map_err(|e| HoError::Storage(format!("Failed to serialize bootstrap state: {}", e)))?;
        delta.put_raw(key, data);
        self.commit_delta(delta).await?;
        debug!("💾 Saved bootstrap session: {}", session_id);
        Ok(())
    }

    /// Load bootstrap session state
    pub async fn load_bootstrap_state(
        &self,
        session_id: &str,
    ) -> HoResult<Option<crate::deploy::BootstrapState>> {
        let snapshot = self.cs.latest_snapshot();
        let key = storage_key(BOOTSTRAP_SESSION_PREFIX, session_id);

        match snapshot.get_raw(&key).await {
            Ok(Some(data)) => {
                let state = serde_json::from_slice(&data)
                    .map_err(|e| HoError::Storage(format!("Failed to deserialize bootstrap state: {}", e)))?;
                Ok(Some(state))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to load bootstrap session {}: {}", session_id, e);
                Err(HoError::Anyhow(e))
            }
        }
    }

    /// List all bootstrap sessions
    pub async fn list_bootstrap_sessions(&self) -> HoResult<Vec<crate::deploy::BootstrapState>> {
        let snapshot = self.cs.latest_snapshot();
        let mut sessions = Vec::new();
        let mut stream = snapshot.prefix_raw(BOOTSTRAP_SESSION_PREFIX);

        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, data)) => {
                    match serde_json::from_slice::<crate::deploy::BootstrapState>(&data) {
                        Ok(state) => sessions.push(state),
                        Err(e) => warn!("Failed to deserialize bootstrap session: {}", e),
                    }
                }
                Err(e) => {
                    warn!("Error reading bootstrap session stream: {}", e);
                    continue;
                }
            }
        }

        // Sort by created_at (most recent first)
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(sessions)
    }

    /// Delete bootstrap session
    pub async fn delete_bootstrap_session(&self, session_id: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let key = storage_key(BOOTSTRAP_SESSION_PREFIX, session_id);
        delta.delete(key);
        self.commit_delta(delta).await?;
        info!("🗑️  Deleted bootstrap session: {}", session_id);
        Ok(())
    }

    // ===== Generic Inbox Storage =====

    /// Get the next inbox message ID (atomic increment).
    pub async fn next_inbox_id(&self) -> HoResult<u64> {
        let snapshot = self.cs.latest_snapshot();
        let current = match snapshot.get_raw(INBOX_NEXT_ID_KEY).await {
            Ok(Some(data)) if data.len() == 8 => {
                u64::from_be_bytes(data.try_into().unwrap())
            }
            _ => 1,
        };
        let next = current + 1;
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        delta.put_raw(INBOX_NEXT_ID_KEY.to_string(), next.to_be_bytes().to_vec());
        self.commit_delta(delta).await?;
        Ok(current)
    }

    /// Save an inbox message with status, sender, and action_type indexes.
    pub async fn save_inbox_message(&self, msg: &InboxMessage) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let id_str = msg.id.to_string();
        let data = serde_json::to_vec(msg)?;

        // Primary key: inbox/{id}
        delta.put_raw(storage_key(INBOX_PREFIX, &id_str), data);

        // Status index: inbox_status/{status}:{id}
        let status_val = msg.status.to_string();
        delta.put_raw(
            storage_key2(INBOX_STATUS_PREFIX, &status_val, &id_str),
            id_str.as_bytes().to_vec(),
        );

        // Sender index: inbox_sender/{hex_pubkey}:{id}
        let sender_hex = hex::encode(&msg.sender_pubkey);
        delta.put_raw(
            storage_key2(INBOX_SENDER_PREFIX, &sender_hex, &id_str),
            id_str.as_bytes().to_vec(),
        );

        // Action type index: inbox_action/{action_type}:{id}
        delta.put_raw(
            storage_key2(INBOX_ACTION_PREFIX, &msg.action_type, &id_str),
            id_str.as_bytes().to_vec(),
        );

        self.commit_delta(delta).await?;
        debug!("Saved inbox message {} (action: {})", msg.id, msg.action_type);
        Ok(())
    }

    /// Get an inbox message by ID (checks inbox first, then history).
    pub async fn get_inbox_message(&self, id: u64) -> HoResult<Option<InboxMessage>> {
        let snapshot = self.cs.latest_snapshot();
        let id_str = id.to_string();

        // Check inbox
        if let Ok(Some(data)) = snapshot.get_raw(&storage_key(INBOX_PREFIX, &id_str)).await {
            let msg: InboxMessage = serde_json::from_slice(&data)?;
            return Ok(Some(msg));
        }

        // Check history
        if let Ok(Some(data)) = snapshot.get_raw(&storage_key(INBOX_HISTORY_PREFIX, &id_str)).await
        {
            let msg: InboxMessage = serde_json::from_slice(&data)?;
            return Ok(Some(msg));
        }

        Ok(None)
    }

    /// List inbox messages by status (prefix scan).
    pub async fn list_inbox_by_status(&self, status: i32) -> HoResult<Vec<InboxMessage>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = query_prefix(INBOX_STATUS_PREFIX, &status.to_string());

        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(&prefix);

        while let Some(entry) = stream.next().await {
            match entry {
                Ok((_key, id_bytes)) => {
                    let id_str = String::from_utf8_lossy(&id_bytes);
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Ok(Some(msg)) = self.get_inbox_message(id).await {
                            results.push(msg);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error scanning inbox status index: {}", e);
                    continue;
                }
            }
        }

        results.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(results)
    }

    /// List inbox messages by action type (prefix scan).
    pub async fn list_inbox_by_action(&self, action_type: &str) -> HoResult<Vec<InboxMessage>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = query_prefix(INBOX_ACTION_PREFIX, action_type);

        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(&prefix);

        while let Some(entry) = stream.next().await {
            match entry {
                Ok((_key, id_bytes)) => {
                    let id_str = String::from_utf8_lossy(&id_bytes);
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Ok(Some(msg)) = self.get_inbox_message(id).await {
                            results.push(msg);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error scanning inbox action index: {}", e);
                    continue;
                }
            }
        }

        results.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(results)
    }

    /// List inbox messages by sender pubkey (prefix scan).
    pub async fn list_inbox_by_sender(&self, pubkey: &[u8]) -> HoResult<Vec<InboxMessage>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = query_prefix(INBOX_SENDER_PREFIX, &hex::encode(pubkey));

        let mut results = Vec::new();
        let mut stream = snapshot.prefix_raw(&prefix);

        while let Some(entry) = stream.next().await {
            match entry {
                Ok((_key, id_bytes)) => {
                    let id_str = String::from_utf8_lossy(&id_bytes);
                    if let Ok(id) = id_str.parse::<u64>() {
                        if let Ok(Some(msg)) = self.get_inbox_message(id).await {
                            results.push(msg);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error scanning inbox sender index: {}", e);
                    continue;
                }
            }
        }

        results.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(results)
    }

    /// Update an inbox message's status. Removes old status index, writes new one.
    pub async fn update_inbox_status(
        &self,
        id: u64,
        new_status: i32,
        reason: &str,
    ) -> HoResult<Option<InboxMessage>> {
        let Some(mut msg) = self.get_inbox_message(id).await? else {
            return Ok(None);
        };

        let old_status = msg.status.to_string();
        let id_str = id.to_string();

        msg.status = new_status;
        if !reason.is_empty() {
            msg.rejection_reason = reason.to_string();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        msg.updated_at = Some(pbjson_types::Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        });

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Remove old status index
        delta.delete(storage_key2(INBOX_STATUS_PREFIX, &old_status, &id_str));

        // Write new status index
        delta.put_raw(
            storage_key2(INBOX_STATUS_PREFIX, &new_status.to_string(), &id_str),
            id_str.as_bytes().to_vec(),
        );

        // Update primary record
        delta.put_raw(
            storage_key(INBOX_PREFIX, &id_str),
            serde_json::to_vec(&msg)?,
        );

        self.commit_delta(delta).await?;
        debug!("Updated inbox message {} status to {}", id, new_status);
        Ok(Some(msg))
    }

    /// Move an inbox message from inbox to history (terminal states).
    pub async fn move_to_inbox_history(&self, id: u64) -> HoResult<()> {
        let id_str = id.to_string();
        let snapshot = self.cs.latest_snapshot();

        let inbox_key = storage_key(INBOX_PREFIX, &id_str);
        let Some(data) = snapshot.get_raw(&inbox_key).await.ok().flatten() else {
            return Ok(());
        };

        let msg: InboxMessage = serde_json::from_slice(&data)?;
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Write to history
        delta.put_raw(storage_key(INBOX_HISTORY_PREFIX, &id_str), data);

        // Remove from inbox
        delta.delete(inbox_key);

        // Remove indexes
        delta.delete(storage_key2(
            INBOX_STATUS_PREFIX,
            &msg.status.to_string(),
            &id_str,
        ));
        delta.delete(storage_key2(
            INBOX_SENDER_PREFIX,
            &hex::encode(&msg.sender_pubkey),
            &id_str,
        ));
        delta.delete(storage_key2(
            INBOX_ACTION_PREFIX,
            &msg.action_type,
            &id_str,
        ));

        self.commit_delta(delta).await?;
        debug!("Moved inbox message {} to history", id);
        Ok(())
    }

    // ===== Granter Config Storage =====

    /// Save granter configuration.
    pub async fn save_granter_config(&self, config: &GranterConfig) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        delta.put_raw(INBOX_CONFIG_KEY.to_string(), serde_json::to_vec(config)?);
        self.commit_delta(delta).await?;
        debug!("Saved granter config");
        Ok(())
    }

    /// Get granter configuration.
    pub async fn get_granter_config(&self) -> HoResult<Option<GranterConfig>> {
        let snapshot = self.cs.latest_snapshot();
        match snapshot.get_raw(INBOX_CONFIG_KEY).await {
            Ok(Some(data)) => {
                let config: GranterConfig = serde_json::from_slice(&data)?;
                Ok(Some(config))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get granter config: {}", e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    // ========================================
    // Authorized CLI Keys Storage
    // ========================================

    /// Get list of authorized CLI keys
    pub async fn get_authorized_cli_keys(
        &self,
    ) -> HoResult<ho_std::types::ergors::management::v1::ListCliKeysResponse> {
        use ho_std::types::ergors::management::v1::ListCliKeysResponse;

        let snapshot = self.cs.latest_snapshot();
        match snapshot.get_raw(AUTHORIZED_CLI_KEYS_KEY).await {
            Ok(Some(data)) => {
                let list: ListCliKeysResponse = serde_json::from_slice(&data)?;
                Ok(list)
            }
            Ok(None) => Ok(ListCliKeysResponse { keys: vec![] }),
            Err(e) => {
                warn!("Failed to get authorized CLI keys: {}", e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// Store authorized CLI keys list
    pub async fn put_authorized_cli_keys(
        &self,
        list: &ho_std::types::ergors::management::v1::ListCliKeysResponse,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let data = serde_json::to_vec(list)?;
        delta.put_raw(AUTHORIZED_CLI_KEYS_KEY.to_string(), data);
        self.commit_delta(delta).await?;
        debug!("Stored {} authorized CLI keys", list.keys.len());
        Ok(())
    }

    /// Add an authorized CLI key (dedup by public_key_hex)
    pub async fn add_authorized_cli_key(
        &self,
        pubkey_hex: &str,
        label: &str,
    ) -> HoResult<()> {
        use ho_std::types::ergors::management::v1::CliKeyEntry;

        let mut list = self.get_authorized_cli_keys().await?;

        if list.keys.iter().any(|k| k.public_key_hex == pubkey_hex) {
            info!("CLI key {} already authorized", pubkey_hex);
            return Ok(());
        }

        let now = pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        };

        list.keys.push(CliKeyEntry {
            public_key_hex: pubkey_hex.to_string(),
            label: label.to_string(),
            added_at: Some(now),
        });

        self.put_authorized_cli_keys(&list).await?;
        info!("Added authorized CLI key: {} ({})", pubkey_hex, label);
        Ok(())
    }

    /// Remove an authorized CLI key
    pub async fn remove_authorized_cli_key(&self, pubkey_hex: &str) -> HoResult<bool> {
        let mut list = self.get_authorized_cli_keys().await?;
        let original_len = list.keys.len();

        list.keys.retain(|k| k.public_key_hex != pubkey_hex);

        if list.keys.len() == original_len {
            info!("CLI key {} not found in authorized list", pubkey_hex);
            return Ok(false);
        }

        self.put_authorized_cli_keys(&list).await?;
        info!("Removed authorized CLI key: {}", pubkey_hex);
        Ok(true)
    }

    /// Check if a CLI key is authorized
    pub async fn is_authorized_cli_key(&self, pubkey_hex: &str) -> HoResult<bool> {
        let list = self.get_authorized_cli_keys().await?;
        Ok(list.keys.iter().any(|k| k.public_key_hex == pubkey_hex))
    }
}

// ===== RAG Storage Types (outside impl block) =====

/// RAG configuration stored in storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RagConfigStored {
    pub endpoint: String,
    pub model: String,
    pub dimension: u32,
}

/// RAG source info for listing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RagSourceInfoStored {
    pub uri: String,
    pub chunk_count: u32,
    pub doc_type: String,
    pub ingested_at: String,
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
