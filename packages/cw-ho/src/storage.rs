use crate::ErgorsStorage;

use cnidarium::{StateRead, StateWrite, Storage as CnidariumStorage};
use futures::StreamExt;

use ho_std::llm::{HoError, HoResult};
use ho_std::traits::{MessageExt, StorageConfigTrait};
use ho_std::types::ergors::{orch::v1::*, storage::v1::*};
use std::path::Path;
use tracing::{debug, info, warn};
use uuid::Uuid;

const PROMPT_PREFIX: &str = "prompts/";
const SESSION_INDEX_PREFIX: &str = "sessions/";
const USER_INDEX_PREFIX: &str = "users/";
const TIMESTAMP_INDEX_PREFIX: &str = "timestamps/";
const OPERATION_PREFIX: &str = "operations/";

impl StorageConfigTrait for ErgorsStorage {
    fn data_dir(&self) -> &str {
        // TODO: default data dir should be in the node_data folder along with config
        todo!()
    }
    fn max_size_mb(&self) -> u32 {
        1000000000
    }

    fn is_compression_enabled(&self) -> bool {
        true
    }

    fn set_data_dir(&mut self, dir: String) {
        todo!()
    }

    fn set_max_size_mb(&mut self, size: u32) {}

    fn set_compression(&mut self, enabled: bool) {}
}

impl ErgorsStorage {
    pub async fn new<P: AsRef<Path>>(data_dir: P) -> HoResult<Self> {
        let path = data_dir.as_ref();
        std::fs::create_dir_all(path)?;

        info!("📂 Initializing Cnidarium storage at: {}", path.display());
        // Define substore prefixes to align with multistore routing
        let prefixes = vec![
            "network_config".to_string(),
            "akashic_record".to_string(),
            "models_tools".to_string(),
        ];

        let cnidarium = CnidariumStorage::load(path.to_path_buf(), prefixes).await?;

        Ok(Self { cnidarium })
    }

    pub async fn store_prompt_with_context(
        &self,
        prompt: &PromptResponse,
        original_request: Option<&PromptRequest>,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cnidarium.latest_snapshot());
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
                // Index by session_id if present
                if let Some(ref session_id) = context.session_id {
                    let session_key = format!("{}{}:{}", SESSION_INDEX_PREFIX, session_id, id);
                    delta.put_raw(session_key, prompt.id.clone());
                    debug!("Created session index for {}: {}", session_id, id);
                }

                // Index by user_id if present
                if let Some(ref user_id) = context.user_id {
                    let user_key = format!("{}{}:{}", USER_INDEX_PREFIX, user_id, id);
                    delta.put_raw(user_key, prompt.id.clone());
                    debug!("Created user index for {}: {}", user_id, id);
                }
            }
        }

        debug!("Storing prompt {} with timestamp index", id);

        // Commit the changes
        self.cnidarium.commit(delta).await?;

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

    // Backward compatibility method
    pub async fn store_prompt(&self, prompt: &PromptResponse) -> HoResult<()> {
        self.store_prompt_with_context(prompt, None).await
    }

    pub async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<PromptResponse>> {
        let snapshot = self.cnidarium.latest_snapshot();
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

    pub async fn query_prompts(&self, query: &QueryRequest) -> HoResult<Vec<PromptResponse>> {
        let snapshot = self.cnidarium.latest_snapshot();
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
                    let key_str = String::from_utf8_lossy(&key.as_bytes());
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
            let b_ts = b.timestamp.expect("always have one");
            let a_ts = a.timestamp.expect("always have one");

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
        if let Some(ref query_session_id) = query.session_id {
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
        if let Some(ref query_user_id) = query.user_id {
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
        let _snapshot = self.cnidarium.latest_snapshot();

        // Try a simple read operation
        let test_key = "health_check";
        let snapshot = self.cnidarium.latest_snapshot();

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
        let _snapshot = self.cnidarium.latest_snapshot();
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
        let mut delta = cnidarium::StateDelta::new(self.cnidarium.latest_snapshot());
        let operation_key = format!("{}{}", OPERATION_PREFIX, id);
        let operation = OperationRecord {
            id: operation_key.to_string(),
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

        delta.put_raw(
            operation_key.clone(),
            operation.to_bytes().map_err(HoError::EncodeError)?,
        );

        // Create timestamp index
        let timestamp_key = format!(
            "{}operations/{:020}:{}",
            TIMESTAMP_INDEX_PREFIX,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            id
        );
        delta.put_raw(timestamp_key, id.as_bytes().to_vec());

        self.cnidarium.commit(delta).await?;

        debug!("📝 Stored operation request: {} ({})", id, operation_type);
        Ok(())
    }

    /// Update operation record with response
    pub async fn op_res(&self, id: &str, response_data: Vec<u8>) -> HoResult<()> {
        let snapshot = self.cnidarium.latest_snapshot();
        let key = format!("{}{}", OPERATION_PREFIX, id);

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

        self.cnidarium.commit(delta).await?;

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
        let snapshot = self.cnidarium.latest_snapshot();
        let operation_key = format!("{}{}", OPERATION_PREFIX, id);

        // Get existing operation
        let existing_data = snapshot
            .get_raw(&operation_key)
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
        delta.put_raw(operation_key, operation_data);

        self.cnidarium.commit(delta).await?;

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
        let limit = limit.unwrap_or(100).min(1000);
        let snapshot = self.cnidarium.latest_snapshot();
        let mut operation_stream = snapshot.prefix_raw(OPERATION_PREFIX);

        while let Some(entry_result) = operation_stream.next().await {
            if count >= limit {
                break;
            }

            match entry_result {
                Ok((key, value)) => {
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
    pub async fn get_operation(&self, id: &str) -> HoResult<Option<OperationRecord>> {
        let snapshot = self.cnidarium.latest_snapshot();
        let operation_key = format!("{}{}", OPERATION_PREFIX, id);

        match snapshot.get_raw(&operation_key).await {
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
}
